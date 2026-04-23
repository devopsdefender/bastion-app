//! `bastion` — CLI front-end for the unified terminal client.
//!
//!   - `bastion whoami`           — show device identity fingerprint + pubkey
//!   - `bastion list`             — dump the connector store
//!   - `bastion add dd-enclave`   — add a DD enclave by label/origin
//!   - `bastion rm <id>`          — remove a connector
//!   - `bastion pair <cp-url>`    — print the enroll URL for this device
//!   - `bastion connect <id>`     — one-shot Noise_IK EE method call
//!   - `bastion attach <id>`      — attach to a remote PTY (typically tmux)

use anyhow::{anyhow, bail, Context, Result};
use bastion_core::{
    ee_attach, fetch_attest, fingerprint, keypair_from_seed, load_or_mint_seed, Attestation,
    Connector, ConnectorKind, EeClient, NoiseClient, Store,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "bastion", version, about, long_about = None)]
struct Cli {
    /// Override the config directory (defaults to the platform's
    /// standard: ~/.config/bastion-app on Linux).
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show this device's identity fingerprint + X25519 pubkey.
    Whoami,
    /// List all configured connectors.
    List,
    /// Add a new connector.
    Add {
        #[command(subcommand)]
        kind: AddKind,
    },
    /// Remove a connector by id.
    Rm { id: String },
    /// Register this device's pubkey with a DD control plane so its
    /// enclaves will accept Noise_IK handshakes from us.
    Pair {
        /// Base URL of the DD control plane (e.g. `https://app.devopsdefender.com`).
        cp_url: String,
        /// Human-readable label the CP shows for this device.
        #[arg(long)]
        label: Option<String>,
    },
    /// Open a Noise_IK session to a `dd-enclave` connector and call
    /// a single EE method. Supports `health`, `list`, `logs`, `exec`.
    Connect {
        /// Connector id (from `bastion list`).
        id: String,
        /// Which EE method to call. Default `health` — cheap sanity check.
        #[arg(long, default_value = "health")]
        method: String,
        /// For `logs`: deployment id to tail.
        #[arg(long)]
        deployment: Option<String>,
        /// For `logs`: number of lines to tail.
        #[arg(long, default_value_t = 200)]
        tail: u32,
        /// For `exec`: argv as a single shell-quoted string, e.g.
        /// `--cmd "uname -a"`. Split via shell_words.
        #[arg(long)]
        cmd: Option<String>,
        /// For `exec`: seconds EE waits for the child before reaping.
        #[arg(long)]
        timeout_secs: Option<u32>,
    },
    /// Open a Noise_IK session to a `dd-enclave` connector and attach
    /// to a PTY — bidirectional raw-byte bridge between the local
    /// stdin/stdout and the enclave's shell. Terminate with Ctrl-D
    /// (stdin EOF) or by exiting the remote shell. No local raw-mode
    /// handling, so line-buffered + local echo until a TTY follow-up.
    Attach {
        id: String,
        /// Shell argv to exec under a PTY. Split by whitespace.
        #[arg(long, default_value = "bash -l")]
        cmd: String,
    },
}

#[derive(Subcommand)]
enum AddKind {
    /// Add a DD enclave.
    DdEnclave {
        #[arg(long)]
        label: String,
        /// HTTPS origin of the enclave's bastion surface, e.g.
        /// `https://block.pr-42.devopsdefender.com`.
        #[arg(long)]
        origin: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_dir = match cli.config_dir {
        Some(p) => p,
        None => bastion_core::identity::default_config_dir()?,
    };

    match cli.cmd {
        Cmd::Whoami => whoami(&config_dir),
        Cmd::List => list(&config_dir),
        Cmd::Add { kind } => add(&config_dir, kind),
        Cmd::Rm { id } => remove(&config_dir, &id),
        Cmd::Pair { cp_url, label } => pair(&config_dir, &cp_url, label).await,
        Cmd::Connect {
            id,
            method,
            deployment,
            tail,
            cmd,
            timeout_secs,
        } => {
            connect(
                &config_dir,
                &id,
                &method,
                deployment.as_deref(),
                tail,
                cmd.as_deref(),
                timeout_secs,
            )
            .await
        }
        Cmd::Attach { id, cmd } => attach(&config_dir, &id, &cmd).await,
    }
}

fn whoami(dir: &std::path::Path) -> Result<()> {
    let seed = load_or_mint_seed(dir)?;
    let kp = keypair_from_seed(&seed);
    let fp = fingerprint(&seed);
    let hex: String = kp
        .public
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    println!("device:      {fp}");
    println!("noise pubkey {hex}");
    println!("config dir:  {}", dir.display());
    Ok(())
}

fn list(dir: &std::path::Path) -> Result<()> {
    let store = Store::load(dir)?;
    if store.list().is_empty() {
        println!("(no connectors — run `bastion add dd-enclave --label ... --origin ...` to add one)");
        return Ok(());
    }
    for c in store.list() {
        let cfg = if c.config.is_empty() {
            String::new()
        } else {
            let bits: Vec<String> = c.config.iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!("  [{}]", bits.join(", "))
        };
        println!("{:<36}  {:<12}  {}{}", c.id, kind_str(c.kind), c.label, cfg);
    }
    Ok(())
}

fn add(dir: &std::path::Path, kind: AddKind) -> Result<()> {
    let mut store = Store::load(dir)?;
    let c = match kind {
        AddKind::DdEnclave { label, origin } => Connector::new(ConnectorKind::DdEnclave, label)
            .with_config("origin", serde_json::json!(origin)),
    };
    let id = c.id.clone();
    store.upsert(c);
    store
        .save()
        .with_context(|| "save connectors.json failed")?;
    println!("added {id}");
    Ok(())
}

fn remove(dir: &std::path::Path, id: &str) -> Result<()> {
    let mut store = Store::load(dir)?;
    match store.remove(id) {
        Some(c) => {
            store.save()?;
            println!("removed {} ({})", c.id, c.label);
            Ok(())
        }
        None => Err(anyhow!("no connector with id {id}")),
    }
}

async fn pair(dir: &std::path::Path, cp_url: &str, label: Option<String>) -> Result<()> {
    let seed = load_or_mint_seed(dir)?;
    let kp = keypair_from_seed(&seed);
    let pubkey_hex: String = kp
        .public
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let label = label.unwrap_or_else(|| format!("bastion-{}", fingerprint(&seed)));

    // Tolerate bare-host input ("app.devopsdefender.com"). DD CPs
    // only serve TLS, so an unscheme'd input means https.
    let cp_url = bastion_core::attest::normalize_origin(cp_url);
    let url = format!(
        "{}/admin/enroll?pubkey={}&label={}",
        cp_url.trim_end_matches('/'),
        pubkey_hex,
        urlencode(&label),
    );

    println!("pair: open this URL in your browser and click Confirm.");
    println!("      the page is behind CF Access, so log in if prompted.");
    println!();
    println!("  {url}");
    println!();
    println!("device pubkey: {pubkey_hex}");
    println!("label:         {label}");
    println!();
    println!("next: once the browser says \"Enrolled ✓\",");
    println!("      bastion add dd-enclave --label <name> --origin <cp-or-enclave>");
    println!("      bastion connect <id>");
    Ok(())
}

/// Minimal query-string encoder — escapes everything that isn't
/// unreserved per RFC 3986. Pulled in so we don't add a dep for one
/// call site.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn connect(
    dir: &std::path::Path,
    id: &str,
    method: &str,
    deployment: Option<&str>,
    tail: u32,
    cmd: Option<&str>,
    timeout_secs: Option<u32>,
) -> Result<()> {
    let mut store = Store::load(dir)?;
    let conn = store
        .list()
        .iter()
        .find(|c| c.id == id)
        .cloned()
        .ok_or_else(|| anyhow!("no connector with id {id}"))?;

    if conn.kind != ConnectorKind::DdEnclave {
        bail!(
            "connect only supports dd-enclave connectors today (got {:?})",
            conn.kind
        );
    }
    let origin = conn
        .config
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("connector {id} missing `origin` config"))?
        .to_string();

    // Fetch attest from the enclave + reconcile with any pinned
    // pubkey on the connector (TOFU — ITA verification is a follow-up).
    let attestation = fetch_attest(&origin).await.context("fetch /attest")?;

    let (attestation, mut conn) = tofu_pin(conn, attestation)?;

    println!(
        "→ {origin}  (noise pubkey {})",
        &attestation.pubkey_hex[..16]
    );

    let seed = load_or_mint_seed(dir)?;
    let kp = keypair_from_seed(&seed);
    let enclave_pubkey = bastion_core::attest::decode_pubkey(&attestation.pubkey_hex)?;

    let mut session = NoiseClient::connect(&origin, &kp, &enclave_pubkey)
        .await
        .context("noise handshake")?;
    println!("✓ noise handshake complete");

    let mut ee = EeClient::new(&mut session);
    let value = match method {
        "health" => ee.health().await?,
        "list" => ee.list().await?,
        "logs" => {
            let dep =
                deployment.ok_or_else(|| anyhow!("--deployment required for `--method logs`"))?;
            ee.logs(dep, tail).await?
        }
        "exec" => {
            let raw = cmd.ok_or_else(|| anyhow!("--cmd required for `--method exec`"))?;
            // Minimal shell-style split so `--cmd "uname -a"` maps
            // to argv `["uname", "-a"]`. No quoting/escaping logic
            // beyond whitespace; callers pass `--cmd "sh -c 'ls | wc -l'"`
            // when they need shell semantics.
            let argv: Vec<&str> = raw.split_whitespace().collect();
            if argv.is_empty() {
                bail!("--cmd is empty");
            }
            ee.exec(&argv, timeout_secs).await?
        }
        other => bail!("unknown method `{other}` (try health / list / logs / exec)"),
    };
    println!("{}", serde_json::to_string_pretty(&value)?);

    session.close().await?;

    // Persist the TOFU pin (and any verified=true state from a future
    // ITA-enabled client) back to disk so the next connect reuses it.
    let attest_json = serde_json::to_value(&attestation)?;
    conn.config.insert("attestation".into(), attest_json);
    store.upsert(conn);
    store.save()?;
    Ok(())
}

async fn attach(dir: &std::path::Path, id: &str, cmd: &str) -> Result<()> {
    let mut store = Store::load(dir)?;
    let conn = store
        .list()
        .iter()
        .find(|c| c.id == id)
        .cloned()
        .ok_or_else(|| anyhow!("no connector with id {id}"))?;
    if conn.kind != ConnectorKind::DdEnclave {
        bail!(
            "attach only supports dd-enclave connectors (got {:?})",
            conn.kind
        );
    }
    let origin = conn
        .config
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("connector {id} missing `origin` config"))?
        .to_string();

    let attestation = fetch_attest(&origin).await.context("fetch /attest")?;
    let (attestation, mut conn) = tofu_pin(conn, attestation)?;
    eprintln!(
        "→ {origin}  (noise pubkey {})",
        &attestation.pubkey_hex[..16]
    );

    let seed = load_or_mint_seed(dir)?;
    let kp = keypair_from_seed(&seed);
    let enclave_pubkey = bastion_core::attest::decode_pubkey(&attestation.pubkey_hex)?;

    let session = NoiseClient::connect(&origin, &kp, &enclave_pubkey)
        .await
        .context("noise handshake")?;
    eprintln!("✓ noise handshake complete");

    let argv: Vec<&str> = cmd.split_whitespace().collect();
    if argv.is_empty() {
        bail!("--cmd is empty");
    }
    let (ack, attach_session) = ee_attach(session, &argv).await.context("attach")?;
    eprintln!("✓ attached ({})", ack);

    // Persist the pin before we drop into the bridge so a Ctrl-C
    // mid-session still leaves the connector up-to-date.
    let attest_json = serde_json::to_value(&attestation)?;
    conn.config.insert("attestation".into(), attest_json);
    store.upsert(conn);
    store.save()?;

    attach_session
        .bridge(tokio::io::stdin(), tokio::io::stdout())
        .await
        .context("attach bridge")
}

/// On first connect, pin the enclave's pubkey to the connector. On
/// subsequent connects, refuse if the pubkey moved — an unscheduled
/// pubkey rotation (enclave restart) requires explicit re-pair.
fn tofu_pin(conn: Connector, fresh: Attestation) -> Result<(Attestation, Connector)> {
    if let Some(existing_val) = conn.config.get("attestation") {
        let existing: Attestation = serde_json::from_value(existing_val.clone())
            .context("pinned attestation is not the expected shape")?;
        if existing.pubkey_hex != fresh.pubkey_hex {
            bail!(
                "attestation pubkey for {} changed ({} -> {}). Re-pair with `bastion rm {}` \
                 + `bastion add dd-enclave ...` if this is expected.",
                conn.label,
                &existing.pubkey_hex[..16],
                &fresh.pubkey_hex[..16],
                conn.id,
            );
        }
        // Same pubkey — keep the older `fetched_at_ms` and `verified`
        // flag so we don't silently regress a previously-verified pin
        // to TOFU.
        return Ok((existing, conn));
    }
    println!(
        "first connect — pinning enclave pubkey {} (TOFU; ITA verification pending)",
        &fresh.pubkey_hex[..16]
    );
    Ok((fresh, conn))
}

fn kind_str(k: ConnectorKind) -> &'static str {
    match k {
        ConnectorKind::DdEnclave => "dd-enclave",
    }
}

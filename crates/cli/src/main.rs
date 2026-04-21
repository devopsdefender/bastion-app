//! `bastion` — CLI front-end for the unified terminal client.
//!
//! First milestone: prove the core scaffolding works end-to-end.
//!   - `bastion whoami`  — show device identity fingerprint + pubkey
//!   - `bastion list`    — dump the connector store
//!   - `bastion add ssh` — add an SSH host by host/user/port
//!   - `bastion rm <id>` — remove a connector
//!
//! Next milestones (not in this commit):
//!   - `bastion connect <id>`  — open an interactive session. SSH
//!     via ssh2/russh, DD enclave via a port of the TS Noise client,
//!     LocalShell via portable-pty.
//!   - OSC 133 block parser wired into the session output.
//!   - A ratatui TUI that shows the sidebar + current session side
//!     by side, same UX as the desktop app will have.

use anyhow::{anyhow, Context, Result};
use bastion_core::{
    fingerprint, keypair_from_seed, load_or_mint_seed, Connector, ConnectorKind, Store,
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
    Rm {
        id: String,
    },
}

#[derive(Subcommand)]
enum AddKind {
    /// Add an SSH host.
    Ssh {
        /// Display label in the sidebar.
        #[arg(long)]
        label: String,
        /// Hostname or IP.
        #[arg(long)]
        host: String,
        /// SSH user.
        #[arg(long, default_value = "root")]
        user: String,
        /// SSH port.
        #[arg(long, default_value_t = 22)]
        port: u16,
    },
    /// Add a DD enclave.
    DdEnclave {
        #[arg(long)]
        label: String,
        /// HTTPS origin of the enclave's bastion surface, e.g.
        /// `https://block.pr-42.devopsdefender.com`.
        #[arg(long)]
        origin: String,
    },
    /// Add an Anthropic API key. The key itself is read from the
    /// env var $ANTHROPIC_API_KEY so it never lands in shell history.
    Anthropic {
        #[arg(long)]
        label: String,
    },
}

fn main() -> Result<()> {
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
        println!("(no connectors — run `bastion add ssh --label ... --host ...` to add one)");
        return Ok(());
    }
    for c in store.list() {
        let cfg = if c.config.is_empty() {
            String::new()
        } else {
            let bits: Vec<String> = c
                .config
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            format!("  [{}]", bits.join(", "))
        };
        println!("{:<36}  {:<12}  {}{}", c.id, kind_str(c.kind), c.label, cfg);
    }
    Ok(())
}

fn add(dir: &std::path::Path, kind: AddKind) -> Result<()> {
    let mut store = Store::load(dir)?;
    let c = match kind {
        AddKind::Ssh {
            label,
            host,
            user,
            port,
        } => Connector::new(ConnectorKind::SshHost, label)
            .with_config("host", serde_json::json!(host))
            .with_config("user", serde_json::json!(user))
            .with_config("port", serde_json::json!(port)),
        AddKind::DdEnclave { label, origin } => {
            Connector::new(ConnectorKind::DdEnclave, label)
                .with_config("origin", serde_json::json!(origin))
        }
        AddKind::Anthropic { label } => {
            let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
                anyhow!("set ANTHROPIC_API_KEY in env before `bastion add anthropic`")
            })?;
            // TODO(phase-2): encrypt the key at rest using the identity
            // keypair — storing raw is a placeholder for v0.
            Connector::new(ConnectorKind::Anthropic, label)
                .with_config("api_key_plaintext", serde_json::json!(key))
        }
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

fn kind_str(k: ConnectorKind) -> &'static str {
    match k {
        ConnectorKind::DdEnclave => "dd-enclave",
        ConnectorKind::SshHost => "ssh-host",
        ConnectorKind::Anthropic => "anthropic",
        ConnectorKind::Github => "github",
        ConnectorKind::LocalShell => "local-shell",
    }
}

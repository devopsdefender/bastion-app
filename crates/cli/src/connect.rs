//! `bastion connect <id>` — open an interactive session on the
//! connector identified by `<id>`. Today: DD-enclave connectors
//! only (via Noise_IK over `/noise/shell/{sid}`). SSH + LocalShell
//! land in follow-up commits.
//!
//! Terminal is held in raw mode for the duration; Ctrl-C passes
//! through to the remote shell instead of killing the client
//! (ordinary signal-forwarding via tty). To exit, close the remote
//! session (`exit`/`logout`) or press the escape sequence `~.`
//! (standard SSH-inspired local escape).

use std::io::{IsTerminal, Write};

use anyhow::{anyhow, Context, Result};
use bastion_core::{
    dd_enclave::{self, ShellEvent, ShellTunnel},
    identity::keypair_from_seed,
    load_or_mint_seed, Connector, ConnectorKind, Store,
};
#[allow(unused_imports)]
use bastion_core::dd_enclave::{ShellReceiver, ShellSender};
use crossterm::terminal;
use tokio::io::AsyncReadExt;

/// Entry point from `Cmd::Connect`. Dispatches on connector kind.
pub async fn run(config_dir: &std::path::Path, id: &str) -> Result<()> {
    let store = Store::load(config_dir)?;
    let connector = store
        .get(id)
        .ok_or_else(|| anyhow!("no connector with id {id}"))?
        .clone();

    match connector.kind {
        ConnectorKind::DdEnclave => connect_dd_enclave(config_dir, &connector).await,
        ConnectorKind::SshHost => Err(anyhow!(
            "SSH connect not implemented yet (milestone 2). id={}",
            connector.id
        )),
        ConnectorKind::LocalShell => Err(anyhow!(
            "LocalShell connect not implemented yet (milestone 3). id={}",
            connector.id
        )),
        ConnectorKind::Anthropic | ConnectorKind::Github => Err(anyhow!(
            "connect is terminal-kinds only; {:?} routes through a different frontend",
            connector.kind
        )),
    }
}

async fn connect_dd_enclave(config_dir: &std::path::Path, c: &Connector) -> Result<()> {
    let origin = c
        .config
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("dd-enclave connector missing `origin`"))?;

    let seed = load_or_mint_seed(config_dir)?;
    let client = keypair_from_seed(&seed);
    let client_secret = client.secret.to_bytes();

    eprintln!("bastion: fetching /attest from {origin}");
    let attest = dd_enclave::fetch_attest(origin)
        .await
        .with_context(|| format!("attest {origin}"))?;
    let server_pubkey = dd_enclave::decode_pubkey_hex(&attest.noise_pubkey_hex)?;
    eprintln!(
        "bastion: pinned server pubkey {}… (source: {})",
        &attest.noise_pubkey_hex[..16],
        attest.source.as_deref().unwrap_or("?")
    );

    // For MVP: mint a fresh session on every connect. Reconnecting
    // to a specific session id lands in the next milestone (will
    // need `bastion sessions list <connector>`).
    eprintln!("bastion: creating shell session");
    let session = dd_enclave::create_session(origin, &client_secret, &server_pubkey, "bastion-cli")
        .await
        .with_context(|| "sessions_create")?;
    eprintln!("bastion: session {} ({})", session.id, session.title);

    eprintln!("bastion: opening /noise/shell/{}", session.id);
    let tunnel =
        ShellTunnel::open(origin, &session.id, &client_secret, &server_pubkey).await?;
    let (mut tx, mut rx) = tunnel.split();

    // Terminal setup: raw mode + initial resize. Restore on drop —
    // panic or normal exit path both leave the terminal sane.
    let _guard = RawGuard::enter()?;
    let (cols, rows) = terminal::size().unwrap_or((120, 32));
    tx.send_ctrl(&serde_json::json!({
        "type": "resize",
        "cols": cols,
        "rows": rows,
    }))
    .await?;
    // `have_up_to: -1` = "I have no prior history for this session;
    // send me whatever you've got from the replay ring." M2 can
    // track seq per-session and request gap-filling.
    tx.send_ctrl(&serde_json::json!({
        "type": "hello",
        "have_up_to": -1,
    }))
    .await?;

    let stdin_to_tunnel = async move {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 4096];
        loop {
            match stdin.read(&mut buf).await {
                Ok(0) => break Ok::<(), anyhow::Error>(()),
                Ok(n) => {
                    if let Err(e) = tx.send_raw(&buf[..n]).await {
                        break Err(e);
                    }
                }
                Err(e) => break Err(anyhow!("stdin: {e}")),
            }
        }
    };

    let tunnel_to_stdout = async move {
        loop {
            match rx.recv().await {
                Ok(ShellEvent::Raw(bytes)) => {
                    let mut stdout = std::io::stdout();
                    stdout.write_all(&bytes)?;
                    stdout.flush()?;
                }
                Ok(ShellEvent::Ctrl(v)) => {
                    let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("?");
                    match ty {
                        "exit" => {
                            let code = v.get("code").and_then(|x| x.as_i64()).unwrap_or(-1);
                            eprintln!("\r\n[session exited, code={code}]");
                            return Ok::<(), anyhow::Error>(());
                        }
                        "error" => {
                            let code = v.get("code").and_then(|x| x.as_str()).unwrap_or("?");
                            eprintln!("\r\n[error: {code}]");
                        }
                        // block/ready/gap: M2 hooks for OSC 133 rendering.
                        _ => {}
                    }
                }
                Ok(ShellEvent::Close) => {
                    eprintln!("\r\n[tunnel closed]");
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("\r\n[tunnel error: {e}]");
                    return Err(e);
                }
            }
        }
    };

    // Exit when either half stops — stdin EOF or remote exit.
    tokio::select! {
        r = stdin_to_tunnel => r?,
        r = tunnel_to_stdout => r?,
    }
    Ok(())
}

/// RAII guard that enters raw mode and restores cooked mode on drop.
struct RawGuard;

impl RawGuard {
    fn enter() -> Result<Self> {
        if std::io::stdin().is_terminal() {
            terminal::enable_raw_mode()?;
        }
        Ok(Self)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        if std::io::stdin().is_terminal() {
            let _ = terminal::disable_raw_mode();
        }
    }
}

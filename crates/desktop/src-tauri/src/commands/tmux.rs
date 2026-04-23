//! tmux-backed session lifecycle.
//!
//! Each DD agent has its own local tmux server. Bastion treats those
//! tmux sessions as the primary "session" unit:
//!
//! - `tmux_list_sessions(agent_origin)` — one Noise roundtrip per
//!   call, runs `tmux list-sessions` + `tmux list-panes -a` via the
//!   `exec` EE method. Returns everything running on that agent,
//!   *including* sessions not created by bastion.
//! - `tmux_new_session` / `tmux_attach` — open an `attach` EE call
//!   whose argv is `tmux new-session -A -s <name> ...` (idempotent
//!   create-or-resume) or `tmux attach -t <name>` respectively. The
//!   AttachSession is driven by a pump task that (a) taps output
//!   bytes into the SQLite FTS log, (b) base64-emits them to the
//!   webview as `session:<id>:data`, (c) feeds frontend keystrokes
//!   into the enclave side via an mpsc.
//! - `tmux_write` / `tmux_detach` / `tmux_kill_session` — the minimal
//!   operational surface. Resize is a no-op in v0 (`attach` envelope
//!   doesn't carry cols/rows; tmux sees whatever the remote PTY
//!   started with).

use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use bastion_core::{
    ee_attach, fetch_attest, keypair_from_seed, load_or_mint_seed, AttachSession, EeClient,
    NoiseClient,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::state::{AppState, AttachHandle};

// --- Types -----------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TmuxSessionInfo {
    pub name: String,
    pub activity_ts: i64,
    pub attached_clients: i64,
    pub foreground_cmd: Option<String>,
    /// Heuristic: "shell" if foreground command looks shell-ish,
    /// "codex" if it looks like an AI agent binary, else None.
    pub kind: Option<String>,
}

fn classify_kind(foreground_cmd: Option<&str>) -> Option<String> {
    let cmd = foreground_cmd?;
    // Strip any path prefix & args.
    let name = cmd.rsplit('/').next()?.split_whitespace().next()?;
    match name {
        "claude" | "codex" | "aider" | "cursor-agent" => Some("codex".into()),
        "bash" | "sh" | "zsh" | "fish" | "ash" | "ksh" | "dash" => Some("shell".into()),
        _ => None,
    }
}

// --- Shared helpers --------------------------------------------------------

async fn open_noise(
    config_dir: &std::path::Path,
    agent_origin: &str,
) -> Result<NoiseClient, String> {
    let origin = bastion_core::attest::normalize_origin(agent_origin);
    let attestation = fetch_attest(&origin)
        .await
        .map_err(|e| format!("fetch /attest: {e}"))?;
    let seed = load_or_mint_seed(config_dir).map_err(|e| e.to_string())?;
    let kp = keypair_from_seed(&seed);
    let enclave_pubkey = bastion_core::attest::decode_pubkey(&attestation.pubkey_hex)
        .map_err(|e| format!("decode pubkey: {e}"))?;
    NoiseClient::connect(&origin, &kp, &enclave_pubkey)
        .await
        .map_err(|e| format!("noise handshake: {e}"))
}

fn parse_exec_stdout(val: &serde_json::Value) -> String {
    val.get("stdout")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

// --- Commands --------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListSessionsArgs {
    pub agent_origin: String,
}

#[tauri::command]
pub async fn tmux_list_sessions(
    state: State<'_, AppState>,
    args: ListSessionsArgs,
) -> Result<Vec<TmuxSessionInfo>, String> {
    let mut session = open_noise(&state.config_dir, &args.agent_origin).await?;
    let mut ee = EeClient::new(&mut session);

    let ls = ee
        .exec(
            &[
                "tmux",
                "list-sessions",
                "-F",
                "#{session_name}\t#{session_activity}\t#{session_attached}",
            ],
            Some(5),
        )
        .await
        .map_err(|e| format!("exec tmux ls: {e}"))?;

    let panes = ee
        .exec(
            &[
                "tmux",
                "list-panes",
                "-a",
                "-F",
                "#{session_name}\t#{pane_current_command}",
            ],
            Some(5),
        )
        .await
        .map_err(|e| format!("exec tmux list-panes: {e}"))?;

    // An agent with zero tmux sessions returns a non-zero exit. Treat
    // that as "empty list" rather than propagating the error.
    let ls_text = parse_exec_stdout(&ls);
    let panes_text = parse_exec_stdout(&panes);

    let mut cmd_by_session: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for line in panes_text.lines() {
        let mut it = line.splitn(2, '\t');
        let (Some(name), Some(cmd)) = (it.next(), it.next()) else {
            continue;
        };
        // Only record the first (top-most) pane we see per session.
        cmd_by_session
            .entry(name.to_string())
            .or_insert_with(|| cmd.to_string());
    }

    let mut out = Vec::new();
    for line in ls_text.lines() {
        let mut it = line.split('\t');
        let name = it.next().unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let activity: i64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let attached: i64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let fg = cmd_by_session.get(&name).cloned();
        let kind = classify_kind(fg.as_deref());
        out.push(TmuxSessionInfo {
            name,
            activity_ts: activity,
            attached_clients: attached,
            foreground_cmd: fg,
            kind,
        });
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct NewSessionArgs {
    pub agent_origin: String,
    pub name: String,
    /// "shell" or "codex" — drives the inner argv inside tmux.
    pub kind: String,
    /// Optional override for the inner command (skips the kind
    /// lookup). For future customization; v0 callers just pass null.
    #[serde(default)]
    pub inner_cmd: Option<Vec<String>>,
}

fn inner_for_kind(kind: &str, inner_cmd: Option<Vec<String>>) -> Vec<String> {
    if let Some(cmd) = inner_cmd {
        return cmd;
    }
    match kind {
        "codex" => vec!["claude".into()],
        _ => vec![], // empty: tmux uses its default-shell
    }
}

#[derive(Serialize)]
pub struct AttachResult {
    pub bastion_session_id: String,
    pub ack: serde_json::Value,
}

#[tauri::command]
pub async fn tmux_new_session(
    state: State<'_, AppState>,
    app: AppHandle,
    args: NewSessionArgs,
) -> Result<AttachResult, String> {
    let inner = inner_for_kind(&args.kind, args.inner_cmd);
    let mut argv: Vec<String> = vec![
        "tmux".into(),
        "new-session".into(),
        "-A".into(),
        "-s".into(),
        args.name.clone(),
    ];
    argv.extend(inner);
    spawn_attach(
        state,
        app,
        args.agent_origin,
        args.name,
        Some(args.kind),
        argv,
    )
    .await
}

#[derive(Deserialize)]
pub struct AttachArgs {
    pub agent_origin: String,
    pub name: String,
    /// Optional pre-classified kind. Helps bookkeeping when the caller
    /// already knows (from `tmux_list_sessions`).
    #[serde(default)]
    pub kind: Option<String>,
}

#[tauri::command]
pub async fn tmux_attach(
    state: State<'_, AppState>,
    app: AppHandle,
    args: AttachArgs,
) -> Result<AttachResult, String> {
    let argv: Vec<String> = vec![
        "tmux".into(),
        "attach".into(),
        "-t".into(),
        args.name.clone(),
    ];
    spawn_attach(state, app, args.agent_origin, args.name, args.kind, argv).await
}

async fn spawn_attach(
    state: State<'_, AppState>,
    app: AppHandle,
    agent_origin: String,
    tmux_name: String,
    kind: Option<String>,
    argv: Vec<String>,
) -> Result<AttachResult, String> {
    let session = open_noise(&state.config_dir, &agent_origin).await?;

    let argv_refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
    let (ack, attach_session) = ee_attach(session, &argv_refs)
        .await
        .map_err(|e| format!("ee attach: {e}"))?;

    // Generate a bastion-local UUID for the session. This is separate
    // from the tmux session name: it scopes the pump, the FTS rows,
    // and the webview event names.
    let bastion_session_id = uuid::Uuid::new_v4().as_simple().to_string();
    state
        .log
        .register_session(
            bastion_session_id.clone(),
            agent_origin.clone(),
            tmux_name.clone(),
            kind.clone(),
        )
        .await
        .map_err(|e| format!("register in log: {e}"))?;

    let (tx_bytes, rx_bytes) = mpsc::unbounded_channel::<Vec<u8>>();

    let abort = spawn_pump(
        app,
        attach_session,
        bastion_session_id.clone(),
        state.log.clone(),
        rx_bytes,
    );

    let handle = AttachHandle {
        bastion_session_id: bastion_session_id.clone(),
        agent_origin,
        tmux_name,
        kind,
        tx_bytes,
        abort,
    };
    state.attaches.lock().await.insert(bastion_session_id.clone(), handle);

    Ok(AttachResult {
        bastion_session_id,
        ack,
    })
}

/// Drives the AttachSession's byte pump. Output bytes are (a) logged
/// into FTS and (b) emitted to the webview. Input bytes come in on
/// an mpsc. The returned AbortHandle kills the bridge task; on bridge
/// exit the pump exits on its own.
fn spawn_pump(
    app: AppHandle,
    attach: AttachSession,
    bastion_session_id: String,
    log: Arc<crate::session_log::SessionLog>,
    mut rx_bytes: mpsc::UnboundedReceiver<Vec<u8>>,
) -> tokio::task::AbortHandle {
    let event_data = format!("session:{}:data", bastion_session_id);
    let event_exit = format!("session:{}:exit", bastion_session_id);

    // Pipes bridging the frontend <-> the Noise AttachSession.
    //   user_write_end       ↦ bridge_read_end      (user input → enclave stdin)
    //   bridge_write_end     ↦ user_read_end        (enclave stdout → user tap)
    let (mut user_write_end, bridge_read_end) = tokio::io::duplex(128 * 1024);
    let (bridge_write_end, mut user_read_end) = tokio::io::duplex(128 * 1024);

    // Bridge task: owns the AttachSession.
    let bridge_task = tokio::spawn(async move {
        let _ = attach.bridge(bridge_read_end, bridge_write_end).await;
    });
    let abort = bridge_task.abort_handle();

    // Reader task: tap output, log, emit.
    let app_r = app.clone();
    let log_r = log.clone();
    let sid_r = bastion_session_id.clone();
    let event_data_r = event_data.clone();
    let reader_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match user_read_end.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = buf[..n].to_vec();
                    // Log asynchronously; we don't block emit on it.
                    let log_sid = sid_r.clone();
                    let log_bytes = chunk.clone();
                    let log2 = log_r.clone();
                    tokio::spawn(async move {
                        let _ = log2.append_bytes(log_sid, log_bytes).await;
                    });
                    let b64 = B64.encode(&chunk);
                    let _ = app_r.emit(&event_data_r, serde_json::json!({ "bytes_b64": b64 }));
                }
            }
        }
    });

    // Writer task: pull keystrokes off mpsc, feed into the bridge.
    let writer_task = tokio::spawn(async move {
        while let Some(bytes) = rx_bytes.recv().await {
            if user_write_end.write_all(&bytes).await.is_err() {
                break;
            }
        }
    });

    // Supervisor: when the bridge ends, tear down the rest and signal
    // exit to the webview.
    let app_s = app;
    let event_exit_s = event_exit;
    tokio::spawn(async move {
        let _ = bridge_task.await;
        reader_task.abort();
        writer_task.abort();
        let _ = app_s.emit(&event_exit_s, serde_json::json!({ "code": null }));
    });

    abort
}

#[derive(Deserialize)]
pub struct WriteArgs {
    pub bastion_session_id: String,
    pub bytes_b64: String,
}

#[tauri::command]
pub async fn tmux_write(state: State<'_, AppState>, args: WriteArgs) -> Result<(), String> {
    let bytes = B64
        .decode(&args.bytes_b64)
        .map_err(|e| format!("b64: {e}"))?;
    let attaches = state.attaches.lock().await;
    let h = attaches
        .get(&args.bastion_session_id)
        .ok_or_else(|| format!("no attach {}", args.bastion_session_id))?;
    h.tx_bytes
        .send(bytes)
        .map_err(|_| "attach writer closed".to_string())
}

#[derive(Deserialize)]
#[allow(dead_code)] // v0 no-op; frontend still sends these
pub struct ResizeArgs {
    pub bastion_session_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[tauri::command]
pub async fn tmux_resize(_state: State<'_, AppState>, _args: ResizeArgs) -> Result<(), String> {
    // v0: remote PTY size was fixed at attach time; no wire support
    // for resize notifications yet. The frontend calls this harmlessly
    // on every resize so the wiring is in place for when the protocol
    // grows a resize message.
    Ok(())
}

#[derive(Deserialize)]
pub struct DetachArgs {
    pub bastion_session_id: String,
}

#[tauri::command]
pub async fn tmux_detach(state: State<'_, AppState>, args: DetachArgs) -> Result<(), String> {
    if let Some(h) = state.attaches.lock().await.remove(&args.bastion_session_id) {
        h.abort.abort();
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct KillArgs {
    pub agent_origin: String,
    pub name: String,
}

#[tauri::command]
pub async fn tmux_kill_session(
    state: State<'_, AppState>,
    args: KillArgs,
) -> Result<(), String> {
    let mut session = open_noise(&state.config_dir, &args.agent_origin).await?;
    let mut ee = EeClient::new(&mut session);
    ee.exec(&["tmux", "kill-session", "-t", &args.name], Some(5))
        .await
        .map_err(|e| format!("kill-session: {e}"))?;
    Ok(())
}

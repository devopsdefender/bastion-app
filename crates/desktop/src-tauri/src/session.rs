//! In-process session manager for the desktop app.
//!
//! Tauri commands can't model a bidirectional byte stream directly —
//! commands are request/response and events are one-way emit. So we
//! keep a `HashMap<handle, ShellSender + abort_handle>` behind a
//! `Mutex` and emit output as `session:<handle>:output` events from a
//! spawned task that owns the `ShellReceiver`.

use std::collections::HashMap;
use std::sync::Arc;

use bastion_core::{
    dd_enclave::{self, ShellSender, ShellTunnel},
    identity::keypair_from_seed,
    load_or_mint_seed, Connector, ConnectorKind, Store,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

/// Per-session handle stored in [`Manager`]. `abort` kills the
/// output-pumping task when the session closes; `sender` is the
/// write half of the Noise tunnel.
struct Session {
    sender: ShellSender,
    abort: tokio::task::AbortHandle,
}

#[derive(Default)]
pub struct Manager {
    inner: Arc<Mutex<HashMap<String, Session>>>,
}

#[derive(Serialize)]
pub struct SessionOpened {
    pub handle: String,
    pub remote_session_id: String,
    pub title: String,
}

impl Manager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a shell against the connector identified by `connector_id`.
    /// Returns a per-open handle the frontend uses as an event-prefix
    /// subscription key. The handle ≠ the remote session id so we can
    /// host multiple windows backed by the same remote session later.
    pub async fn open(
        &self,
        app: AppHandle,
        config_dir: std::path::PathBuf,
        connector_id: String,
    ) -> Result<SessionOpened, String> {
        let store = Store::load(&config_dir).map_err(|e| e.to_string())?;
        let connector: Connector = store
            .get(&connector_id)
            .cloned()
            .ok_or_else(|| format!("no connector with id {connector_id}"))?;
        if !matches!(connector.kind, ConnectorKind::DdEnclave) {
            return Err(format!(
                "session open: unsupported kind {:?} (dd-enclave only today)",
                connector.kind
            ));
        }
        let origin = connector
            .config
            .get("origin")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "connector missing `origin`".to_string())?
            .to_string();

        let seed = load_or_mint_seed(&config_dir).map_err(|e| e.to_string())?;
        let kp = keypair_from_seed(&seed);
        let client_secret = kp.secret.to_bytes();

        let attest = dd_enclave::fetch_attest(&origin)
            .await
            .map_err(|e| format!("attest: {e}"))?;
        let server_pubkey = dd_enclave::decode_pubkey_hex(&attest.noise_pubkey_hex)
            .map_err(|e| e.to_string())?;

        let remote = dd_enclave::create_session(&origin, &client_secret, &server_pubkey, "bastion-desktop")
            .await
            .map_err(|e| format!("sessions_create: {e}"))?;
        let tunnel = ShellTunnel::open(&origin, &remote.id, &client_secret, &server_pubkey)
            .await
            .map_err(|e| format!("shell open: {e}"))?;
        let (sender, mut receiver) = tunnel.split();

        let handle = new_handle();

        // Output pump — emits `session:<handle>:output` for raw bytes
        // and `session:<handle>:ctrl` for JSON control frames. Runs
        // until the peer closes or the task is aborted.
        let out_app = app.clone();
        let out_handle = handle.clone();
        let task = tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(dd_enclave::ShellEvent::Raw(bytes)) => {
                        let _ = out_app
                            .emit(&format!("session:{out_handle}:output"), bytes);
                    }
                    Ok(dd_enclave::ShellEvent::Ctrl(v)) => {
                        let _ = out_app
                            .emit(&format!("session:{out_handle}:ctrl"), v);
                    }
                    Ok(dd_enclave::ShellEvent::Close) | Err(_) => {
                        let _ = out_app
                            .emit(&format!("session:{out_handle}:close"), ());
                        break;
                    }
                }
            }
        });
        let abort = task.abort_handle();

        let mut g = self.inner.lock().await;
        g.insert(
            handle.clone(),
            Session {
                sender,
                abort,
            },
        );
        Ok(SessionOpened {
            handle,
            remote_session_id: remote.id,
            title: remote.title,
        })
    }

    pub async fn input(&self, handle: &str, bytes: Vec<u8>) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        let s = g
            .get_mut(handle)
            .ok_or_else(|| format!("unknown session handle {handle}"))?;
        s.sender
            .send_raw(&bytes)
            .await
            .map_err(|e| format!("send_raw: {e}"))
    }

    pub async fn resize(&self, handle: &str, cols: u16, rows: u16) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        let s = g
            .get_mut(handle)
            .ok_or_else(|| format!("unknown session handle {handle}"))?;
        s.sender
            .send_ctrl(&serde_json::json!({"type":"resize","cols":cols,"rows":rows}))
            .await
            .map_err(|e| format!("send_ctrl resize: {e}"))
    }

    pub async fn hello(&self, handle: &str, have_up_to: i64) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        let s = g
            .get_mut(handle)
            .ok_or_else(|| format!("unknown session handle {handle}"))?;
        s.sender
            .send_ctrl(&serde_json::json!({"type":"hello","have_up_to":have_up_to}))
            .await
            .map_err(|e| format!("send_ctrl hello: {e}"))
    }

    pub async fn close(&self, handle: &str) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        if let Some(s) = g.remove(handle) {
            s.abort.abort();
        }
        Ok(())
    }
}

fn new_handle() -> String {
    use rand::RngCore;
    let mut b = [0u8; 8];
    rand::rngs::OsRng.fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

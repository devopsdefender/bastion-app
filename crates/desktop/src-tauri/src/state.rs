//! Shared app state. Passed into Tauri commands via `State<AppState>`.
//!
//! - `config_dir` — where `bastion-core::Store` persists connectors +
//!   the identity seed. Same dir the CLI uses, by default.
//! - `attaches` — live tmux-attached sessions. Each one owns a pump
//!   task pulling bytes off the enclave's AttachSession, a writer
//!   channel into which the frontend pipes keystrokes, and an abort
//!   handle so `tmux_detach` can tear it down cleanly.
//! - `log` — SQLite-backed per-device FTS5 index of session text.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use crate::session_log::SessionLog;

/// Live attach-session handle. The pump task is the authoritative
/// owner of the underlying `AttachSession`; the handle exposes a
/// writer channel and an abort knob.
#[allow(dead_code)]
pub struct AttachHandle {
    // Reserved for future endpoints that surface live attaches in the
    // sidebar ("attached here / elsewhere / detached" badges).
    pub bastion_session_id: String,
    pub agent_origin: String,
    pub tmux_name: String,
    pub kind: Option<String>,
    /// Bytes from the frontend (keystrokes) go here.
    pub tx_bytes: mpsc::UnboundedSender<Vec<u8>>,
    /// Aborts the pump task. Used by `tmux_detach`.
    pub abort: tokio::task::AbortHandle,
}

pub type AttachMap = HashMap<String, AttachHandle>;

#[derive(Clone)]
pub struct AppState {
    pub config_dir: PathBuf,
    pub attaches: Arc<Mutex<AttachMap>>,
    pub log: Arc<SessionLog>,
}

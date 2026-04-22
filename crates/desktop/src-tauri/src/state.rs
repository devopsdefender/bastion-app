//! Shared app state. Passed into Tauri commands via `State<AppState>`.
//!
//! - `config_dir` — where `bastion-core::Store` persists connectors +
//!   the identity seed. Same dir the CLI uses, by default.
//! - `sessions` — live `NoiseClient`s keyed by a server-generated
//!   `SessionId`. A connector can have multiple open sessions; each
//!   is an independent Noise handshake against the same enclave.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bastion_core::NoiseClient;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

pub type SessionMap = HashMap<SessionId, Arc<Mutex<NoiseClient>>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        // v4 UUID, no dashes — compact for logs.
        Self(uuid::Uuid::new_v4().as_simple().to_string())
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config_dir: PathBuf,
    pub sessions: Arc<Mutex<SessionMap>>,
}

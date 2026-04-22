//! Fleet-level queries on top of a `dd-enclave` connector.
//!
//! - `fetch_agents(connector_id)` — `GET {origin}/api/agents` over
//!   HTTPS. The CP's `/api/agents` is CF-Access-bypassed and gated
//!   in-code by a three-way policy (loopback / GH-OIDC / ITA). A
//!   request that goes via the CF tunnel arrives at the CP's axum
//!   listener on loopback, which is enough for the handler to
//!   answer. Clients that don't have that path need to mint an ITA
//!   — out of scope for the desktop today.
//!
//! This endpoint returns the full agent catalog, including the CP
//! itself (as `agent_id: "control-plane"`). Each entry has its own
//! `hostname` where the Noise gateway + dashboard live; the desktop
//! opens a fresh Noise session per agent on demand.

use bastion_core::Store;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub agent_id: String,
    pub vm_name: String,
    pub hostname: String,
    pub status: String,
    #[serde(default)]
    pub last_seen: Option<String>,
}

#[tauri::command]
pub async fn fetch_agents(
    state: State<'_, AppState>,
    connector_id: String,
) -> Result<Vec<Agent>, String> {
    let store = Store::load(&state.config_dir).map_err(|e| e.to_string())?;
    let conn = store
        .list()
        .iter()
        .find(|c| c.id == connector_id)
        .cloned()
        .ok_or_else(|| format!("no connector with id {connector_id}"))?;

    let origin_raw = conn
        .config
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("connector {connector_id} missing `origin` config"))?;
    let origin = bastion_core::attest::normalize_origin(origin_raw);
    let url = format!("{}/api/agents", origin.trim_end_matches('/'));

    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("GET {url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GET {url} -> {}", resp.status()));
    }
    resp.json::<Vec<Agent>>()
        .await
        .map_err(|e| format!("parse /api/agents: {e}"))
}

//! Noise_IK session lifecycle over Tauri commands.
//!
//! - `connect_start` fetches `/attest`, TOFU-pins the enclave's Noise
//!   pubkey into the connector's `config["attestation"]`, opens a
//!   `NoiseClient`, stashes it in `AppState.sessions` keyed by a
//!   generated `SessionId`.
//! - `connect_send` issues one JSON request on that session and
//!   returns the decrypted response.
//! - `connect_close` drops the session.

use std::sync::Arc;

use bastion_core::{
    fetch_attest, keypair_from_seed, load_or_mint_seed, Attestation, Connector, ConnectorKind,
    NoiseClient, Store,
};
use serde::Deserialize;
use tauri::State;
use tokio::sync::Mutex;

use crate::state::{AppState, SessionId};

#[derive(Deserialize)]
pub struct ConnectStartArgs {
    pub connector_id: String,
}

#[derive(Deserialize)]
pub struct ConnectStartToArgs {
    /// Raw origin (`https://<host>` or bare `<host>`) to open a Noise
    /// session against. Unlike `connect_start`, this doesn't require
    /// a connector and doesn't persist TOFU pins — suitable for the
    /// fleet tree, which opens one session per agent enumerated via
    /// `fetch_agents`.
    pub origin: String,
}

/// Open a Noise_IK session to an arbitrary origin. Fetches `/attest`,
/// handshakes with the device keypair against the returned Noise
/// static pubkey, stashes the session. No persistence — every call
/// re-fetches `/attest`, which is fine for the fleet tree's
/// lazy-open-per-agent pattern.
#[tauri::command]
pub async fn connect_start_to(
    state: State<'_, AppState>,
    args: ConnectStartToArgs,
) -> Result<serde_json::Value, String> {
    let origin = bastion_core::attest::normalize_origin(&args.origin);
    let attestation = fetch_attest(&origin)
        .await
        .map_err(|e| format!("fetch /attest: {e}"))?;
    let seed = load_or_mint_seed(&state.config_dir).map_err(|e| e.to_string())?;
    let kp = keypair_from_seed(&seed);
    let enclave_pubkey = bastion_core::attest::decode_pubkey(&attestation.pubkey_hex)
        .map_err(|e| format!("decode pubkey: {e}"))?;
    let session = NoiseClient::connect(&origin, &kp, &enclave_pubkey)
        .await
        .map_err(|e| format!("noise handshake: {e}"))?;

    let id = SessionId::new();
    state
        .sessions
        .lock()
        .await
        .insert(id.clone(), Arc::new(Mutex::new(session)));

    Ok(serde_json::json!({
        "session_id": id.0,
        "origin": origin,
        "pubkey_hex": attestation.pubkey_hex,
        "verified": attestation.verified,
    }))
}

#[tauri::command]
pub async fn connect_start(
    state: State<'_, AppState>,
    args: ConnectStartArgs,
) -> Result<serde_json::Value, String> {
    let mut store = Store::load(&state.config_dir).map_err(|e| e.to_string())?;
    let conn = store
        .list()
        .iter()
        .find(|c| c.id == args.connector_id)
        .cloned()
        .ok_or_else(|| format!("no connector with id {}", args.connector_id))?;

    if conn.kind != ConnectorKind::DdEnclave {
        return Err(format!(
            "connect is only supported for dd-enclave today (got {:?})",
            conn.kind
        ));
    }
    let origin = conn
        .config
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("connector {} missing `origin` config", conn.id))?
        .to_string();

    // Fetch + TOFU pin.
    let fresh = fetch_attest(&origin)
        .await
        .map_err(|e| format!("fetch /attest: {e}"))?;
    let (attestation, mut conn) = tofu_pin(conn, fresh)?;

    // Open Noise session.
    let seed = load_or_mint_seed(&state.config_dir).map_err(|e| e.to_string())?;
    let kp = keypair_from_seed(&seed);
    let enclave_pubkey = bastion_core::attest::decode_pubkey(&attestation.pubkey_hex)
        .map_err(|e| format!("decode pubkey: {e}"))?;
    let session = NoiseClient::connect(&origin, &kp, &enclave_pubkey)
        .await
        .map_err(|e| format!("noise handshake: {e}"))?;

    // Persist the TOFU pin (or the previously-pinned struct) back to
    // disk so subsequent connects use the same pubkey without a
    // fresh /attest round-trip.
    let attest_json = serde_json::to_value(&attestation).map_err(|e| e.to_string())?;
    conn.config.insert("attestation".into(), attest_json);
    store.upsert(conn);
    store.save().map_err(|e| e.to_string())?;

    // Stash and return the id.
    let id = SessionId::new();
    state
        .sessions
        .lock()
        .await
        .insert(id.clone(), Arc::new(Mutex::new(session)));

    Ok(serde_json::json!({
        "session_id": id.0,
        "origin": origin,
        "pubkey_hex": attestation.pubkey_hex,
        "verified": attestation.verified,
    }))
}

#[derive(Deserialize)]
pub struct ConnectSendArgs {
    pub session_id: String,
    /// Raw EE-method envelope, e.g. `{"method": "health"}` or
    /// `{"method": "logs", "id": "...", "tail": 200}`. The Noise
    /// gateway gates this against its allowlist server-side.
    pub request: serde_json::Value,
}

#[tauri::command]
pub async fn connect_send(
    state: State<'_, AppState>,
    args: ConnectSendArgs,
) -> Result<serde_json::Value, String> {
    let sessions = state.sessions.lock().await;
    let handle = sessions
        .get(&SessionId(args.session_id.clone()))
        .ok_or_else(|| format!("no live session {}", args.session_id))?
        .clone();
    drop(sessions);

    let mut session = handle.lock().await;
    session
        .roundtrip(&args.request)
        .await
        .map_err(|e| format!("roundtrip: {e}"))
}

#[derive(Deserialize)]
pub struct ConnectCloseArgs {
    pub session_id: String,
}

#[tauri::command]
pub async fn connect_close(
    state: State<'_, AppState>,
    args: ConnectCloseArgs,
) -> Result<(), String> {
    let mut sessions = state.sessions.lock().await;
    if let Some(handle) = sessions.remove(&SessionId(args.session_id)) {
        // Drop the only remaining Arc → WebSocket closes on Drop.
        drop(handle);
    }
    Ok(())
}

fn tofu_pin(conn: Connector, fresh: Attestation) -> Result<(Attestation, Connector), String> {
    if let Some(existing_val) = conn.config.get("attestation") {
        let existing: Attestation = serde_json::from_value(existing_val.clone())
            .map_err(|e| format!("pinned attestation shape: {e}"))?;
        if existing.pubkey_hex != fresh.pubkey_hex {
            return Err(format!(
                "attestation pubkey for {} changed ({} -> {}); re-pair to accept",
                conn.label,
                &existing.pubkey_hex[..16],
                &fresh.pubkey_hex[..16],
            ));
        }
        return Ok((existing, conn));
    }
    Ok((fresh, conn))
}

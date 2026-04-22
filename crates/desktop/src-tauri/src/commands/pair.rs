//! `pair <cp-url>` — enroll this device's X25519 pubkey with a DD
//! control plane so its enclaves will accept Noise_IK handshakes
//! from us.

use bastion_core::{fingerprint, keypair_from_seed, load_or_mint_seed};
use serde::Deserialize;
use tauri::State;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct PairArgs {
    pub cp_url: String,
    pub label: Option<String>,
}

#[tauri::command]
pub async fn pair(state: State<'_, AppState>, args: PairArgs) -> Result<serde_json::Value, String> {
    let seed = load_or_mint_seed(&state.config_dir).map_err(|e| e.to_string())?;
    let kp = keypair_from_seed(&seed);
    let pubkey_hex: String = kp
        .public
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let label = args
        .label
        .unwrap_or_else(|| format!("bastion-{}", fingerprint(&seed)));

    let url = format!("{}/api/v1/devices", args.cp_url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({ "pubkey": pubkey_hex, "label": label }))
        .send()
        .await
        .map_err(|e| format!("POST {url}: {e}"))?;
    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .unwrap_or(serde_json::json!({"status_only": true}));
    if !status.is_success() {
        return Err(format!("POST {url} → {status}: {body}"));
    }
    Ok(serde_json::json!({
        "status": status.as_u16(),
        "pubkey": pubkey_hex,
        "label": label,
        "body": body,
    }))
}

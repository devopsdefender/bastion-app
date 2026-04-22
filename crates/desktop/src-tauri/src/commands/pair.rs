//! `pair` — bounce the operator's browser at the CP's
//! `/admin/enroll` page so they can authenticate via CF Access and
//! confirm enrolling this device's X25519 pubkey.
//!
//! Before this landed, `pair` POSTed directly to `/api/v1/devices`
//! from the desktop process — but that route is behind the CP's
//! human CF Access app, so the POST got the CF login HTML back and
//! the enrollment silently didn't happen. `bastion connect` then
//! failed the Noise_IK handshake with `unknown peer`.
//!
//! The new flow is three clicks:
//!   1. Desktop "pair this device" opens the browser.
//!   2. CF Access challenges; operator logs in.
//!   3. Confirm button on `/admin/enroll` fires a same-origin POST
//!      to `/api/v1/devices` (cookie carries) → enrolled.

use bastion_core::{fingerprint, keypair_from_seed, load_or_mint_seed};
use serde::Deserialize;
use tauri::AppHandle;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct PairArgs {
    pub cp_url: String,
    pub label: Option<String>,
}

#[tauri::command]
pub async fn pair(
    app: AppHandle,
    state: State<'_, AppState>,
    args: PairArgs,
) -> Result<serde_json::Value, String> {
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

    let cp_url = bastion_core::attest::normalize_origin(&args.cp_url);
    let url = format!(
        "{}/admin/enroll?pubkey={}&label={}",
        cp_url.trim_end_matches('/'),
        pubkey_hex,
        urlencode(&label),
    );

    app.opener()
        .open_url(&url, None::<String>)
        .map_err(|e| format!("open browser: {e}"))?;

    Ok(serde_json::json!({
        "url": url,
        "cp_url": cp_url,
        "pubkey_hex": pubkey_hex,
        "label": label,
    }))
}

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

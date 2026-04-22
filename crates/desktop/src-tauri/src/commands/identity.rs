use bastion_core::{fingerprint, keypair_from_seed, load_or_mint_seed};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

#[derive(Serialize)]
pub struct Whoami {
    pub fingerprint: String,
    pub pubkey_hex: String,
    pub config_dir: String,
}

#[tauri::command]
pub async fn whoami(state: State<'_, AppState>) -> Result<Whoami, String> {
    let dir = state.config_dir.clone();
    let seed = load_or_mint_seed(&dir).map_err(|e| e.to_string())?;
    let kp = keypair_from_seed(&seed);
    let hex: String = kp
        .public
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(Whoami {
        fingerprint: fingerprint(&seed),
        pubkey_hex: hex,
        config_dir: dir.display().to_string(),
    })
}

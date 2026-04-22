//! Tauri shell around `bastion-core`. Every command here is a thin
//! delegation to the core library — if you find yourself writing
//! logic in this file, it probably belongs one layer down so the
//! CLI can reuse it.

use bastion_core::{
    dd_enclave, fingerprint, identity, keypair_from_seed, load_or_mint_seed, Connector,
    ConnectorKind, Store,
};
use serde::Serialize;
use std::path::PathBuf;

/// Per-process handle to the config dir + cached identity. Tauri
/// passes this to commands via managed state so we don't re-derive
/// the keypair on every IPC call.
struct AppState {
    config_dir: PathBuf,
}

#[derive(Serialize)]
struct Whoami {
    fingerprint: String,
    pubkey_hex: String,
    config_dir: String,
}

#[tauri::command]
fn whoami(state: tauri::State<'_, AppState>) -> Result<Whoami, String> {
    let seed = load_or_mint_seed(&state.config_dir).map_err(|e| e.to_string())?;
    let kp = keypair_from_seed(&seed);
    let pubkey_hex: String = kp
        .public
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(Whoami {
        fingerprint: fingerprint(&seed),
        pubkey_hex,
        config_dir: state.config_dir.display().to_string(),
    })
}

#[tauri::command]
fn list_connectors(state: tauri::State<'_, AppState>) -> Result<Vec<Connector>, String> {
    let store = Store::load(&state.config_dir).map_err(|e| e.to_string())?;
    Ok(store.list().to_vec())
}

#[tauri::command]
fn add_dd_enclave(
    state: tauri::State<'_, AppState>,
    label: String,
    origin: String,
) -> Result<Connector, String> {
    let mut store = Store::load(&state.config_dir).map_err(|e| e.to_string())?;
    let c = Connector::new(ConnectorKind::DdEnclave, label)
        .with_config("origin", serde_json::json!(origin));
    let out = c.clone();
    store.upsert(c);
    store.save().map_err(|e| e.to_string())?;
    Ok(out)
}

#[tauri::command]
fn remove_connector(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Option<Connector>, String> {
    let mut store = Store::load(&state.config_dir).map_err(|e| e.to_string())?;
    let removed = store.remove(&id);
    if removed.is_some() {
        store.save().map_err(|e| e.to_string())?;
    }
    Ok(removed)
}

/// Fetch `/attest` for a dd-enclave connector and return the pinned
/// pubkey + TDX quote (when present). Frontend uses this to display
/// the verification state in the sidebar before the user opens a
/// shell.
#[tauri::command]
async fn attest(origin: String) -> Result<dd_enclave::AttestResponse, String> {
    dd_enclave::fetch_attest(&origin).await.map_err(|e| e.to_string())
}

/// Tauri entry point. Called from both the desktop binary
/// (`src/main.rs`) and from `tauri-cli`'s mobile bootstrap macros.
pub fn run() {
    let config_dir = identity::default_config_dir()
        .expect("could not resolve config dir (no $HOME?)");

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .manage(AppState { config_dir })
        .invoke_handler(tauri::generate_handler![
            whoami,
            list_connectors,
            add_dd_enclave,
            remove_connector,
            attest,
        ])
        .run(tauri::generate_context!())
        .expect("tauri app run");
}

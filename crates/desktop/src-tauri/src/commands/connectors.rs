use bastion_core::{Connector, ConnectorKind, Store};
use serde::Deserialize;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn list_connectors(state: State<'_, AppState>) -> Result<Vec<Connector>, String> {
    let store = Store::load(&state.config_dir).map_err(|e| e.to_string())?;
    Ok(store.list().to_vec())
}

#[derive(Deserialize)]
pub struct AddConnectorArgs {
    pub kind: ConnectorKind,
    pub label: String,
    /// Config entries specific to the kind — e.g. `origin` for a
    /// `dd-enclave`, `host`/`user`/`port` for SSH, etc.
    pub config: serde_json::Map<String, serde_json::Value>,
}

#[tauri::command]
pub async fn add_connector(
    state: State<'_, AppState>,
    args: AddConnectorArgs,
) -> Result<Connector, String> {
    let mut store = Store::load(&state.config_dir).map_err(|e| e.to_string())?;
    let mut c = Connector::new(args.kind, args.label);
    for (k, v) in args.config {
        c = c.with_config(&k, v);
    }
    let saved = c.clone();
    store.upsert(c);
    store.save().map_err(|e| e.to_string())?;
    Ok(saved)
}

#[tauri::command]
pub async fn remove_connector(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut store = Store::load(&state.config_dir).map_err(|e| e.to_string())?;
    if store.remove(&id).is_none() {
        return Err(format!("no connector with id {id}"));
    }
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

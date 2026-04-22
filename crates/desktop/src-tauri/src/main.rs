#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod state;
mod commands {
    pub mod connectors;
    pub mod fleet;
    pub mod identity;
    pub mod pair;
    pub mod session;
}

use std::path::PathBuf;
use std::sync::Arc;

use bastion_core::identity::default_config_dir;
use tokio::sync::Mutex;

use state::AppState;

fn resolve_config_dir() -> PathBuf {
    std::env::var_os("BASTION_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| default_config_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn main() {
    let state = AppState {
        config_dir: resolve_config_dir(),
        sessions: Arc::new(Mutex::new(Default::default())),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::identity::whoami,
            commands::connectors::list_connectors,
            commands::connectors::add_connector,
            commands::connectors::remove_connector,
            commands::pair::pair,
            commands::fleet::fetch_agents,
            commands::session::connect_start,
            commands::session::connect_send,
            commands::session::connect_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

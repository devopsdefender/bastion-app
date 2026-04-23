#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod session_log;
mod state;
mod commands {
    pub mod connectors;
    pub mod fleet;
    pub mod identity;
    pub mod pair;
    pub mod search;
    pub mod tmux;
}

use std::path::PathBuf;
use std::sync::Arc;

use bastion_core::identity::default_config_dir;
use tokio::sync::Mutex;

use session_log::SessionLog;
use state::AppState;

fn resolve_config_dir() -> PathBuf {
    std::env::var_os("BASTION_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| default_config_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn main() {
    let config_dir = resolve_config_dir();
    let log = SessionLog::open(&config_dir).expect("open session log");
    let state = AppState {
        config_dir,
        attaches: Arc::new(Mutex::new(Default::default())),
        log: Arc::new(log),
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
            commands::tmux::tmux_list_sessions,
            commands::tmux::tmux_new_session,
            commands::tmux::tmux_attach,
            commands::tmux::tmux_write,
            commands::tmux::tmux_resize,
            commands::tmux::tmux_detach,
            commands::tmux::tmux_kill_session,
            commands::search::search_sessions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

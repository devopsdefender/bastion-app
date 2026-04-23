//! Local FTS5 search over logged session text.
//!
//! Queries go through `SessionLog::search` which runs on a blocking
//! thread. Returned hits carry session_id + agent + tmux_name so the
//! frontend can route a click back to the right sidebar entry.

use serde::Deserialize;
use tauri::State;

use crate::session_log::SearchHit;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SearchArgs {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

#[tauri::command]
pub async fn search_sessions(
    state: State<'_, AppState>,
    args: SearchArgs,
) -> Result<Vec<SearchHit>, String> {
    if args.query.trim().is_empty() {
        return Ok(Vec::new());
    }
    state
        .log
        .search(args.query, args.limit)
        .await
        .map_err(|e| format!("search: {e}"))
}

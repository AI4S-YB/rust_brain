use std::sync::Arc;

use rb_ai::session::store::{SessionIndex, SessionStore};
use rb_ai::session::ChatSession;
use rb_core::runner::Runner;
use tauri::State;

use crate::state::AppState;

/// Locate the SessionStore for the currently-open project.
/// Returns an error if no project is open yet.
async fn session_store_for(state: &AppState) -> Result<SessionStore, String> {
    let runner: Arc<Runner> = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no open project".to_string())?
            .clone()
    };
    let root = {
        let proj = runner.project().lock().await;
        proj.root_dir.clone()
    };
    Ok(SessionStore::new(&root))
}

#[tauri::command]
pub async fn chat_list_sessions(state: State<'_, AppState>) -> Result<SessionIndex, String> {
    let store = session_store_for(&state).await?;
    store.list().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_create_session(
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<ChatSession, String> {
    let store = session_store_for(&state).await?;
    let id = SessionStore::generate_session_id();
    let session = ChatSession::new(id, title.unwrap_or_else(|| "New chat".into()), None);
    store
        .save_session(&session)
        .await
        .map_err(|e| e.to_string())?;
    Ok(session)
}

#[tauri::command]
pub async fn chat_get_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ChatSession, String> {
    let store = session_store_for(&state).await?;
    store
        .load_session(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_delete_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let store = session_store_for(&state).await?;
    store
        .delete_session(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_rename_session(
    state: State<'_, AppState>,
    session_id: String,
    title: String,
) -> Result<(), String> {
    let store = session_store_for(&state).await?;
    store
        .rename_session(&session_id, title)
        .await
        .map_err(|e| e.to_string())
}

use std::path::PathBuf;

use rb_core::binary::BinaryStatus;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn get_binary_paths(state: State<'_, AppState>) -> Result<Vec<BinaryStatus>, String> {
    let resolver = state.binary_resolver.lock().await;
    Ok(resolver.list_known())
}

#[tauri::command]
pub async fn set_binary_path(
    name: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut resolver = state.binary_resolver.lock().await;
    resolver
        .set(&name, PathBuf::from(path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_binary_path(
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut resolver = state.binary_resolver.lock().await;
    resolver.clear(&name).map_err(|e| e.to_string())
}

use rb_core::asset::AssetRecord;
use tauri::State;

use crate::state::AppState;

async fn with_project<F, T>(state: &State<'_, AppState>, f: F) -> Result<T, String>
where
    F: FnOnce(&mut rb_core::project::Project) -> Result<T, String>,
{
    let runner = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no project open".to_string())?
            .clone()
    };
    let mut project = runner.project().lock().await;
    f(&mut project)
}

#[tauri::command]
pub async fn list_assets(state: State<'_, AppState>) -> Result<Vec<AssetRecord>, String> {
    with_project(&state, |p| Ok(p.assets.clone())).await
}

#[tauri::command]
pub async fn delete_asset(id: String, state: State<'_, AppState>) -> Result<(), String> {
    with_project(&state, |p| p.delete_asset(&id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn orphan_assets_for_run(
    run_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    with_project(&state, |p| Ok(p.orphan_assets_if_run_deleted(&run_id))).await
}

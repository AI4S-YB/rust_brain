use rb_core::input::{InputKind, InputPatch, InputRecord, InputScanReport};
use serde::Serialize;
use std::path::PathBuf;
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
pub async fn list_inputs(state: State<'_, AppState>) -> Result<Vec<InputRecord>, String> {
    with_project(&state, |p| Ok(p.inputs.clone())).await
}

#[tauri::command]
pub async fn register_input(
    path: String,
    kind: Option<InputKind>,
    display_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<InputRecord, String> {
    with_project(&state, |p| {
        p.register_input(&PathBuf::from(&path), kind, display_name)
            .map_err(|e| e.to_string())
    })
    .await
}

#[derive(Debug, Serialize)]
pub struct BatchRegisterResult {
    pub registered: Vec<InputRecord>,
    pub errors: Vec<BatchRegisterError>,
}

#[derive(Debug, Serialize)]
pub struct BatchRegisterError {
    pub path: String,
    pub message: String,
}

#[tauri::command]
pub async fn register_inputs_batch(
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<BatchRegisterResult, String> {
    let pbufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    with_project(&state, |p| {
        let (registered, errs) = p.register_inputs_batch(&pbufs);
        let errors = errs
            .into_iter()
            .map(|(path, message)| BatchRegisterError {
                path: path.display().to_string(),
                message,
            })
            .collect();
        Ok(BatchRegisterResult { registered, errors })
    })
    .await
}

#[tauri::command]
pub async fn update_input(
    id: String,
    patch: InputPatch,
    state: State<'_, AppState>,
) -> Result<InputRecord, String> {
    with_project(&state, |p| {
        p.update_input(&id, patch).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_input(id: String, state: State<'_, AppState>) -> Result<(), String> {
    with_project(&state, |p| p.delete_input(&id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn scan_inputs(state: State<'_, AppState>) -> Result<InputScanReport, String> {
    with_project(&state, |p| p.scan_inputs().map_err(|e| e.to_string())).await
}

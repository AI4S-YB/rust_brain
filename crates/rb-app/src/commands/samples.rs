use rb_core::sample::{SamplePatch, SampleRecord};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct SampleSheetImport {
    pub created: Vec<SampleRecord>,
    pub errors: Vec<String>,
}

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
pub async fn list_samples(state: State<'_, AppState>) -> Result<Vec<SampleRecord>, String> {
    with_project(&state, |p| Ok(p.samples.clone())).await
}

#[tauri::command]
pub async fn create_sample(
    name: String,
    group: Option<String>,
    condition: Option<String>,
    input_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<SampleRecord, String> {
    with_project(&state, |p| {
        p.create_sample(name, group, condition, input_ids)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn update_sample(
    id: String,
    patch: SamplePatch,
    state: State<'_, AppState>,
) -> Result<SampleRecord, String> {
    with_project(&state, |p| {
        p.update_sample(&id, patch).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_sample(id: String, state: State<'_, AppState>) -> Result<(), String> {
    with_project(&state, |p| p.delete_sample(&id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn auto_pair_samples(state: State<'_, AppState>) -> Result<Vec<SampleRecord>, String> {
    with_project(&state, |p| p.auto_pair_samples().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn import_samples_from_tsv(
    path: String,
    state: State<'_, AppState>,
) -> Result<SampleSheetImport, String> {
    with_project(&state, |p| {
        let (created, errors) = p
            .import_samples_from_tsv(&PathBuf::from(&path))
            .map_err(|e| e.to_string())?;
        Ok(SampleSheetImport { created, errors })
    })
    .await
}

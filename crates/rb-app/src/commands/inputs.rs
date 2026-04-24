use chrono::Utc;
use rb_core::input::{InputKind, InputPatch, InputRecord, InputScanReport};
use serde::Serialize;
use std::path::{Component, Path, PathBuf};
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

fn sanitize_cell(s: &str) -> String {
    s.replace(['\t', '\r', '\n'], " ")
}

fn validate_simple_filename(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("filename must not be empty".into());
    }
    let comps: Vec<Component> = Path::new(name).components().collect();
    if comps.len() != 1 || !matches!(comps[0], Component::Normal(_)) {
        return Err("filename must be a simple file name (no path separators or '..')".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn write_sample_sheet(
    filename: Option<String>,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<InputRecord, String> {
    if headers.is_empty() {
        return Err("headers must not be empty".into());
    }
    if rows.is_empty() {
        return Err("at least one row is required".into());
    }
    for (i, r) in rows.iter().enumerate() {
        if r.len() != headers.len() {
            return Err(format!(
                "row {} has {} cells, expected {}",
                i,
                r.len(),
                headers.len()
            ));
        }
    }

    let resolved_name = filename
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("coldata_{}.tsv", Utc::now().format("%Y%m%d_%H%M%S")));
    validate_simple_filename(&resolved_name)?;

    let mut tsv = String::new();
    tsv.push_str(
        &headers
            .iter()
            .map(|h| sanitize_cell(h))
            .collect::<Vec<_>>()
            .join("\t"),
    );
    tsv.push('\n');
    for row in &rows {
        tsv.push_str(
            &row.iter()
                .map(|c| sanitize_cell(c))
                .collect::<Vec<_>>()
                .join("\t"),
        );
        tsv.push('\n');
    }

    with_project(&state, move |p| {
        let input_dir = p.root_dir.join("input");
        std::fs::create_dir_all(&input_dir).map_err(|e| format!("create input dir: {}", e))?;
        let target = input_dir.join(&resolved_name);
        std::fs::write(&target, &tsv).map_err(|e| format!("write {}: {}", target.display(), e))?;
        p.register_input(&target, Some(InputKind::SampleSheet), None)
            .map_err(|e| e.to_string())
    })
    .await
}

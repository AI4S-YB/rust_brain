use rb_core::module::ValidationError;
use rb_core::project::RunRecord;
use serde_json::Value;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn validate_params(
    module_id: String,
    params: Value,
    state: State<'_, AppState>,
) -> Result<Vec<ValidationError>, String> {
    let module = {
        let registry = state.registry.lock().await;
        registry
            .get(&module_id)
            .ok_or_else(|| format!("module '{}' not found", module_id))?
    };
    Ok(module.validate(&params))
}

#[tauri::command]
pub async fn run_module(
    module_id: String,
    params: Value,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let module = {
        let registry = state.registry.lock().await;
        registry
            .get(&module_id)
            .ok_or_else(|| format!("module '{}' not found", module_id))?
    };

    let runner = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no project open".to_string())?
            .clone()
    };

    runner.spawn(module, params).await
}

#[tauri::command]
pub async fn cancel_run(run_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let runner = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no project open".to_string())?
            .clone()
    };
    runner.cancel(&run_id).await;
    Ok(())
}

#[tauri::command]
pub async fn get_run_result(
    run_id: String,
    state: State<'_, AppState>,
) -> Result<Option<RunRecord>, String> {
    let runner = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no project open".to_string())?
            .clone()
    };

    let project = runner.project().lock().await;
    let record = project.runs.iter().find(|r| r.id == run_id).cloned();
    Ok(record)
}

#[tauri::command]
pub async fn list_runs(
    module_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<RunRecord>, String> {
    let runner = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no project open".to_string())?
            .clone()
    };

    let project = runner.project().lock().await;
    let runs: Vec<RunRecord> = project
        .runs
        .iter()
        .filter(|r| {
            module_id
                .as_deref()
                .map(|id| r.module_id == id)
                .unwrap_or(true)
        })
        .cloned()
        .collect();

    Ok(runs)
}

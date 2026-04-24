use rb_core::module::ValidationError;
use rb_core::project::{RunRecord, RunStatus};
use serde_json::Value;
use std::collections::HashMap;
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
    inputs_used: Vec<String>,
    assets_used: Vec<String>,
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

    runner.spawn(module, params, inputs_used, assets_used).await
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

#[tauri::command]
pub async fn delete_run(run_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let runner = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no project open".to_string())?
            .clone()
    };

    let mut project = runner.project().lock().await;
    project.delete_run(&run_id).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct ClearRunsResult {
    pub deleted: u32,
    pub skipped_running: u32,
    pub errors: Vec<String>,
}

/// Bulk-delete runs matching filters.
/// - `module_id`: only runs for this backend module id (None = any module).
/// - `statuses`: only runs whose status matches one of these (None = any status).
/// Running / Pending runs are always skipped to avoid racing the Runner.
#[tauri::command]
pub async fn clear_runs(
    module_id: Option<String>,
    statuses: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<ClearRunsResult, String> {
    let runner = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no project open".to_string())?
            .clone()
    };

    let wanted: Option<Vec<RunStatus>> =
        statuses.map(|v| v.into_iter().filter_map(parse_status).collect());

    let ids_to_delete: Vec<String> = {
        let project = runner.project().lock().await;
        project
            .runs
            .iter()
            .filter(|r| match &module_id {
                Some(m) => r.module_id == *m,
                None => true,
            })
            .filter(|r| match &wanted {
                Some(list) => list.contains(&r.status),
                None => true,
            })
            .filter(|r| !matches!(r.status, RunStatus::Running | RunStatus::Pending))
            .map(|r| r.id.clone())
            .collect()
    };

    let mut deleted = 0u32;
    let mut errors = Vec::new();
    {
        let mut project = runner.project().lock().await;
        for id in &ids_to_delete {
            match project.delete_run(id) {
                Ok(()) => deleted += 1,
                Err(e) => errors.push(format!("{}: {}", id, e)),
            }
        }
    }

    let skipped_running: u32 = {
        let project = runner.project().lock().await;
        project
            .runs
            .iter()
            .filter(|r| match &module_id {
                Some(m) => r.module_id == *m,
                None => true,
            })
            .filter(|r| match &wanted {
                Some(list) => list.contains(&r.status),
                None => true,
            })
            .filter(|r| matches!(r.status, RunStatus::Running | RunStatus::Pending))
            .count() as u32
    };

    Ok(ClearRunsResult {
        deleted,
        skipped_running,
        errors,
    })
}

fn parse_status(s: String) -> Option<RunStatus> {
    match s.as_str() {
        "Pending" => Some(RunStatus::Pending),
        "Running" => Some(RunStatus::Running),
        "Done" => Some(RunStatus::Done),
        "Failed" => Some(RunStatus::Failed),
        "Cancelled" => Some(RunStatus::Cancelled),
        _ => None,
    }
}

#[tauri::command]
pub async fn get_run_sizes(
    run_ids: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<HashMap<String, u64>, String> {
    let runner = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no project open".to_string())?
            .clone()
    };

    let project = runner.project().lock().await;
    let ids: Vec<String> = match run_ids {
        Some(v) => v,
        None => project.runs.iter().map(|r| r.id.clone()).collect(),
    };

    let mut out = HashMap::new();
    for id in ids {
        out.insert(id.clone(), project.run_dir_size(&id));
    }
    Ok(out)
}

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ModuleDescriptor {
    pub id: String,      // backend id (e.g. "qc", "rustqc")
    pub view_id: String, // frontend view id (built-ins use existing ids; plugins == backend id)
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub icon: String,
    pub source: String,        // "builtin" | "bundled-plugin" | "user-plugin"
    pub has_native_view: bool, // true → use frontend/js/modules/<view-id>/view.js
    pub binary_id: Option<String>,
}

#[tauri::command]
pub async fn list_modules(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<ModuleDescriptor>, String> {
    let modules = {
        let reg = state.registry.lock().await;
        reg.list_all()
    };
    let plugin_tags: std::collections::HashMap<String, crate::state::PluginSourceTag> = state
        .plugins
        .lock()
        .await
        .loaded
        .iter()
        .cloned()
        .map(|t| (t.id.clone(), t))
        .collect();

    let mut out = Vec::new();
    for m in modules {
        let id = m.id().to_string();
        let plugin_tag = plugin_tags.get(&id);
        let descriptor = match plugin_tag {
            None => ModuleDescriptor {
                id: id.clone(),
                view_id: view_id_for_builtin(&id),
                name: m.name().to_string(),
                description: None,
                category: category_for_builtin(&id).to_string(),
                icon: icon_for_builtin(&id).to_string(),
                source: "builtin".into(),
                has_native_view: true,
                binary_id: None,
            },
            Some(tag) => ModuleDescriptor {
                id: id.clone(),
                view_id: id.clone(),
                name: m.name().to_string(),
                description: tag.description.clone(),
                category: tag.category.clone().unwrap_or_else(|| "other".into()),
                icon: tag.icon.clone().unwrap_or_else(|| "plug".into()),
                source: if tag.source == "bundled" {
                    "bundled-plugin".into()
                } else {
                    "user-plugin".into()
                },
                has_native_view: false,
                binary_id: Some(tag.binary_id.clone()),
            },
        };
        out.push(descriptor);
    }
    Ok(out)
}

fn view_id_for_builtin(id: &str) -> String {
    match id {
        "deseq2" => "differential".into(),
        "gff_convert" => "gff-convert".into(),
        "star_index" => "star-index".into(),
        "star_align" => "star-align".into(),
        other => other.into(),
    }
}

fn category_for_builtin(id: &str) -> &'static str {
    match id {
        "qc" => "qc",
        "trimming" => "trimming",
        "star_index" | "star_align" => "alignment",
        "gff_convert" => "annotation",
        "deseq2" => "differential",
        _ => "other",
    }
}

fn icon_for_builtin(id: &str) -> &'static str {
    match id {
        "qc" => "microscope",
        "trimming" => "scissors",
        "star_align" => "git-merge",
        "star_index" => "database",
        "gff_convert" => "file-code-2",
        "deseq2" => "flame",
        _ => "puzzle",
    }
}

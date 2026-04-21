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

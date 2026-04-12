use std::path::PathBuf;
use std::sync::Arc;

use rb_core::project::Project;
use rb_core::runner::Runner;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

#[derive(Serialize)]
pub struct ProjectInfo {
    pub name: String,
    pub root_dir: String,
    pub run_count: usize,
}

fn setup_runner(project: Project, app: &AppHandle) -> Runner {
    let project_arc = Arc::new(tokio::sync::Mutex::new(project));
    let app_handle = app.clone();
    Runner::new(project_arc).on_progress(Box::new(move |run_id, progress| {
        let _ = app_handle.emit(
            "run-progress",
            serde_json::json!({
                "runId": run_id,
                "fraction": progress.fraction,
                "message": progress.message,
            }),
        );
    }))
}

#[tauri::command]
pub async fn create_project(
    name: String,
    dir: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ProjectInfo, String> {
    let root_dir = PathBuf::from(&dir);
    let project = Project::create(&name, &root_dir).map_err(|e| e.to_string())?;

    let info = ProjectInfo {
        name: project.name.clone(),
        root_dir: project.root_dir.to_string_lossy().to_string(),
        run_count: project.runs.len(),
    };

    let runner = setup_runner(project, &app);
    *state.runner.lock().await = Some(runner);

    {
        let mut recent = state.recent_projects.lock().await;
        let path = PathBuf::from(&dir);
        recent.retain(|p| p != &path);
        recent.insert(0, path);
    }

    Ok(info)
}

#[tauri::command]
pub async fn open_project(
    dir: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ProjectInfo, String> {
    let root_dir = PathBuf::from(&dir);
    let project = Project::load(&root_dir).map_err(|e| e.to_string())?;

    let info = ProjectInfo {
        name: project.name.clone(),
        root_dir: project.root_dir.to_string_lossy().to_string(),
        run_count: project.runs.len(),
    };

    let runner = setup_runner(project, &app);
    *state.runner.lock().await = Some(runner);

    {
        let mut recent = state.recent_projects.lock().await;
        let path = PathBuf::from(&dir);
        recent.retain(|p| p != &path);
        recent.insert(0, path);
    }

    Ok(info)
}

#[tauri::command]
pub async fn list_recent_projects(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let recent = state.recent_projects.lock().await;
    Ok(recent
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

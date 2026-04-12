#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::{AppState, ModuleRegistry};
use std::sync::Arc;

fn main() {
    let mut registry = ModuleRegistry::new();
    registry.register(Arc::new(rb_deseq2::DeseqModule));
    registry.register(Arc::new(rb_qc::QcModule));
    registry.register(Arc::new(rb_trimming::TrimmingModule));

    tauri::Builder::default()
        .manage(AppState::new(registry))
        .invoke_handler(tauri::generate_handler![
            commands::project::create_project,
            commands::project::open_project,
            commands::project::list_recent_projects,
            commands::modules::validate_params,
            commands::modules::run_module,
            commands::modules::cancel_run,
            commands::modules::get_run_result,
            commands::modules::list_runs,
            commands::files::select_files,
            commands::files::select_directory,
            commands::files::read_table_preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RustBrain");
}

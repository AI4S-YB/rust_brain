#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::{AppState, ModuleRegistry};
use std::sync::Arc;
use tauri::{path::BaseDirectory, Manager};

fn main() {
    let mut registry = ModuleRegistry::new();
    registry.register(Arc::new(rb_deseq2::DeseqModule));
    registry.register(Arc::new(rb_qc::QcModule));
    registry.register(Arc::new(rb_trimming::TrimmingModule));
    registry.register(Arc::new(rb_star_index::StarIndexModule));
    registry.register(Arc::new(rb_star_align::StarAlignModule));

    tauri::Builder::default()
        .manage(AppState::new(registry))
        .setup(|app| {
            register_bundled_star(app);
            Ok(())
        })
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
            commands::settings::get_binary_paths,
            commands::settings::set_binary_path,
            commands::settings::clear_binary_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RustBrain");
}

fn register_bundled_star(app: &tauri::App) {
    let exe = if cfg!(windows) { "star.exe" } else { "star" };
    let path = match app
        .path()
        .resolve(format!("binaries/{exe}"), BaseDirectory::Resource)
    {
        Ok(p) if p.exists() => p,
        _ => return,
    };
    let state = app.state::<AppState>();
    let resolver = state.binary_resolver.clone();
    tauri::async_runtime::block_on(async move {
        resolver.lock().await.register_bundled("star", path);
    });
}

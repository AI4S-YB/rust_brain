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
            register_bundled(app, "star", "star");
            register_bundled(app, "gffread-rs", "gffread-rs");
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

fn register_bundled(app: &tauri::App, binary_id: &str, filename_stem: &str) {
    let exe = if cfg!(windows) {
        format!("{filename_stem}.exe")
    } else {
        filename_stem.to_string()
    };
    let path = match app
        .path()
        .resolve(format!("binaries/{exe}"), BaseDirectory::Resource)
    {
        Ok(p) if p.exists() => p,
        _ => return,
    };
    let state = app.state::<AppState>();
    let resolver = state.binary_resolver.clone();
    let id = binary_id.to_string();
    tauri::async_runtime::block_on(async move {
        resolver.lock().await.register_bundled(&id, path);
    });
}

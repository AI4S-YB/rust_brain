#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai_state;
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
    registry.register(Arc::new(rb_gff_convert::GffConvertModule));
    registry.register(Arc::new(rb_star_index::StarIndexModule));
    registry.register(Arc::new(rb_star_align::StarAlignModule));

    // Build per-language tool registries from the same module set.
    let modules_for_ai: Vec<Arc<dyn rb_core::module::Module>> = vec![
        Arc::new(rb_deseq2::DeseqModule),
        Arc::new(rb_qc::QcModule),
        Arc::new(rb_trimming::TrimmingModule),
        Arc::new(rb_gff_convert::GffConvertModule),
        Arc::new(rb_star_index::StarIndexModule),
        Arc::new(rb_star_align::StarAlignModule),
    ];
    let mut tools_by_lang = std::collections::HashMap::new();
    tools_by_lang.insert(
        "en".to_string(),
        Arc::new(ai_state::build_tool_registry(&modules_for_ai, "en")),
    );
    tools_by_lang.insert(
        "zh".to_string(),
        Arc::new(ai_state::build_tool_registry(&modules_for_ai, "zh")),
    );

    // Load persisted AI config.
    let config_path = rb_ai::config::AiConfig::default_path();
    let ai_config =
        tauri::async_runtime::block_on(rb_ai::config::AiConfig::load_or_default(&config_path))
            .unwrap_or_default();

    // Pick KeyStore: prefer OS keyring; fall back to encrypted file if the
    // keyring probe fails (e.g., headless Linux without libsecret).
    let keystore: Arc<dyn rb_ai::config::keyring::KeyStore> = {
        use rb_ai::config::keyring::KeyStore as _;
        let k = rb_ai::config::keyring::KeyringStore;
        match k.get("__probe__") {
            Ok(_) => Arc::new(k),
            Err(_) => {
                let fallback_path = dirs::config_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("rustbrain")
                    .join("secrets.enc.json");
                let machine_id = std::fs::read_to_string("/etc/machine-id")
                    .unwrap_or_else(|_| "rustbrain-fallback".into());
                Arc::new(
                    rb_ai::config::keyring::EncryptedFileStore::new(
                        fallback_path,
                        machine_id.trim().as_bytes(),
                    )
                    .expect("encrypted-file keystore"),
                )
            }
        }
    };

    let ai = Arc::new(ai_state::AiState {
        tools_by_lang,
        keystore,
        config_path,
        config: tokio::sync::Mutex::new(ai_config),
        plans: rb_ai::orchestrator::PlanCardRegistry::new(),
        active_turns: tokio::sync::Mutex::new(std::collections::HashMap::new()),
    });

    tauri::Builder::default()
        .manage(AppState::new(registry, ai))
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
            commands::chat::chat_list_sessions,
            commands::chat::chat_create_session,
            commands::chat::chat_get_session,
            commands::chat::chat_delete_session,
            commands::chat::chat_rename_session,
            commands::chat::chat_send_message,
            commands::chat::chat_approve_tool,
            commands::chat::chat_reject_tool,
            commands::chat::chat_cancel_turn,
            commands::chat::chat_cancel_run,
            commands::ai_provider::ai_get_config,
            commands::ai_provider::ai_set_provider_config,
            commands::ai_provider::ai_set_default_provider,
            commands::ai_provider::ai_set_api_key,
            commands::ai_provider::ai_clear_api_key,
            commands::ai_provider::ai_has_api_key,
            commands::ai_provider::ai_backend_info,
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

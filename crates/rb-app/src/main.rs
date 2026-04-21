#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai_state;
mod commands;
mod state;

use state::{AppState, ModuleRegistry};
use std::sync::Arc;
use tauri::{path::BaseDirectory, Manager};

/// Bundled plugin manifests embedded at compile time. The directory may be
/// empty (it always contains a `.keep` to ensure it's checked into git).
pub static BUNDLED_PLUGINS: include_dir::Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/plugins");

fn main() {
    // 1. First-party module registry (unchanged).
    let mut registry = ModuleRegistry::new();
    registry.register(Arc::new(rb_deseq2::DeseqModule));
    registry.register(Arc::new(rb_qc::QcModule));
    registry.register(Arc::new(rb_trimming::TrimmingModule));
    registry.register(Arc::new(rb_gff_convert::GffConvertModule));
    registry.register(Arc::new(rb_star_index::StarIndexModule));
    registry.register(Arc::new(rb_star_align::StarAlignModule));

    // 2. Build the binary resolver up-front so plugin modules can share it.
    let binary_resolver_inner = rb_core::binary::BinaryResolver::load().unwrap_or_else(|e| {
        eprintln!(
            "warning: failed to load binary settings ({}); using defaults",
            e
        );
        rb_core::binary::BinaryResolver::with_defaults_at(
            rb_core::binary::BinaryResolver::default_settings_path(),
        )
    });
    let binary_resolver = Arc::new(tokio::sync::Mutex::new(binary_resolver_inner));

    // 3. Load plugins from bundled dir + user dir.
    let user_plugin_dir = directories::ProjectDirs::from("", "", "rust_brain")
        .map(|pd| pd.config_dir().join("plugins"))
        .unwrap_or_else(|| std::path::PathBuf::from("plugins"));
    let plugin_reg = rb_plugin::load_plugins(&BUNDLED_PLUGINS, Some(&user_plugin_dir));

    // 4. Register dynamic binaries from plugin manifests so they show up in
    //    Settings and resolve correctly.
    {
        let mut resolver = tauri::async_runtime::block_on(binary_resolver.lock());
        for loaded in plugin_reg.by_id.values() {
            resolver.register_known_dynamic(rb_core::binary::KnownBinaryEntry {
                id: loaded.manifest.binary.id.clone(),
                display_name: loaded
                    .manifest
                    .binary
                    .display_name
                    .clone()
                    .unwrap_or_else(|| loaded.manifest.name.clone()),
                install_hint: loaded
                    .manifest
                    .binary
                    .install_hint
                    .clone()
                    .unwrap_or_else(|| {
                        format!(
                            "Install '{}' and configure its path.",
                            loaded.manifest.binary.id
                        )
                    }),
            });
        }
    }

    // 5. Build plugin modules using the shared resolver.
    let plugin_modules: Vec<Arc<dyn rb_core::module::Module>> = plugin_reg
        .by_id
        .values()
        .map(|loaded| {
            let manifest = Arc::new(loaded.manifest.clone());
            Arc::new(state::LazyResolvingPluginModule::new(
                manifest,
                loaded.manifest.binary.id.clone(),
                binary_resolver.clone(),
            )) as Arc<dyn rb_core::module::Module>
        })
        .collect();

    // 6. Register plugin modules into the first-party registry.
    for m in &plugin_modules {
        registry.register(m.clone());
    }

    // 7. Build modules_for_ai = first-party + plugin (per-language tool registries).
    let mut modules_for_ai: Vec<Arc<dyn rb_core::module::Module>> = vec![
        Arc::new(rb_deseq2::DeseqModule),
        Arc::new(rb_qc::QcModule),
        Arc::new(rb_trimming::TrimmingModule),
        Arc::new(rb_gff_convert::GffConvertModule),
        Arc::new(rb_star_index::StarIndexModule),
        Arc::new(rb_star_align::StarAlignModule),
    ];
    modules_for_ai.extend(plugin_modules.iter().cloned());

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

    // 9. Build AppState (now with the pre-built resolver).
    let app_state = AppState::new(registry, binary_resolver.clone(), user_plugin_dir, ai);

    // 10. Populate plugin metadata stores so future Tauri commands can read them.
    {
        let mut mans = tauri::async_runtime::block_on(app_state.plugin_manifests.lock());
        for loaded in plugin_reg.by_id.values() {
            mans.insert(
                loaded.manifest.id.clone(),
                Arc::new(loaded.manifest.clone()),
            );
        }
    }
    {
        let mut diag = tauri::async_runtime::block_on(app_state.plugins.lock());
        diag.loaded = plugin_reg
            .by_id
            .iter()
            .map(|(id, lp)| state::PluginSourceTag {
                id: id.clone(),
                source: match lp.source {
                    rb_plugin::PluginSource::Bundled => "bundled".into(),
                    rb_plugin::PluginSource::User => "user".into(),
                },
                origin_path: lp.origin_path.clone(),
                category: lp.manifest.category.clone(),
                icon: lp.manifest.icon.clone(),
                description: lp.manifest.description.clone(),
                binary_id: lp.manifest.binary.id.clone(),
            })
            .collect();
        diag.errors = plugin_reg
            .errors
            .iter()
            .map(|e| state::PluginErrorView {
                source_label: e.source_label.clone(),
                message: e.message.clone(),
            })
            .collect();
    }

    tauri::Builder::default()
        .manage(app_state)
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
            commands::ai_provider::ai_test_connection,
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

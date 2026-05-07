#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_runtime;
mod ai_state;
mod commands;
mod rnaseq_pipeline;
mod state;

use state::{AppState, ModuleRegistry};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{path::BaseDirectory, Emitter, Manager};

/// Bundled plugin manifests embedded at compile time. The directory may be
/// empty (it always contains a `.keep` to ensure it's checked into git).
pub static BUNDLED_PLUGINS: include_dir::Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/plugins");

/// Shared flag so `force_close_app` can tell the window-close handler to let
/// the next CloseRequested through without re-prompting.
struct CloseConfirmed(Arc<AtomicBool>);

#[tauri::command]
async fn force_close_app(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    close_state: tauri::State<'_, CloseConfirmed>,
) -> Result<(), String> {
    // Cancel in-flight runs in parallel so subprocesses get a chance to exit
    // cleanly before the window (and with it, the tokio runtime) tears down.
    // Each `cancel` call has a 500ms cooperative grace period — running them
    // sequentially would scale badly with many active runs.
    let runner_opt = { state.runner.lock().await.clone() };
    if let Some(runner) = runner_opt {
        let ids = runner.active_run_ids().await;
        let cancels = ids.into_iter().map(|id| {
            let runner = runner.clone();
            async move { runner.cancel(&id).await }
        });
        futures_util::future::join_all(cancels).await;
    }
    close_state.0.store(true, Ordering::SeqCst);
    // If close() itself fails, clear the flag so a later close attempt still
    // triggers the confirm prompt instead of silently slipping through the
    // one-shot short-circuit in the window-close handler.
    if let Err(e) = window.close() {
        close_state.0.store(false, Ordering::SeqCst);
        return Err(e.to_string());
    }
    Ok(())
}

fn main() {
    // 1. First-party module registry (unchanged).
    let mut registry = ModuleRegistry::new();
    registry.register(Arc::new(rb_deseq2::DeseqModule));
    registry.register(Arc::new(rb_qc::QcModule));
    registry.register(Arc::new(rb_trimming::TrimmingModule));
    registry.register(Arc::new(rb_gff_convert::GffConvertModule));
    registry.register(Arc::new(rb_star_index::StarIndexModule));
    registry.register(Arc::new(rb_star_align::StarAlignModule));
    registry.register(Arc::new(rb_star_align::CountsMergeModule));
    registry.register(Arc::new(rb_rustqc::RustqcModule));
    registry.register(Arc::new(rb_gene_length::GeneLengthModule));
    registry.register(Arc::new(rb_expr_norm::ExprNormModule));

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

    // 7. Load persisted AI config.
    let config_path = rb_ai::config::AiConfig::default_path();
    let mut ai_config =
        tauri::async_runtime::block_on(rb_ai::config::AiConfig::load_or_default(&config_path))
            .unwrap_or_default();
    ai_config.apply_env_defaults();

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
        keystore,
        config_path,
        config: tokio::sync::Mutex::new(ai_config),
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

    // Flips to true once the user confirms closing while runs are in flight —
    // stops the window close handler from asking twice.
    let close_confirmed = Arc::new(AtomicBool::new(false));

    // Agent runtime (self-evolving agent). Shared via Tauri state so the
    // agent_* commands can look up per-project handles.
    let agent_runtime = Arc::new(
        agent_runtime::AgentRuntime::new().unwrap_or_else(|e| panic!("init AgentRuntime: {e}")),
    );

    tauri::Builder::default()
        .manage(app_state)
        .manage(CloseConfirmed(close_confirmed.clone()))
        .manage(agent_runtime.clone())
        .setup(|app| {
            register_bundled(app, "star", "star");
            register_bundled(app, "gffread-rs", "gffread-rs");
            register_bundled(app, "cutadapt-rs", "cutadapt-rs");
            register_bundled(app, "rustqc", "rustqc");
            // wgcna ships with sibling runtime deps (libs/*.dylib, openblas.dll).
            register_bundled(app, "wgcna", "wgcna-dist/wgcna");
            Ok(())
        })
        .on_window_event({
            let close_confirmed = close_confirmed.clone();
            move |window, event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if close_confirmed.load(Ordering::SeqCst) {
                        return;
                    }
                    let state = window.state::<AppState>();
                    let runner_mutex = state.runner.clone();
                    // `active_run_count` is a lock-free atomic, so a `try_lock`
                    // is only needed to confirm a runner exists — the count
                    // itself never blocks.
                    let active = match runner_mutex.try_lock() {
                        Ok(guard) => guard.as_ref().map(|r| r.active_run_count()).unwrap_or(0),
                        // If the runner lock is momentarily held, err on the
                        // side of asking — a spurious prompt is better than
                        // silently killing a run.
                        Err(_) => 1,
                    };
                    if active > 0 {
                        api.prevent_close();
                        let _ =
                            window.emit("close-requested", serde_json::json!({ "active": active }));
                    }
                }
            }
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
            commands::modules::delete_run,
            commands::modules::clear_runs,
            commands::modules::get_run_sizes,
            commands::modules::list_modules,
            force_close_app,
            commands::inputs::list_inputs,
            commands::inputs::register_input,
            commands::inputs::register_inputs_batch,
            commands::inputs::update_input,
            commands::inputs::delete_input,
            commands::inputs::scan_inputs,
            commands::inputs::write_sample_sheet,
            commands::samples::list_samples,
            commands::samples::create_sample,
            commands::samples::update_sample,
            commands::samples::delete_sample,
            commands::samples::auto_pair_samples,
            commands::samples::preview_auto_pair_samples,
            commands::samples::import_samples_from_tsv,
            commands::assets::list_assets,
            commands::assets::delete_asset,
            commands::assets::orphan_assets_for_run,
            commands::files::select_files,
            commands::files::select_directory,
            commands::files::read_table_preview,
            commands::settings::get_binary_paths,
            commands::settings::set_binary_path,
            commands::settings::clear_binary_path,
            commands::ai_provider::ai_get_config,
            commands::ai_provider::ai_set_provider_config,
            commands::ai_provider::ai_set_default_provider,
            commands::ai_provider::ai_set_api_key,
            commands::ai_provider::ai_clear_api_key,
            commands::ai_provider::ai_has_api_key,
            commands::ai_provider::ai_backend_info,
            commands::ai_provider::ai_test_connection,
            commands::plugins::list_plugin_status,
            commands::plugins::reload_plugins,
            commands::plugins::get_plugin_manifest,
            commands::agent::agent_start_session,
            commands::agent::agent_send,
            commands::agent::agent_approve,
            commands::agent::agent_reject,
            commands::agent::agent_answer,
            commands::agent::agent_cancel,
            commands::agent::agent_set_full_permission,
            commands::agent::agent_list_archives,
            commands::agent::agent_load_archive,
            commands::agent::agent_list_skills,
            commands::agent::agent_edit_memory,
            rb_fastq_viewer::commands::fastq_viewer_open,
            rb_fastq_viewer::commands::fastq_viewer_close,
            rb_fastq_viewer::commands::fastq_viewer_status,
            rb_fastq_viewer::commands::fastq_viewer_read,
            rb_fastq_viewer::commands::fastq_viewer_search_id,
            rb_genome_viewer::commands::genome_viewer_load_reference,
            rb_genome_viewer::commands::genome_viewer_add_track,
            rb_genome_viewer::commands::genome_viewer_remove_track,
            rb_genome_viewer::commands::genome_viewer_list_tracks,
            rb_genome_viewer::commands::genome_viewer_fetch_reference_region,
            rb_genome_viewer::commands::genome_viewer_fetch_track_features,
            rb_genome_viewer::commands::genome_viewer_search_feature,
            rb_genome_viewer::commands::genome_viewer_bgzip_and_tabix,
            rb_genome_viewer::commands::genome_viewer_get_session_state,
            rb_genome_viewer::commands::genome_viewer_save_session_state,
            rb_bam_tools::commands::bam_tools_index,
            rb_bam_tools::commands::bam_tools_index_status,
            rb_bam_tools::commands::bam_tools_header_references,
            rb_bam_tools::commands::bam_tools_extract_region,
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
    let resolved_path = match app
        .path()
        .resolve(format!("binaries/{exe}"), BaseDirectory::Resource)
    {
        Ok(p) if p.exists() => p,
        _ => return,
    };
    let normalized_path = rb_core::binary::normalize_windows_extended_path(resolved_path.clone());
    let path = if normalized_path.exists() {
        normalized_path
    } else {
        resolved_path
    };
    if let Some(parent) = path.parent() {
        prepend_to_process_path(parent);
    }
    let state = app.state::<AppState>();
    let resolver = state.binary_resolver.clone();
    let id = binary_id.to_string();
    tauri::async_runtime::block_on(async move {
        resolver.lock().await.register_bundled(&id, path);
    });
}

fn prepend_to_process_path(dir: &Path) {
    if dir.as_os_str().is_empty() {
        return;
    }
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut paths: Vec<PathBuf> = std::env::split_paths(&current).collect();
    if paths.iter().any(|p| same_path(p, dir)) {
        return;
    }
    paths.insert(0, dir.to_path_buf());
    if let Ok(joined) = std::env::join_paths(paths) {
        std::env::set_var("PATH", joined);
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let a = rb_core::binary::normalize_windows_extended_path(a.to_path_buf());
    let b = rb_core::binary::normalize_windows_extended_path(b.to_path_buf());
    #[cfg(windows)]
    {
        a.to_string_lossy()
            .eq_ignore_ascii_case(b.to_string_lossy().as_ref())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

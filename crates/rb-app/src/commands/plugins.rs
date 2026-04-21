use std::collections::HashMap;
use std::sync::Arc;

use rb_plugin::PluginManifest;
use serde::Serialize;
use tauri::{Emitter, State};

use crate::state::{AppState, PluginDiagnostics, PluginErrorView, PluginSourceTag};

#[derive(Debug, Serialize)]
pub struct PluginManifestView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub binary_id: String,
    pub params_schema: serde_json::Value,
    pub strings: serde_json::Value,
    pub params: serde_json::Value,
    pub outputs: serde_json::Value,
}

#[tauri::command]
pub async fn list_plugin_status(
    state: State<'_, AppState>,
) -> Result<PluginDiagnostics, String> {
    Ok(state.plugins.lock().await.clone())
}

#[tauri::command]
pub async fn get_plugin_manifest(
    id: String,
    state: State<'_, AppState>,
) -> Result<PluginManifestView, String> {
    let manifests = state.plugin_manifests.lock().await;
    let manifest = manifests
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("plugin '{id}' not found"))?;
    Ok(PluginManifestView {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        category: manifest.category.clone(),
        icon: manifest.icon.clone(),
        binary_id: manifest.binary.id.clone(),
        params_schema: rb_plugin::schema::derive_json_schema(&manifest),
        strings: serde_json::to_value(&manifest.strings).unwrap_or(serde_json::Value::Null),
        params: serde_json::to_value(&manifest.params).unwrap_or(serde_json::Value::Null),
        outputs: serde_json::to_value(&manifest.outputs).unwrap_or(serde_json::Value::Null),
    })
}

/// Re-scan bundled + user plugin dirs and update AppState in place.
/// Returns the new diagnostics so the caller can decide whether to broadcast.
pub async fn reload_plugins_impl(
    state: &AppState,
    bundled: &include_dir::Dir<'_>,
) -> PluginDiagnostics {
    let user_dir = state.user_plugin_dir.clone();
    let plugin_reg = rb_plugin::load_plugins(bundled, Some(&user_dir));

    // Drop existing plugin entries from the module registry while keeping
    // first-party modules. The set of plugin ids comes from the current
    // plugin_manifests map (the source of truth for "what is a plugin?").
    let plugin_ids: Vec<String> =
        state.plugin_manifests.lock().await.keys().cloned().collect();
    {
        let mut reg = state.registry.lock().await;
        for id in &plugin_ids {
            reg.remove(id);
        }
    }

    // Rebuild plugin_manifests + module registry + dynamic binaries
    // from the freshly-loaded plugin_reg.
    let mut new_manifests: HashMap<String, Arc<PluginManifest>> = HashMap::new();
    {
        let mut reg = state.registry.lock().await;
        let mut resolver = state.binary_resolver.lock().await;
        for loaded in plugin_reg.by_id.values() {
            let manifest = Arc::new(loaded.manifest.clone());
            resolver.register_known_dynamic(rb_core::binary::KnownBinaryEntry {
                id: manifest.binary.id.clone(),
                display_name: manifest
                    .binary
                    .display_name
                    .clone()
                    .unwrap_or_else(|| manifest.name.clone()),
                install_hint: manifest.binary.install_hint.clone().unwrap_or_else(|| {
                    format!("Install '{}' and configure its path.", manifest.binary.id)
                }),
            });
            let module: Arc<dyn rb_core::module::Module> =
                Arc::new(crate::state::LazyResolvingPluginModule::new(
                    manifest.clone(),
                    manifest.binary.id.clone(),
                    state.binary_resolver.clone(),
                ));
            reg.register(module);
            new_manifests.insert(manifest.id.clone(), manifest);
        }
    }
    *state.plugin_manifests.lock().await = new_manifests;

    let diag = PluginDiagnostics {
        loaded: plugin_reg
            .by_id
            .iter()
            .map(|(id, lp)| PluginSourceTag {
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
            .collect(),
        errors: plugin_reg
            .errors
            .iter()
            .map(|e| PluginErrorView {
                source_label: e.source_label.clone(),
                message: e.message.clone(),
            })
            .collect(),
    };
    *state.plugins.lock().await = diag.clone();
    diag
}

#[tauri::command]
pub async fn reload_plugins(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<PluginDiagnostics, String> {
    let diag = reload_plugins_impl(&state, &crate::BUNDLED_PLUGINS).await;
    let _ = app.emit("modules-changed", &serde_json::Value::Null);
    Ok(diag)
}

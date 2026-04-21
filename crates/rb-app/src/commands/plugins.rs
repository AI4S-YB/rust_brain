use serde::Serialize;
use tauri::State;

use crate::state::{AppState, PluginDiagnostics};

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

/// Stub — full implementation lands in Task 13. Returns NotImplemented for now.
#[tauri::command]
pub async fn reload_plugins(_state: State<'_, AppState>) -> Result<(), String> {
    Err("not yet implemented — see Task 13".into())
}

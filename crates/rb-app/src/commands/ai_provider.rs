use tauri::State;

use rb_ai::config::{AiConfig, ProviderConfig};

use crate::state::AppState;

#[tauri::command]
pub async fn ai_get_config(state: State<'_, AppState>) -> Result<AiConfig, String> {
    Ok(state.ai.config.lock().await.clone())
}

#[tauri::command]
pub async fn ai_set_provider_config(
    state: State<'_, AppState>,
    provider_id: String,
    config: ProviderConfig,
) -> Result<(), String> {
    let mut cfg = state.ai.config.lock().await;
    cfg.providers.insert(provider_id.clone(), config);
    if cfg.default_provider.is_none() {
        cfg.default_provider = Some(provider_id);
    }
    cfg.save(&state.ai.config_path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_set_default_provider(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<(), String> {
    let mut cfg = state.ai.config.lock().await;
    if !cfg.providers.contains_key(&provider_id) {
        return Err(format!("unknown provider {provider_id}"));
    }
    cfg.default_provider = Some(provider_id);
    cfg.save(&state.ai.config_path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_set_api_key(
    state: State<'_, AppState>,
    provider_id: String,
    key: String,
) -> Result<(), String> {
    state
        .ai
        .keystore
        .set(&provider_id, &key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_clear_api_key(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<(), String> {
    state
        .ai
        .keystore
        .clear(&provider_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_has_api_key(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<bool, String> {
    Ok(state
        .ai
        .keystore
        .get(&provider_id)
        .map_err(|e| e.to_string())?
        .is_some())
}

#[tauri::command]
pub async fn ai_backend_info(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "keystore_backend": state.ai.keystore.backend(),
        "config_path": state.ai.config_path,
    }))
}

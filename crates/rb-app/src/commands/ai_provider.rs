use tauri::State;

use rb_ai::config::{AiConfig, ProviderConfig};
use rb_ai::provider::ThinkingConfig;

use crate::state::AppState;

pub(crate) fn effective_thinking(config: &ProviderConfig) -> ThinkingConfig {
    ThinkingConfig {
        enabled: config.effective_thinking_enabled(),
        reasoning_effort: config.effective_reasoning_effort(),
    }
}

fn env_api_key_for(provider_id: &str, base_url: &str) -> Option<String> {
    let base_url = base_url.trim().to_ascii_lowercase();
    let candidates: &[&str] = if base_url.contains("api.deepseek.com") {
        &["DEEPSEEK_API_KEY"]
    } else if provider_id == "openai-compat" {
        &["OPENAI_API_KEY"]
    } else {
        &[]
    };
    candidates.iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

pub(crate) fn resolve_api_key(
    state: &AppState,
    provider_id: &str,
    base_url: &str,
) -> Result<String, String> {
    state
        .ai
        .keystore
        .get(provider_id)
        .map_err(|e| e.to_string())?
        .or_else(|| env_api_key_for(provider_id, base_url))
        .ok_or_else(|| {
            if base_url
                .trim()
                .to_ascii_lowercase()
                .contains("api.deepseek.com")
            {
                "API key not set for provider; set it in Settings or export DEEPSEEK_API_KEY"
                    .to_string()
            } else {
                "API key not set for provider".to_string()
            }
        })
}

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
    let stored = state
        .ai
        .keystore
        .get(&provider_id)
        .map_err(|e| e.to_string())?
        .is_some();
    if stored {
        return Ok(true);
    }
    let base_url = {
        let cfg = state.ai.config.lock().await;
        cfg.providers
            .get(&provider_id)
            .map(|p| p.base_url.clone())
            .unwrap_or_default()
    };
    Ok(env_api_key_for(&provider_id, &base_url).is_some())
}

#[tauri::command]
pub async fn ai_backend_info(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "keystore_backend": state.ai.keystore.backend(),
        "config_path": state.ai.config_path,
    }))
}

/// Connectivity test: sends a one-shot "say hi" message using the given
/// endpoint. If `api_key` is empty, falls back to the key stored under
/// `provider_id`. Returns the model's reply text on success.
#[tauri::command]
pub async fn ai_test_connection(
    state: State<'_, AppState>,
    provider_id: String,
    base_url: String,
    model: String,
    temperature: Option<f32>,
    thinking_enabled: Option<bool>,
    reasoning_effort: Option<String>,
    api_key: Option<String>,
) -> Result<String, String> {
    use rb_ai::provider::{
        openai_compat::OpenAiCompatProvider, ChatProvider, ChatRequest, ProviderEvent,
        ProviderMessage,
    };
    use rb_core::cancel::CancellationToken;

    let base_url = base_url.trim().to_string();
    let model = model.trim().to_string();
    if base_url.is_empty() {
        return Err("base URL is empty".into());
    }
    if model.is_empty() {
        return Err("model is empty".into());
    }

    let key = match api_key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(k) => k.to_string(),
        None => resolve_api_key(&state, &provider_id, &base_url)?,
    };

    let provider_config = ProviderConfig {
        base_url: base_url.clone(),
        model: model.clone(),
        temperature: temperature.unwrap_or(0.2),
        thinking_enabled,
        reasoning_effort,
    };
    let provider = OpenAiCompatProvider::new(base_url, key);
    let req = ChatRequest {
        model,
        system: "You are a connectivity test. Reply with a very short greeting.".into(),
        messages: vec![ProviderMessage::User {
            content: "Say hi to me in one short sentence.".into(),
        }],
        tools: vec![],
        temperature: provider_config.temperature,
        thinking: effective_thinking(&provider_config),
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<ProviderEvent>(32);
    let cancel = CancellationToken::new();
    let send_fut = provider.send(req, tx, cancel);
    let collect_fut = async move {
        let mut out = String::new();
        while let Some(ev) = rx.recv().await {
            if let ProviderEvent::TextDelta(s) = ev {
                out.push_str(&s);
            }
        }
        out
    };
    let (send_res, text) = tokio::join!(send_fut, collect_fut);
    send_res.map_err(|e| e.to_string())?;
    let reply = text.trim().to_string();
    if reply.is_empty() {
        return Err("empty reply from provider".into());
    }
    Ok(reply)
}

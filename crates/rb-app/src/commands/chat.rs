use std::sync::Arc;

use futures_util::future::BoxFuture;
use rb_ai::error::AiError;
use rb_ai::orchestrator::{run_turn, ChatStreamEvent, OrchestratorCtx, SaveFn};
use rb_ai::provider::openai_compat::OpenAiCompatProvider;
use rb_ai::provider::ChatProvider;
use rb_ai::session::store::{SessionIndex, SessionStore};
use rb_ai::session::ChatSession;
use rb_core::cancel::CancellationToken;
use rb_core::runner::Runner;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::state::AppState;

/// Locate the SessionStore for the currently-open project.
/// Returns an error if no project is open yet.
async fn session_store_for(state: &AppState) -> Result<SessionStore, String> {
    let runner: Arc<Runner> = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no open project".to_string())?
            .clone()
    };
    let root = {
        let proj = runner.project().lock().await;
        proj.root_dir.clone()
    };
    Ok(SessionStore::new(&root))
}

#[tauri::command]
pub async fn chat_list_sessions(state: State<'_, AppState>) -> Result<SessionIndex, String> {
    let store = session_store_for(&state).await?;
    store.list().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_create_session(
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<ChatSession, String> {
    let store = session_store_for(&state).await?;
    let id = SessionStore::generate_session_id();
    let session = ChatSession::new(id, title.unwrap_or_else(|| "New chat".into()), None);
    store
        .save_session(&session)
        .await
        .map_err(|e| e.to_string())?;
    Ok(session)
}

#[tauri::command]
pub async fn chat_get_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ChatSession, String> {
    let store = session_store_for(&state).await?;
    store
        .load_session(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_delete_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let store = session_store_for(&state).await?;
    store
        .delete_session(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_rename_session(
    state: State<'_, AppState>,
    session_id: String,
    title: String,
) -> Result<(), String> {
    let store = session_store_for(&state).await?;
    store
        .rename_session(&session_id, title)
        .await
        .map_err(|e| e.to_string())
}

/// Resolve the app's configured provider + API key into a ready-to-use ChatProvider.
///
/// Returns (provider, model, temperature) so the orchestrator can populate the
/// ChatRequest without re-reading config each turn.
async fn acquire_provider(
    state: &AppState,
) -> Result<(Arc<dyn ChatProvider>, String, f32), String> {
    let cfg = state.ai.config.lock().await.clone();
    let provider_id = cfg
        .default_provider
        .clone()
        .ok_or_else(|| "no default provider configured".to_string())?;
    let pc = cfg
        .providers
        .get(&provider_id)
        .ok_or_else(|| format!("provider {provider_id} not found in config"))?
        .clone();
    let key = state
        .ai
        .keystore
        .get(&provider_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "API key not set for provider".to_string())?;
    let provider: Arc<dyn ChatProvider> = Arc::new(OpenAiCompatProvider::new(pc.base_url, key));
    Ok((provider, pc.model, pc.temperature))
}

/// Pick the language for this turn. Phase 1 reads RUSTBRAIN_LANG env var;
/// later the frontend will pass it per command.
fn language_for() -> String {
    std::env::var("RUSTBRAIN_LANG").unwrap_or_else(|_| "en".into())
}

/// Build a SaveFn closure that persists the session via `store.save_session`.
/// Captured values must be `Send + Sync + 'static`, which is why we clone the
/// store (it's cheap — just a PathBuf inside).
fn make_save_fn(store: SessionStore) -> SaveFn {
    Arc::new(move |snapshot: ChatSession| {
        let store = store.clone();
        let fut: BoxFuture<'static, Result<(), AiError>> =
            Box::pin(async move { store.save_session(&snapshot).await });
        fut
    })
}

#[tauri::command]
pub async fn chat_send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    text: String,
) -> Result<(), String> {
    let store = session_store_for(&state).await?;
    let session = store
        .load_session(&session_id)
        .await
        .map_err(|e| e.to_string())?;
    let session = Arc::new(Mutex::new(session));

    let (provider, model, temperature) = acquire_provider(&state).await?;

    let runner: Arc<Runner> = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no open project".to_string())?
            .clone()
    };

    let lang = language_for();
    let tools = state
        .ai
        .tools_by_lang
        .get(&lang)
        .cloned()
        .or_else(|| state.ai.tools_by_lang.get("en").cloned())
        .ok_or_else(|| "no tool registry registered".to_string())?;

    let ctx = OrchestratorCtx {
        project: runner.project().clone(),
        runner: runner.clone(),
        binary_resolver: state.binary_resolver.clone(),
        tools,
        provider,
        model,
        temperature,
        plans: state.ai.plans.clone(),
        lang,
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<ChatStreamEvent>(64);
    let cancel = CancellationToken::new();
    state
        .ai
        .active_turns
        .lock()
        .await
        .insert(session_id.clone(), cancel.clone());

    // Forward orchestrator events to the webview as Tauri events.
    let app_for_emit = app.clone();
    let sid_for_emit = session_id.clone();
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let _ = app_for_emit.emit("chat-stream", &ev);
        }
        let _ = app_for_emit.emit(
            "chat-session-updated",
            serde_json::json!({ "session_id": sid_for_emit }),
        );
    });

    // Run the turn on a background task so the command returns immediately.
    // (Frontend gets state via chat-stream events, not via the return value.)
    let save_fn = make_save_fn(store);
    let ai_state = state.ai.clone();
    let sid_for_cleanup = session_id.clone();
    tokio::spawn(async move {
        let res = run_turn(&ctx, session, text, tx, cancel, save_fn).await;
        ai_state.active_turns.lock().await.remove(&sid_for_cleanup);
        if let Err(e) = res {
            tracing::warn!("chat run_turn ended with error: {e:?}");
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn chat_approve_tool(
    state: State<'_, AppState>,
    call_id: String,
    edited_args: Option<serde_json::Value>,
) -> Result<(), String> {
    state
        .ai
        .plans
        .approve(&call_id, edited_args)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_reject_tool(
    state: State<'_, AppState>,
    call_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    state
        .ai
        .plans
        .reject(&call_id, reason)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_cancel_turn(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    if let Some(token) = state.ai.active_turns.lock().await.remove(&session_id) {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn chat_cancel_run(state: State<'_, AppState>, run_id: String) -> Result<(), String> {
    let runner: Arc<Runner> = {
        let guard = state.runner.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "no open project".to_string())?
            .clone()
    };
    runner.cancel(&run_id).await;
    Ok(())
}

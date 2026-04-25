use std::sync::Arc;

use futures_util::future::BoxFuture;
use rb_ai::error::AiError;
use rb_ai::orchestrator::{run_turn, ChatStreamEvent, OrchestratorCtx, SaveFn};
use rb_ai::provider::openai_compat::OpenAiCompatProvider;
use rb_ai::provider::{
    ChatProvider, ChatRequest, FinishReason, ProviderEvent, ProviderMessage, ProviderToolCall,
};
use rb_ai::session::store::{SessionIndex, SessionStore};
use rb_ai::session::{ChatSession, Message};
use rb_core::cancel::CancellationToken;
use rb_core::runner::Runner;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::state::AppState;

use super::ai_provider::{effective_thinking, resolve_api_key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatScope {
    Project,
    Direct,
}

impl ChatScope {
    fn parse(scope: Option<String>) -> Result<Self, String> {
        match scope.as_deref().unwrap_or("project") {
            "project" => Ok(Self::Project),
            "direct" => Ok(Self::Direct),
            other => Err(format!("unknown chat scope: {other}")),
        }
    }

    fn key_prefix(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Direct => "direct",
        }
    }
}

fn active_turn_key(scope: ChatScope, session_id: &str) -> String {
    format!("{}:{session_id}", scope.key_prefix())
}

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

fn direct_session_store() -> Result<SessionStore, String> {
    let root = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("rustbrain")
        .join("direct-chats");
    Ok(SessionStore::at(root))
}

async fn session_store_for_scope(
    state: &AppState,
    scope: ChatScope,
) -> Result<SessionStore, String> {
    match scope {
        ChatScope::Project => session_store_for(state).await,
        ChatScope::Direct => direct_session_store(),
    }
}

#[tauri::command]
pub async fn chat_list_sessions(
    state: State<'_, AppState>,
    scope: Option<String>,
) -> Result<SessionIndex, String> {
    let scope = ChatScope::parse(scope)?;
    let store = session_store_for_scope(&state, scope).await?;
    store.list().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_create_session(
    state: State<'_, AppState>,
    title: Option<String>,
    scope: Option<String>,
) -> Result<ChatSession, String> {
    let scope = ChatScope::parse(scope)?;
    let store = session_store_for_scope(&state, scope).await?;
    let id = SessionStore::generate_session_id();
    let default_title = match scope {
        ChatScope::Project => "New project chat",
        ChatScope::Direct => "New direct chat",
    };
    let session = ChatSession::new(id, title.unwrap_or_else(|| default_title.into()), None);
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
    scope: Option<String>,
) -> Result<ChatSession, String> {
    let scope = ChatScope::parse(scope)?;
    let store = session_store_for_scope(&state, scope).await?;
    store
        .load_session(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_delete_session(
    state: State<'_, AppState>,
    session_id: String,
    scope: Option<String>,
) -> Result<(), String> {
    let scope = ChatScope::parse(scope)?;
    let store = session_store_for_scope(&state, scope).await?;
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
    scope: Option<String>,
) -> Result<(), String> {
    let scope = ChatScope::parse(scope)?;
    let store = session_store_for_scope(&state, scope).await?;
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
) -> Result<
    (
        Arc<dyn ChatProvider>,
        String,
        f32,
        rb_ai::provider::ThinkingConfig,
    ),
    String,
> {
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
    let key = resolve_api_key(state, &provider_id, &pc.base_url)?;
    let thinking = effective_thinking(&pc);
    let provider: Arc<dyn ChatProvider> =
        Arc::new(OpenAiCompatProvider::new(pc.base_url.clone(), key));
    Ok((provider, pc.model, pc.temperature, thinking))
}

/// Pick the language for this turn. Phase 1 reads RUSTBRAIN_LANG env var;
/// later the frontend will pass it per command.
fn language_for() -> String {
    std::env::var("RUSTBRAIN_LANG").unwrap_or_else(|_| "en".into())
}

fn direct_system_prompt(lang: &str) -> &'static str {
    match lang {
        "zh" => {
            "你是 RustBrain 的通用 AI 助手。用户当前没有绑定项目上下文。直接回答问题；不要声称你能查看项目文件、样本、运行记录或启动分析任务。若用户需要项目数据分析，请建议切换到项目 AI 模式并选择或打开项目。"
        }
        _ => {
            "You are RustBrain's general AI assistant. The user is not bound to a project context. Answer directly; do not claim you can inspect project files, samples, runs, or start analyses. If the user needs project data analysis, suggest switching to Project AI mode and selecting or opening a project."
        }
    }
}

fn direct_to_provider_messages(messages: &[Message]) -> Vec<ProviderMessage> {
    messages
        .iter()
        .filter_map(|m| match m {
            Message::User { content } => Some(ProviderMessage::User {
                content: content.clone(),
            }),
            Message::Assistant {
                content,
                reasoning_content,
                tool_calls,
                ..
            } => Some(ProviderMessage::Assistant {
                content: content.clone(),
                reasoning_content: reasoning_content.clone(),
                tool_calls: tool_calls
                    .iter()
                    .map(|tc| ProviderToolCall {
                        id: tc.call_id.clone(),
                        name: tc.name.clone(),
                        args: tc.args.clone(),
                    })
                    .collect(),
            }),
            Message::Tool { .. } => None,
        })
        .collect()
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
    scope: Option<String>,
) -> Result<(), String> {
    let scope = ChatScope::parse(scope)?;
    if scope == ChatScope::Direct {
        return chat_send_direct_message(app, &state, session_id, text).await;
    }

    let store = session_store_for_scope(&state, scope).await?;
    let session = store
        .load_session(&session_id)
        .await
        .map_err(|e| e.to_string())?;
    let session = Arc::new(Mutex::new(session));

    let (provider, model, temperature, thinking) = acquire_provider(&state).await?;

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
        thinking,
        plans: state.ai.plans.clone(),
        lang,
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<ChatStreamEvent>(64);
    let cancel = CancellationToken::new();
    let turn_key = active_turn_key(scope, &session_id);
    state
        .ai
        .active_turns
        .lock()
        .await
        .insert(turn_key.clone(), cancel.clone());

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
    tokio::spawn(async move {
        let res = run_turn(&ctx, session, text, tx, cancel, save_fn).await;
        ai_state.active_turns.lock().await.remove(&turn_key);
        if let Err(e) = res {
            tracing::warn!("chat run_turn ended with error: {e:?}");
        }
    });

    Ok(())
}

async fn chat_send_direct_message(
    app: AppHandle,
    state: &AppState,
    session_id: String,
    text: String,
) -> Result<(), String> {
    let scope = ChatScope::Direct;
    let store = direct_session_store()?;
    let session = store
        .load_session(&session_id)
        .await
        .map_err(|e| e.to_string())?;
    let session = Arc::new(Mutex::new(session));
    let (provider, model, temperature, thinking) = acquire_provider(state).await?;
    let lang = language_for();
    let system = direct_system_prompt(&lang).to_string();

    {
        let mut s = session.lock().await;
        s.messages.push(Message::User { content: text });
        s.updated_at = chrono::Utc::now();
        let snapshot = s.clone();
        drop(s);
        store
            .save_session(&snapshot)
            .await
            .map_err(|e| e.to_string())?;
    }

    let req = {
        let s = session.lock().await;
        ChatRequest {
            model,
            system,
            messages: direct_to_provider_messages(&s.messages),
            tools: vec![],
            temperature,
            thinking,
        }
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<ChatStreamEvent>(64);
    let cancel = CancellationToken::new();
    let turn_key = active_turn_key(scope, &session_id);
    state
        .ai
        .active_turns
        .lock()
        .await
        .insert(turn_key.clone(), cancel.clone());

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

    let ai_state = state.ai.clone();
    let save_fn = make_save_fn(store);
    tokio::spawn(async move {
        let res = run_direct_turn(
            provider,
            req,
            session,
            session_id.clone(),
            tx,
            cancel,
            save_fn,
        )
        .await;
        ai_state.active_turns.lock().await.remove(&turn_key);
        if let Err(e) = res {
            tracing::warn!("direct chat turn ended with error: {e:?}");
        }
    });

    Ok(())
}

async fn run_direct_turn(
    provider: Arc<dyn ChatProvider>,
    req: ChatRequest,
    session: Arc<Mutex<ChatSession>>,
    session_id: String,
    sink: tokio::sync::mpsc::Sender<ChatStreamEvent>,
    cancel: CancellationToken,
    store_save: SaveFn,
) -> Result<(), AiError> {
    let (p_tx, mut p_rx) = tokio::sync::mpsc::channel::<ProviderEvent>(32);
    let provider_handle = tokio::spawn(async move { provider.send(req, p_tx, cancel).await });
    let mut text_buf = String::new();
    let mut reasoning_buf = String::new();
    let mut finish: Option<FinishReason> = None;

    while let Some(ev) = p_rx.recv().await {
        match ev {
            ProviderEvent::TextDelta(s) => {
                text_buf.push_str(&s);
                let _ = sink
                    .send(ChatStreamEvent::Text {
                        session_id: session_id.clone(),
                        delta: s,
                    })
                    .await;
            }
            ProviderEvent::ReasoningDelta(s) => {
                reasoning_buf.push_str(&s);
                let _ = sink
                    .send(ChatStreamEvent::Reasoning {
                        session_id: session_id.clone(),
                        delta: s,
                    })
                    .await;
            }
            ProviderEvent::ToolCall { name, .. } => {
                let _ = sink
                    .send(ChatStreamEvent::Error {
                        session_id: session_id.clone(),
                        message: format!(
                            "direct chat does not support project tool calls ({name})"
                        ),
                    })
                    .await;
            }
            ProviderEvent::Finish(r) => finish = Some(r),
        }
    }

    match provider_handle.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let _ = sink
                .send(ChatStreamEvent::Error {
                    session_id: session_id.clone(),
                    message: format!("{e}"),
                })
                .await;
            return Err(AiError::Provider(format!("{e}")));
        }
        Err(e) => {
            let _ = sink
                .send(ChatStreamEvent::Error {
                    session_id: session_id.clone(),
                    message: format!("provider join: {e}"),
                })
                .await;
            return Err(AiError::Provider(format!("join: {e}")));
        }
    }

    {
        let mut s = session.lock().await;
        s.messages.push(Message::Assistant {
            content: text_buf,
            reasoning_content: if reasoning_buf.is_empty() {
                None
            } else {
                Some(reasoning_buf)
            },
            tool_calls: vec![],
            interrupted: false,
        });
        s.updated_at = chrono::Utc::now();
        let snapshot = s.clone();
        drop(s);
        store_save(snapshot).await?;
    }

    match finish {
        Some(FinishReason::Length) => {
            let _ = sink
                .send(ChatStreamEvent::Error {
                    session_id,
                    message: "response truncated by model length limit".into(),
                })
                .await;
        }
        Some(FinishReason::Error(e)) => {
            let _ = sink
                .send(ChatStreamEvent::Error {
                    session_id,
                    message: e,
                })
                .await;
        }
        _ => {
            let _ = sink.send(ChatStreamEvent::Done { session_id }).await;
        }
    }
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
    scope: Option<String>,
) -> Result<(), String> {
    let scope = ChatScope::parse(scope)?;
    let key = active_turn_key(scope, &session_id);
    if let Some(token) = state.ai.active_turns.lock().await.remove(&key) {
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

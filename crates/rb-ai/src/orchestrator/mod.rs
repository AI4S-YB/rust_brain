pub mod plan_card;
pub mod prompt;
pub mod snapshot;

use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::project::Project;
use rb_core::runner::Runner;

use crate::error::AiError;
use crate::provider::{
    ChatProvider, ChatRequest, FinishReason, ProviderEvent, ProviderMessage, ProviderToolCall,
    ThinkingConfig,
};
use crate::session::{ChatSession, Message, ToolCall};
use crate::tools::{RiskLevel, ToolContext, ToolOutput, ToolRegistry};

pub use plan_card::{PlanCardRegistry, PlanDecision};

/// Events streamed to the frontend (serialized as Tauri events).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind")]
pub enum ChatStreamEvent {
    Text {
        session_id: String,
        delta: String,
    },
    Reasoning {
        session_id: String,
        delta: String,
    },
    ToolCall {
        session_id: String,
        call_id: String,
        name: String,
        risk: String,
        args: serde_json::Value,
        status: String,
    },
    ToolResult {
        session_id: String,
        call_id: String,
        result: serde_json::Value,
    },
    Done {
        session_id: String,
    },
    Error {
        session_id: String,
        message: String,
    },
}

pub struct OrchestratorCtx {
    pub project: Arc<Mutex<Project>>,
    pub runner: Arc<Runner>,
    pub binary_resolver: Arc<Mutex<BinaryResolver>>,
    pub tools: Arc<ToolRegistry>,
    pub provider: Arc<dyn ChatProvider>,
    pub model: String,
    pub temperature: f32,
    pub thinking: ThinkingConfig,
    pub plans: PlanCardRegistry,
    pub lang: String,
}

pub type SaveFn = Arc<
    dyn Fn(ChatSession) -> futures_util::future::BoxFuture<'static, Result<(), AiError>>
        + Send
        + Sync,
>;

/// Drive one user turn to completion. Appends the user message, calls the
/// provider, dispatches each tool call by risk level, and persists the
/// session at each state change via the caller-provided save function.
pub async fn run_turn(
    ctx: &OrchestratorCtx,
    session: Arc<Mutex<ChatSession>>,
    user_text: String,
    sink: mpsc::Sender<ChatStreamEvent>,
    cancel: CancellationToken,
    store_save: SaveFn,
) -> Result<(), AiError> {
    // 1. Append the user message and persist.
    {
        let mut s = session.lock().await;
        s.messages.push(Message::User { content: user_text });
        s.updated_at = chrono::Utc::now();
        let snapshot = s.clone();
        drop(s);
        store_save(snapshot).await?;
    }

    let session_id = {
        let s = session.lock().await;
        s.id.clone()
    };

    loop {
        if cancel.is_cancelled() {
            let _ = sink
                .send(ChatStreamEvent::Error {
                    session_id: session_id.clone(),
                    message: "cancelled".into(),
                })
                .await;
            return Err(AiError::Cancelled);
        }

        // 2. Build the ChatRequest for this iteration.
        let snap = snapshot::build(&ctx.project).await;
        let system = prompt::compose(&ctx.lang, &snap);
        let provider_msgs = to_provider_messages(&session.lock().await.messages);
        let req = ChatRequest {
            model: ctx.model.clone(),
            system,
            messages: provider_msgs,
            tools: ctx.tools.all_for_ai(),
            temperature: ctx.temperature,
            thinking: ctx.thinking.clone(),
        };

        // 3. Drive the provider and collect streamed events.
        let (p_tx, mut p_rx) = mpsc::channel::<ProviderEvent>(32);
        let provider = ctx.provider.clone();
        let cancel_for_prov = cancel.clone();
        let sink_for_text = sink.clone();
        let sid_for_text = session_id.clone();

        let prov_handle =
            tokio::spawn(async move { provider.send(req, p_tx, cancel_for_prov).await });

        let mut text_buf = String::new();
        let mut reasoning_buf = String::new();
        let mut tool_calls: Vec<ProviderToolCall> = vec![];
        let mut finish: Option<FinishReason> = None;

        while let Some(ev) = p_rx.recv().await {
            match ev {
                ProviderEvent::TextDelta(s) => {
                    text_buf.push_str(&s);
                    let _ = sink_for_text
                        .send(ChatStreamEvent::Text {
                            session_id: sid_for_text.clone(),
                            delta: s,
                        })
                        .await;
                }
                ProviderEvent::ReasoningDelta(s) => {
                    reasoning_buf.push_str(&s);
                    let _ = sink_for_text
                        .send(ChatStreamEvent::Reasoning {
                            session_id: sid_for_text.clone(),
                            delta: s,
                        })
                        .await;
                }
                ProviderEvent::ToolCall { id, name, args } => {
                    tool_calls.push(ProviderToolCall { id, name, args });
                }
                ProviderEvent::Finish(r) => {
                    finish = Some(r);
                }
            }
        }
        let prov_result = prov_handle.await;
        match prov_result {
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

        // 4. Persist the assistant message (with any tool calls).
        let call_list: Vec<ToolCall> = tool_calls
            .iter()
            .map(|tc| ToolCall {
                call_id: tc.id.clone(),
                name: tc.name.clone(),
                args: tc.args.clone(),
            })
            .collect();
        let interrupted = cancel.is_cancelled();
        {
            let mut s = session.lock().await;
            s.messages.push(Message::Assistant {
                content: text_buf.clone(),
                reasoning_content: if reasoning_buf.is_empty() {
                    None
                } else {
                    Some(reasoning_buf.clone())
                },
                tool_calls: call_list.clone(),
                interrupted,
            });
            s.updated_at = chrono::Utc::now();
            let snap = s.clone();
            drop(s);
            store_save(snap).await?;
        }

        if tool_calls.is_empty() {
            let _ = sink
                .send(ChatStreamEvent::Done {
                    session_id: session_id.clone(),
                })
                .await;
            return Ok(());
        }

        // 5. Dispatch each tool call by risk.
        for tc in tool_calls {
            let risk = match ctx.tools.get(&tc.name) {
                Some(entry) => entry.def.risk,
                None => {
                    let result =
                        serde_json::json!({ "error": format!("unknown tool: {}", tc.name) });
                    push_tool_result(
                        &session,
                        &tc.id,
                        &tc.name,
                        result,
                        &sink,
                        &session_id,
                        &store_save,
                    )
                    .await?;
                    continue;
                }
            };
            let risk_s = match risk {
                RiskLevel::Read => "read",
                RiskLevel::RunLow => "run_low",
                RiskLevel::RunMid => "run_mid",
                RiskLevel::Destructive => "destructive",
            };
            let _ = sink
                .send(ChatStreamEvent::ToolCall {
                    session_id: session_id.clone(),
                    call_id: tc.id.clone(),
                    name: tc.name.clone(),
                    risk: risk_s.to_string(),
                    args: tc.args.clone(),
                    status: match risk {
                        RiskLevel::Read => "running".into(),
                        _ => "pending".into(),
                    },
                })
                .await;

            let resolved_args = match risk {
                RiskLevel::Read => tc.args.clone(),
                RiskLevel::RunLow | RiskLevel::RunMid => {
                    let rx = ctx.plans.register(tc.id.clone()).await;
                    let decision = rx
                        .await
                        .map_err(|_| AiError::InvalidState("plan waiter dropped".into()))?;
                    match decision {
                        PlanDecision::Approve { edited_args } => {
                            edited_args.unwrap_or_else(|| tc.args.clone())
                        }
                        PlanDecision::Reject { reason } => {
                            let result = serde_json::json!({
                                "error": "rejected_by_user",
                                "reason": reason.unwrap_or_default(),
                            });
                            push_tool_result(
                                &session,
                                &tc.id,
                                &tc.name,
                                result,
                                &sink,
                                &session_id,
                                &store_save,
                            )
                            .await?;
                            continue;
                        }
                    }
                }
                RiskLevel::Destructive => {
                    let result = serde_json::json!({
                        "error": "destructive tools are disabled in Phase 1",
                    });
                    push_tool_result(
                        &session,
                        &tc.id,
                        &tc.name,
                        result,
                        &sink,
                        &session_id,
                        &store_save,
                    )
                    .await?;
                    continue;
                }
            };

            // Execute the tool. Look up entry again because we can't hold the
            // borrow across awaits above.
            let entry = match ctx.tools.get(&tc.name) {
                Some(e) => e,
                None => {
                    let result =
                        serde_json::json!({ "error": format!("unknown tool: {}", tc.name) });
                    push_tool_result(
                        &session,
                        &tc.id,
                        &tc.name,
                        result,
                        &sink,
                        &session_id,
                        &store_save,
                    )
                    .await?;
                    continue;
                }
            };
            let exec_ctx = ToolContext {
                project: &ctx.project,
                runner: &ctx.runner,
                binary_resolver: &ctx.binary_resolver,
            };
            let exec_result = entry.executor.execute(&resolved_args, exec_ctx).await;
            let result_value = match exec_result {
                Ok(ToolOutput::Value(v)) => v,
                Err(e) => serde_json::json!({ "error": e.to_string() }),
            };
            push_tool_result(
                &session,
                &tc.id,
                &tc.name,
                result_value,
                &sink,
                &session_id,
                &store_save,
            )
            .await?;
        }

        match finish {
            Some(FinishReason::ToolCalls) | None => continue,
            Some(FinishReason::Stop) => {
                let _ = sink
                    .send(ChatStreamEvent::Done {
                        session_id: session_id.clone(),
                    })
                    .await;
                return Ok(());
            }
            Some(FinishReason::Length) => {
                let _ = sink
                    .send(ChatStreamEvent::Error {
                        session_id: session_id.clone(),
                        message: "response truncated by model length limit".into(),
                    })
                    .await;
                return Ok(());
            }
            Some(FinishReason::Error(e)) => {
                let _ = sink
                    .send(ChatStreamEvent::Error {
                        session_id: session_id.clone(),
                        message: e,
                    })
                    .await;
                return Ok(());
            }
        }
    }
}

async fn push_tool_result(
    session: &Arc<Mutex<ChatSession>>,
    call_id: &str,
    name: &str,
    result: serde_json::Value,
    sink: &mpsc::Sender<ChatStreamEvent>,
    session_id: &str,
    store_save: &SaveFn,
) -> Result<(), AiError> {
    {
        let mut s = session.lock().await;
        s.messages.push(Message::Tool {
            call_id: call_id.into(),
            name: name.into(),
            result: result.clone(),
        });
        s.updated_at = chrono::Utc::now();
        let snap = s.clone();
        drop(s);
        store_save(snap).await?;
    }
    let _ = sink
        .send(ChatStreamEvent::ToolResult {
            session_id: session_id.into(),
            call_id: call_id.into(),
            result,
        })
        .await;
    Ok(())
}

fn to_provider_messages(messages: &[Message]) -> Vec<ProviderMessage> {
    messages
        .iter()
        .map(|m| match m {
            Message::User { content } => ProviderMessage::User {
                content: content.clone(),
            },
            Message::Assistant {
                content,
                reasoning_content,
                tool_calls,
                ..
            } => ProviderMessage::Assistant {
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
            },
            Message::Tool {
                call_id,
                name,
                result,
            } => ProviderMessage::Tool {
                call_id: call_id.clone(),
                name: name.clone(),
                result: serde_json::to_string(result).unwrap_or_else(|_| "{}".into()),
            },
        })
        .collect()
}

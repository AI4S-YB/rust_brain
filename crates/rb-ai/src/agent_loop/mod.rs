//! Perceive→reason→execute→record main loop.

pub mod execute;
pub mod perceive;
pub mod reason;
pub mod record;
pub mod types;

pub use execute::{execute_call, ApprovalVerdict, ExecCtx};
pub use record::{finalize, fsync_checkpoint};
pub use types::{AgentEvent, AgentSession, SharedSession};

// run_session: orchestrate perceive → reason → execute → record.
//
// The caller hands us:
// - the project + runner + binary_resolver (rb_core),
// - a built ToolRegistry (with builtin + module_derived + skill tools),
// - a SandboxPolicy,
// - a Recaller (BM25 fallback, optionally Composite with Flash),
// - a ChatProvider,
// - mpsc senders for AgentEvent and AskUserRequest, and a receiver for
//   ApprovalVerdict.
//
// The session runs until the agent emits `task_done`, until an unrecoverable
// error, or until cancelled.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::error::AiError;
use crate::memory::layers::ArchiveOutcome;
use crate::memory::recall::Recaller;
use crate::memory::MemoryStore;
use crate::provider::{
    ChatProvider, FinishReason, ProviderMessage, ProviderToolCall, ThinkingConfig,
};
use crate::sandbox::policy::SandboxPolicy;
use crate::sandbox::NetLogger;
use crate::tools::{AskUserRequest, ToolRegistry};

use self::perceive::{perceive, PerceiveCtx};
use self::reason::reason;

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub model: String,
    pub temperature: f32,
    pub thinking: ThinkingConfig,
    pub recall_budget_tokens: usize,
    pub max_consecutive_failures: u32,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".into(),
            temperature: 0.2,
            thinking: ThinkingConfig::default(),
            recall_budget_tokens: 4096,
            max_consecutive_failures: 5,
        }
    }
}

pub struct RunSessionCtx {
    pub project: Arc<Mutex<rb_core::project::Project>>,
    pub runner: Arc<rb_core::runner::Runner>,
    pub binary_resolver: Arc<Mutex<rb_core::binary::BinaryResolver>>,
    pub registry: Arc<ToolRegistry>,
    pub policy: Arc<SandboxPolicy>,
    pub memory: Arc<MemoryStore>,
    pub recaller: Arc<dyn Recaller>,
    pub provider: Arc<dyn ChatProvider>,
    pub net_log: Arc<NetLogger>,
    pub project_root: std::path::PathBuf,
    pub config: RunConfig,
}

pub async fn run_session(
    ctx: RunSessionCtx,
    user_text: String,
    session: SharedSession,
    event_sink: mpsc::Sender<AgentEvent>,
    ask_user_tx: mpsc::Sender<AskUserRequest>,
    approval_rx: Arc<Mutex<mpsc::Receiver<(String, ApprovalVerdict)>>>,
    cancel: CancellationToken,
) -> Result<(), AiError> {
    // 1. Append the user message.
    {
        let mut s = session.lock().await;
        s.messages
            .push(serde_json::json!({"role":"user","content":user_text.clone()}));
    }
    let session_id = session.lock().await.id.clone();

    // Outer perceive — recall once per session-start (not per turn) keeps
    // determinism. Per-turn updates are achievable via recall_memory tool.
    let proj_summary = orchestrator_compat::project_summary(&ctx.project).await;
    let perceive_out = perceive(
        &ctx.memory,
        Some(&ctx.project_root),
        ctx.recaller.clone(),
        &PerceiveCtx {
            user_text: user_text.clone(),
            project_summary: proj_summary.clone(),
        },
        ctx.config.recall_budget_tokens,
    )
    .await?;
    let _ = event_sink
        .send(AgentEvent::Memory {
            session_id: session_id.clone(),
            recalled: perceive_out
                .recalled
                .iter()
                .map(|c| serde_json::to_value(c).unwrap())
                .collect(),
        })
        .await;

    let system_prompt = perceive_out.system_prompt;

    // 2. Loop.
    let mut consecutive_fail: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    loop {
        if cancel.is_cancelled() {
            finalize(
                &ctx.memory,
                &ctx.project_root,
                &*session.lock().await,
                "cancelled".into(),
                vec!["cancelled".into()],
                ArchiveOutcome::Cancelled,
                ctx.net_log.path().map(|p| p.display().to_string()),
            )
            .await?;
            let _ = event_sink
                .send(AgentEvent::Error {
                    session_id: session_id.clone(),
                    message: "cancelled".into(),
                })
                .await;
            return Err(AiError::Cancelled);
        }

        let history = to_provider_messages(&session.lock().await.messages);
        let tools = ctx.registry.all_for_ai();
        let r = reason(
            ctx.provider.clone(),
            &ctx.config.model,
            system_prompt.clone(),
            history,
            tools,
            ctx.config.temperature,
            ctx.config.thinking.clone(),
            cancel.clone(),
            event_sink.clone(),
            &session_id,
        )
        .await?;

        // Append assistant message.
        {
            let mut s = session.lock().await;
            s.messages.push(serde_json::json!({
                "role":"assistant",
                "content": r.text,
                "reasoning_content": r.reasoning,
                "tool_calls": r.tool_calls,
            }));
        }

        if r.tool_calls.is_empty() {
            // Natural stop without task_done — finalize with Done outcome.
            finalize(
                &ctx.memory,
                &ctx.project_root,
                &*session.lock().await,
                "session ended without task_done".into(),
                vec![],
                ArchiveOutcome::Done,
                ctx.net_log.path().map(|p| p.display().to_string()),
            )
            .await?;
            let _ = event_sink
                .send(AgentEvent::Done {
                    session_id: session_id.clone(),
                })
                .await;
            return Ok(());
        }

        let mut got_task_done: Option<(String, Vec<String>)> = None;
        for call in r.tool_calls {
            // Special-case: task_done becomes a finalize.
            if call.name == "task_done" {
                let headline = call
                    .args
                    .get("headline")
                    .and_then(|v| v.as_str())
                    .unwrap_or("done")
                    .to_string();
                let tags: Vec<String> = call
                    .args
                    .get("tags")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                got_task_done = Some((headline, tags));
                push_tool_result(
                    &session,
                    &call.id,
                    &call.name,
                    serde_json::json!({"task_done": true}),
                )
                .await;
                let _ = event_sink
                    .send(AgentEvent::ToolResult {
                        session_id: session_id.clone(),
                        call_id: call.id.clone(),
                        result: serde_json::json!({"task_done": true}),
                    })
                    .await;
                continue;
            }

            let result = execute_call(
                ExecCtx {
                    policy: &ctx.policy,
                    registry: &ctx.registry,
                    project: &ctx.project,
                    runner: &ctx.runner,
                    binary_resolver: &ctx.binary_resolver,
                    memory: Some(&ctx.memory),
                    session_id: &session_id,
                    project_root: Some(&ctx.project_root),
                    ask_user_tx: Some(&ask_user_tx),
                    approval_rx: &approval_rx,
                    event_sink: &event_sink,
                },
                ProviderToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    args: call.args.clone(),
                },
            )
            .await?;

            // Track consecutive failures.
            if result.get("error").is_some() {
                let n = consecutive_fail.entry(call.name.clone()).or_insert(0);
                *n += 1;
                if *n >= ctx.config.max_consecutive_failures {
                    finalize(
                        &ctx.memory,
                        &ctx.project_root,
                        &*session.lock().await,
                        format!("aborted: {} kept failing", call.name),
                        vec!["failed".into()],
                        ArchiveOutcome::Failed,
                        ctx.net_log.path().map(|p| p.display().to_string()),
                    )
                    .await?;
                    let _ = event_sink
                        .send(AgentEvent::Error {
                            session_id: session_id.clone(),
                            message: format!("{} failed {} times in a row", call.name, n),
                        })
                        .await;
                    return Err(AiError::Tool(format!("{} failed {} times", call.name, n)));
                }
            } else {
                consecutive_fail.remove(&call.name);
            }

            push_tool_result(&session, &call.id, &call.name, result).await;
        }

        // After tool execution, fsync checkpoint.
        fsync_checkpoint(
            &ctx.memory,
            &ctx.project_root,
            &*session.lock().await,
            &system_prompt,
        )
        .await?;

        if let Some((headline, tags)) = got_task_done {
            finalize(
                &ctx.memory,
                &ctx.project_root,
                &*session.lock().await,
                headline,
                tags,
                ArchiveOutcome::Done,
                ctx.net_log.path().map(|p| p.display().to_string()),
            )
            .await?;
            let _ = event_sink
                .send(AgentEvent::Done {
                    session_id: session_id.clone(),
                })
                .await;
            return Ok(());
        }

        match r.finish {
            Some(FinishReason::Stop) | None | Some(FinishReason::ToolCalls) => continue,
            Some(FinishReason::Length) => {
                let _ = event_sink
                    .send(AgentEvent::Error {
                        session_id: session_id.clone(),
                        message: "model length limit hit".into(),
                    })
                    .await;
                return Ok(());
            }
            Some(FinishReason::Error(e)) => {
                let _ = event_sink
                    .send(AgentEvent::Error {
                        session_id: session_id.clone(),
                        message: e,
                    })
                    .await;
                return Ok(());
            }
        }
    }
}

async fn push_tool_result(session: &SharedSession, call_id: &str, name: &str, result: Value) {
    let mut s = session.lock().await;
    s.messages.push(serde_json::json!({
        "role":"tool",
        "tool_call_id": call_id,
        "name": name,
        "content": result,
    }));
}

fn to_provider_messages(messages: &[Value]) -> Vec<ProviderMessage> {
    messages
        .iter()
        .filter_map(|m| {
            let role = m.get("role").and_then(|v| v.as_str())?;
            match role {
                "user" => Some(ProviderMessage::User {
                    content: m
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                }),
                "assistant" => {
                    let calls: Vec<ProviderToolCall> = m
                        .get("tool_calls")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    Some(ProviderMessage::Assistant {
                        content: m
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        reasoning_content: m
                            .get("reasoning_content")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        tool_calls: calls,
                    })
                }
                "tool" => Some(ProviderMessage::Tool {
                    call_id: m
                        .get("tool_call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: m
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    result: m.get("content").map(|v| v.to_string()).unwrap_or_default(),
                }),
                _ => None,
            }
        })
        .collect()
}

// orchestrator_compat module: snapshot helper. We deleted orchestrator/, so
// we keep a slim copy of `snapshot::build` here under that name.
mod orchestrator_compat {
    use std::sync::Arc;
    use tokio::sync::Mutex;

    pub async fn project_summary(project: &Arc<Mutex<rb_core::project::Project>>) -> String {
        // Inline minimal version of the deleted orchestrator/snapshot.rs::build.
        let p = project.lock().await;
        format!(
            "Project: {}\nDefault view: {}\nRecent runs:\n{}",
            p.name,
            p.default_view.as_deref().unwrap_or("manual"),
            p.runs
                .iter()
                .rev()
                .take(10)
                .map(|r| format!("  {}: {} {:?}", r.id, r.module_id, r.status))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

//! Stage 3 of the loop: dispatch tool calls through SandboxPolicy, possibly
//! pausing for user approval. Approval channel: when a Decision::ApproveOnce
//! or AlwaysAsk fires, we surface an AgentEvent::ToolCall(decision="pending")
//! and wait on an `approval_rx` channel for the user's verdict. The Tauri
//! command surface in Plan 2 owns the channel.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, Mutex};

use crate::error::AiError;
use crate::provider::ProviderToolCall;
use crate::sandbox::policy::{Bucket, Decision, SandboxPolicy};
use crate::tools::{ToolContext, ToolOutput, ToolRegistry};

use super::types::AgentEvent;

pub enum ApprovalVerdict {
    Approve { edited_args: Option<Value> },
    Reject { reason: Option<String> },
}

pub struct ExecCtx<'a> {
    pub policy: &'a SandboxPolicy,
    pub registry: &'a ToolRegistry,
    pub memory: Option<&'a Arc<crate::memory::MemoryStore>>,
    pub session_id: &'a str,
    pub project_root: Option<&'a std::path::Path>,
    pub ask_user_tx: Option<&'a mpsc::Sender<crate::tools::AskUserRequest>>,
    pub approval_rx: &'a Mutex<mpsc::Receiver<(String, ApprovalVerdict)>>,
    pub event_sink: &'a mpsc::Sender<AgentEvent>,
}

pub async fn execute_call(ctx: ExecCtx<'_>, call: ProviderToolCall) -> Result<Value, AiError> {
    let (bucket, decision) = ctx.policy.classify(&call.name, &call.args);
    let bucket_str = bucket_label(&bucket);
    let decision_str = decision_label(&decision);

    let _ = ctx
        .event_sink
        .send(AgentEvent::ToolCall {
            session_id: ctx.session_id.into(),
            call_id: call.id.clone(),
            name: call.name.clone(),
            bucket: bucket_str.clone(),
            decision: decision_str.clone(),
            args: call.args.clone(),
        })
        .await;

    let resolved_args = if ctx.policy.should_run(&bucket, &decision) {
        call.args.clone()
    } else {
        // Wait for verdict.
        let mut rx = ctx.approval_rx.lock().await;
        loop {
            let (cid, verdict) = rx
                .recv()
                .await
                .ok_or_else(|| AiError::InvalidState("approval channel closed".into()))?;
            if cid != call.id {
                continue;
            }
            match verdict {
                ApprovalVerdict::Approve { edited_args } => {
                    ctx.policy.record_approval(bucket.clone());
                    break edited_args.unwrap_or_else(|| call.args.clone());
                }
                ApprovalVerdict::Reject { reason } => {
                    return Ok(serde_json::json!({
                        "error": "rejected_by_user",
                        "reason": reason.unwrap_or_default()
                    }));
                }
            }
        }
    };

    let entry = ctx
        .registry
        .get(&call.name)
        .ok_or_else(|| AiError::Tool(format!("unknown tool: {}", call.name)))?;
    let tool_ctx = ToolContext {
        memory: ctx.memory,
        session_id: Some(ctx.session_id),
        project_root: ctx.project_root,
        ask_user_tx: ctx.ask_user_tx,
    };
    let out = entry.executor.execute(&resolved_args, tool_ctx).await;
    let value = match out {
        Ok(ToolOutput::Value(v)) => v,
        Err(e) => serde_json::json!({"error": e.to_string()}),
    };
    let _ = ctx
        .event_sink
        .send(AgentEvent::ToolResult {
            session_id: ctx.session_id.into(),
            call_id: call.id,
            result: value.clone(),
        })
        .await;
    Ok(value)
}

fn bucket_label(b: &Bucket) -> String {
    match b {
        Bucket::ReadFs => "read_fs".into(),
        Bucket::SandboxWrite => "sandbox_write".into(),
        Bucket::ProjectModule { module } => format!("project_module:{module}"),
        Bucket::CodeRunSandbox => "code_run_sandbox".into(),
        Bucket::CodeRunOutOfSandbox => "code_run_out_of_sandbox".into(),
        Bucket::Web => "web".into(),
        Bucket::MemoryWrite => "memory_write".into(),
        Bucket::DestructiveDelete => "destructive_delete".into(),
        Bucket::AskUser => "ask_user".into(),
    }
}

fn decision_label(d: &Decision) -> String {
    match d {
        Decision::Allow => "allow".into(),
        Decision::ApproveOnce => "approve_once".into(),
        Decision::AlwaysAsk => "always_ask".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{
        builtin::file::FileWriteExec, schema::ToolDef, RiskLevel, ToolEntry, ToolRegistry,
    };
    use serde_json::json;
    use tempfile::tempdir;

    fn registry() -> ToolRegistry {
        let mut r = ToolRegistry::new();
        // Use file_write since it does no sandbox check internally.
        r.register(ToolEntry {
            def: ToolDef {
                name: "file_write".into(),
                description: "".into(),
                risk: RiskLevel::RunLow,
                params: json!({"type":"object"}),
            },
            executor: Arc::new(FileWriteExec),
        });
        r
    }

    #[tokio::test]
    async fn allow_path_runs_immediately() {
        let tmp = tempdir().unwrap();
        let policy = SandboxPolicy::new(tmp.path().to_path_buf(), "sandbox");
        let reg = registry();
        let (sink_tx, mut sink_rx) = mpsc::channel(16);
        let (_appr_tx, appr_rx) = mpsc::channel(1);
        let appr_rx = Mutex::new(appr_rx);
        let path = tmp.path().join("sandbox/x.txt");
        let res = execute_call(
            ExecCtx {
                policy: &policy,
                registry: &reg,
                memory: None,
                session_id: "s",
                project_root: Some(tmp.path()),
                ask_user_tx: None,
                approval_rx: &appr_rx,
                event_sink: &sink_tx,
            },
            ProviderToolCall {
                id: "c1".into(),
                name: "file_write".into(),
                args: json!({"path": path.display().to_string(), "content":"x"}),
            },
        )
        .await
        .unwrap();
        assert!(res.get("path").is_some());
        // Drain: ToolCall + ToolResult
        let _ = sink_rx.recv().await;
        let _ = sink_rx.recv().await;
        assert!(path.exists());
    }
}

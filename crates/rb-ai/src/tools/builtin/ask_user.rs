//! ask_user — pause and surface a prompt to the user. Implementation drains
//! through a channel installed in ToolContext.ask_user_tx; agent_loop owns
//! the receiver. The tool blocks until user replies via responder.

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::tools::{
    schema::{RiskLevel, ToolDef, ToolError},
    AskUserRequest, ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry,
};

pub fn register(reg: &mut ToolRegistry) {
    reg.register(ToolEntry {
        def: def(),
        executor: std::sync::Arc::new(AskUserExec),
    });
}

fn def() -> ToolDef {
    ToolDef {
        name: "ask_user".into(),
        description: "Pause and ask the user a question. Returns their reply.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {"prompt": {"type": "string"}},
            "required": ["prompt"]
        }),
    }
}

struct AskUserExec;
#[async_trait]
impl ToolExecutor for AskUserExec {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("prompt required".into()))?;
        let tx = ctx
            .ask_user_tx
            .ok_or_else(|| ToolError::Execution("ask_user channel not wired".into()))?;
        let (responder_tx, mut responder_rx) = tokio::sync::mpsc::channel::<String>(1);
        tx.send(AskUserRequest {
            call_id: Uuid::new_v4().simple().to_string(),
            prompt: prompt.into(),
            responder: responder_tx,
        })
        .await
        .map_err(|e| ToolError::Execution(format!("ask_user send: {e}")))?;
        let reply = responder_rx
            .recv()
            .await
            .ok_or_else(|| ToolError::Execution("ask_user channel closed".into()))?;
        Ok(ToolOutput::Value(json!({"reply": reply})))
    }
}

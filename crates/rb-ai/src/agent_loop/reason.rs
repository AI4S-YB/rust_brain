//! Stage 2 of the loop: call provider, accumulate text/reasoning, parse
//! tool calls, surface stream events.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::error::AiError;
use crate::provider::{
    ChatProvider, ChatRequest, FinishReason, ProviderEvent, ProviderMessage, ProviderToolCall,
    ThinkingConfig,
};
use crate::tools::ToolDef;

use super::types::AgentEvent;

pub struct ReasonOut {
    pub text: String,
    pub reasoning: String,
    pub tool_calls: Vec<ProviderToolCall>,
    pub finish: Option<FinishReason>,
}

#[allow(clippy::too_many_arguments)]
pub async fn reason(
    provider: Arc<dyn ChatProvider>,
    model: &str,
    system: String,
    history: Vec<ProviderMessage>,
    tools: Vec<ToolDef>,
    temperature: f32,
    thinking: ThinkingConfig,
    cancel: CancellationToken,
    sink: mpsc::Sender<AgentEvent>,
    session_id: &str,
) -> Result<ReasonOut, AiError> {
    let req = ChatRequest {
        model: model.into(),
        system,
        messages: history,
        tools,
        temperature,
        thinking,
    };
    let (tx, mut rx) = mpsc::channel::<ProviderEvent>(32);
    let cancel_for = cancel.clone();
    let prov = provider.clone();
    let h = tokio::spawn(async move { prov.send(req, tx, cancel_for).await });

    let mut out = ReasonOut {
        text: String::new(),
        reasoning: String::new(),
        tool_calls: vec![],
        finish: None,
    };
    while let Some(ev) = rx.recv().await {
        match ev {
            ProviderEvent::TextDelta(s) => {
                out.text.push_str(&s);
                let _ = sink
                    .send(AgentEvent::Text {
                        session_id: session_id.into(),
                        delta: s,
                    })
                    .await;
            }
            ProviderEvent::ReasoningDelta(s) => {
                out.reasoning.push_str(&s);
                let _ = sink
                    .send(AgentEvent::Reasoning {
                        session_id: session_id.into(),
                        delta: s,
                    })
                    .await;
            }
            ProviderEvent::ToolCall { id, name, args } => {
                out.tool_calls.push(ProviderToolCall { id, name, args });
            }
            ProviderEvent::Finish(r) => {
                out.finish = Some(r);
            }
        }
    }
    match h.await {
        Ok(Ok(())) => Ok(out),
        Ok(Err(e)) => Err(AiError::Provider(format!("{e}"))),
        Err(e) => Err(AiError::Provider(format!("provider join: {e}"))),
    }
}

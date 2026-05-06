pub mod openai_compat;

#[cfg(feature = "anthropic")]
pub mod anthropic;

#[cfg(feature = "ollama-native")]
pub mod ollama;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

use rb_core::cancel::CancellationToken;

use crate::tools::ToolDef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<ProviderMessage>,
    pub tools: Vec<ToolDef>,
    pub temperature: f32,
    pub thinking: ThinkingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThinkingConfig {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

/// Neutral message shape handed to provider adapters. Distinct from any
/// persisted `Message` type, which can carry extra metadata like
/// `interrupted` that providers don't need to see.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ProviderMessage {
    User {
        content: String,
    },
    Assistant {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_content: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ProviderToolCall>,
    },
    Tool {
        call_id: String,
        name: String,
        /// Stringified JSON result. Providers expect a string body here
        /// (OpenAI serializes tool results as text content).
        result: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderToolCall {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum ProviderEvent {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCall {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    Finish(FinishReason),
}

#[derive(Debug, Clone)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    Error(String),
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("http error: {0}")]
    Http(String),
    #[error("auth error (check API key): {0}")]
    Auth(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("malformed response: {0}")]
    Malformed(String),
    #[error("cancelled")]
    Cancelled,
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn send(
        &self,
        req: ChatRequest,
        sink: mpsc::Sender<ProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ProviderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_message_roundtrips_via_serde() {
        let m = ProviderMessage::Assistant {
            content: "hi".into(),
            reasoning_content: None,
            tool_calls: vec![ProviderToolCall {
                id: "tc1".into(),
                name: "ls".into(),
                args: serde_json::json!({"path":"/"}),
            }],
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: ProviderMessage = serde_json::from_str(&s).unwrap();
        match back {
            ProviderMessage::Assistant { tool_calls, .. } => assert_eq!(tool_calls.len(), 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn assistant_with_empty_tool_calls_skips_field() {
        let m = ProviderMessage::Assistant {
            content: "hi".into(),
            reasoning_content: None,
            tool_calls: vec![],
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(
            !s.contains("tool_calls"),
            "empty tool_calls must be skipped"
        );
    }
}

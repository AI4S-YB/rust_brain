//! Anthropic provider. Phase 1 ships a stub; gated behind the `anthropic`
//! Cargo feature so the dependency graph stays lean in default builds.

use async_trait::async_trait;
use tokio::sync::mpsc;

use rb_core::cancel::CancellationToken;

use super::{ChatProvider, ChatRequest, ProviderError, ProviderEvent};

pub struct AnthropicProvider;

#[async_trait]
impl ChatProvider for AnthropicProvider {
    fn id(&self) -> &str {
        "anthropic"
    }

    async fn send(
        &self,
        _req: ChatRequest,
        _sink: mpsc::Sender<ProviderEvent>,
        _cancel: CancellationToken,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::Http(
            "Anthropic provider is not implemented in Phase 1".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn anthropic_stub_returns_http_error() {
        let p = AnthropicProvider;
        let (tx, _rx) = mpsc::channel(1);
        let err = p
            .send(
                ChatRequest {
                    model: "m".into(),
                    system: "s".into(),
                    messages: vec![],
                    tools: vec![],
                    temperature: 0.0,
                    thinking: Default::default(),
                },
                tx,
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::Http(_)));
    }
}

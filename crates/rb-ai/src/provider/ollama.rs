//! Native Ollama provider (using Ollama's own /api/chat protocol). Phase 1
//! ships a stub; users wanting Ollama today should point the OpenAI-compatible
//! provider at Ollama's `/v1/*` endpoint. Gated behind `ollama-native` feature.

use async_trait::async_trait;
use tokio::sync::mpsc;

use tokio_util::sync::CancellationToken;

use super::{ChatProvider, ChatRequest, ProviderError, ProviderEvent};

pub struct OllamaProvider;

#[async_trait]
impl ChatProvider for OllamaProvider {
    fn id(&self) -> &str {
        "ollama"
    }

    async fn send(
        &self,
        _req: ChatRequest,
        _sink: mpsc::Sender<ProviderEvent>,
        _cancel: CancellationToken,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::Http(
            "Native Ollama provider is not implemented in Phase 1; use OpenAI-compat with Ollama's /v1 endpoint instead."
                .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn ollama_stub_returns_http_error() {
        let p = OllamaProvider;
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

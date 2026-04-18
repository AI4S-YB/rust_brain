use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use crate::cancel::CancellationToken;
use crate::run_event::RunEvent;

// Retained for backwards-compat of RunRecord/ModuleResult consumers;
// Progress is now a shape carried inside RunEvent::Progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub fraction: f64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleResult {
    pub output_files: Vec<PathBuf>,
    pub summary: serde_json::Value,
    pub log: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("invalid parameters: {0:?}")]
    InvalidParams(Vec<ValidationError>),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("tool error: {0}")]
    ToolError(String),
    #[error("cancelled")]
    Cancelled,
}

#[async_trait::async_trait]
pub trait Module: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError>;
    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        events_tx: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cancel::CancellationToken;
    use crate::run_event::RunEvent;

    struct DummyModule;

    #[async_trait::async_trait]
    impl Module for DummyModule {
        fn id(&self) -> &str {
            "dummy"
        }
        fn name(&self) -> &str {
            "Dummy"
        }
        fn validate(&self, _params: &serde_json::Value) -> Vec<ValidationError> {
            vec![]
        }
        async fn run(
            &self,
            _params: &serde_json::Value,
            _project_dir: &std::path::Path,
            events_tx: tokio::sync::mpsc::Sender<RunEvent>,
            _cancel: CancellationToken,
        ) -> Result<ModuleResult, ModuleError> {
            let _ = events_tx
                .send(RunEvent::Progress {
                    fraction: 1.0,
                    message: "done".into(),
                })
                .await;
            Ok(ModuleResult {
                output_files: vec![],
                summary: serde_json::json!({}),
                log: String::new(),
            })
        }
    }

    #[tokio::test]
    async fn module_trait_accepts_run_event_and_cancel() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<RunEvent>(4);
        let token = CancellationToken::new();
        let m = DummyModule;
        let res = m
            .run(
                &serde_json::json!({}),
                std::path::Path::new("/tmp"),
                tx,
                token,
            )
            .await
            .unwrap();
        assert!(res.output_files.is_empty());
        match rx.recv().await.unwrap() {
            RunEvent::Progress { fraction, .. } => assert_eq!(fraction, 1.0),
            _ => panic!("expected Progress"),
        }
    }
}

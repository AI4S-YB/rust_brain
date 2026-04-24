use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use crate::cancel::CancellationToken;
use crate::run_event::RunEvent;

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

    /// Opt-in: returning `Some(schema)` exposes this module to the AI tool registry.
    /// Schema is JSON Schema draft-07 describing `run` params.
    fn params_schema(&self) -> Option<serde_json::Value> {
        None
    }

    /// One-paragraph hint for the LLM, in `"en"` or `"zh"`. Unknown langs fall back to `"en"`.
    /// Only consulted when `params_schema` returns `Some(_)`.
    fn ai_hint(&self, _lang: &str) -> String {
        String::new()
    }

    /// Describe the reusable assets a successful run produces. The Runner
    /// uses these declarations to auto-register `AssetRecord`s in the
    /// project's `assets[]` list with `produced_by_run_id` set.
    ///
    /// The `relative_path` on each `DeclaredAsset` is resolved against the
    /// run's output directory. Default: no declared assets.
    fn produced_assets(&self, _result: &ModuleResult) -> Vec<crate::asset::DeclaredAsset> {
        Vec::new()
    }
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

    #[test]
    fn module_default_schema_is_none_and_hint_is_empty() {
        let m = DummyModule;
        assert!(m.params_schema().is_none());
        assert_eq!(m.ai_hint("en"), "");
        assert_eq!(m.ai_hint("zh"), "");
    }

    struct SchemaModule;

    #[async_trait::async_trait]
    impl Module for SchemaModule {
        fn id(&self) -> &str {
            "schema"
        }
        fn name(&self) -> &str {
            "Schema"
        }
        fn validate(&self, _p: &serde_json::Value) -> Vec<ValidationError> {
            vec![]
        }
        fn params_schema(&self) -> Option<serde_json::Value> {
            Some(serde_json::json!({
                "type": "object",
                "properties": { "foo": { "type": "string" } },
                "required": ["foo"]
            }))
        }
        fn ai_hint(&self, lang: &str) -> String {
            match lang {
                "zh" => "测试".into(),
                _ => "test".into(),
            }
        }
        async fn run(
            &self,
            _p: &serde_json::Value,
            _d: &std::path::Path,
            _tx: tokio::sync::mpsc::Sender<RunEvent>,
            _c: CancellationToken,
        ) -> Result<ModuleResult, ModuleError> {
            Ok(ModuleResult {
                output_files: vec![],
                summary: serde_json::json!({}),
                log: String::new(),
            })
        }
    }

    #[test]
    fn module_can_override_schema_and_hint() {
        let m = SchemaModule;
        let schema = m.params_schema().expect("expected Some schema");
        assert_eq!(schema["type"], "object");
        assert_eq!(m.ai_hint("zh"), "测试");
        assert_eq!(m.ai_hint("en"), "test");
    }
}

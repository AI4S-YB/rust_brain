use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

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
        progress_tx: mpsc::Sender<Progress>,
    ) -> Result<ModuleResult, ModuleError>;
}

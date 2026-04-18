use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::Path;
use tokio::sync::mpsc;

pub struct GffConvertModule;

#[async_trait::async_trait]
impl Module for GffConvertModule {
    fn id(&self) -> &str {
        "gff_convert"
    }
    fn name(&self) -> &str {
        "GFF Converter"
    }

    fn validate(&self, _params: &serde_json::Value) -> Vec<ValidationError> {
        Vec::new()
    }

    async fn run(
        &self,
        _params: &serde_json::Value,
        _project_dir: &Path,
        _events_tx: mpsc::Sender<RunEvent>,
        _cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        Err(ModuleError::ToolError("not yet implemented".into()))
    }
}

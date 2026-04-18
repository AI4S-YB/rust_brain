use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::Path;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetFormat {
    Gtf,
    Gff3,
}

impl TargetFormat {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "gtf" => Some(Self::Gtf),
            "gff3" => Some(Self::Gff3),
            _ => None,
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Self::Gtf => "gtf",
            Self::Gff3 => "gff3",
        }
    }

    pub fn needs_t_flag(self) -> bool {
        matches!(self, Self::Gtf)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_format_from_str_accepts_gtf_and_gff3() {
        assert_eq!(TargetFormat::from_str("gtf"), Some(TargetFormat::Gtf));
        assert_eq!(TargetFormat::from_str("gff3"), Some(TargetFormat::Gff3));
    }

    #[test]
    fn target_format_from_str_rejects_unknown() {
        assert_eq!(TargetFormat::from_str("bed"), None);
        assert_eq!(TargetFormat::from_str(""), None);
        assert_eq!(TargetFormat::from_str("GTF"), None); // case-sensitive
    }

    #[test]
    fn target_format_ext() {
        assert_eq!(TargetFormat::Gtf.ext(), "gtf");
        assert_eq!(TargetFormat::Gff3.ext(), "gff3");
    }

    #[test]
    fn target_format_needs_t_flag() {
        assert!(TargetFormat::Gtf.needs_t_flag());
        assert!(!TargetFormat::Gff3.needs_t_flag());
    }
}

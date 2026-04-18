use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::Path;
use tokio::sync::mpsc;

pub mod subprocess;

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

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        match params.get("input_file").and_then(|v| v.as_str()) {
            None => errors.push(ValidationError {
                field: "input_file".into(),
                message: "input_file is required".into(),
            }),
            Some("") => errors.push(ValidationError {
                field: "input_file".into(),
                message: "input_file must not be empty".into(),
            }),
            Some(p) if !std::path::Path::new(p).is_file() => errors.push(ValidationError {
                field: "input_file".into(),
                message: format!("input_file does not exist: {p}"),
            }),
            Some(_) => {}
        }

        match params.get("target_format").and_then(|v| v.as_str()) {
            None => errors.push(ValidationError {
                field: "target_format".into(),
                message: "target_format is required (gtf or gff3)".into(),
            }),
            Some(s) if TargetFormat::from_str(s).is_none() => errors.push(ValidationError {
                field: "target_format".into(),
                message: format!("target_format must be 'gtf' or 'gff3', got: {s}"),
            }),
            Some(_) => {}
        }

        if let Some(v) = params.get("extra_args") {
            match v.as_array() {
                None => errors.push(ValidationError {
                    field: "extra_args".into(),
                    message: "extra_args must be an array of strings".into(),
                }),
                Some(arr) if arr.iter().any(|e| !e.is_string()) => errors.push(ValidationError {
                    field: "extra_args".into(),
                    message: "all extra_args elements must be strings".into(),
                }),
                Some(_) => {}
            }
        }

        // Surface binary resolution failures at validate time so the UI can
        // show "Missing binary" immediately instead of at run time.
        if let Ok(resolver) = rb_core::binary::BinaryResolver::load() {
            if let Err(e) = resolver.resolve("gffread-rs") {
                errors.push(ValidationError {
                    field: "binary".into(),
                    message: e.to_string(),
                });
            }
        }

        errors
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

pub fn build_argv(
    input: &Path,
    output: &Path,
    target: TargetFormat,
    extra_args: &[String],
) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = Vec::new();
    args.push(input.as_os_str().to_os_string());
    if target.needs_t_flag() {
        args.push("-T".into());
    }
    args.push("-o".into());
    args.push(output.as_os_str().to_os_string());
    for a in extra_args {
        args.push(std::ffi::OsString::from(a));
    }
    args
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

    use serde_json::json;

    #[test]
    fn validate_requires_input_file() {
        let m = GffConvertModule;
        let errs = m.validate(&json!({ "target_format": "gtf" }));
        assert!(
            errs.iter().any(|e| e.field == "input_file"),
            "expected input_file error, got {:?}",
            errs
        );
    }

    #[test]
    fn validate_requires_existing_input_file() {
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": "/definitely/does/not/exist.gff3",
            "target_format": "gtf",
        }));
        assert!(
            errs.iter().any(|e| e.field == "input_file"),
            "expected input_file error for missing file, got {:?}",
            errs
        );
    }

    #[test]
    fn validate_requires_target_format() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": tmp.path().to_string_lossy(),
        }));
        assert!(
            errs.iter().any(|e| e.field == "target_format"),
            "expected target_format error, got {:?}",
            errs
        );
    }

    #[test]
    fn validate_rejects_unknown_target_format() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": tmp.path().to_string_lossy(),
            "target_format": "bed",
        }));
        assert!(errs.iter().any(|e| e.field == "target_format"));
    }

    #[test]
    fn validate_rejects_non_array_extra_args() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": tmp.path().to_string_lossy(),
            "target_format": "gtf",
            "extra_args": "not-an-array",
        }));
        assert!(errs.iter().any(|e| e.field == "extra_args"));
    }

    #[test]
    fn validate_rejects_non_string_extra_args_elements() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": tmp.path().to_string_lossy(),
            "target_format": "gtf",
            "extra_args": ["-T", 42, "--keep-comments"],
        }));
        assert!(errs.iter().any(|e| e.field == "extra_args"));
    }

    #[test]
    fn validate_accepts_valid_params() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": tmp.path().to_string_lossy(),
            "target_format": "gtf",
            "extra_args": ["--keep-comments"],
        }));
        // binary error may or may not be present depending on PATH; filter it out.
        let other: Vec<_> = errs.iter().filter(|e| e.field != "binary").collect();
        assert!(other.is_empty(), "unexpected errors: {:?}", other);
    }

    use std::ffi::OsString;
    use std::path::PathBuf;

    fn os(s: &str) -> OsString {
        OsString::from(s)
    }

    #[test]
    fn argv_gtf_target() {
        let input = PathBuf::from("/data/anno.gff3");
        let output = PathBuf::from("/runs/anno.gtf");
        let argv = build_argv(&input, &output, TargetFormat::Gtf, &[]);
        assert_eq!(
            argv,
            vec![
                os("/data/anno.gff3"),
                os("-T"),
                os("-o"),
                os("/runs/anno.gtf"),
            ]
        );
    }

    #[test]
    fn argv_gff3_target_omits_dash_t() {
        let input = PathBuf::from("/data/anno.gtf");
        let output = PathBuf::from("/runs/anno.gff3");
        let argv = build_argv(&input, &output, TargetFormat::Gff3, &[]);
        assert_eq!(
            argv,
            vec![os("/data/anno.gtf"), os("-o"), os("/runs/anno.gff3"),]
        );
    }

    #[test]
    fn argv_appends_extra_args_after_output() {
        let input = PathBuf::from("/data/anno.gff3");
        let output = PathBuf::from("/runs/anno.gtf");
        let extras = vec!["--keep-comments".to_string(), "--force-exons".to_string()];
        let argv = build_argv(&input, &output, TargetFormat::Gtf, &extras);
        assert_eq!(
            argv,
            vec![
                os("/data/anno.gff3"),
                os("-T"),
                os("-o"),
                os("/runs/anno.gtf"),
                os("--keep-comments"),
                os("--force-exons"),
            ]
        );
    }
}

pub mod counts;
pub mod log_final;
mod subprocess;

use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

pub struct StarAlignModule;

fn sample_name_from_r1(r1: &str) -> String {
    let p = Path::new(r1);
    let mut name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    for ext in [".gz", ".fastq", ".fq", ".txt"] {
        if let Some(stripped) = name.strip_suffix(ext) {
            name = stripped.to_string();
        }
    }
    for suffix in ["_R1", "_1"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    name
}

#[async_trait::async_trait]
impl Module for StarAlignModule {
    fn id(&self) -> &str { "star_align" }
    fn name(&self) -> &str { "STAR Alignment & Quantification" }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        match params.get("genome_dir").and_then(|v| v.as_str()) {
            None => errors.push(ValidationError {
                field: "genome_dir".into(), message: "genome_dir is required".into(),
            }),
            Some(s) => {
                let p = Path::new(s);
                if !p.is_dir() {
                    errors.push(ValidationError {
                        field: "genome_dir".into(),
                        message: format!("genome_dir does not exist or is not a directory: {}", s),
                    });
                } else if !p.join("SA").exists() {
                    errors.push(ValidationError {
                        field: "genome_dir".into(),
                        message: format!("genome_dir does not look like a STAR index (missing SA): {}", s),
                    });
                }
            }
        }

        let r1 = params.get("reads_1").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if r1.is_empty() {
            errors.push(ValidationError {
                field: "reads_1".into(), message: "reads_1 must be a non-empty array".into(),
            });
        }
        for (i, v) in r1.iter().enumerate() {
            match v.as_str() {
                None => errors.push(ValidationError {
                    field: format!("reads_1[{}]", i),
                    message: "must be a string path".into(),
                }),
                Some(p) => if !Path::new(p).exists() {
                    errors.push(ValidationError {
                        field: format!("reads_1[{}]", i),
                        message: format!("file does not exist: {}", p),
                    });
                }
            }
        }

        if let Some(r2) = params.get("reads_2").and_then(|v| v.as_array()) {
            if !r2.is_empty() && r2.len() != r1.len() {
                errors.push(ValidationError {
                    field: "reads_2".into(),
                    message: format!("reads_2 length ({}) must match reads_1 length ({})", r2.len(), r1.len()),
                });
            }
            for (i, v) in r2.iter().enumerate() {
                if let Some(p) = v.as_str() {
                    if !Path::new(p).exists() {
                        errors.push(ValidationError {
                            field: format!("reads_2[{}]", i),
                            message: format!("file does not exist: {}", p),
                        });
                    }
                }
            }
        }

        if let Some(names) = params.get("sample_names").and_then(|v| v.as_array()) {
            if names.len() != r1.len() {
                errors.push(ValidationError {
                    field: "sample_names".into(),
                    message: format!("sample_names length ({}) must match reads_1 length ({})", names.len(), r1.len()),
                });
            }
            let mut seen = std::collections::HashSet::new();
            for (i, v) in names.iter().enumerate() {
                let s = v.as_str().unwrap_or("");
                if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c)) {
                    errors.push(ValidationError {
                        field: format!("sample_names[{}]", i),
                        message: "must be non-empty and match [A-Za-z0-9_.-]+".into(),
                    });
                }
                if !seen.insert(s) {
                    errors.push(ValidationError {
                        field: format!("sample_names[{}]", i),
                        message: format!("duplicate sample name: {}", s),
                    });
                }
            }
        }

        match params.get("strand").and_then(|v| v.as_str()).unwrap_or("unstranded") {
            "unstranded" | "forward" | "reverse" => {}
            other => errors.push(ValidationError {
                field: "strand".into(),
                message: format!("strand must be unstranded/forward/reverse, got '{}'", other),
            }),
        }

        if let Some(v) = params.get("extra_args") {
            if !v.is_array() || !v.as_array().unwrap().iter().all(|x| x.is_string()) {
                errors.push(ValidationError {
                    field: "extra_args".into(),
                    message: "extra_args must be an array of strings".into(),
                });
            }
        }

        if let Ok(resolver) = BinaryResolver::load() {
            if let Err(e) = resolver.resolve("star") {
                errors.push(ValidationError { field: "binary".into(), message: e.to_string() });
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
        // Implemented in Task 17
        Err(ModuleError::ToolError("run() not implemented yet".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_name_strips_common_suffixes() {
        assert_eq!(sample_name_from_r1("/x/S1_R1.fastq.gz"), "S1");
        assert_eq!(sample_name_from_r1("/x/S2_1.fq"),       "S2");
        assert_eq!(sample_name_from_r1("/x/raw.fastq"),     "raw");
        assert_eq!(sample_name_from_r1("/x/odd.name.fq.gz"), "odd.name");
    }

    #[test]
    fn validate_requires_genome_and_reads() {
        let m = StarAlignModule;
        let errs = m.validate(&serde_json::json!({}));
        let fields: Vec<_> = errs.iter().map(|e| e.field.clone()).collect();
        assert!(fields.iter().any(|f| f == "genome_dir"));
        assert!(fields.iter().any(|f| f == "reads_1"));
    }

    #[test]
    fn validate_rejects_length_mismatch_between_r1_and_r2() {
        let tmp = tempfile::tempdir().unwrap();
        let gd = tmp.path().join("gdir");
        std::fs::create_dir_all(&gd).unwrap();
        std::fs::write(gd.join("SA"), "").unwrap();
        let r1 = tmp.path().join("a_R1.fq"); std::fs::write(&r1, "").unwrap();
        let r2a = tmp.path().join("a_R2.fq"); std::fs::write(&r2a, "").unwrap();
        let r2b = tmp.path().join("b_R2.fq"); std::fs::write(&r2b, "").unwrap();
        let m = StarAlignModule;
        let errs = m.validate(&serde_json::json!({
            "genome_dir": gd,
            "reads_1": [r1],
            "reads_2": [r2a, r2b],
        }));
        assert!(errs.iter().any(|e| e.field == "reads_2" && e.message.contains("length")));
    }

    #[test]
    fn validate_rejects_bad_strand() {
        let tmp = tempfile::tempdir().unwrap();
        let gd = tmp.path().join("gdir");
        std::fs::create_dir_all(&gd).unwrap();
        std::fs::write(gd.join("SA"), "").unwrap();
        let r1 = tmp.path().join("a_R1.fq"); std::fs::write(&r1, "").unwrap();
        let m = StarAlignModule;
        let errs = m.validate(&serde_json::json!({
            "genome_dir": gd, "reads_1": [r1], "strand": "weird",
        }));
        assert!(errs.iter().any(|e| e.field == "strand"));
    }
}

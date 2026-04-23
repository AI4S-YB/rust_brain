use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::{LogStream, RunEvent};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

pub struct QcModule;

#[async_trait::async_trait]
impl Module for QcModule {
    fn id(&self) -> &str {
        "qc"
    }

    fn name(&self) -> &str {
        "FastQC Quality Control"
    }

    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "input_files": {
                    "type": "array",
                    "items": {
                        "type": "string",
                        "description": "Absolute path to a FASTQ or FASTQ.gz file."
                    },
                    "minItems": 1,
                    "description": "Input FASTQ file paths. May be a single file or multiple samples."
                },
                "threads": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum number of files to process simultaneously."
                },
                "sequence_format": {
                    "type": "string",
                    "enum": ["fastq", "bam", "sam", "bam_mapped", "sam_mapped"],
                    "description": "Optional explicit sequence format override."
                },
                "output_dir": {
                    "type": "string",
                    "description": "Optional output directory override."
                },
                "casava": {
                    "type": "boolean"
                },
                "nogroup": {
                    "type": "boolean"
                },
                "kmer_size": {
                    "type": "integer",
                    "minimum": 2
                }
            },
            "required": ["input_files"],
            "additionalProperties": false
        }))
    }

    fn ai_hint(&self, lang: &str) -> String {
        match lang {
            "zh" => "用 run_qc 对用户提供的 FASTQ 文件做质量评估。通常是流水线的第一步 (修剪之前)。参数 input_files 接受一个 FASTQ 文件路径数组,每个样本一个条目。".into(),
            _    => "Use run_qc to assess read quality for raw FASTQ input. This is typically the first step of a pipeline, before trimming. The `input_files` array takes one FASTQ path per sample.".into(),
        }
    }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        match params.get("input_files") {
            None => {
                errors.push(ValidationError {
                    field: "input_files".to_string(),
                    message: "input_files must be a non-empty array".to_string(),
                });
            }
            Some(v) => {
                let arr = v.as_array();
                if arr.map_or(true, |a| a.is_empty()) {
                    errors.push(ValidationError {
                        field: "input_files".to_string(),
                        message: "input_files must be a non-empty array".to_string(),
                    });
                }
            }
        }

        if let Some(v) = params.get("threads") {
            if v.as_u64().map_or(true, |n| n == 0) {
                errors.push(ValidationError {
                    field: "threads".to_string(),
                    message: "threads must be an integer >= 1".to_string(),
                });
            }
        }

        if let Some(v) = params.get("sequence_format") {
            let ok = matches!(
                v.as_str(),
                Some("fastq" | "bam" | "sam" | "bam_mapped" | "sam_mapped")
            );
            if !ok {
                errors.push(ValidationError {
                    field: "sequence_format".to_string(),
                    message:
                        "sequence_format must be one of fastq, bam, sam, bam_mapped, sam_mapped"
                            .to_string(),
                });
            }
        }

        if let Some(v) = params.get("kmer_size") {
            if v.as_u64().map_or(true, |n| n < 2) {
                errors.push(ValidationError {
                    field: "kmer_size".to_string(),
                    message: "kmer_size must be an integer >= 2".to_string(),
                });
            }
        }

        errors
    }

    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        events_tx: mpsc::Sender<RunEvent>,
        _cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        let errors = self.validate(params);
        if !errors.is_empty() {
            return Err(ModuleError::InvalidParams(errors));
        }

        let input_files: Vec<PathBuf> = params["input_files"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(PathBuf::from))
            .collect();

        let output_dir = params
            .get("output_dir")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| project_dir.join("qc_output"));
        std::fs::create_dir_all(&output_dir)?;

        let total = input_files.len();
        let mut output_files = Vec::new();
        let mut processed = Vec::new();
        let mut reports = Vec::new();
        let mut log_lines = Vec::new();
        let threads = params
            .get("threads")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let sequence_format = params
            .get("sequence_format")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let casava = params
            .get("casava")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let nogroup = params
            .get("nogroup")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let kmer_size = params
            .get("kmer_size")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);

        for (idx, input_path) in input_files.iter().enumerate() {
            let fraction = idx as f64 / total as f64;
            let progress_msg = format!(
                "Processing {} ({}/{})",
                input_path.display(),
                idx + 1,
                total
            );
            let _ = events_tx
                .send(RunEvent::Progress {
                    fraction,
                    message: progress_msg.clone(),
                })
                .await;
            let _ = events_tx
                .send(RunEvent::Log {
                    line: progress_msg,
                    stream: LogStream::Stdout,
                })
                .await;

            let input_str = input_path.to_string_lossy().to_string();

            // Use fastqc-rs library directly via process_file
            let input_path_clone = input_path.clone();
            let output_dir_clone = output_dir.clone();
            let sequence_format_clone = sequence_format.clone();
            let result = tokio::task::spawn_blocking(move || {
                let mut config = fastqc_rs::config::Config::default_config();
                config.files = vec![input_path_clone.clone()];
                config.output_dir = Some(output_dir_clone);
                if let Some(threads) = threads {
                    config.threads = threads;
                }
                config.sequence_format = sequence_format_clone;
                config.casava = casava;
                config.nogroup = nogroup;
                config.kmer_size = kmer_size;
                config.json = true;
                config.quiet = true;
                fastqc_rs::analysis::process_file(&input_path_clone, &config)
            })
            .await
            .map_err(|e| ModuleError::ToolError(e.to_string()))?;

            match result {
                Ok(()) => {
                    let (html_path, zip_path, json_path) =
                        expected_qc_paths(input_path, &output_dir);

                    if html_path.exists() {
                        output_files.push(html_path);
                    }
                    if zip_path.exists() {
                        output_files.push(zip_path);
                    }
                    if json_path.exists() {
                        output_files.push(json_path.clone());
                    }

                    let (fastqc_report, report_error) = match read_fastqc_json_report(&json_path) {
                        Ok(report) => (Some(report), None),
                        Err(err) => (
                            None,
                            Some(format!("structured FastQC report unavailable: {}", err)),
                        ),
                    };

                    processed.push(serde_json::json!({
                        "file": input_str,
                        "status": "ok",
                    }));
                    reports.push(serde_json::json!({
                        "input_file": input_str,
                        "status": "ok",
                        "error": report_error,
                        "fastqc_report": fastqc_report,
                    }));
                    let ok_line = format!("OK: {}", input_str);
                    log_lines.push(ok_line.clone());
                    let _ = events_tx
                        .send(RunEvent::Log {
                            line: ok_line,
                            stream: LogStream::Stdout,
                        })
                        .await;
                }
                Err(e) => {
                    processed.push(serde_json::json!({
                        "file": input_str,
                        "status": "error",
                        "error": e.to_string(),
                    }));
                    reports.push(serde_json::json!({
                        "input_file": input_str,
                        "status": "error",
                        "error": e.to_string(),
                        "fastqc_report": Value::Null,
                    }));
                    let err_line = format!("ERROR: {} — {}", input_str, e);
                    log_lines.push(err_line.clone());
                    let _ = events_tx
                        .send(RunEvent::Log {
                            line: err_line,
                            stream: LogStream::Stderr,
                        })
                        .await;
                }
            }
        }

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 1.0,
                message: "Done".to_string(),
            })
            .await;

        let ok_count = processed.iter().filter(|v| v["status"] == "ok").count();

        let summary = serde_json::json!({
            "total_files": total,
            "processed_ok": ok_count,
            "output_directory": output_dir.display().to_string(),
            "files": processed,
            "reports": reports,
        });

        Ok(ModuleResult {
            output_files,
            summary,
            log: log_lines.join("\n"),
        })
    }
}

fn strip_seq_extensions(name: &str) -> String {
    let mut s = name.to_string();
    for ext in &[
        ".gz", ".bz2", ".txt", ".fastq", ".fq", ".csfastq", ".sam", ".bam", ".ubam",
    ] {
        if let Some(stripped) = s.strip_suffix(ext) {
            s = stripped.to_string();
        }
    }
    s
}

fn expected_qc_paths(input_path: &Path, output_dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let file_stem = input_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let base = strip_seq_extensions(&file_stem);
    (
        output_dir.join(format!("{}_fastqc.html", base)),
        output_dir.join(format!("{}_fastqc.zip", base)),
        output_dir.join(format!("{}_fastqc.json", base)),
    )
}

fn read_fastqc_json_report(path: &Path) -> Result<Value, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    serde_json::from_str(&raw).map_err(|e| format!("failed to parse {}: {}", path.display(), e))
}

#[cfg(test)]
mod ai_schema_tests {
    use super::*;
    use rb_core::module::Module;

    #[test]
    fn qc_schema_requires_input() {
        let schema = QcModule.params_schema().expect("qc exposes a schema");
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().expect("required list");
        assert!(
            required.iter().any(|v| v == "input_files"),
            "QC schema must require 'input_files'"
        );
    }

    #[test]
    fn qc_hint_mentions_fastq_in_both_languages() {
        let en = QcModule.ai_hint("en").to_lowercase();
        let zh = QcModule.ai_hint("zh");
        assert!(en.contains("fastq"), "en hint should mention fastq");
        assert!(!zh.is_empty(), "zh hint must not be empty");
    }
}

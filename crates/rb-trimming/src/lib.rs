use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::{LogStream, RunEvent};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub struct TrimmingModule;

#[async_trait::async_trait]
impl Module for TrimmingModule {
    fn id(&self) -> &str {
        "trimming"
    }
    fn name(&self) -> &str {
        "Cutadapt Adapter Trimming"
    }

    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "input_files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "description": "FASTQ file paths to trim."
                },
                "adapter": {
                    "type": "string",
                    "description": "Adapter sequence to remove (3' end, passed as -a). Omit to skip explicit adapter trimming."
                },
                "quality_cutoff": {
                    "type": "integer",
                    "minimum": 0, "maximum": 40,
                    "description": "Phred quality threshold for trimming (default 20, passed as -q)."
                },
                "min_length": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Discard reads shorter than this after trimming (default 20, passed as -m)."
                }
            },
            "required": ["input_files"],
            "additionalProperties": false
        }))
    }

    fn ai_hint(&self, lang: &str) -> String {
        match lang {
            "zh" => "用 run_trimming 调用 cutadapt 去除接头并按质量裁剪 FASTQ。输入是 run_qc 看过的原始 FASTQ,输出会被 run_star_align 使用。参数 adapter、quality_cutoff、min_length 不确定时省略即可,默认值较合理。".into(),
            _    => "Use run_trimming to remove adapters and quality-trim FASTQ via cutadapt. Input is typically the same raw FASTQ QC has already inspected; output feeds run_star_align. Omit adapter / quality_cutoff / min_length if unsure — defaults are sensible.".into(),
        }
    }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        match params.get("input_files") {
            None => errors.push(ValidationError {
                field: "input_files".into(),
                message: "input_files must be a non-empty array".into(),
            }),
            Some(v) => {
                if v.as_array().map_or(true, |a| a.is_empty()) {
                    errors.push(ValidationError {
                        field: "input_files".into(),
                        message: "input_files must be a non-empty array".into(),
                    });
                }
            }
        }
        // Surface resolver errors at validate time for UI feedback.
        if let Ok(resolver) = BinaryResolver::load() {
            if let Err(e) = resolver.resolve("cutadapt-rs") {
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
        params: &serde_json::Value,
        project_dir: &Path,
        events_tx: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        let errors = self.validate(params);
        if !errors.is_empty() {
            return Err(ModuleError::InvalidParams(errors));
        }

        let resolver = BinaryResolver::load().map_err(|e| ModuleError::ToolError(e.to_string()))?;
        let bin = resolver
            .resolve("cutadapt-rs")
            .map_err(|e| ModuleError::ToolError(e.to_string()))?;

        let input_files: Vec<PathBuf> = params["input_files"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(PathBuf::from))
            .collect();

        let adapter = params
            .get("adapter")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let quality_cutoff = params
            .get("quality_cutoff")
            .and_then(|v| v.as_u64())
            .unwrap_or(20);
        let min_length = params
            .get("min_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(20);

        let output_dir = project_dir.join("trimmed");
        std::fs::create_dir_all(&output_dir)?;

        let total = input_files.len();
        let mut output_files = Vec::new();
        let mut file_summaries = Vec::new();
        let mut log_lines = Vec::new();

        for (idx, input_path) in input_files.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(ModuleError::Cancelled);
            }
            let fraction = idx as f64 / total as f64;
            let _ = events_tx
                .send(RunEvent::Progress {
                    fraction,
                    message: format!("Trimming {} ({}/{})", input_path.display(), idx + 1, total),
                })
                .await;

            let input_str = input_path.to_string_lossy().to_string();
            let file_name = input_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("output_{}.fastq.gz", idx));
            let output_path = output_dir.join(&file_name);
            let output_str = output_path.to_string_lossy().to_string();

            let mut cmd = Command::new(&bin);
            cmd.arg("-o").arg(&output_str);
            cmd.arg("-q").arg(quality_cutoff.to_string());
            cmd.arg("-m").arg(min_length.to_string());
            if !adapter.is_empty() {
                cmd.arg("-a").arg(&adapter);
            }
            cmd.arg(&input_str);
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

            match cmd.spawn() {
                Ok(mut child) => {
                    let stdout = child.stdout.take().expect("piped");
                    let stderr = child.stderr.take().expect("piped");
                    let tx_out = events_tx.clone();
                    let tx_err = events_tx.clone();
                    tokio::spawn(async move {
                        let mut r = BufReader::new(stdout).lines();
                        while let Ok(Some(line)) = r.next_line().await {
                            let _ = tx_out
                                .send(RunEvent::Log {
                                    line,
                                    stream: LogStream::Stdout,
                                })
                                .await;
                        }
                    });
                    tokio::spawn(async move {
                        let mut r = BufReader::new(stderr).lines();
                        while let Ok(Some(line)) = r.next_line().await {
                            let _ = tx_err
                                .send(RunEvent::Log {
                                    line,
                                    stream: LogStream::Stderr,
                                })
                                .await;
                        }
                    });
                    let status_or_cancel = tokio::select! {
                        s = child.wait() => Ok(s),
                        _ = cancel.cancelled() => {
                            let _ = child.kill().await;
                            Err(ModuleError::Cancelled)
                        }
                    };
                    match status_or_cancel {
                        Err(e) => return Err(e),
                        Ok(Ok(status)) => {
                            if status.success() {
                                if output_path.exists() {
                                    output_files.push(output_path.clone());
                                }
                                file_summaries.push(serde_json::json!({
                                    "file": input_str,
                                    "output": output_str,
                                    "status": "ok",
                                }));
                                log_lines.push(format!("OK: {} -> {}", input_str, output_str));
                            } else {
                                file_summaries.push(serde_json::json!({
                                    "file": input_str,
                                    "status": "error",
                                    "exit_code": status.code(),
                                }));
                                log_lines.push(format!(
                                    "ERROR: {} exit={}",
                                    input_str,
                                    status.code().unwrap_or(-1)
                                ));
                            }
                        }
                        Ok(Err(e)) => {
                            file_summaries.push(serde_json::json!({
                                "file": input_str, "status": "error", "error": e.to_string(),
                            }));
                            log_lines.push(format!("ERROR waiting for child: {}", e));
                        }
                    }
                }
                Err(e) => {
                    file_summaries.push(serde_json::json!({
                        "file": input_str, "status": "error", "error": e.to_string(),
                    }));
                    log_lines.push(format!("ERROR spawning: {}", e));
                }
            }
        }

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 1.0,
                message: "Done".into(),
            })
            .await;

        let ok_count = file_summaries
            .iter()
            .filter(|v| v["status"] == "ok")
            .count();
        let summary = serde_json::json!({
            "total_files": total,
            "trimmed_ok": ok_count,
            "output_directory": output_dir.display().to_string(),
            "adapter": adapter,
            "quality_cutoff": quality_cutoff,
            "min_length": min_length,
            "files": file_summaries,
        });

        Ok(ModuleResult {
            output_files,
            summary,
            log: log_lines.join("\n"),
        })
    }
}

#[cfg(test)]
mod ai_schema_tests {
    use super::*;
    use rb_core::module::Module;

    #[test]
    fn trimming_schema_declares_required_input_field() {
        let s = TrimmingModule.params_schema().unwrap();
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "input_files"));
        assert_eq!(s["type"], "object");
    }

    #[test]
    fn trimming_hint_nonempty_both_languages() {
        assert!(!TrimmingModule.ai_hint("en").is_empty());
        assert!(!TrimmingModule.ai_hint("zh").is_empty());
    }
}

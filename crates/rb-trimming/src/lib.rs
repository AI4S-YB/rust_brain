use rb_core::module::{Module, ModuleError, ModuleResult, Progress, ValidationError};
use std::path::{Path, PathBuf};
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
                if v.as_array().map_or(true, |a| a.is_empty()) {
                    errors.push(ValidationError {
                        field: "input_files".to_string(),
                        message: "input_files must be a non-empty array".to_string(),
                    });
                }
            }
        }

        errors
    }

    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        progress_tx: mpsc::Sender<Progress>,
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

        // Optional params
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
            let fraction = idx as f64 / total as f64;
            let _ = progress_tx
                .send(Progress {
                    fraction,
                    message: format!("Trimming {} ({}/{})", input_path.display(), idx + 1, total),
                })
                .await;

            let input_str = input_path.to_string_lossy().to_string();

            // Build output file path
            let file_name = input_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("output_{}.fastq.gz", idx));
            let output_path = output_dir.join(&file_name);
            let output_str = output_path.to_string_lossy().to_string();

            // Build cutadapt-rs subprocess command
            let mut cmd = std::process::Command::new("cutadapt-rs");
            cmd.arg("-o").arg(&output_str);
            cmd.arg("-q").arg(quality_cutoff.to_string());
            cmd.arg("-m").arg(min_length.to_string());
            if !adapter.is_empty() {
                cmd.arg("-a").arg(&adapter);
            }
            cmd.arg(&input_str);

            let child_result = cmd.output();

            match child_result {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let combined = format!("{}{}", stdout, stderr);

                    // Parse basic statistics from output
                    let reads_written = parse_stat(&combined, "Reads written");
                    let bp_written = parse_stat(&combined, "Basepairs written");

                    if output.status.success() {
                        if output_path.exists() {
                            output_files.push(output_path.clone());
                        }
                        file_summaries.push(serde_json::json!({
                            "file": input_str,
                            "output": output_str,
                            "status": "ok",
                            "reads_written": reads_written,
                            "bp_written": bp_written,
                        }));
                        log_lines.push(format!("OK: {} -> {}", input_str, output_str));
                        if !combined.is_empty() {
                            log_lines.push(combined);
                        }
                    } else {
                        file_summaries.push(serde_json::json!({
                            "file": input_str,
                            "status": "error",
                            "error": combined,
                        }));
                        log_lines.push(format!(
                            "ERROR: {} exit={} {}",
                            input_str,
                            output.status.code().unwrap_or(-1),
                            combined
                        ));
                    }
                }
                Err(e) => {
                    // cutadapt-rs binary not found — log and continue
                    file_summaries.push(serde_json::json!({
                        "file": input_str,
                        "status": "error",
                        "error": e.to_string(),
                    }));
                    log_lines.push(format!("ERROR: {} — {}", input_str, e));
                }
            }
        }

        let _ = progress_tx
            .send(Progress {
                fraction: 1.0,
                message: "Done".to_string(),
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

/// Attempt to extract a numeric value from cutadapt stdout for a given stat label.
fn parse_stat(output: &str, label: &str) -> Option<u64> {
    for line in output.lines() {
        if line.contains(label) {
            // Lines look like: "Reads written (passing filters):   1,234 (56.7%)"
            // Extract the first run of digits (ignoring commas)
            let digits: String = line
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit() || *c == ',')
                .filter(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                return digits.parse().ok();
            }
        }
    }
    None
}

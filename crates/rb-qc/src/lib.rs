use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
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

        let output_dir = project_dir.join("qc_output");
        std::fs::create_dir_all(&output_dir)?;

        let total = input_files.len();
        let mut output_files = Vec::new();
        let mut processed = Vec::new();
        let mut log_lines = Vec::new();

        for (idx, input_path) in input_files.iter().enumerate() {
            let fraction = idx as f64 / total as f64;
            let _ = events_tx
                .send(RunEvent::Progress {
                    fraction,
                    message: format!(
                        "Processing {} ({}/{})",
                        input_path.display(),
                        idx + 1,
                        total
                    ),
                })
                .await;

            let input_str = input_path.to_string_lossy().to_string();

            // Use fastqc-rs library directly via process_file
            let input_path_clone = input_path.clone();
            let output_dir_clone = output_dir.clone();
            let result = tokio::task::spawn_blocking(move || {
                let mut config = fastqc_rs::config::Config::default_config();
                config.files = vec![input_path_clone.clone()];
                config.output_dir = Some(output_dir_clone);
                config.quiet = true;
                fastqc_rs::analysis::process_file(&input_path_clone, &config)
            })
            .await
            .map_err(|e| ModuleError::ToolError(e.to_string()))?;

            match result {
                Ok(()) => {
                    // Determine expected output file name
                    let file_stem = input_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    let base = strip_seq_extensions(&file_stem);
                    let html_name = format!("{}_fastqc.html", base);
                    let zip_name = format!("{}_fastqc.zip", base);
                    let html_path = output_dir.join(&html_name);
                    let zip_path = output_dir.join(&zip_name);

                    if html_path.exists() {
                        output_files.push(html_path);
                    }
                    if zip_path.exists() {
                        output_files.push(zip_path);
                    }

                    processed.push(serde_json::json!({
                        "file": input_str,
                        "status": "ok",
                    }));
                    log_lines.push(format!("OK: {}", input_str));
                }
                Err(e) => {
                    processed.push(serde_json::json!({
                        "file": input_str,
                        "status": "error",
                        "error": e.to_string(),
                    }));
                    log_lines.push(format!("ERROR: {} — {}", input_str, e));
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

use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::{LogStream, RunEvent};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub struct RustqcModule;

fn sample_name_from_bam(bam: &str) -> String {
    let p = Path::new(bam);
    let mut name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    for ext in [".bam", ".sam", ".cram"] {
        if let Some(stripped) = name.strip_suffix(ext) {
            name = stripped.to_string();
        }
    }
    for suffix in [".markdup", ".mark_dup", ".dedup", ".sorted"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            name = stripped.to_string();
        }
    }
    name
}

#[async_trait::async_trait]
impl Module for RustqcModule {
    fn id(&self) -> &str {
        "rustqc"
    }
    fn name(&self) -> &str {
        "RustQC RNA-Seq Post-Alignment QC"
    }

    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "input_bams": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "description": "Duplicate-marked BAM/SAM/CRAM paths produced after alignment (one entry per sample)."
                },
                "gtf": {
                    "type": "string",
                    "description": "GTF gene annotation file (plain or .gz)."
                },
                "paired": {
                    "type": "boolean",
                    "default": false,
                    "description": "Paired-end reads (passes -p to rustqc rna)."
                },
                "stranded": {
                    "type": "string",
                    "enum": ["unstranded", "forward", "reverse"],
                    "description": "Library strandedness (passes -s). Omit for rustqc's default inference."
                },
                "threads": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Threads per sample (passes -t)."
                },
                "mapq": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "MAPQ cutoff (passes -Q). rustqc's default is 30."
                },
                "reference": {
                    "type": "string",
                    "description": "Reference FASTA (required for CRAM input)."
                },
                "output_dir": {
                    "type": "string",
                    "description": "Output directory (default: <project>/rustqc_output)."
                },
                "extra_args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Additional raw `rustqc rna` flags appended verbatim (escape hatch)."
                }
            },
            "required": ["input_bams", "gtf"],
            "additionalProperties": false
        }))
    }

    fn ai_hint(&self, lang: &str) -> String {
        match lang {
            "zh" => "用 run_rustqc 对 run_star_align 产出的已标记重复 BAM 做 RNA-Seq 后比对质控(一次调用跑 featureCounts/dupRadar/Qualimap/samtools stats/RSeQC)。input_bams 是 BAM 列表,gtf 与 STAR 索引使用的注释一致,paired=true 代表双端。输出目录与 MultiQC 兼容。".into(),
            _    => "Use run_rustqc for post-alignment RNA-Seq QC on duplicate-marked BAMs from run_star_align (runs featureCounts/dupRadar/Qualimap/samtools stats/RSeQC in one pass). `input_bams` lists BAMs, `gtf` must match the annotation used for STAR indexing, and `paired=true` indicates paired-end. Output is MultiQC-compatible.".into(),
        }
    }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let bams = params
            .get("input_bams")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if bams.is_empty() {
            errors.push(ValidationError {
                field: "input_bams".into(),
                message: "input_bams must be a non-empty array".into(),
            });
        }
        for (i, v) in bams.iter().enumerate() {
            match v.as_str() {
                None => errors.push(ValidationError {
                    field: format!("input_bams[{}]", i),
                    message: "must be a string path".into(),
                }),
                Some(p) => {
                    if !Path::new(p).exists() {
                        errors.push(ValidationError {
                            field: format!("input_bams[{}]", i),
                            message: format!("file does not exist: {}", p),
                        });
                    }
                }
            }
        }

        match params.get("gtf").and_then(|v| v.as_str()) {
            None => errors.push(ValidationError {
                field: "gtf".into(),
                message: "gtf is required".into(),
            }),
            Some(p) => {
                if !Path::new(p).exists() {
                    errors.push(ValidationError {
                        field: "gtf".into(),
                        message: format!("gtf does not exist: {}", p),
                    });
                }
            }
        }

        if let Some(v) = params.get("stranded").and_then(|v| v.as_str()) {
            if !matches!(v, "unstranded" | "forward" | "reverse") {
                errors.push(ValidationError {
                    field: "stranded".into(),
                    message: "stranded must be one of: unstranded, forward, reverse".into(),
                });
            }
        }

        if let Ok(resolver) = BinaryResolver::load() {
            if let Err(e) = resolver.resolve("rustqc") {
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
            .resolve("rustqc")
            .map_err(|e| ModuleError::ToolError(e.to_string()))?;

        let input_bams: Vec<PathBuf> = params["input_bams"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(PathBuf::from))
            .collect();
        let gtf = params["gtf"].as_str().unwrap().to_string();
        let paired = params
            .get("paired")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let stranded = params
            .get("stranded")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let threads = params.get("threads").and_then(|v| v.as_u64());
        let mapq = params.get("mapq").and_then(|v| v.as_u64());
        let reference = params
            .get("reference")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let extra_args: Vec<String> = params
            .get("extra_args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let output_dir = params
            .get("output_dir")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| project_dir.join("rustqc_output"));
        std::fs::create_dir_all(&output_dir)?;

        let total = input_bams.len();
        let mut output_files = Vec::new();
        let mut file_summaries = Vec::new();
        let mut log_lines = Vec::new();

        for (idx, bam_path) in input_bams.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(ModuleError::Cancelled);
            }
            let fraction = idx as f64 / total as f64;
            let bam_str = bam_path.to_string_lossy().to_string();
            let sample = sample_name_from_bam(&bam_str);
            let sample_outdir = output_dir.join(&sample);
            std::fs::create_dir_all(&sample_outdir)?;

            let _ = events_tx
                .send(RunEvent::Progress {
                    fraction,
                    message: format!("RustQC: {} ({}/{})", sample, idx + 1, total),
                })
                .await;

            let mut cmd = Command::new(&bin);
            cmd.arg("rna");
            cmd.arg("--gtf").arg(&gtf);
            cmd.arg("--outdir").arg(&sample_outdir);
            cmd.arg("--sample-name").arg(&sample);
            if paired {
                cmd.arg("-p");
            }
            if let Some(s) = &stranded {
                cmd.arg("-s").arg(s);
            }
            if let Some(t) = threads {
                cmd.arg("-t").arg(t.to_string());
            }
            if let Some(q) = mapq {
                cmd.arg("-Q").arg(q.to_string());
            }
            if let Some(r) = &reference {
                cmd.arg("-r").arg(r);
            }
            for a in &extra_args {
                cmd.arg(a);
            }
            cmd.arg(&bam_str);
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
                                if let Ok(entries) = std::fs::read_dir(&sample_outdir) {
                                    for e in entries.flatten() {
                                        output_files.push(e.path());
                                    }
                                }
                                file_summaries.push(serde_json::json!({
                                    "sample": sample,
                                    "input": bam_str,
                                    "output_dir": sample_outdir.display().to_string(),
                                    "status": "ok",
                                }));
                                log_lines.push(format!(
                                    "OK: {} -> {}",
                                    bam_str,
                                    sample_outdir.display()
                                ));
                            } else {
                                file_summaries.push(serde_json::json!({
                                    "sample": sample,
                                    "input": bam_str,
                                    "status": "error",
                                    "exit_code": status.code(),
                                }));
                                log_lines.push(format!(
                                    "ERROR: {} exit={}",
                                    bam_str,
                                    status.code().unwrap_or(-1)
                                ));
                            }
                        }
                        Ok(Err(e)) => {
                            file_summaries.push(serde_json::json!({
                                "sample": sample, "input": bam_str, "status": "error", "error": e.to_string(),
                            }));
                            log_lines.push(format!("ERROR waiting for child: {}", e));
                        }
                    }
                }
                Err(e) => {
                    file_summaries.push(serde_json::json!({
                        "sample": sample, "input": bam_str, "status": "error", "error": e.to_string(),
                    }));
                    log_lines.push(format!("ERROR spawning rustqc: {}", e));
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
            "total_samples": total,
            "processed_ok": ok_count,
            "output_directory": output_dir.display().to_string(),
            "gtf": gtf,
            "paired": paired,
            "stranded": stranded,
            "samples": file_summaries,
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
    fn rustqc_schema_requires_bam_and_gtf() {
        let s = RustqcModule.params_schema().unwrap();
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "input_bams"));
        assert!(req.iter().any(|v| v == "gtf"));
        assert_eq!(s["type"], "object");
    }

    #[test]
    fn rustqc_hint_nonempty_both_languages() {
        assert!(!RustqcModule.ai_hint("en").is_empty());
        assert!(!RustqcModule.ai_hint("zh").is_empty());
    }

    #[test]
    fn sample_name_strips_common_suffixes() {
        assert_eq!(
            sample_name_from_bam("/tmp/sample_A.markdup.bam"),
            "sample_A"
        );
        assert_eq!(sample_name_from_bam("/tmp/foo.sorted.bam"), "foo");
        assert_eq!(sample_name_from_bam("/tmp/plain.bam"), "plain");
    }
}

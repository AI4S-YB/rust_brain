use rb_core::asset::{AssetKind, DeclaredAsset};
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

pub mod normalize;

pub struct ExprNormModule;

#[async_trait::async_trait]
impl Module for ExprNormModule {
    fn id(&self) -> &str {
        "expr_norm"
    }
    fn name(&self) -> &str {
        "Expression Normalize"
    }

    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "counts": {
                    "type": "string",
                    "description": "Path to counts matrix TSV (header: gene_id, sample1, sample2, ...)."
                },
                "lengths": {
                    "type": "string",
                    "description": "Path to gene-length TSV produced by run_gene_length (must contain length_union and/or length_longest_tx columns)."
                },
                "length_mode": {
                    "type": "string",
                    "enum": ["union", "longest"],
                    "default": "union",
                    "description": "Which length column to use: 'union' (all-exon merge) or 'longest' (longest transcript)."
                },
                "method": {
                    "type": "string",
                    "enum": ["tpm", "fpkm", "both"],
                    "default": "tpm",
                    "description": "Normalization to compute: 'tpm', 'fpkm', or 'both' (writes one TSV per method)."
                }
            },
            "required": ["counts", "lengths"],
            "additionalProperties": false
        }))
    }

    fn ai_hint(&self, lang: &str) -> String {
        match lang {
            "zh" => "用 run_expr_norm 把 counts matrix 转成 TPM/FPKM。counts 一般来自 run_counts_merge,lengths 来自 run_gene_length,length_mode 选 union 或 longest,method 选 tpm/fpkm/both (both 会同时输出两个 TSV)。".into(),
            _ => "Use run_expr_norm to convert a counts matrix into TPM/FPKM. counts usually comes from run_counts_merge, lengths from run_gene_length. Pick length_mode=union or longest and method=tpm/fpkm/both ('both' writes one TSV per method).".into(),
        }
    }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        match params.get("counts").and_then(|v| v.as_str()) {
            None => errors.push(ValidationError {
                field: "counts".into(),
                message: "counts path is required".into(),
            }),
            Some("") => errors.push(ValidationError {
                field: "counts".into(),
                message: "counts path must not be empty".into(),
            }),
            Some(p) if !Path::new(p).is_file() => errors.push(ValidationError {
                field: "counts".into(),
                message: format!("counts file does not exist: {p}"),
            }),
            Some(_) => {}
        }
        match params.get("lengths").and_then(|v| v.as_str()) {
            None => errors.push(ValidationError {
                field: "lengths".into(),
                message: "lengths path is required".into(),
            }),
            Some("") => errors.push(ValidationError {
                field: "lengths".into(),
                message: "lengths path must not be empty".into(),
            }),
            Some(p) if !Path::new(p).is_file() => errors.push(ValidationError {
                field: "lengths".into(),
                message: format!("lengths file does not exist: {p}"),
            }),
            Some(_) => {}
        }
        if let Some(s) = params.get("length_mode").and_then(|v| v.as_str()) {
            if normalize::LengthMode::from_str(s).is_none() {
                errors.push(ValidationError {
                    field: "length_mode".into(),
                    message: format!("length_mode must be 'union' or 'longest', got: {s}"),
                });
            }
        }
        if let Some(s) = params.get("method").and_then(|v| v.as_str()) {
            if !matches!(s, "tpm" | "fpkm" | "both") {
                errors.push(ValidationError {
                    field: "method".into(),
                    message: format!("method must be 'tpm', 'fpkm', or 'both', got: {s}"),
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

        let counts_path = PathBuf::from(params["counts"].as_str().unwrap());
        let lengths_path = PathBuf::from(params["lengths"].as_str().unwrap());
        let length_mode_str = params
            .get("length_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("union");
        let length_mode = normalize::LengthMode::from_str(length_mode_str).expect("validated");
        let method_str = params
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("tpm");
        let methods: Vec<normalize::Method> = match method_str {
            "fpkm" => vec![normalize::Method::Fpkm],
            "both" => vec![normalize::Method::Tpm, normalize::Method::Fpkm],
            _ => vec![normalize::Method::Tpm],
        };

        std::fs::create_dir_all(project_dir)?;

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 0.05,
                message: "Reading counts matrix".into(),
            })
            .await;

        let cancel_for_blocking = cancel.clone();
        let project_dir_owned = project_dir.to_path_buf();
        let counts_for_blocking = counts_path.clone();
        let lengths_for_blocking = lengths_path.clone();
        let methods_for_blocking = methods.clone();

        let outcome: Result<NormOutcome, ModuleError> = tokio::task::spawn_blocking(move || {
            let counts =
                normalize::read_counts_tsv(&counts_for_blocking).map_err(ModuleError::ToolError)?;
            if cancel_for_blocking.is_cancelled() {
                return Err(ModuleError::Cancelled);
            }
            let lengths = normalize::read_gene_lengths(&lengths_for_blocking, length_mode)
                .map_err(ModuleError::ToolError)?;
            if cancel_for_blocking.is_cancelled() {
                return Err(ModuleError::Cancelled);
            }

            let mut output_paths = Vec::new();
            let mut method_summary: Vec<serde_json::Value> = Vec::new();
            let mut last_stats: Option<normalize::NormalizeStats> = None;
            for method in &methods_for_blocking {
                let (matrix, stats) = match method {
                    normalize::Method::Tpm => normalize::compute_tpm(&counts, &lengths),
                    normalize::Method::Fpkm => normalize::compute_fpkm(&counts, &lengths),
                };
                let out_name = format!("{}_matrix.tsv", method.label());
                let out_path = project_dir_owned.join(&out_name);
                normalize::write_matrix(&out_path, &matrix)
                    .map_err(|e| ModuleError::ToolError(format!("write {}: {e}", out_name)))?;
                method_summary.push(serde_json::json!({
                    "method": method.label(),
                    "output": out_path.display().to_string(),
                    "matched_count": stats.matched_count,
                    "missing_length_count": stats.missing_length_count,
                    "zero_length_count": stats.zero_length_count,
                }));
                output_paths.push(out_path);
                last_stats = Some(stats);
            }

            Ok(NormOutcome {
                samples: counts.samples,
                input_gene_count: counts.gene_ids.len(),
                last_stats: last_stats.unwrap_or_default(),
                outputs: output_paths,
                methods: method_summary,
            })
        })
        .await
        .map_err(|e| ModuleError::ToolError(format!("worker join: {e}")))?;

        let outcome = outcome?;

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 1.0,
                message: "Done".into(),
            })
            .await;

        let summary = serde_json::json!({
            "counts_input": counts_path.display().to_string(),
            "lengths_input": lengths_path.display().to_string(),
            "length_mode": length_mode_str,
            "samples": outcome.samples,
            "sample_count": outcome.samples.len(),
            "input_gene_count": outcome.input_gene_count,
            "matched_count": outcome.last_stats.matched_count,
            "missing_length_count": outcome.last_stats.missing_length_count,
            "zero_length_count": outcome.last_stats.zero_length_count,
            "methods": outcome.methods,
        });

        let log = format!(
            "Normalized {} genes ({} matched, {} missing length, {} zero length) across {} samples; wrote {} matrix file(s)",
            outcome.input_gene_count,
            outcome.last_stats.matched_count,
            outcome.last_stats.missing_length_count,
            outcome.last_stats.zero_length_count,
            outcome.samples.len(),
            outcome.outputs.len()
        );

        Ok(ModuleResult {
            output_files: outcome.outputs,
            summary,
            log,
        })
    }

    fn produced_assets(&self, result: &ModuleResult) -> Vec<DeclaredAsset> {
        let Some(methods) = result.summary.get("methods").and_then(|v| v.as_array()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in methods {
            let Some(path_str) = entry.get("output").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(file_name) = Path::new(path_str).file_name() else {
                continue;
            };
            let label = entry
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("matrix")
                .to_uppercase();
            out.push(DeclaredAsset {
                kind: AssetKind::Other,
                relative_path: PathBuf::from(file_name.to_os_string()),
                display_name: format!("{label} matrix"),
                schema: Some("gene_id x samples normalized expression (TSV)".into()),
            });
        }
        out
    }
}

struct NormOutcome {
    samples: Vec<String>,
    input_gene_count: usize,
    last_stats: normalize::NormalizeStats,
    outputs: Vec<PathBuf>,
    methods: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_requires_counts_and_lengths() {
        let errs = ExprNormModule.validate(&json!({}));
        assert!(errs.iter().any(|e| e.field == "counts"));
        assert!(errs.iter().any(|e| e.field == "lengths"));
    }

    #[test]
    fn validate_rejects_unknown_length_mode() {
        let counts = tempfile::NamedTempFile::new().unwrap();
        let lengths = tempfile::NamedTempFile::new().unwrap();
        let errs = ExprNormModule.validate(&json!({
            "counts": counts.path().to_string_lossy(),
            "lengths": lengths.path().to_string_lossy(),
            "length_mode": "median",
        }));
        assert!(errs.iter().any(|e| e.field == "length_mode"));
    }

    #[test]
    fn validate_rejects_unknown_method() {
        let counts = tempfile::NamedTempFile::new().unwrap();
        let lengths = tempfile::NamedTempFile::new().unwrap();
        let errs = ExprNormModule.validate(&json!({
            "counts": counts.path().to_string_lossy(),
            "lengths": lengths.path().to_string_lossy(),
            "method": "rpkm",
        }));
        assert!(errs.iter().any(|e| e.field == "method"));
    }

    #[test]
    fn validate_accepts_tpm_fpkm_or_both() {
        let counts = tempfile::NamedTempFile::new().unwrap();
        let lengths = tempfile::NamedTempFile::new().unwrap();
        for m in &["tpm", "fpkm", "both"] {
            let errs = ExprNormModule.validate(&json!({
                "counts": counts.path().to_string_lossy(),
                "lengths": lengths.path().to_string_lossy(),
                "method": m,
            }));
            assert!(
                errs.iter().all(|e| e.field != "method"),
                "expected method '{m}' accepted, got {:?}",
                errs
            );
        }
    }

    #[test]
    fn schema_requires_counts_and_lengths() {
        let s = ExprNormModule.params_schema().unwrap();
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "counts"));
        assert!(req.iter().any(|v| v == "lengths"));
    }
}

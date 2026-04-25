use rb_core::asset::{AssetKind, DeclaredAsset};
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

pub mod gtf;

pub struct GeneLengthModule;

#[async_trait::async_trait]
impl Module for GeneLengthModule {
    fn id(&self) -> &str {
        "gene_length"
    }
    fn name(&self) -> &str {
        "Gene Length"
    }

    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "gtf": {
                    "type": "string",
                    "description": "Path to a GTF annotation file."
                },
                "output_name": {
                    "type": "string",
                    "default": "gene_lengths.tsv",
                    "description": "Output TSV filename inside the run directory."
                }
            },
            "required": ["gtf"],
            "additionalProperties": false
        }))
    }

    fn ai_hint(&self, lang: &str) -> String {
        match lang {
            "zh" => "用 run_gene_length 从 GTF 提取每个 gene 的两种长度: length_union (所有外显子并集去重后总长) 和 length_longest_tx (最长转录本的外显子总长). 输出 TSV 可作为 run_expr_norm 的长度输入.".into(),
            _ => "Use run_gene_length to derive per-gene lengths from a GTF: length_union (union of all exons across transcripts) and length_longest_tx (sum of exon lengths in the longest transcript). The TSV output feeds run_expr_norm.".into(),
        }
    }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        match params.get("gtf").and_then(|v| v.as_str()) {
            None => errors.push(ValidationError {
                field: "gtf".into(),
                message: "gtf path is required".into(),
            }),
            Some("") => errors.push(ValidationError {
                field: "gtf".into(),
                message: "gtf path must not be empty".into(),
            }),
            Some(p) if !Path::new(p).is_file() => errors.push(ValidationError {
                field: "gtf".into(),
                message: format!("gtf file does not exist: {p}"),
            }),
            Some(_) => {}
        }
        if let Some(name) = params.get("output_name").and_then(|v| v.as_str()) {
            if name.trim().is_empty()
                || name.contains('/')
                || name.contains('\\')
                || name == "."
                || name == ".."
            {
                errors.push(ValidationError {
                    field: "output_name".into(),
                    message: "output_name must be a filename inside the run directory".into(),
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
        let gtf_path = PathBuf::from(params["gtf"].as_str().unwrap());
        let output_name = params
            .get("output_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("gene_lengths.tsv");
        let output_path = project_dir.join(output_name);
        std::fs::create_dir_all(project_dir)?;

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 0.05,
                message: format!("Parsing {}", gtf_path.display()),
            })
            .await;

        let cancel_for_blocking = cancel.clone();
        let gtf_for_blocking = gtf_path.clone();
        let output_for_blocking = output_path.clone();
        let (gene_count, tx_count, exon_count) = tokio::task::spawn_blocking(move || {
            let (genes, stats) = gtf::parse_gtf(&gtf_for_blocking).map_err(|e| {
                ModuleError::ToolError(format!("parse GTF: {} ({})", gtf_for_blocking.display(), e))
            })?;
            if cancel_for_blocking.is_cancelled() {
                return Err(ModuleError::Cancelled);
            }
            let mut f = std::fs::File::create(&output_for_blocking)
                .map_err(|e| ModuleError::ToolError(format!("create output: {e}")))?;
            writeln!(f, "gene_id\tlength_union\tlength_longest_tx")
                .map_err(|e| ModuleError::ToolError(format!("write header: {e}")))?;
            for g in &genes {
                let (u, lt) = gtf::gene_lengths(g);
                writeln!(f, "{}\t{}\t{}", g.gene_id, u, lt)
                    .map_err(|e| ModuleError::ToolError(format!("write row: {e}")))?;
            }
            Ok::<_, ModuleError>((stats.gene_count, stats.transcript_count, stats.exon_count))
        })
        .await
        .map_err(|e| ModuleError::ToolError(format!("worker join: {e}")))??;

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 1.0,
                message: "Done".into(),
            })
            .await;

        let summary = serde_json::json!({
            "gtf": gtf_path.display().to_string(),
            "output": output_path.display().to_string(),
            "gene_count": gene_count,
            "transcript_count": tx_count,
            "exon_count": exon_count,
        });

        Ok(ModuleResult {
            output_files: vec![output_path],
            summary,
            log: format!("Parsed {gene_count} genes / {tx_count} transcripts / {exon_count} exons"),
        })
    }

    fn produced_assets(&self, result: &ModuleResult) -> Vec<DeclaredAsset> {
        let Some(path) = result.summary.get("output").and_then(|v| v.as_str()) else {
            return Vec::new();
        };
        let Some(file_name) = Path::new(path).file_name() else {
            return Vec::new();
        };
        vec![DeclaredAsset {
            kind: AssetKind::Other,
            relative_path: PathBuf::from(file_name.to_os_string()),
            display_name: "Gene lengths".into(),
            schema: Some("gene_id, length_union, length_longest_tx (TSV)".into()),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_requires_gtf_path() {
        let errs = GeneLengthModule.validate(&json!({}));
        assert!(errs.iter().any(|e| e.field == "gtf"));
    }

    #[test]
    fn validate_rejects_missing_gtf_file() {
        let errs = GeneLengthModule.validate(&json!({"gtf": "/nope/missing.gtf"}));
        assert!(errs.iter().any(|e| e.field == "gtf"));
    }

    #[test]
    fn validate_rejects_bad_output_name() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let errs = GeneLengthModule.validate(&json!({
            "gtf": tmp.path().to_string_lossy(),
            "output_name": "../etc/passwd",
        }));
        assert!(errs.iter().any(|e| e.field == "output_name"));
    }

    #[test]
    fn schema_has_required_gtf() {
        let s = GeneLengthModule.params_schema().unwrap();
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "gtf"));
    }
}

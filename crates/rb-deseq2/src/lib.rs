use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::Path;
use tokio::sync::mpsc;

pub struct DeseqModule;

/// Normalize a user-supplied design string.
/// Accepts both bare column names ("condition") and R-style single-factor
/// formulas ("~condition", "~ condition"). Multi-factor operators are kept
/// in place so callers can detect and reject them in validate().
fn normalize_design(raw: &str) -> String {
    let s = raw.trim();
    let s = s.strip_prefix('~').unwrap_or(s);
    s.trim().to_string()
}

#[async_trait::async_trait]
impl Module for DeseqModule {
    fn id(&self) -> &str {
        "deseq2"
    }

    fn name(&self) -> &str {
        "DESeq2 Differential Expression"
    }

    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "counts_path": {
                    "type": "string",
                    "description": "Path to counts matrix TSV (from run_counts_merge or equivalent)."
                },
                "coldata_path": {
                    "type": "string",
                    "description": "Path to sample metadata TSV/CSV with a condition column."
                },
                "design": {
                    "type": "string",
                    "description": "Name of a single factor column in coldata (e.g. 'condition'). A leading '~' is tolerated for R compatibility, but multi-factor formulas like '~batch+condition' are not yet supported."
                },
                "reference": {
                    "type": "string",
                    "description": "Reference level of the design factor used as the baseline for contrasts."
                }
            },
            "required": ["counts_path", "coldata_path", "design", "reference"],
            "additionalProperties": false
        }))
    }

    fn ai_hint(&self, lang: &str) -> String {
        match lang {
            "zh" => "用 run_deseq2 做差异表达分析。counts_path 通常是 run_counts_merge 产出的 counts_matrix.tsv;coldata_path 是用户在项目里提供的样本分组表;design 是 coldata 里的**单个因子列名**(例如 'condition'),当前不支持 ~batch+condition 这类多因子公式;reference 指定该因子的基线水平。".into(),
            _ => "Use run_deseq2 for differential expression analysis. counts_path is typically the counts_matrix.tsv produced by run_counts_merge; coldata_path is a user-provided sample metadata table; design is the name of a SINGLE factor column in coldata (e.g. 'condition') — multi-factor formulas like '~batch+condition' are not supported yet; reference sets the baseline level of that factor.".into(),
        }
    }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for field in &["counts_path", "coldata_path", "design", "reference"] {
            match params.get(field).and_then(|v| v.as_str()) {
                None | Some("") => {
                    errors.push(ValidationError {
                        field: field.to_string(),
                        message: format!("{} must be a non-empty string", field),
                    });
                }
                _ => {}
            }
        }

        if let Some(raw) = params.get("design").and_then(|v| v.as_str()) {
            let normalized = normalize_design(raw);
            if normalized.is_empty() {
                errors.push(ValidationError {
                    field: "design".into(),
                    message: "design must name a factor column in coldata (e.g. 'condition')"
                        .into(),
                });
            } else if normalized.contains(['+', '*', ':']) {
                errors.push(ValidationError {
                    field: "design".into(),
                    message: format!(
                        "multi-factor formula '{}' is not supported yet; provide a single column name",
                        raw
                    ),
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

        let counts_path = params["counts_path"].as_str().unwrap().to_string();
        let coldata_path = params["coldata_path"].as_str().unwrap().to_string();
        let design = normalize_design(params["design"].as_str().unwrap());
        let reference = params["reference"].as_str().unwrap().to_string();
        let output_dir = project_dir.to_path_buf();

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 0.0,
                message: "Loading count matrix and coldata...".to_string(),
            })
            .await;

        let results = tokio::task::spawn_blocking(move || {
            use deseq2_rs::data::Contrast;
            use deseq2_rs::pipeline::DESeqDataSet;
            use std::path::Path;

            let mut dds = DESeqDataSet::from_csv(
                Path::new(&counts_path),
                Path::new(&coldata_path),
                &design,
                &reference,
            )
            .map_err(|e| ModuleError::ToolError(e.to_string()))?;

            dds.run()
                .map_err(|e| ModuleError::ToolError(e.to_string()))?;

            let results = dds
                .results(Contrast::LastCoefficient)
                .map_err(|e| ModuleError::ToolError(e))?;

            Ok::<_, ModuleError>((results, output_dir))
        })
        .await
        .map_err(|e| ModuleError::ToolError(e.to_string()))??;

        let (gene_results, output_dir) = results;

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 0.8,
                message: "Writing results TSV...".to_string(),
            })
            .await;

        // Write TSV output
        let tsv_path = output_dir.join("deseq2_results.tsv");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tsv_path)?;
            writeln!(
                f,
                "gene\tbaseMean\tlog2FoldChange\tlfcSE\tstat\tpvalue\tpadj"
            )?;
            for r in &gene_results {
                writeln!(
                    f,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    r.gene,
                    r.base_mean,
                    r.log2_fold_change,
                    r.lfc_se,
                    r.stat,
                    r.p_value,
                    r.p_adjusted
                )?;
            }
        }

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 1.0,
                message: "Done".to_string(),
            })
            .await;

        // Build summary statistics
        let total_genes = gene_results.len();
        let significant: Vec<_> = gene_results
            .iter()
            .filter(|r| r.p_adjusted < 0.05)
            .collect();
        let sig_count = significant.len();
        let up_count = significant
            .iter()
            .filter(|r| r.log2_fold_change > 0.0)
            .count();
        let down_count = sig_count - up_count;

        let per_gene: Vec<serde_json::Value> = gene_results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "gene": r.gene,
                    "baseMean": r.base_mean,
                    "log2FoldChange": r.log2_fold_change,
                    "lfcSE": r.lfc_se,
                    "stat": r.stat,
                    "pvalue": r.p_value,
                    "padj": r.p_adjusted,
                })
            })
            .collect();

        let summary = serde_json::json!({
            "total_genes": total_genes,
            "significant": sig_count,
            "up": up_count,
            "down": down_count,
            "results": per_gene,
        });

        Ok(ModuleResult {
            output_files: vec![tsv_path],
            summary,
            log: format!(
                "DESeq2 complete: {} genes tested, {} significant (padj < 0.05), {} up, {} down",
                total_genes, sig_count, up_count, down_count
            ),
        })
    }
}

#[cfg(test)]
mod ai_schema_tests {
    use super::*;
    use rb_core::module::Module;

    #[test]
    fn deseq2_schema_requires_counts_and_metadata_fields() {
        let s = DeseqModule.params_schema().unwrap();
        assert_eq!(s["type"], "object");
        let req = s["required"].as_array().unwrap();
        assert!(req.len() >= 2, "expected >=2 required fields");
        assert!(req.iter().any(|v| v == "counts_path"));
        assert!(req.iter().any(|v| v == "coldata_path"));
        assert!(req.iter().any(|v| v == "design"));
        assert!(req.iter().any(|v| v == "reference"));
    }
    #[test]
    fn deseq2_hint_nonempty_both_languages() {
        assert!(!DeseqModule.ai_hint("en").is_empty());
        assert!(!DeseqModule.ai_hint("zh").is_empty());
    }

    #[test]
    fn normalize_design_strips_tilde_and_whitespace() {
        assert_eq!(normalize_design("condition"), "condition");
        assert_eq!(normalize_design("~condition"), "condition");
        assert_eq!(normalize_design("  ~ condition "), "condition");
        assert_eq!(normalize_design("~"), "");
    }

    #[test]
    fn validate_rejects_multi_factor_design() {
        let params = serde_json::json!({
            "counts_path": "counts.tsv",
            "coldata_path": "coldata.tsv",
            "design": "~batch+condition",
            "reference": "ctrl",
        });
        let errs = DeseqModule.validate(&params);
        assert!(
            errs.iter()
                .any(|e| e.field == "design" && e.message.contains("not supported")),
            "expected multi-factor rejection, got {:?}",
            errs
        );
    }

    #[test]
    fn validate_accepts_tilde_prefixed_single_factor() {
        let params = serde_json::json!({
            "counts_path": "counts.tsv",
            "coldata_path": "coldata.tsv",
            "design": "~condition",
            "reference": "ctrl",
        });
        let errs = DeseqModule.validate(&params);
        assert!(
            errs.iter().all(|e| e.field != "design"),
            "expected tilde-prefixed design to pass, got {:?}",
            errs
        );
    }
}

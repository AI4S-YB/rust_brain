use rb_core::module::{Module, ModuleError, ModuleResult, Progress, ValidationError};
use std::path::Path;
use tokio::sync::mpsc;

pub struct DeseqModule;

#[async_trait::async_trait]
impl Module for DeseqModule {
    fn id(&self) -> &str {
        "deseq2"
    }

    fn name(&self) -> &str {
        "DESeq2 Differential Expression"
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

        let counts_path = params["counts_path"].as_str().unwrap().to_string();
        let coldata_path = params["coldata_path"].as_str().unwrap().to_string();
        let design = params["design"].as_str().unwrap().to_string();
        let reference = params["reference"].as_str().unwrap().to_string();
        let output_dir = project_dir.to_path_buf();

        let _ = progress_tx
            .send(Progress {
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

            dds.run().map_err(|e| ModuleError::ToolError(e.to_string()))?;

            let results = dds
                .results(Contrast::LastCoefficient)
                .map_err(|e| ModuleError::ToolError(e))?;

            Ok::<_, ModuleError>((results, output_dir))
        })
        .await
        .map_err(|e| ModuleError::ToolError(e.to_string()))??;

        let (gene_results, output_dir) = results;

        let _ = progress_tx
            .send(Progress {
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

        let _ = progress_tx
            .send(Progress {
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

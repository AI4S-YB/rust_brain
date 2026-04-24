pub mod counts;
pub mod log_final;
mod subprocess;

use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::asset::{AssetKind, DeclaredAsset};
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

pub struct StarAlignModule;
pub struct CountsMergeModule;

fn sample_name_from_r1(r1: &str) -> String {
    let p = Path::new(r1);
    let mut name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
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

fn sample_name_from_reads_per_gene(path: &str) -> String {
    let p = Path::new(path);
    if let Some(parent) = p.parent().and_then(|x| x.file_name()) {
        let name = parent.to_string_lossy();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    p.file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "sample".into())
}

fn string_array_or_lines(params: &serde_json::Value, key: &str) -> Vec<String> {
    match params.get(key) {
        Some(v) if v.is_array() => v
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
        Some(v) if v.is_string() => v
            .as_str()
            .unwrap()
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn validate_sample_names(
    names: &[String],
    expected_len: usize,
    field: &str,
    errors: &mut Vec<ValidationError>,
) {
    if !names.is_empty() && names.len() != expected_len {
        errors.push(ValidationError {
            field: field.into(),
            message: format!(
                "{} length ({}) must match input length ({})",
                field,
                names.len(),
                expected_len
            ),
        });
    }
    let mut seen = std::collections::HashSet::new();
    for (i, s) in names.iter().enumerate() {
        if s.is_empty()
            || !s
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        {
            errors.push(ValidationError {
                field: format!("{}[{}]", field, i),
                message: "must be non-empty and match [A-Za-z0-9_.-]+".into(),
            });
        }
        if !seen.insert(s) {
            errors.push(ValidationError {
                field: format!("{}[{}]", field, i),
                message: format!("duplicate sample name: {}", s),
            });
        }
    }
}

fn strand_from_params(params: &serde_json::Value) -> Result<(String, counts::Strand), String> {
    let strand_str = params
        .get("strand")
        .and_then(|v| v.as_str())
        .unwrap_or("unstranded")
        .to_string();
    let strand = counts::Strand::from_str(&strand_str)
        .ok_or_else(|| format!("strand must be unstranded/forward/reverse, got '{}'", strand_str))?;
    Ok((strand_str, strand))
}

#[async_trait::async_trait]
impl Module for StarAlignModule {
    fn id(&self) -> &str {
        "star_align"
    }
    fn name(&self) -> &str {
        "STAR Single-Sample Alignment"
    }

    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "genome_dir": {
                    "type": "string",
                    "description": "Path to STAR index directory produced by run_star_index (must contain an SA file)."
                },
                "reads_1": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "maxItems": 1,
                    "description": "FASTQ path for mate 1 (or single-end reads). STAR runs one sample at a time."
                },
                "reads_2": {
                    "type": "array",
                    "items": { "type": "string" },
                    "maxItems": 1,
                    "description": "FASTQ path for mate 2. Omit or leave empty for single-end."
                },
                "sample_names": {
                    "type": "array",
                    "items": { "type": "string" },
                    "maxItems": 1,
                    "description": "Optional sample name (chars [A-Za-z0-9_.-]). Defaults are derived from the reads_1 filename."
                },
                "threads": {
                    "type": "integer",
                    "minimum": 1,
                    "default": 4,
                    "description": "Threads passed to STAR via --runThreadN."
                },
                "extra_args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Additional raw STAR CLI flags appended verbatim (escape hatch for power users/AI)."
                }
            },
            "required": ["genome_dir", "reads_1"],
            "additionalProperties": false
        }))
    }

    fn ai_hint(&self, lang: &str) -> String {
        match lang {
            "zh" => "用 run_star_align 把单个样本的 reads 比对到基因组,产出 BAM、Log.final.out 和 ReadsPerGene.out.tab。多个样本完成后,再用 run_counts_merge 合并多个 ReadsPerGene.out.tab 生成 DESeq2 所需的 counts_matrix.tsv。".into(),
            _    => "Use run_star_align to align one sample at a time and produce BAM, Log.final.out, and ReadsPerGene.out.tab. After multiple samples finish, use run_counts_merge to merge ReadsPerGene.out.tab files into the counts_matrix.tsv consumed by DESeq2.".into(),
        }
    }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        match params.get("genome_dir").and_then(|v| v.as_str()) {
            None => errors.push(ValidationError {
                field: "genome_dir".into(),
                message: "genome_dir is required".into(),
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
                        message: format!(
                            "genome_dir does not look like a STAR index (missing SA): {}",
                            s
                        ),
                    });
                }
            }
        }

        let r1 = params
            .get("reads_1")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if r1.is_empty() {
            errors.push(ValidationError {
                field: "reads_1".into(),
                message: "reads_1 must be a non-empty array".into(),
            });
        }
        if r1.len() > 1 {
            errors.push(ValidationError {
                field: "reads_1".into(),
                message: "STAR alignment runs one sample at a time; provide exactly one reads_1 path".into(),
            });
        }
        for (i, v) in r1.iter().enumerate() {
            match v.as_str() {
                None => errors.push(ValidationError {
                    field: format!("reads_1[{}]", i),
                    message: "must be a string path".into(),
                }),
                Some(p) => {
                    if !Path::new(p).exists() {
                        errors.push(ValidationError {
                            field: format!("reads_1[{}]", i),
                            message: format!("file does not exist: {}", p),
                        });
                    }
                }
            }
        }

        if let Some(r2) = params.get("reads_2").and_then(|v| v.as_array()) {
            if r2.len() > 1 {
                errors.push(ValidationError {
                    field: "reads_2".into(),
                    message: "STAR alignment runs one sample at a time; provide at most one reads_2 path".into(),
                });
            }
            if !r2.is_empty() && r2.len() != r1.len() {
                errors.push(ValidationError {
                    field: "reads_2".into(),
                    message: format!(
                        "reads_2 length ({}) must match reads_1 length ({})",
                        r2.len(),
                        r1.len()
                    ),
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

        let names = string_array_or_lines(params, "sample_names");
        validate_sample_names(&names, r1.len(), "sample_names", &mut errors);

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
            .resolve("star")
            .map_err(|e| ModuleError::ToolError(e.to_string()))?;

        let genome_dir = params["genome_dir"].as_str().unwrap().to_string();
        let reads_1: Vec<String> = params["reads_1"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let reads_2: Vec<String> = params
            .get("reads_2")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let mut sample_names = string_array_or_lines(params, "sample_names");
        if sample_names.is_empty() {
            sample_names = reads_1.iter().map(|r| sample_name_from_r1(r)).collect();
        }
        let threads = params.get("threads").and_then(|v| v.as_u64()).unwrap_or(4);
        let extra: Vec<String> = params
            .get("extra_args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let run_dir = project_dir.to_path_buf();
        std::fs::create_dir_all(&run_dir)?;

        let total = reads_1.len();
        let mut samples_summary: Vec<serde_json::Value> = Vec::with_capacity(total);
        let mut output_files: Vec<PathBuf> = Vec::new();
        let mut combined_log = String::new();

        for i in 0..total {
            if cancel.is_cancelled() {
                return Err(ModuleError::Cancelled);
            }
            let name = &sample_names[i];
            let r1 = &reads_1[i];
            let r2 = reads_2.get(i);
            let fraction = i as f64 / total as f64;
            let _ = events_tx
                .send(RunEvent::Progress {
                    fraction,
                    message: format!("Aligning {} ({}/{})", name, i + 1, total),
                })
                .await;

            let sample_out = run_dir.join(name);
            std::fs::create_dir_all(&sample_out)?;
            let prefix = format!("{}/", sample_out.display());

            let is_gz = r1.ends_with(".gz") || r2.map(|p| p.ends_with(".gz")).unwrap_or(false);

            let mut args: Vec<String> = vec![
                "--runMode".into(),
                "alignReads".into(),
                "--genomeDir".into(),
                genome_dir.clone(),
                "--readFilesIn".into(),
                r1.clone(),
            ];
            if let Some(r2v) = r2 {
                args.push(r2v.clone());
            }
            if is_gz {
                args.push("--readFilesCommand".into());
                args.push("zcat".into());
            }
            args.push("--outFileNamePrefix".into());
            args.push(prefix);
            args.push("--runThreadN".into());
            args.push(threads.to_string());
            args.push("--quantMode".into());
            args.push("GeneCounts".into());
            args.push("--outSAMtype".into());
            args.push("BAM".into());
            args.push("Unsorted".into());
            args.extend(extra.iter().cloned());

            let status =
                subprocess::run_star_streaming(&bin, &args, events_tx.clone(), cancel.clone())
                    .await?;

            let log_final_path = sample_out.join("Log.final.out");
            let reads_per_gene = sample_out.join("ReadsPerGene.out.tab");
            let bam = sample_out.join("Aligned.out.bam");

            if !status.success() {
                samples_summary.push(serde_json::json!({
                    "name": name, "r1": r1, "r2": r2,
                    "status": "error",
                    "exit_code": status.code(),
                }));
                combined_log.push_str(&format!(
                    "\n[{}] STAR exited with code {}\n",
                    name,
                    status.code().unwrap_or(-1)
                ));
                continue;
            }

            let log_stats = std::fs::read_to_string(&log_final_path)
                .ok()
                .map(|t| log_final::parse(&t));
            let sample_counts = counts::read_reads_per_gene(
                &reads_per_gene,
                counts::Strand::Unstranded,
            )
            .ok();

            let stats_json = log_stats
                .as_ref()
                .map(|s| {
                    let counts_summary = sample_counts.as_ref().map(|c| &c.summary);
                    serde_json::json!({
                        "input_reads":           s.input_reads,
                        "uniquely_mapped":       s.uniquely_mapped,
                        "uniquely_mapped_pct":   s.uniquely_mapped_pct,
                        "multi_mapped":          s.multi_mapped,
                        "multi_mapped_pct":      s.multi_mapped_pct,
                        "unmapped":              s.unmapped,
                        "unmapped_pct":          s.unmapped_pct,
                        "n_unmapped":            counts_summary.map(|c| c.n_unmapped),
                        "n_multimapping":        counts_summary.map(|c| c.n_multimapping),
                        "n_nofeature":           counts_summary.map(|c| c.n_nofeature),
                        "n_ambiguous":           counts_summary.map(|c| c.n_ambiguous),
                    })
                })
                .unwrap_or(serde_json::Value::Null);

            samples_summary.push(serde_json::json!({
                "name": name, "r1": r1, "r2": r2,
                "status": "ok",
                "bam": bam.display().to_string(),
                "reads_per_gene": reads_per_gene.display().to_string(),
                "log_final":      log_final_path.display().to_string(),
                "stats": stats_json,
            }));

            if bam.exists() {
                output_files.push(bam);
            }
            if reads_per_gene.exists() {
                output_files.push(reads_per_gene);
            }
            if log_final_path.exists() {
                output_files.push(log_final_path);
            }
        }

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 1.0,
                message: "Done".into(),
            })
            .await;

        let summary = serde_json::json!({
            "run_dir": run_dir.display().to_string(),
            "reads_per_gene": samples_summary
                .iter()
                .find_map(|s| s.get("reads_per_gene").and_then(|v| v.as_str()))
                .unwrap_or(""),
            "genome_dir": genome_dir,
            "samples": samples_summary,
        });

        Ok(ModuleResult {
            output_files,
            summary,
            log: combined_log,
        })
    }
}

#[async_trait::async_trait]
impl Module for CountsMergeModule {
    fn id(&self) -> &str {
        "counts_merge"
    }

    fn name(&self) -> &str {
        "Counts Matrix Merge"
    }

    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "reads_per_gene": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "description": "ReadsPerGene.out.tab files produced by STAR, one per sample."
                },
                "sample_names": {
                    "type": ["array", "string"],
                    "description": "Optional sample names matching reads_per_gene order. A textarea string is split by lines."
                },
                "strand": {
                    "type": "string",
                    "enum": ["unstranded", "forward", "reverse"],
                    "default": "unstranded",
                    "description": "Which count column to use from each ReadsPerGene.out.tab file."
                },
                "output_name": {
                    "type": "string",
                    "default": "counts_matrix.tsv",
                    "description": "Output TSV filename written inside this run directory."
                }
            },
            "required": ["reads_per_gene"],
            "additionalProperties": false
        }))
    }

    fn ai_hint(&self, lang: &str) -> String {
        match lang {
            "zh" => "用 run_counts_merge 合并多个 STAR ReadsPerGene.out.tab 文件。reads_per_gene 按样本顺序传入, sample_names 可选但建议提供; strand 选择使用 unstranded/forward/reverse 哪一列,输出 counts_matrix.tsv 供 run_deseq2 使用。".into(),
            _ => "Use run_counts_merge to merge multiple STAR ReadsPerGene.out.tab files. Pass reads_per_gene in sample order, optionally provide sample_names, choose the unstranded/forward/reverse count column with strand, and use the output counts_matrix.tsv in run_deseq2.".into(),
        }
    }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let files = params
            .get("reads_per_gene")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if files.is_empty() {
            errors.push(ValidationError {
                field: "reads_per_gene".into(),
                message: "reads_per_gene must contain at least one file".into(),
            });
        }
        for (i, v) in files.iter().enumerate() {
            match v.as_str() {
                Some(path) if Path::new(path).is_file() => {}
                Some(path) => errors.push(ValidationError {
                    field: format!("reads_per_gene[{}]", i),
                    message: format!("file does not exist: {}", path),
                }),
                None => errors.push(ValidationError {
                    field: format!("reads_per_gene[{}]", i),
                    message: "must be a string path".into(),
                }),
            }
        }

        let names = string_array_or_lines(params, "sample_names");
        validate_sample_names(&names, files.len(), "sample_names", &mut errors);

        if let Err(message) = strand_from_params(params) {
            errors.push(ValidationError {
                field: "strand".into(),
                message,
            });
        }

        if let Some(output_name) = params.get("output_name").and_then(|v| v.as_str()) {
            if output_name.trim().is_empty()
                || output_name.contains('/')
                || output_name.contains('\\')
                || output_name == "."
                || output_name == ".."
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
        _cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        let errors = self.validate(params);
        if !errors.is_empty() {
            return Err(ModuleError::InvalidParams(errors));
        }

        let files: Vec<String> = params["reads_per_gene"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let mut sample_names = string_array_or_lines(params, "sample_names");
        if sample_names.is_empty() {
            sample_names = files
                .iter()
                .map(|p| sample_name_from_reads_per_gene(p))
                .collect();
        }
        let (strand_str, strand) =
            strand_from_params(params).map_err(ModuleError::ToolError)?;
        let output_name = params
            .get("output_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("counts_matrix.tsv");

        std::fs::create_dir_all(project_dir)?;
        let mut per_sample = Vec::with_capacity(files.len());
        for (idx, path) in files.iter().enumerate() {
            let _ = events_tx
                .send(RunEvent::Progress {
                    fraction: idx as f64 / files.len() as f64,
                    message: format!("Reading {}", sample_names[idx]),
                })
                .await;
            let counts = counts::read_reads_per_gene(Path::new(path), strand).map_err(|e| {
                ModuleError::ToolError(format!("parse {}: {}", path, e))
            })?;
            per_sample.push(counts);
        }

        let matrix_path = project_dir.join(output_name);
        counts::write_counts_matrix(&matrix_path, &sample_names, &per_sample)?;
        let gene_count = counts::union_gene_count(&per_sample);

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 1.0,
                message: "Done".into(),
            })
            .await;

        let summary = serde_json::json!({
            "counts_matrix": matrix_path.display().to_string(),
            "strand": strand_str,
            "sample_count": sample_names.len(),
            "gene_count": gene_count,
            "samples": sample_names.iter().zip(files.iter()).map(|(name, path)| {
                serde_json::json!({ "name": name, "reads_per_gene": path })
            }).collect::<Vec<_>>(),
        });

        Ok(ModuleResult {
            output_files: vec![matrix_path],
            summary,
            log: format!(
                "Merged {} ReadsPerGene files into a {}-gene count matrix",
                sample_names.len(),
                gene_count
            ),
        })
    }

    fn produced_assets(&self, result: &ModuleResult) -> Vec<DeclaredAsset> {
        let Some(path) = result.summary.get("counts_matrix").and_then(|v| v.as_str()) else {
            return Vec::new();
        };
        let Some(file_name) = Path::new(path).file_name() else {
            return Vec::new();
        };
        vec![DeclaredAsset {
            kind: AssetKind::CountsMatrix,
            relative_path: PathBuf::from(file_name.to_os_string()),
            display_name: "Counts matrix".into(),
            schema: Some("gene_id x samples count matrix (TSV)".into()),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_name_strips_common_suffixes() {
        assert_eq!(sample_name_from_r1("/x/S1_R1.fastq.gz"), "S1");
        assert_eq!(sample_name_from_r1("/x/S2_1.fq"), "S2");
        assert_eq!(sample_name_from_r1("/x/raw.fastq"), "raw");
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
        let r1 = tmp.path().join("a_R1.fq");
        std::fs::write(&r1, "").unwrap();
        let r2a = tmp.path().join("a_R2.fq");
        std::fs::write(&r2a, "").unwrap();
        let r2b = tmp.path().join("b_R2.fq");
        std::fs::write(&r2b, "").unwrap();
        let m = StarAlignModule;
        let errs = m.validate(&serde_json::json!({
            "genome_dir": gd,
            "reads_1": [r1],
            "reads_2": [r2a, r2b],
        }));
        assert!(errs
            .iter()
            .any(|e| e.field == "reads_2" && e.message.contains("length")));
    }

    #[test]
    fn validate_rejects_bad_strand() {
        let m = CountsMergeModule;
        let errs = m.validate(&serde_json::json!({
            "reads_per_gene": [fixture_path("ReadsPerGene.sample1.out.tab")],
            "strand": "weird",
        }));
        assert!(errs.iter().any(|e| e.field == "strand"));
    }

    #[test]
    fn validate_rejects_multiple_r1_files() {
        let tmp = tempfile::tempdir().unwrap();
        let gd = tmp.path().join("gdir");
        std::fs::create_dir_all(&gd).unwrap();
        std::fs::write(gd.join("SA"), "").unwrap();
        let r1 = tmp.path().join("a_R1.fq");
        std::fs::write(&r1, "").unwrap();
        let r1b = tmp.path().join("b_R1.fq");
        std::fs::write(&r1b, "").unwrap();
        let m = StarAlignModule;
        let errs = m.validate(&serde_json::json!({
            "genome_dir": gd, "reads_1": [r1, r1b],
        }));
        assert!(errs.iter().any(|e| e.field == "reads_1"));
    }

    fn fixture_path(name: &str) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
            .display()
            .to_string()
    }

    #[tokio::test]
    async fn counts_merge_writes_matrix_and_declares_asset() {
        let tmp = tempfile::tempdir().unwrap();
        let m = CountsMergeModule;
        let (tx, mut rx) = mpsc::channel(4);
        let result = m
            .run(
                &serde_json::json!({
                    "reads_per_gene": [
                        fixture_path("ReadsPerGene.sample1.out.tab"),
                        fixture_path("ReadsPerGene.sample2.out.tab")
                    ],
                    "sample_names": ["S1", "S2"],
                    "strand": "unstranded"
                }),
                tmp.path(),
                tx,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        while rx.try_recv().is_ok() {}
        let matrix = tmp.path().join("counts_matrix.tsv");
        assert!(matrix.exists());
        let text = std::fs::read_to_string(matrix).unwrap();
        assert!(text.starts_with("gene_id\tS1\tS2\n"));
        assert_eq!(result.summary["gene_count"], 4);
        let assets = m.produced_assets(&result);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].kind, AssetKind::CountsMatrix);
    }
}

#[cfg(test)]
mod ai_schema_tests {
    use super::*;
    use rb_core::module::Module;

    #[test]
    fn star_align_schema_is_object_with_required_fields() {
        let s = StarAlignModule.params_schema().unwrap();
        assert_eq!(s["type"], "object");
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "genome_dir"));
        assert!(req.iter().any(|v| v == "reads_1"));
        assert!(req.len() >= 2);
    }
    #[test]
    fn star_align_hint_nonempty_both_languages() {
        assert!(!StarAlignModule.ai_hint("en").is_empty());
        assert!(!StarAlignModule.ai_hint("zh").is_empty());
    }
}

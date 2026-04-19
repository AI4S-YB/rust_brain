pub mod counts;
pub mod log_final;
mod subprocess;

use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

pub struct StarAlignModule;

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

#[async_trait::async_trait]
impl Module for StarAlignModule {
    fn id(&self) -> &str {
        "star_align"
    }
    fn name(&self) -> &str {
        "STAR Alignment & Quantification"
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
                    "description": "FASTQ paths for mate 1 (or single-end reads), one entry per sample."
                },
                "reads_2": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "FASTQ paths for mate 2. Omit or leave empty for single-end. If provided, length must match reads_1."
                },
                "sample_names": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional sample names (must match reads_1 length, chars [A-Za-z0-9_.-]). Defaults are derived from reads_1 filenames."
                },
                "strand": {
                    "type": "string",
                    "enum": ["unstranded", "forward", "reverse"],
                    "default": "unstranded",
                    "description": "Library strandedness; selects which column of ReadsPerGene.out.tab to use."
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
            "zh" => "用 run_star_align 把测序 reads 比对到基因组,并产出 counts_matrix.tsv 供 run_deseq2 使用。genome_dir 用 run_star_index 的输出目录;reads_1 是 mate1/单端 FASTQ 列表,双端时用 reads_2 提供等长的 mate2 列表。".into(),
            _    => "Use run_star_align to align reads to the genome and produce a counts_matrix.tsv consumed by run_deseq2. `genome_dir` is the output of run_star_index; `reads_1` lists mate-1 (or single-end) FASTQs and `reads_2` lists the matching mate-2 files for paired-end data.".into(),
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

        if let Some(names) = params.get("sample_names").and_then(|v| v.as_array()) {
            if names.len() != r1.len() {
                errors.push(ValidationError {
                    field: "sample_names".into(),
                    message: format!(
                        "sample_names length ({}) must match reads_1 length ({})",
                        names.len(),
                        r1.len()
                    ),
                });
            }
            let mut seen = std::collections::HashSet::new();
            for (i, v) in names.iter().enumerate() {
                let s = v.as_str().unwrap_or("");
                if s.is_empty()
                    || !s
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
                {
                    errors.push(ValidationError {
                        field: format!("sample_names[{}]", i),
                        message: "must be non-empty and match [A-Za-z0-9_.-]+".into(),
                    });
                }
                if !seen.insert(s) {
                    errors.push(ValidationError {
                        field: format!("sample_names[{}]", i),
                        message: format!("duplicate sample name: {}", s),
                    });
                }
            }
        }

        match params
            .get("strand")
            .and_then(|v| v.as_str())
            .unwrap_or("unstranded")
        {
            "unstranded" | "forward" | "reverse" => {}
            other => errors.push(ValidationError {
                field: "strand".into(),
                message: format!("strand must be unstranded/forward/reverse, got '{}'", other),
            }),
        }

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
        let sample_names: Vec<String> = match params.get("sample_names").and_then(|v| v.as_array())
        {
            Some(a) => a
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            None => reads_1.iter().map(|r| sample_name_from_r1(r)).collect(),
        };
        let threads = params.get("threads").and_then(|v| v.as_u64()).unwrap_or(4);
        let strand_str = params
            .get("strand")
            .and_then(|v| v.as_str())
            .unwrap_or("unstranded")
            .to_string();
        let strand = counts::Strand::from_str(&strand_str).unwrap();
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
        let mut per_sample_counts: Vec<counts::SampleCounts> = Vec::with_capacity(total);
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
                // Insert an empty counts entry so matrix alignment stays consistent,
                // otherwise the sample would be missing from the matrix columns.
                per_sample_counts.push(counts::SampleCounts {
                    summary: counts::SampleSummary::default(),
                    genes: std::collections::BTreeMap::new(),
                });
                continue;
            }

            let log_stats = std::fs::read_to_string(&log_final_path)
                .ok()
                .map(|t| log_final::parse(&t));
            let sample_counts =
                counts::read_reads_per_gene(&reads_per_gene, strand).map_err(|e| {
                    ModuleError::ToolError(format!("parse {}: {}", reads_per_gene.display(), e))
                })?;

            let stats_json = log_stats
                .as_ref()
                .map(|s| {
                    serde_json::json!({
                        "input_reads":           s.input_reads,
                        "uniquely_mapped":       s.uniquely_mapped,
                        "uniquely_mapped_pct":   s.uniquely_mapped_pct,
                        "multi_mapped":          s.multi_mapped,
                        "multi_mapped_pct":      s.multi_mapped_pct,
                        "unmapped":              s.unmapped,
                        "unmapped_pct":          s.unmapped_pct,
                        "n_unmapped":            sample_counts.summary.n_unmapped,
                        "n_multimapping":        sample_counts.summary.n_multimapping,
                        "n_nofeature":           sample_counts.summary.n_nofeature,
                        "n_ambiguous":           sample_counts.summary.n_ambiguous,
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

            per_sample_counts.push(sample_counts);
        }

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 1.0,
                message: "Merging counts matrix".into(),
            })
            .await;

        let matrix_path = run_dir.join("counts_matrix.tsv");
        counts::write_counts_matrix(&matrix_path, &sample_names, &per_sample_counts)?;
        output_files.push(matrix_path.clone());

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 1.0,
                message: "Done".into(),
            })
            .await;

        let summary = serde_json::json!({
            "run_dir": run_dir.display().to_string(),
            "counts_matrix": matrix_path.display().to_string(),
            "strand": strand_str,
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
        let tmp = tempfile::tempdir().unwrap();
        let gd = tmp.path().join("gdir");
        std::fs::create_dir_all(&gd).unwrap();
        std::fs::write(gd.join("SA"), "").unwrap();
        let r1 = tmp.path().join("a_R1.fq");
        std::fs::write(&r1, "").unwrap();
        let m = StarAlignModule;
        let errs = m.validate(&serde_json::json!({
            "genome_dir": gd, "reads_1": [r1], "strand": "weird",
        }));
        assert!(errs.iter().any(|e| e.field == "strand"));
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

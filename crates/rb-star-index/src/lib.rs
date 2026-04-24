pub mod subprocess;

use rb_core::asset::{AssetKind, DeclaredAsset};
use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::sync::mpsc;

pub struct StarIndexModule;

#[async_trait::async_trait]
impl Module for StarIndexModule {
    fn id(&self) -> &str {
        "star_index"
    }
    fn name(&self) -> &str {
        "STAR Genome Index"
    }

    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "genome_fasta": {
                    "type": "string",
                    "description": "Reference genome FASTA path."
                },
                "gtf_file": {
                    "type": "string",
                    "description": "Matching annotation GTF path (use run_gff_convert if only GFF3 is available)."
                },
                "sjdb_overhang": {
                    "type": "integer",
                    "minimum": 1,
                    "default": 100,
                    "description": "--sjdbOverhang value; set to read length - 1."
                },
                "genome_sa_index_nbases": {
                    "type": "integer",
                    "minimum": 1,
                    "default": 14,
                    "description": "--genomeSAindexNbases; reduce for small genomes (min(14, log2(GenomeLength)/2 - 1))."
                },
                "threads": {
                    "type": "integer",
                    "minimum": 1,
                    "default": 4,
                    "description": "Number of threads (--runThreadN)."
                },
                "extra_args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Extra CLI flags forwarded verbatim to STAR."
                },
                "output_dir": {
                    "type": "string",
                    "description": "Where to write the genome index. Defaults to the run's auto-created directory under `<project>/runs/star_index_<uuid>/`. Override to reuse a shared location like `/data/references/human_gr38_star/`."
                }
            },
            "required": ["genome_fasta", "gtf_file"],
            "additionalProperties": false
        }))
    }

    fn ai_hint(&self, lang: &str) -> String {
        match lang {
            "zh" => "用 run_star_index 为 STAR 比对生成基因组索引。每个参考基因组只需做一次,生成的索引目录之后喂给 run_star_align 的 genome_dir。".into(),
            _ => "Use run_star_index to build a STAR genome index — a one-time setup per reference. The resulting directory is passed to run_star_align as `genome_dir`.".into(),
        }
    }

    fn produced_assets(&self, result: &ModuleResult) -> Vec<DeclaredAsset> {
        // summary.genome_dir carries the absolute output directory.
        // Path::join with an absolute path returns that path as-is,
        // so this works whether the index sits inside the run dir or at
        // a user-chosen location like /data/references/.
        let Some(genome_dir) = result.summary.get("genome_dir").and_then(|v| v.as_str()) else {
            return Vec::new();
        };
        let fasta_name = result
            .summary
            .get("genome_fasta")
            .and_then(|v| v.as_str())
            .and_then(|p| std::path::Path::new(p).file_stem().and_then(|s| s.to_str()))
            .unwrap_or("genome")
            .to_string();
        vec![DeclaredAsset {
            kind: AssetKind::StarIndex,
            relative_path: PathBuf::from(genome_dir),
            display_name: format!("STAR index ({})", fasta_name),
            schema: Some("STAR 2.7 genome index".into()),
        }]
    }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let require_path = |field: &str, errors: &mut Vec<ValidationError>| match params
            .get(field)
            .and_then(|v| v.as_str())
        {
            None => errors.push(ValidationError {
                field: field.into(),
                message: format!("{} is required", field),
            }),
            Some(s) => {
                if !Path::new(s).exists() {
                    errors.push(ValidationError {
                        field: field.into(),
                        message: format!("{} does not exist: {}", field, s),
                    });
                }
            }
        };
        require_path("genome_fasta", &mut errors);
        require_path("gtf_file", &mut errors);

        if let Some(v) = params.get("extra_args") {
            if !v.is_array() || !v.as_array().unwrap().iter().all(|x| x.is_string()) {
                errors.push(ValidationError {
                    field: "extra_args".into(),
                    message: "extra_args must be an array of strings".into(),
                });
            }
        }

        if let Some(s) = params.get("output_dir").and_then(|v| v.as_str()) {
            if !s.is_empty() {
                let p = Path::new(s);
                let parent = p.parent().unwrap_or(Path::new("."));
                if !parent.as_os_str().is_empty() && !parent.exists() {
                    errors.push(ValidationError {
                        field: "output_dir".into(),
                        message: format!("parent directory does not exist: {}", parent.display()),
                    });
                }
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

        let genome_fasta = params["genome_fasta"].as_str().unwrap();
        let gtf_file = params["gtf_file"].as_str().unwrap();
        let threads = params.get("threads").and_then(|v| v.as_u64()).unwrap_or(4);
        let sjdb_overhang = params
            .get("sjdb_overhang")
            .and_then(|v| v.as_u64())
            .unwrap_or(100);
        let sa_nbases = params
            .get("genome_sa_index_nbases")
            .and_then(|v| v.as_u64())
            .unwrap_or(14);
        let extra: Vec<String> = params
            .get("extra_args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // project_dir here is the run directory prepared by Runner; honor an
        // explicit user override so shared indexes can live outside the project.
        let out_dir = match params.get("output_dir").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => PathBuf::from(s),
            _ => project_dir.to_path_buf(),
        };
        std::fs::create_dir_all(&out_dir)?;

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 0.0,
                message: "Starting genome generation".into(),
            })
            .await;

        let mut args: Vec<String> = vec![
            "--runMode".into(),
            "genomeGenerate".into(),
            "--genomeDir".into(),
            out_dir.display().to_string(),
            "--genomeFastaFiles".into(),
            genome_fasta.into(),
            "--sjdbGTFfile".into(),
            gtf_file.into(),
            "--runThreadN".into(),
            threads.to_string(),
            "--sjdbOverhang".into(),
            sjdb_overhang.to_string(),
            "--genomeSAindexNbases".into(),
            sa_nbases.to_string(),
        ];
        args.extend(extra.iter().cloned());

        let started = Instant::now();
        let status = subprocess::run_star_streaming(&bin, &args, events_tx.clone(), cancel).await?;
        let elapsed = started.elapsed().as_secs();

        if !status.success() {
            return Err(ModuleError::ToolError(format!(
                "STAR genomeGenerate exited with code {}",
                status.code().unwrap_or(-1),
            )));
        }

        // Verify key artifacts.
        let required = [
            "SA",
            "SAindex",
            "Genome",
            "chrNameLength.txt",
            "geneInfo.tab",
        ];
        let mut output_files: Vec<PathBuf> = Vec::new();
        for name in required {
            let p = out_dir.join(name);
            if !p.exists() {
                return Err(ModuleError::ToolError(format!(
                    "missing expected artifact: {}",
                    p.display()
                )));
            }
            output_files.push(p);
        }
        let log_out = out_dir.join("Log.out");
        if log_out.exists() {
            output_files.push(log_out);
        }

        let index_size = dir_size(&out_dir).unwrap_or(0);

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 1.0,
                message: "Done".into(),
            })
            .await;

        let summary = serde_json::json!({
            "genome_dir": out_dir.display().to_string(),
            "genome_fasta": genome_fasta,
            "gtf_file": gtf_file,
            "threads": threads,
            "sjdb_overhang": sjdb_overhang,
            "genome_sa_index_nbases": sa_nbases,
            "index_size_bytes": index_size,
            "generation_seconds": elapsed,
        });

        Ok(ModuleResult {
            output_files,
            summary,
            log: String::new(),
        })
    }
}

fn dir_size(p: &Path) -> std::io::Result<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(p)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_requires_genome_fasta_and_gtf() {
        let m = StarIndexModule;
        let errs = m.validate(&serde_json::json!({}));
        let fields: Vec<_> = errs.iter().map(|e| e.field.clone()).collect();
        assert!(fields.iter().any(|f| f == "genome_fasta"));
        assert!(fields.iter().any(|f| f == "gtf_file"));
    }

    #[test]
    fn validate_rejects_missing_files() {
        let m = StarIndexModule;
        let errs = m.validate(&serde_json::json!({
            "genome_fasta": "/nonexistent/genome.fa",
            "gtf_file": "/nonexistent/anno.gtf",
        }));
        assert!(errs.iter().any(|e| e.field == "genome_fasta"));
        assert!(errs.iter().any(|e| e.field == "gtf_file"));
    }

    #[test]
    fn validate_rejects_bad_extra_args() {
        let m = StarIndexModule;
        let tmp = tempfile::tempdir().unwrap();
        let fa = tmp.path().join("g.fa");
        let gtf = tmp.path().join("a.gtf");
        std::fs::write(&fa, "").unwrap();
        std::fs::write(&gtf, "").unwrap();
        let errs = m.validate(&serde_json::json!({
            "genome_fasta": fa,
            "gtf_file": gtf,
            "extra_args": "not-an-array",
        }));
        assert!(errs.iter().any(|e| e.field == "extra_args"));
    }
}

#[cfg(test)]
mod ai_schema_tests {
    use super::*;
    use rb_core::module::Module;

    #[test]
    fn star_index_requires_genome_and_annotation_fields() {
        let s = StarIndexModule.params_schema().unwrap();
        assert_eq!(s["type"], "object");
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "genome_fasta"));
        assert!(req.iter().any(|v| v == "gtf_file"));
    }
    #[test]
    fn star_index_hint_nonempty_both_languages() {
        assert!(!StarIndexModule.ai_hint("en").is_empty());
        assert!(!StarIndexModule.ai_hint("zh").is_empty());
    }
}

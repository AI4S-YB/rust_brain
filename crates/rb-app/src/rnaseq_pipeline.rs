//! The RNA-seq pipeline tool registry was wired into the chat orchestrator
//! that has been removed in preparation for the self-evolving agent rewrite.
//! The implementation is kept as a starting point for the agent_loop tool
//! surface; once that lands the `register_all` entry point will be invoked
//! again. Until then everything in this module is intentionally orphaned.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rb_ai::tools::{
    RiskLevel, ToolContext, ToolDef, ToolEntry, ToolError, ToolExecutor, ToolOutput, ToolRegistry,
};
use rb_core::asset::AssetKind;
use rb_core::input::{InputKind, InputRecord};
use rb_core::module::{Module, ModuleResult, ValidationError};
use rb_core::project::{Project, RunRecord, RunStatus};
use rb_core::sample::{default_read_pair_patterns, SampleRecord};
use serde_json::{json, Value};

pub fn register_all(registry: &mut ToolRegistry, modules: &[Arc<dyn Module>], lang: &str) {
    registry.register(rnaseq_pipeline_entry(modules, lang));
}

fn rnaseq_pipeline_entry(modules: &[Arc<dyn Module>], lang: &str) -> ToolEntry {
    let description = match lang {
        "zh" => "自动运行标准 RNA-seq / 转录组分析流水线: 发现项目样本和参考 FASTA/GTF,复用或构建 STAR index,逐样本比对,合并 ReadsPerGene counts,并在条件满足时继续做基因长度、TPM/FPKM、RustQC 和 DESeq2。工具内部会等待每个阻塞步骤完成。".to_string(),
        _ => "Run a standard RNA-seq / transcriptome pipeline: discover project samples and reference FASTA/GTF, reuse or build a STAR index, align samples, merge ReadsPerGene counts, and continue with gene lengths, TPM/FPKM, RustQC, and DESeq2 when inputs are available. The tool waits for blocking steps internally.".to_string(),
    };
    ToolEntry {
        def: ToolDef {
            name: "run_rnaseq_pipeline".into(),
            description,
            risk: RiskLevel::RunMid,
            params: json!({
                "type": "object",
                "properties": {
                    "genome_fasta": {
                        "type": "string",
                        "description": "Optional reference FASTA path override. If omitted, the single registered Fasta input is used."
                    },
                    "gtf_file": {
                        "type": "string",
                        "description": "Optional GTF annotation path override. If omitted, the single registered Gtf input is used."
                    },
                    "star_index_dir": {
                        "type": "string",
                        "description": "Optional existing STAR genomeDir override. If omitted, a registered StarIndex asset is reused or a new index is built."
                    },
                    "force_rebuild_index": {
                        "type": "boolean",
                        "default": false,
                        "description": "Build a new STAR index even when a usable StarIndex asset exists."
                    },
                    "threads": {
                        "type": "integer",
                        "minimum": 1,
                        "default": 4,
                        "description": "Threads per STAR/module run."
                    },
                    "sjdb_overhang": {
                        "type": "integer",
                        "minimum": 1,
                        "default": 100,
                        "description": "STAR --sjdbOverhang for index generation, usually read length - 1."
                    },
                    "genome_sa_index_nbases": {
                        "type": "integer",
                        "minimum": 1,
                        "default": 14,
                        "description": "STAR --genomeSAindexNbases; reduce for small genomes."
                    },
                    "strand": {
                        "type": "string",
                        "enum": ["unstranded", "forward", "reverse"],
                        "default": "unstranded",
                        "description": "ReadsPerGene count column to use when merging counts."
                    },
                    "sort_bam": {
                        "type": "string",
                        "enum": ["unsorted", "coordinate", "both"],
                        "default": "coordinate",
                        "description": "BAM output mode for STAR alignments."
                    },
                    "parallel_alignments": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 32,
                        "default": 1,
                        "description": "How many independent sample alignments to keep in flight."
                    },
                    "run_gene_length": {
                        "type": "boolean",
                        "default": true
                    },
                    "run_expression_normalization": {
                        "type": "boolean",
                        "default": true
                    },
                    "normalization_method": {
                        "type": "string",
                        "enum": ["tpm", "fpkm", "both"],
                        "default": "both"
                    },
                    "length_mode": {
                        "type": "string",
                        "enum": ["union", "longest"],
                        "default": "union"
                    },
                    "run_rustqc": {
                        "type": "boolean",
                        "default": true,
                        "description": "Run post-alignment RustQC when the binary is available. Validation failures are reported as skipped warnings instead of aborting the core pipeline."
                    },
                    "run_differential": {
                        "type": "boolean",
                        "default": true,
                        "description": "Run DESeq2 when coldata/reference or project sample conditions make the contrast unambiguous."
                    },
                    "coldata_path": {
                        "type": "string",
                        "description": "Optional DESeq2 sample metadata TSV path. If omitted, project sample condition/group metadata may be used."
                    },
                    "design": {
                        "type": "string",
                        "default": "condition",
                        "description": "Single DESeq2 design column, usually condition or group."
                    },
                    "reference": {
                        "type": "string",
                        "description": "Reference level for DESeq2. If omitted, a common control-like level may be inferred; otherwise DESeq2 is skipped."
                    },
                    "timeout_seconds_per_step": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 86400,
                        "default": 86400
                    },
                    "poll_interval_seconds": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 60,
                        "default": 5
                    }
                },
                "additionalProperties": false
            }),
        },
        executor: Arc::new(RnaSeqPipelineExec {
            modules: modules
                .iter()
                .map(|m| (m.id().to_string(), m.clone()))
                .collect(),
        }),
    }
}

struct RnaSeqPipelineExec {
    modules: HashMap<String, Arc<dyn Module>>,
}

#[async_trait]
impl ToolExecutor for RnaSeqPipelineExec {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let params = PipelineArgs::from_value(args)?;
        let discovery = {
            let project = ctx.project.lock().await;
            discover_project(&project, &params)?
        };

        let mut steps = Vec::new();
        let mut warnings = Vec::new();

        let star_index = self.required_module("star_index")?;
        let star_align = self.required_module("star_align")?;
        let counts_merge = self.required_module("counts_merge")?;

        let (genome_dir, star_index_asset_ids) =
            if let Some(existing) = discovery.existing_star_index.clone() {
                warnings.push(json!({
                    "step": "star_index",
                    "status": "reused",
                    "genome_dir": existing.path,
                    "asset_id": existing.asset_id,
                }));
                (
                    existing.path,
                    existing.asset_id.into_iter().collect::<Vec<_>>(),
                )
            } else {
                let index_params = json!({
                    "genome_fasta": discovery.reference.genome_fasta,
                    "gtf_file": discovery.reference.gtf_file,
                    "sjdb_overhang": params.sjdb_overhang,
                    "genome_sa_index_nbases": params.genome_sa_index_nbases,
                    "threads": params.threads,
                });
                let input_ids = ids_present(&[
                    discovery.reference.genome_input_id.clone(),
                    discovery.reference.gtf_input_id.clone(),
                ]);
                let record = spawn_and_wait_required(
                    &ctx,
                    star_index,
                    index_params,
                    input_ids,
                    Vec::new(),
                    &params,
                )
                .await?;
                let genome_dir = record
                    .result
                    .as_ref()
                    .and_then(|r| r.summary.get("genome_dir"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::Execution(format!(
                            "star_index run {} finished without summary.genome_dir",
                            record.id
                        ))
                    })?
                    .to_string();
                let asset_ids = record.assets_produced.clone();
                steps.push(compact_step("star_index", &record));
                (genome_dir, asset_ids)
            };

        let alignments = run_alignments(
            &ctx,
            star_align,
            &discovery.samples,
            &genome_dir,
            &star_index_asset_ids,
            &params,
            &mut steps,
        )
        .await?;

        let reads_per_gene: Vec<String> = alignments
            .iter()
            .map(|a| a.reads_per_gene.clone())
            .collect();
        let sample_names: Vec<String> = alignments.iter().map(|a| a.sample_name.clone()).collect();
        let fastq_input_ids = discovery
            .samples
            .iter()
            .flat_map(|s| [Some(s.r1_input_id.clone()), s.r2_input_id.clone()])
            .flatten()
            .collect::<Vec<_>>();

        let counts_params = json!({
            "reads_per_gene": reads_per_gene,
            "sample_names": sample_names,
            "strand": params.strand,
            "output_name": "counts_matrix.tsv",
        });
        let counts_record = spawn_and_wait_required(
            &ctx,
            counts_merge,
            counts_params,
            fastq_input_ids,
            Vec::new(),
            &params,
        )
        .await?;
        let counts_matrix = required_summary_string(&counts_record, "counts_matrix")?;
        steps.push(compact_step("counts_merge", &counts_record));

        let mut gene_lengths: Option<String> = None;
        if params.run_gene_length {
            if let Some(module) = self.modules.get("gene_length").cloned() {
                let record = spawn_and_wait_optional(
                    &ctx,
                    module,
                    json!({
                        "gtf": discovery.reference.gtf_file,
                        "output_name": "gene_lengths.tsv",
                    }),
                    ids_present(&[discovery.reference.gtf_input_id.clone()]),
                    Vec::new(),
                    &params,
                )
                .await;
                match record {
                    OptionalRun::Done(record) => {
                        gene_lengths = record
                            .result
                            .as_ref()
                            .and_then(|r| r.summary.get("output"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string);
                        steps.push(compact_step("gene_length", &record));
                    }
                    OptionalRun::Skipped(reason) => warnings.push(reason),
                }
            } else {
                warnings.push(json!({"step": "gene_length", "status": "skipped", "reason": "module not available"}));
            }
        }

        if params.run_expression_normalization {
            match gene_lengths.clone() {
                Some(lengths) => {
                    if let Some(module) = self.modules.get("expr_norm").cloned() {
                        match spawn_and_wait_optional(
                            &ctx,
                            module,
                            json!({
                                "counts": counts_matrix,
                                "lengths": lengths,
                                "length_mode": params.length_mode,
                                "method": params.normalization_method,
                            }),
                            Vec::new(),
                            Vec::new(),
                            &params,
                        )
                        .await
                        {
                            OptionalRun::Done(record) => {
                                steps.push(compact_step("expr_norm", &record));
                            }
                            OptionalRun::Skipped(reason) => warnings.push(reason),
                        }
                    } else {
                        warnings.push(json!({"step": "expr_norm", "status": "skipped", "reason": "module not available"}));
                    }
                }
                None => warnings.push(json!({
                    "step": "expr_norm",
                    "status": "skipped",
                    "reason": "gene length output is unavailable",
                })),
            }
        }

        if params.run_rustqc {
            if let Some(module) = self.modules.get("rustqc").cloned() {
                let bam_paths = alignments
                    .iter()
                    .filter_map(|a| a.bam.clone())
                    .collect::<Vec<_>>();
                if bam_paths.is_empty() {
                    warnings.push(json!({"step": "rustqc", "status": "skipped", "reason": "no BAM outputs found"}));
                } else {
                    match spawn_and_wait_optional(
                        &ctx,
                        module,
                        json!({
                            "input_bams": bam_paths,
                            "gtf": discovery.reference.gtf_file,
                            "paired": discovery.samples.iter().any(|s| s.r2_path.is_some()),
                            "stranded": params.strand,
                            "threads": params.threads,
                            "reference": discovery.reference.genome_fasta,
                        }),
                        Vec::new(),
                        Vec::new(),
                        &params,
                    )
                    .await
                    {
                        OptionalRun::Done(record) => steps.push(compact_step("rustqc", &record)),
                        OptionalRun::Skipped(reason) => warnings.push(reason),
                    }
                }
            } else {
                warnings.push(json!({"step": "rustqc", "status": "skipped", "reason": "module not available"}));
            }
        }

        if params.run_differential {
            match prepare_deseq2_coldata(&ctx, &discovery.samples, &params).await {
                Ok(Some(coldata)) => {
                    if let Some(module) = self.modules.get("deseq2").cloned() {
                        match spawn_and_wait_optional(
                            &ctx,
                            module,
                            json!({
                                "counts_path": counts_matrix,
                                "coldata_path": coldata.path,
                                "design": coldata.design,
                                "reference": coldata.reference,
                            }),
                            Vec::new(),
                            Vec::new(),
                            &params,
                        )
                        .await
                        {
                            OptionalRun::Done(record) => steps.push(compact_step("deseq2", &record)),
                            OptionalRun::Skipped(reason) => warnings.push(reason),
                        }
                    } else {
                        warnings.push(json!({"step": "deseq2", "status": "skipped", "reason": "module not available"}));
                    }
                }
                Ok(None) => warnings.push(json!({
                    "step": "deseq2",
                    "status": "skipped",
                    "reason": "contrast metadata is incomplete or ambiguous; provide coldata_path/design/reference or sample conditions",
                })),
                Err(e) => warnings.push(json!({
                    "step": "deseq2",
                    "status": "skipped",
                    "reason": e.to_string(),
                })),
            }
        }

        Ok(ToolOutput::Value(json!({
            "status": if warnings.is_empty() { "completed" } else { "completed_with_warnings" },
            "sample_count": discovery.samples.len(),
            "samples": discovery.samples.iter().map(|s| json!({
                "name": s.name,
                "original_name": s.original_name,
                "paired": s.r2_path.is_some(),
                "reads_1": s.r1_path,
                "reads_2": s.r2_path,
            })).collect::<Vec<_>>(),
            "reference": {
                "genome_fasta": discovery.reference.genome_fasta,
                "gtf_file": discovery.reference.gtf_file,
                "genome_dir": genome_dir,
            },
            "steps": steps,
            "warnings": warnings,
        })))
    }
}

impl RnaSeqPipelineExec {
    fn required_module(&self, id: &str) -> Result<Arc<dyn Module>, ToolError> {
        self.modules
            .get(id)
            .cloned()
            .ok_or_else(|| ToolError::Execution(format!("required module not available: {id}")))
    }
}

#[derive(Clone, Debug)]
struct PipelineArgs {
    genome_fasta: Option<String>,
    gtf_file: Option<String>,
    star_index_dir: Option<String>,
    force_rebuild_index: bool,
    threads: u64,
    sjdb_overhang: u64,
    genome_sa_index_nbases: u64,
    strand: String,
    sort_bam: String,
    parallel_alignments: usize,
    run_gene_length: bool,
    run_expression_normalization: bool,
    normalization_method: String,
    length_mode: String,
    run_rustqc: bool,
    run_differential: bool,
    coldata_path: Option<String>,
    design: String,
    reference: Option<String>,
    timeout_seconds_per_step: u64,
    poll_interval_seconds: u64,
}

impl PipelineArgs {
    fn from_value(args: &Value) -> Result<Self, ToolError> {
        let strand = string_enum(
            args,
            "strand",
            "unstranded",
            &["unstranded", "forward", "reverse"],
        )?;
        let sort_bam = string_enum(
            args,
            "sort_bam",
            "coordinate",
            &["unsorted", "coordinate", "both"],
        )?;
        let normalization_method = string_enum(
            args,
            "normalization_method",
            "both",
            &["tpm", "fpkm", "both"],
        )?;
        let length_mode = string_enum(args, "length_mode", "union", &["union", "longest"])?;
        Ok(Self {
            genome_fasta: optional_string(args, "genome_fasta"),
            gtf_file: optional_string(args, "gtf_file"),
            star_index_dir: optional_string(args, "star_index_dir"),
            force_rebuild_index: bool_arg(args, "force_rebuild_index", false),
            threads: u64_arg(args, "threads", 4, 1, u64::MAX)?,
            sjdb_overhang: u64_arg(args, "sjdb_overhang", 100, 1, u64::MAX)?,
            genome_sa_index_nbases: u64_arg(args, "genome_sa_index_nbases", 14, 1, u64::MAX)?,
            strand,
            sort_bam,
            parallel_alignments: u64_arg(args, "parallel_alignments", 1, 1, 32)? as usize,
            run_gene_length: bool_arg(args, "run_gene_length", true),
            run_expression_normalization: bool_arg(args, "run_expression_normalization", true),
            normalization_method,
            length_mode,
            run_rustqc: bool_arg(args, "run_rustqc", true),
            run_differential: bool_arg(args, "run_differential", true),
            coldata_path: optional_string(args, "coldata_path"),
            design: optional_string(args, "design").unwrap_or_else(|| "condition".to_string()),
            reference: optional_string(args, "reference"),
            timeout_seconds_per_step: u64_arg(args, "timeout_seconds_per_step", 86_400, 1, 86_400)?,
            poll_interval_seconds: u64_arg(args, "poll_interval_seconds", 5, 1, 60)?,
        })
    }
}

#[derive(Clone, Debug)]
struct PipelineDiscovery {
    samples: Vec<PipelineSample>,
    reference: PipelineReference,
    existing_star_index: Option<StarIndexChoice>,
}

#[derive(Clone, Debug)]
struct PipelineReference {
    genome_fasta: String,
    genome_input_id: Option<String>,
    gtf_file: String,
    gtf_input_id: Option<String>,
}

#[derive(Clone, Debug)]
struct StarIndexChoice {
    path: String,
    asset_id: Option<String>,
}

#[derive(Clone, Debug)]
struct PipelineSample {
    original_name: String,
    name: String,
    r1_input_id: String,
    r1_path: String,
    r2_input_id: Option<String>,
    r2_path: Option<String>,
    group: Option<String>,
    condition: Option<String>,
}

#[derive(Clone, Debug)]
struct AlignmentOutput {
    sample_name: String,
    reads_per_gene: String,
    bam: Option<String>,
}

fn discover_project(
    project: &Project,
    args: &PipelineArgs,
) -> Result<PipelineDiscovery, ToolError> {
    let mut samples = discover_samples(project)?;
    dedupe_sample_names(&mut samples);
    if samples.is_empty() {
        return Err(ToolError::InvalidArgs(
            "no configured samples or usable FASTQ inputs were found".into(),
        ));
    }

    let genome = pick_input_path(
        project,
        InputKind::Fasta,
        args.genome_fasta.as_deref(),
        "genome_fasta",
    )?;
    let gtf = pick_input_path(
        project,
        InputKind::Gtf,
        args.gtf_file.as_deref(),
        "gtf_file",
    )?;
    let existing_star_index = if args.force_rebuild_index {
        None
    } else {
        pick_star_index(project, args.star_index_dir.as_deref())?
    };

    Ok(PipelineDiscovery {
        samples,
        reference: PipelineReference {
            genome_fasta: genome.path,
            genome_input_id: genome.input_id,
            gtf_file: gtf.path,
            gtf_input_id: gtf.input_id,
        },
        existing_star_index,
    })
}

#[derive(Clone, Debug)]
struct ResolvedPath {
    path: String,
    input_id: Option<String>,
}

fn pick_input_path(
    project: &Project,
    kind: InputKind,
    override_path: Option<&str>,
    field: &str,
) -> Result<ResolvedPath, ToolError> {
    if let Some(path) = override_path {
        let p = Path::new(path);
        if !p.is_file() {
            return Err(ToolError::InvalidArgs(format!(
                "{field} does not exist or is not a file: {path}"
            )));
        }
        let input_id = project
            .inputs
            .iter()
            .find(|i| same_path(&i.path, p))
            .map(|i| i.id.clone());
        return Ok(ResolvedPath {
            path: path.to_string(),
            input_id,
        });
    }

    let candidates = project
        .inputs
        .iter()
        .filter(|i| i.kind == kind && !i.missing && i.path.is_file())
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [single] => Ok(ResolvedPath {
            path: single.path.to_string_lossy().to_string(),
            input_id: Some(single.id.clone()),
        }),
        [] => Err(ToolError::InvalidArgs(format!(
            "no registered {:?} input found; pass {field}",
            kind
        ))),
        _ => Err(ToolError::InvalidArgs(format!(
            "multiple registered {:?} inputs found; pass {field}",
            kind
        ))),
    }
}

fn pick_star_index(
    project: &Project,
    override_path: Option<&str>,
) -> Result<Option<StarIndexChoice>, ToolError> {
    if let Some(path) = override_path {
        if !is_star_index_dir(Path::new(path)) {
            return Err(ToolError::InvalidArgs(format!(
                "star_index_dir does not look like a STAR index: {path}"
            )));
        }
        let asset_id = project
            .assets
            .iter()
            .find(|a| a.kind == AssetKind::StarIndex && same_path(&a.path, Path::new(path)))
            .map(|a| a.id.clone());
        return Ok(Some(StarIndexChoice {
            path: path.to_string(),
            asset_id,
        }));
    }

    Ok(project
        .assets
        .iter()
        .rev()
        .find(|a| a.kind == AssetKind::StarIndex && is_star_index_dir(&a.path))
        .map(|a| StarIndexChoice {
            path: a.path.to_string_lossy().to_string(),
            asset_id: Some(a.id.clone()),
        }))
}

fn discover_samples(project: &Project) -> Result<Vec<PipelineSample>, ToolError> {
    let by_id = project
        .inputs
        .iter()
        .map(|i| (i.id.as_str(), i))
        .collect::<HashMap<_, _>>();
    let mut out = Vec::new();

    for sample in &project.samples {
        if let Some(pipeline_sample) = sample_from_record(sample, &by_id) {
            out.push(pipeline_sample);
        }
    }
    if !out.is_empty() {
        return Ok(out);
    }

    let previews = project.preview_auto_pair_samples(&default_read_pair_patterns());
    for preview in previews {
        let inputs = preview
            .inputs
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .filter(|i| i.kind == InputKind::Fastq && !i.missing && i.path.is_file())
            .collect::<Vec<_>>();
        if inputs.is_empty() {
            continue;
        }
        out.push(PipelineSample {
            original_name: preview.name.clone(),
            name: safe_sample_name(&preview.name),
            r1_input_id: inputs[0].id.clone(),
            r1_path: inputs[0].path.to_string_lossy().to_string(),
            r2_input_id: inputs.get(1).map(|i| i.id.clone()),
            r2_path: inputs.get(1).map(|i| i.path.to_string_lossy().to_string()),
            group: None,
            condition: None,
        });
    }
    Ok(out)
}

fn sample_from_record(
    sample: &SampleRecord,
    by_id: &HashMap<&str, &InputRecord>,
) -> Option<PipelineSample> {
    let inputs = sample
        .inputs
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).copied())
        .filter(|i| i.kind == InputKind::Fastq && !i.missing && i.path.is_file())
        .collect::<Vec<_>>();
    if inputs.is_empty() {
        return None;
    }
    Some(PipelineSample {
        original_name: sample.name.clone(),
        name: safe_sample_name(&sample.name),
        r1_input_id: inputs[0].id.clone(),
        r1_path: inputs[0].path.to_string_lossy().to_string(),
        r2_input_id: inputs.get(1).map(|i| i.id.clone()),
        r2_path: inputs.get(1).map(|i| i.path.to_string_lossy().to_string()),
        group: sample.group.clone(),
        condition: sample.condition.clone(),
    })
}

fn dedupe_sample_names(samples: &mut [PipelineSample]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for sample in samples {
        let base = sample.name.clone();
        let count = seen.entry(base.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            sample.name = format!("{base}_{count}");
        }
    }
}

async fn run_alignments(
    ctx: &ToolContext<'_>,
    module: Arc<dyn Module>,
    samples: &[PipelineSample],
    genome_dir: &str,
    star_index_asset_ids: &[String],
    params: &PipelineArgs,
    steps: &mut Vec<Value>,
) -> Result<Vec<AlignmentOutput>, ToolError> {
    let mut outputs = Vec::with_capacity(samples.len());
    let mut pending = Vec::<(String, PipelineSample)>::new();

    for sample in samples {
        let mut align_params = json!({
            "genome_dir": genome_dir,
            "reads_1": [sample.r1_path.clone()],
            "sample_names": [sample.name.clone()],
            "threads": params.threads,
            "sort_bam": params.sort_bam,
        });
        if let Some(r2) = &sample.r2_path {
            align_params["reads_2"] = json!([r2]);
        }
        validate_module(module.as_ref(), &align_params)?;
        let input_ids =
            ids_present(&[Some(sample.r1_input_id.clone()), sample.r2_input_id.clone()]);
        let run_id = ctx
            .runner
            .spawn(
                module.clone(),
                align_params,
                input_ids,
                star_index_asset_ids.to_vec(),
            )
            .await
            .map_err(ToolError::Execution)?;
        pending.push((run_id, sample.clone()));

        if pending.len() >= params.parallel_alignments {
            drain_alignment_batch(ctx.project, &mut pending, &mut outputs, params, steps).await?;
        }
    }

    drain_alignment_batch(ctx.project, &mut pending, &mut outputs, params, steps).await?;
    Ok(outputs)
}

async fn drain_alignment_batch(
    project: &Arc<tokio::sync::Mutex<Project>>,
    pending: &mut Vec<(String, PipelineSample)>,
    outputs: &mut Vec<AlignmentOutput>,
    params: &PipelineArgs,
    steps: &mut Vec<Value>,
) -> Result<(), ToolError> {
    for (run_id, sample) in pending.drain(..) {
        let record = wait_for_terminal(project, &run_id, params).await?;
        ensure_done(&record)?;
        outputs.push(alignment_output(&record, &sample.name)?);
        steps.push(compact_step("star_align", &record));
    }
    Ok(())
}

async fn spawn_and_wait_required(
    ctx: &ToolContext<'_>,
    module: Arc<dyn Module>,
    run_params: Value,
    inputs_used: Vec<String>,
    assets_used: Vec<String>,
    params: &PipelineArgs,
) -> Result<RunRecord, ToolError> {
    validate_module(module.as_ref(), &run_params)?;
    let run_id = ctx
        .runner
        .spawn(module, run_params, inputs_used, assets_used)
        .await
        .map_err(ToolError::Execution)?;
    let record = wait_for_terminal(ctx.project, &run_id, params).await?;
    ensure_done(&record)?;
    Ok(record)
}

enum OptionalRun {
    Done(RunRecord),
    Skipped(Value),
}

async fn spawn_and_wait_optional(
    ctx: &ToolContext<'_>,
    module: Arc<dyn Module>,
    run_params: Value,
    inputs_used: Vec<String>,
    assets_used: Vec<String>,
    params: &PipelineArgs,
) -> OptionalRun {
    if let Err(e) = validate_module(module.as_ref(), &run_params) {
        return OptionalRun::Skipped(json!({
            "step": module.id(),
            "status": "skipped",
            "reason": e.to_string(),
        }));
    }
    let run_id = match ctx
        .runner
        .spawn(module.clone(), run_params, inputs_used, assets_used)
        .await
    {
        Ok(id) => id,
        Err(e) => {
            return OptionalRun::Skipped(json!({
                "step": module.id(),
                "status": "skipped",
                "reason": e,
            }))
        }
    };
    match wait_for_terminal(ctx.project, &run_id, params).await {
        Ok(record) if matches!(record.status, RunStatus::Done) => OptionalRun::Done(record),
        Ok(record) => OptionalRun::Skipped(json!({
            "step": module.id(),
            "run_id": record.id,
            "status": format!("{:?}", record.status),
            "reason": record.error,
        })),
        Err(e) => OptionalRun::Skipped(json!({
            "step": module.id(),
            "run_id": run_id,
            "status": "skipped",
            "reason": e.to_string(),
        })),
    }
}

async fn wait_for_terminal(
    project: &Arc<tokio::sync::Mutex<Project>>,
    run_id: &str,
    params: &PipelineArgs,
) -> Result<RunRecord, ToolError> {
    let deadline =
        tokio::time::Instant::now() + Duration::from_secs(params.timeout_seconds_per_step);
    loop {
        let record = {
            let project = project.lock().await;
            project.runs.iter().find(|r| r.id == run_id).cloned()
        }
        .ok_or_else(|| ToolError::Execution(format!("run not found: {run_id}")))?;

        if matches!(
            record.status,
            RunStatus::Done | RunStatus::Failed | RunStatus::Cancelled
        ) {
            return Ok(record);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ToolError::Execution(format!(
                "timed out waiting for run {run_id}"
            )));
        }
        tokio::time::sleep(Duration::from_secs(params.poll_interval_seconds)).await;
    }
}

fn validate_module(module: &dyn Module, params: &Value) -> Result<(), ToolError> {
    let errors = module.validate(params);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ToolError::InvalidArgs(format_validation_errors(&errors)))
    }
}

fn ensure_done(record: &RunRecord) -> Result<(), ToolError> {
    if matches!(record.status, RunStatus::Done) {
        Ok(())
    } else {
        Err(ToolError::Execution(format!(
            "{} run {} ended with status {:?}: {}",
            record.module_id,
            record.id,
            record.status,
            record.error.clone().unwrap_or_default()
        )))
    }
}

fn alignment_output(
    record: &RunRecord,
    fallback_sample_name: &str,
) -> Result<AlignmentOutput, ToolError> {
    let result = record
        .result
        .as_ref()
        .ok_or_else(|| ToolError::Execution(format!("align run {} has no result", record.id)))?;
    let sample = result
        .summary
        .get("samples")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|s| s.get("status").and_then(|v| v.as_str()) == Some("ok"))
        })
        .ok_or_else(|| {
            ToolError::Execution(format!(
                "align run {} did not report an ok sample",
                record.id
            ))
        })?;
    let reads_per_gene = sample
        .get("reads_per_gene")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| find_output_file(result, "ReadsPerGene.out.tab"))
        .ok_or_else(|| {
            ToolError::Execution(format!(
                "align run {} did not produce ReadsPerGene.out.tab",
                record.id
            ))
        })?
        .to_string();
    let bam = sample
        .get("bam")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            result
                .output_files
                .iter()
                .find(|p| p.extension().and_then(|e| e.to_str()) == Some("bam"))
                .map(|p| p.to_string_lossy().to_string())
        });
    let sample_name = sample
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback_sample_name)
        .to_string();
    Ok(AlignmentOutput {
        sample_name,
        reads_per_gene,
        bam,
    })
}

fn required_summary_string(record: &RunRecord, key: &str) -> Result<String, ToolError> {
    record
        .result
        .as_ref()
        .and_then(|r| r.summary.get(key))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            ToolError::Execution(format!(
                "{} run {} finished without summary.{key}",
                record.module_id, record.id
            ))
        })
}

fn find_output_file<'a>(result: &'a ModuleResult, file_name: &str) -> Option<&'a str> {
    result.output_files.iter().find_map(|p| {
        (p.file_name().and_then(|n| n.to_str()) == Some(file_name))
            .then(|| p.to_str())
            .flatten()
    })
}

fn compact_step(step: &str, record: &RunRecord) -> Value {
    let (outputs, summary) = record
        .result
        .as_ref()
        .map(|result| {
            (
                result
                    .output_files
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect::<Vec<_>>(),
                compact_summary(&record.module_id, &result.summary),
            )
        })
        .unwrap_or_else(|| (Vec::new(), Value::Null));
    json!({
        "step": step,
        "run_id": record.id,
        "module_id": record.module_id,
        "status": format!("{:?}", record.status),
        "outputs": outputs,
        "summary": summary,
        "error": record.error,
    })
}

fn compact_summary(module_id: &str, summary: &Value) -> Value {
    let mut summary = summary.clone();
    if module_id == "deseq2" {
        if let Some(obj) = summary.as_object_mut() {
            obj.remove("results");
        }
    }
    summary
}

#[derive(Clone, Debug)]
struct DeseqColdata {
    path: String,
    design: String,
    reference: String,
}

async fn prepare_deseq2_coldata(
    ctx: &ToolContext<'_>,
    samples: &[PipelineSample],
    params: &PipelineArgs,
) -> Result<Option<DeseqColdata>, ToolError> {
    let design = params.design.trim();
    if design.is_empty() {
        return Ok(None);
    }

    if let Some(path) = params.coldata_path.clone() {
        if !Path::new(&path).is_file() {
            return Err(ToolError::InvalidArgs(format!(
                "coldata_path does not exist: {path}"
            )));
        }
        let reference = match params.reference.clone() {
            Some(r) => r,
            None => infer_reference_from_coldata(Path::new(&path), design).ok_or_else(|| {
                ToolError::InvalidArgs("reference is required for supplied coldata_path".into())
            })?,
        };
        return Ok(Some(DeseqColdata {
            path,
            design: design.to_string(),
            reference,
        }));
    }

    let values = samples
        .iter()
        .map(|s| sample_design_value(s, design))
        .collect::<Option<Vec<_>>>();
    let Some(values) = values else {
        return Ok(None);
    };
    let unique = values.iter().cloned().collect::<HashSet<_>>();
    if unique.len() < 2 {
        return Ok(None);
    }
    let reference = params
        .reference
        .clone()
        .or_else(|| infer_control_like_reference(&unique));
    let Some(reference) = reference else {
        return Ok(None);
    };

    let root = { ctx.project.lock().await.root_dir.clone() };
    let dir = root.join("ai_workflows");
    std::fs::create_dir_all(&dir)
        .map_err(|e| ToolError::Execution(format!("create ai_workflows: {e}")))?;
    let path = dir.join(format!(
        "rnaseq_coldata_{}.tsv",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    ));
    let mut data = format!("\t{design}\n");
    for (sample, value) in samples.iter().zip(values.iter()) {
        data.push_str(&sample.name);
        data.push('\t');
        data.push_str(value);
        data.push('\n');
    }
    std::fs::write(&path, data).map_err(|e| ToolError::Execution(format!("write coldata: {e}")))?;
    Ok(Some(DeseqColdata {
        path: path.to_string_lossy().to_string(),
        design: design.to_string(),
        reference,
    }))
}

fn sample_design_value(sample: &PipelineSample, design: &str) -> Option<String> {
    match design {
        "condition" => sample.condition.clone(),
        "group" => sample.group.clone(),
        _ => None,
    }
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}

fn infer_reference_from_coldata(path: &Path, design: &str) -> Option<String> {
    let data = std::fs::read_to_string(path).ok()?;
    let mut lines = data.lines();
    let header = lines.next()?;
    let cols = header.split('\t').map(str::trim).collect::<Vec<_>>();
    let design_idx = cols.iter().position(|c| *c == design)?;
    let mut values = HashSet::new();
    for line in lines {
        let fields = line.split('\t').map(str::trim).collect::<Vec<_>>();
        if let Some(value) = fields.get(design_idx) {
            if !value.is_empty() {
                values.insert((*value).to_string());
            }
        }
    }
    infer_control_like_reference(&values)
}

fn infer_control_like_reference(values: &HashSet<String>) -> Option<String> {
    let markers = [
        "control",
        "ctrl",
        "untreated",
        "untrt",
        "normal",
        "wt",
        "mock",
        "vehicle",
        "baseline",
    ];
    values.iter().find_map(|v| {
        let lower = v.to_ascii_lowercase();
        markers
            .iter()
            .any(|marker| lower == *marker || lower.contains(marker))
            .then(|| v.clone())
    })
}

fn optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn bool_arg(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn u64_arg(args: &Value, key: &str, default: u64, min: u64, max: u64) -> Result<u64, ToolError> {
    let value = args.get(key).and_then(|v| v.as_u64()).unwrap_or(default);
    if value < min || value > max {
        return Err(ToolError::InvalidArgs(format!(
            "{key} must be between {min} and {max}"
        )));
    }
    Ok(value)
}

fn string_enum(
    args: &Value,
    key: &str,
    default: &str,
    allowed: &[&str],
) -> Result<String, ToolError> {
    let value = optional_string(args, key).unwrap_or_else(|| default.to_string());
    if allowed.iter().any(|x| *x == value) {
        Ok(value)
    } else {
        Err(ToolError::InvalidArgs(format!(
            "{key} must be one of: {}",
            allowed.join(", ")
        )))
    }
}

fn ids_present(values: &[Option<String>]) -> Vec<String> {
    values.iter().filter_map(Clone::clone).collect()
}

fn safe_sample_name(raw: &str) -> String {
    let mut out = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "sample".into()
    } else {
        out
    }
}

fn is_star_index_dir(path: &Path) -> bool {
    path.is_dir() && path.join("SA").exists()
}

fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn format_validation_errors(errors: &[ValidationError]) -> String {
    errors
        .iter()
        .map(|e| format!("{}: {}", e.field, e.message))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rb_core::asset::{AssetKind, AssetRecord};
    use rb_core::input::InputKind;
    use rb_core::project::Project;

    #[test]
    fn safe_sample_names_are_star_compatible_and_deduped() {
        let mut samples = vec![
            PipelineSample {
                original_name: "sample A".into(),
                name: safe_sample_name("sample A"),
                r1_input_id: "r1".into(),
                r1_path: "a.fq".into(),
                r2_input_id: None,
                r2_path: None,
                group: None,
                condition: None,
            },
            PipelineSample {
                original_name: "sample/A".into(),
                name: safe_sample_name("sample/A"),
                r1_input_id: "r2".into(),
                r1_path: "b.fq".into(),
                r2_input_id: None,
                r2_path: None,
                group: None,
                condition: None,
            },
        ];
        dedupe_sample_names(&mut samples);
        assert_eq!(samples[0].name, "sample_A");
        assert_eq!(samples[1].name, "sample_A_2");
    }

    #[test]
    fn discovery_uses_configured_samples_reference_and_star_index_asset() {
        let tmp = tempfile::tempdir().unwrap();
        let mut project = Project::create("p", tmp.path()).unwrap();
        let r1 = tmp.path().join("s1_R1.fq");
        let r2 = tmp.path().join("s1_R2.fq");
        let fasta = tmp.path().join("genome.fa");
        let gtf = tmp.path().join("genes.gtf");
        let index = tmp.path().join("star_index");
        std::fs::write(&r1, "").unwrap();
        std::fs::write(&r2, "").unwrap();
        std::fs::write(&fasta, "").unwrap();
        std::fs::write(&gtf, "").unwrap();
        std::fs::create_dir_all(&index).unwrap();
        std::fs::write(index.join("SA"), "").unwrap();

        let in_r1 = project
            .register_input(&r1, Some(InputKind::Fastq), None)
            .unwrap();
        let in_r2 = project
            .register_input(&r2, Some(InputKind::Fastq), None)
            .unwrap();
        project
            .register_input(&fasta, Some(InputKind::Fasta), None)
            .unwrap();
        project
            .register_input(&gtf, Some(InputKind::Gtf), None)
            .unwrap();
        project
            .create_sample(
                "sample 1".into(),
                None,
                Some("control".into()),
                vec![in_r1.id.clone(), in_r2.id.clone()],
            )
            .unwrap();
        project.assets.push(AssetRecord {
            id: "as_star".into(),
            kind: AssetKind::StarIndex,
            path: index.clone(),
            size_bytes: 0,
            produced_by_run_id: "run1".into(),
            display_name: "STAR index".into(),
            schema: None,
            created_at: chrono::Utc::now(),
        });

        let args = PipelineArgs::from_value(&json!({})).unwrap();
        let discovered = discover_project(&project, &args).unwrap();
        assert_eq!(discovered.samples.len(), 1);
        assert_eq!(discovered.samples[0].name, "sample_1");
        assert_eq!(discovered.reference.genome_fasta, fasta.to_string_lossy());
        assert_eq!(
            discovered.existing_star_index.unwrap().path,
            index.to_string_lossy()
        );
    }
}

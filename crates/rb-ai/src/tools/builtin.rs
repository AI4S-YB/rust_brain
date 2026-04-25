use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::schema::{RiskLevel, ToolDef, ToolError};
use super::{ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry};

pub fn register_all(registry: &mut ToolRegistry) {
    registry.register(list_project_files_entry());
    registry.register(list_project_inputs_entry());
    registry.register(list_project_samples_entry());
    registry.register(list_project_assets_entry());
    registry.register(list_project_runs_entry());
    registry.register(read_table_preview_entry());
    registry.register(get_project_info_entry());
    registry.register(get_run_status_entry());
    registry.register(summarize_run_entry());
    registry.register(list_known_binaries_entry());
}

// ----- list_project_files -----

fn list_project_files_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "list_project_files".into(),
            description: "List files and directories inside the current project, \
                optionally under a subdirectory. Returns at most 200 entries with \
                type (file|dir), size in bytes, and a detected format tag."
                .into(),
            risk: RiskLevel::Read,
            params: json!({
                "type": "object",
                "properties": {
                    "subdir": {
                        "type": "string",
                        "description": "Optional subdirectory relative to the project root."
                    }
                },
                "additionalProperties": false
            }),
        },
        executor: Arc::new(ListProjectFiles),
    }
}

struct ListProjectFiles;

#[async_trait]
impl ToolExecutor for ListProjectFiles {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let subdir = args.get("subdir").and_then(|v| v.as_str()).unwrap_or("");
        let root = { ctx.project.lock().await.root_dir.clone() };
        let target = if subdir.is_empty() {
            root.clone()
        } else {
            root.join(subdir)
        };
        if std::path::Path::new(subdir)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(ToolError::InvalidArgs(
                "subdir must stay inside project".into(),
            ));
        }
        let canonical_root = tokio::fs::canonicalize(&root).await.unwrap_or(root.clone());
        let target = tokio::fs::canonicalize(&target)
            .await
            .map_err(|e| ToolError::Execution(format!("read_dir: {e}")))?;
        if !target.starts_with(&canonical_root) {
            return Err(ToolError::InvalidArgs(
                "subdir must stay inside project".into(),
            ));
        }

        let mut out: Vec<Value> = vec![];
        let mut entries = tokio::fs::read_dir(&target)
            .await
            .map_err(|e| ToolError::Execution(format!("read_dir: {e}")))?;
        while let Some(ent) = entries
            .next_entry()
            .await
            .map_err(|e| ToolError::Execution(format!("next_entry: {e}")))?
        {
            if out.len() >= 200 {
                break;
            }
            let meta = ent
                .metadata()
                .await
                .map_err(|e| ToolError::Execution(format!("metadata: {e}")))?;
            let name = ent.file_name().to_string_lossy().to_string();
            let kind = if meta.is_dir() { "dir" } else { "file" };
            let format = detect_format(&name);
            out.push(json!({
                "name": name,
                "type": kind,
                "size": meta.len(),
                "format": format,
            }));
        }
        let truncated = out.len() == 200;
        Ok(ToolOutput::Value(json!({
            "subdir": subdir,
            "entries": out,
            "truncated": truncated,
        })))
    }
}

fn detect_format(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.ends_with(".fastq.gz")
        || n.ends_with(".fq.gz")
        || n.ends_with(".fastq")
        || n.ends_with(".fq")
    {
        "fastq"
    } else if n.ends_with(".bam") {
        "bam"
    } else if n.ends_with(".sam") {
        "sam"
    } else if n.ends_with(".gtf") {
        "gtf"
    } else if n.ends_with(".gff3") || n.ends_with(".gff") {
        "gff"
    } else if n.ends_with(".fa") || n.ends_with(".fasta") || n.ends_with(".fna") {
        "fasta"
    } else if n.ends_with(".tsv") {
        "tsv"
    } else if n.ends_with(".csv") {
        "csv"
    } else {
        "other"
    }
}

fn value_limit(args: &Value, default: usize, max: usize) -> usize {
    args.get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(default)
        .min(max)
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

// ----- list_project_inputs -----

fn list_project_inputs_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "list_project_inputs".into(),
            description: "List registered project inputs such as FASTQ, FASTA, GTF, counts matrices, and sample sheets. Returns ids, paths, kind, size, sample links, and missing-file state.".into(),
            risk: RiskLevel::Read,
            params: json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "description": "Optional InputKind filter, e.g. Fastq, Fasta, Gtf, Gff, CountsMatrix, SampleSheet, Other."
                    },
                    "include_missing": {
                        "type": "boolean",
                        "default": true
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 500,
                        "default": 200
                    }
                },
                "additionalProperties": false
            }),
        },
        executor: Arc::new(ListProjectInputs),
    }
}

struct ListProjectInputs;

#[async_trait]
impl ToolExecutor for ListProjectInputs {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let kind_filter = args
            .get("kind")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());
        let include_missing = args
            .get("include_missing")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let limit = value_limit(args, 200, 500);

        let proj = ctx.project.lock().await;
        let mut items = Vec::new();
        for rec in &proj.inputs {
            let kind = format!("{:?}", rec.kind);
            if let Some(filter) = kind_filter.as_deref() {
                if kind.to_ascii_lowercase() != filter {
                    continue;
                }
            }
            if !include_missing && rec.missing {
                continue;
            }
            if items.len() >= limit {
                break;
            }
            items.push(json!({
                "id": rec.id,
                "display_name": rec.display_name,
                "kind": kind,
                "path": path_string(&rec.path),
                "size_bytes": rec.size_bytes,
                "registered_at": rec.registered_at,
                "sample_id": rec.sample_id,
                "paired_with": rec.paired_with,
                "missing": rec.missing,
                "notes": rec.notes,
            }));
        }
        Ok(ToolOutput::Value(json!({
            "count": proj.inputs.len(),
            "returned": items.len(),
            "inputs": items,
        })))
    }
}

// ----- list_project_samples -----

fn list_project_samples_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "list_project_samples".into(),
            description: "List registered biological samples with group/condition metadata and resolved input file references.".into(),
            risk: RiskLevel::Read,
            params: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 200 }
                },
                "additionalProperties": false
            }),
        },
        executor: Arc::new(ListProjectSamples),
    }
}

struct ListProjectSamples;

#[async_trait]
impl ToolExecutor for ListProjectSamples {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let limit = value_limit(args, 200, 500);
        let proj = ctx.project.lock().await;
        let mut samples = Vec::new();
        for sample in proj.samples.iter().take(limit) {
            let inputs: Vec<Value> = sample
                .inputs
                .iter()
                .map(|input_id| {
                    if let Some(rec) = proj.inputs.iter().find(|r| &r.id == input_id) {
                        json!({
                            "id": rec.id,
                            "display_name": rec.display_name,
                            "kind": format!("{:?}", rec.kind),
                            "path": path_string(&rec.path),
                            "missing": rec.missing,
                        })
                    } else {
                        json!({ "id": input_id, "missing_record": true })
                    }
                })
                .collect();
            samples.push(json!({
                "id": sample.id,
                "name": sample.name,
                "group": sample.group,
                "condition": sample.condition,
                "paired": sample.paired,
                "inputs": inputs,
                "notes": sample.notes,
            }));
        }
        Ok(ToolOutput::Value(json!({
            "count": proj.samples.len(),
            "returned": samples.len(),
            "samples": samples,
        })))
    }
}

// ----- list_project_assets -----

fn list_project_assets_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "list_project_assets".into(),
            description: "List registered derived assets such as STAR indexes, BAMs, trimmed FASTQs, count matrices, GTFs, and reports, including lineage back to producing runs.".into(),
            risk: RiskLevel::Read,
            params: json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "description": "Optional AssetKind filter, e.g. StarIndex, Bam, TrimmedFastq, CountsMatrix, Report, Other."
                    },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 200 }
                },
                "additionalProperties": false
            }),
        },
        executor: Arc::new(ListProjectAssets),
    }
}

struct ListProjectAssets;

#[async_trait]
impl ToolExecutor for ListProjectAssets {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let kind_filter = args
            .get("kind")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());
        let limit = value_limit(args, 200, 500);

        let proj = ctx.project.lock().await;
        let mut assets = Vec::new();
        for asset in &proj.assets {
            let kind = format!("{:?}", asset.kind);
            if let Some(filter) = kind_filter.as_deref() {
                if kind.to_ascii_lowercase() != filter {
                    continue;
                }
            }
            if assets.len() >= limit {
                break;
            }
            assets.push(json!({
                "id": asset.id,
                "kind": kind,
                "path": path_string(&asset.path),
                "size_bytes": asset.size_bytes,
                "produced_by_run_id": asset.produced_by_run_id,
                "display_name": asset.display_name,
                "schema": asset.schema,
                "created_at": asset.created_at,
            }));
        }
        Ok(ToolOutput::Value(json!({
            "count": proj.assets.len(),
            "returned": assets.len(),
            "assets": assets,
        })))
    }
}

// ----- list_project_runs -----

fn list_project_runs_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "list_project_runs".into(),
            description: "List analysis runs with module id, status, timestamps, lineage ids, errors, and compact result summaries.".into(),
            risk: RiskLevel::Read,
            params: json!({
                "type": "object",
                "properties": {
                    "module_id": { "type": "string" },
                    "status": { "type": "string", "description": "Pending, Running, Done, Failed, or Cancelled." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 50 }
                },
                "additionalProperties": false
            }),
        },
        executor: Arc::new(ListProjectRuns),
    }
}

struct ListProjectRuns;

#[async_trait]
impl ToolExecutor for ListProjectRuns {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let module_filter = args
            .get("module_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let status_filter = args
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());
        let limit = value_limit(args, 50, 500);

        let proj = ctx.project.lock().await;
        let mut runs = Vec::new();
        for r in proj.runs.iter().rev() {
            if let Some(module_id) = module_filter {
                if r.module_id != module_id {
                    continue;
                }
            }
            let status = format!("{:?}", r.status);
            if let Some(filter) = status_filter.as_deref() {
                if status.to_ascii_lowercase() != filter {
                    continue;
                }
            }
            if runs.len() >= limit {
                break;
            }
            runs.push(json!({
                "id": r.id,
                "module_id": r.module_id,
                "status": status,
                "started_at": r.started_at,
                "finished_at": r.finished_at,
                "inputs_used": r.inputs_used,
                "assets_used": r.assets_used,
                "assets_produced": r.assets_produced,
                "error": r.error,
                "summary": r.result.as_ref().map(|res| res.summary.clone()),
                "output_files": r.result.as_ref().map(|res| {
                    res.output_files
                        .iter()
                        .map(|p| path_string(p))
                        .collect::<Vec<_>>()
                }),
            }));
        }
        Ok(ToolOutput::Value(json!({
            "count": proj.runs.len(),
            "returned": runs.len(),
            "runs": runs,
        })))
    }
}

// ----- read_table_preview -----

fn read_table_preview_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "read_table_preview".into(),
            description: "Read the first N lines of a TSV/CSV/FASTQ file inside the project. \
                Returns raw text — do not request > 200 rows."
                .into(),
            risk: RiskLevel::Read,
            params: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "rows": { "type": "integer", "minimum": 1, "maximum": 200, "default": 10 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        executor: Arc::new(ReadTablePreview),
    }
}

struct ReadTablePreview;

#[async_trait]
impl ToolExecutor for ReadTablePreview {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let rows = args
            .get("rows")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .min(200) as usize;
        let root = { ctx.project.lock().await.root_dir.clone() };
        // Early reject: any `..` component in the requested path is a
        // traversal attempt, regardless of whether the target exists yet.
        if std::path::Path::new(path)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(ToolError::InvalidArgs("path must be inside project".into()));
        }
        let full = root.join(path);
        // For extra safety, canonicalize both sides when the target exists
        // and verify containment; if the file doesn't exist, fall back to
        // the joined path (already `..`-free from the check above).
        let canonical_root = tokio::fs::canonicalize(&root).await.unwrap_or(root.clone());
        let target = tokio::fs::canonicalize(&full).await.unwrap_or(full.clone());
        if !target.starts_with(&canonical_root) {
            return Err(ToolError::InvalidArgs("path must be inside project".into()));
        }
        let text = tokio::fs::read_to_string(&target)
            .await
            .map_err(|e| ToolError::Execution(format!("read: {e}")))?;
        let preview: Vec<_> = text.lines().take(rows).collect();
        Ok(ToolOutput::Value(json!({
            "path": path,
            "rows": preview.len(),
            "content": preview.join("\n"),
        })))
    }
}

// ----- get_project_info -----

fn get_project_info_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "get_project_info".into(),
            description: "Return project name, creation time, and a summary of recent runs.".into(),
            risk: RiskLevel::Read,
            params: json!({ "type": "object", "additionalProperties": false }),
        },
        executor: Arc::new(GetProjectInfo),
    }
}

struct GetProjectInfo;

#[async_trait]
impl ToolExecutor for GetProjectInfo {
    async fn execute(&self, _args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let proj = ctx.project.lock().await;
        let runs: Vec<_> = proj
            .runs
            .iter()
            .rev()
            .take(10)
            .map(|r| {
                json!({
                    "id": r.id,
                    "module_id": r.module_id,
                    "status": format!("{:?}", r.status),
                    "started_at": r.started_at,
                    "finished_at": r.finished_at,
                })
            })
            .collect();
        Ok(ToolOutput::Value(json!({
            "name": proj.name,
            "created_at": proj.created_at,
            "runs_count": proj.runs.len(),
            "recent_runs": runs,
            "default_view": proj.default_view,
        })))
    }
}

// ----- get_run_status -----

fn get_run_status_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "get_run_status".into(),
            description:
                "Look up a run by id. Returns status, timestamps, result summary, and output files."
                    .into(),
            risk: RiskLevel::Read,
            params: json!({
                "type": "object",
                "properties": { "run_id": { "type": "string" } },
                "required": ["run_id"],
                "additionalProperties": false
            }),
        },
        executor: Arc::new(GetRunStatus),
    }
}

struct GetRunStatus;

#[async_trait]
impl ToolExecutor for GetRunStatus {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let id = args
            .get("run_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("run_id required".into()))?;
        let proj = ctx.project.lock().await;
        let r = proj
            .runs
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| ToolError::Execution(format!("run {id} not found")))?;
        Ok(ToolOutput::Value(json!({
            "run_id": r.id,
            "module_id": r.module_id,
            "status": format!("{:?}", r.status),
            "started_at": r.started_at,
            "finished_at": r.finished_at,
            "result": r.result,
        })))
    }
}

// ----- summarize_run -----

fn summarize_run_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "summarize_run".into(),
            description: "Return a structured summary for one run, including status, params, result summary, output files, lineage ids, and registered assets it produced.".into(),
            risk: RiskLevel::Read,
            params: json!({
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" },
                    "include_params": { "type": "boolean", "default": true }
                },
                "required": ["run_id"],
                "additionalProperties": false
            }),
        },
        executor: Arc::new(SummarizeRun),
    }
}

struct SummarizeRun;

#[async_trait]
impl ToolExecutor for SummarizeRun {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let id = args
            .get("run_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("run_id required".into()))?;
        let include_params = args
            .get("include_params")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let proj = ctx.project.lock().await;
        let r = proj
            .runs
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| ToolError::Execution(format!("run {id} not found")))?;

        let produced_assets: Vec<Value> = proj
            .assets
            .iter()
            .filter(|a| r.assets_produced.iter().any(|id| id == &a.id))
            .map(|a| {
                json!({
                    "id": a.id,
                    "kind": format!("{:?}", a.kind),
                    "path": path_string(&a.path),
                    "size_bytes": a.size_bytes,
                    "display_name": a.display_name,
                    "schema": a.schema,
                })
            })
            .collect();

        let mut out = json!({
            "run_id": r.id,
            "module_id": r.module_id,
            "status": format!("{:?}", r.status),
            "started_at": r.started_at,
            "finished_at": r.finished_at,
            "error": r.error,
            "inputs_used": r.inputs_used,
            "assets_used": r.assets_used,
            "assets_produced": r.assets_produced,
            "produced_assets": produced_assets,
            "result": r.result.as_ref().map(|res| json!({
                "summary": res.summary,
                "output_files": res.output_files.iter().map(|p| path_string(p)).collect::<Vec<_>>(),
                "log_preview": res.log.lines().take(40).collect::<Vec<_>>().join("\n"),
                "log_truncated": res.log.lines().nth(40).is_some(),
            })),
        });
        if include_params {
            out["params"] = r.params.clone();
        }
        Ok(ToolOutput::Value(out))
    }
}

// ----- list_known_binaries -----

fn list_known_binaries_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "list_known_binaries".into(),
            description:
                "List external binaries known to RustBrain, including plugin-declared binaries, \
                with configured/bundled/PATH-detected status. Python, R, bash, and PowerShell \
                are not general built-in execution environments; only use them when a plugin \
                exposes a run_* tool and its required binary is available."
                    .into(),
            risk: RiskLevel::Read,
            params: json!({ "type": "object", "additionalProperties": false }),
        },
        executor: Arc::new(ListKnownBinaries),
    }
}

struct ListKnownBinaries;

#[async_trait]
impl ToolExecutor for ListKnownBinaries {
    async fn execute(&self, _args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let resolver = ctx.binary_resolver.lock().await;
        let items = resolver.list_known();
        Ok(ToolOutput::Value(json!({ "binaries": items })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rb_core::binary::BinaryResolver;
    use rb_core::module::ModuleResult;
    use rb_core::project::{Project, RunStatus};
    use rb_core::runner::Runner;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::{tempdir, TempDir};
    use tokio::sync::Mutex;

    fn make_ctx_fixture() -> (
        Arc<Mutex<Project>>,
        Arc<Runner>,
        Arc<Mutex<BinaryResolver>>,
        TempDir,
    ) {
        let tmp = tempdir().unwrap();
        let project = Project::create("t", tmp.path()).unwrap();
        std::fs::write(tmp.path().join("a.tsv"), "h1\th2\n1\t2\n3\t4\n").unwrap();
        std::fs::create_dir_all(tmp.path().join("data")).unwrap();
        let project = Arc::new(Mutex::new(project));
        let runner = Arc::new(Runner::new(project.clone()));
        let resolver = Arc::new(Mutex::new(BinaryResolver::with_defaults_at(
            tmp.path().join("binaries.json"),
        )));
        (project, runner, resolver, tmp)
    }

    #[tokio::test]
    async fn list_project_files_sees_top_level() {
        let (project, runner, resolver, _tmp) = make_ctx_fixture();
        let exec = ListProjectFiles;
        let out = exec
            .execute(
                &json!({}),
                ToolContext {
                    project: &project,
                    runner: &runner,
                    binary_resolver: &resolver,
                },
            )
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        let entries = v["entries"].as_array().unwrap();
        assert!(entries.iter().any(|e| e["name"] == "a.tsv"));
    }

    #[tokio::test]
    async fn read_table_preview_limits_rows() {
        let (project, runner, resolver, _tmp) = make_ctx_fixture();
        let exec = ReadTablePreview;
        let out = exec
            .execute(
                &json!({"path":"a.tsv","rows":2}),
                ToolContext {
                    project: &project,
                    runner: &runner,
                    binary_resolver: &resolver,
                },
            )
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["rows"], 2);
    }

    #[tokio::test]
    async fn read_table_preview_rejects_path_outside_project() {
        let (project, runner, resolver, _tmp) = make_ctx_fixture();
        let exec = ReadTablePreview;
        let err = exec
            .execute(
                &json!({"path":"../escape.tsv"}),
                ToolContext {
                    project: &project,
                    runner: &runner,
                    binary_resolver: &resolver,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArgs(_)));
    }

    #[tokio::test]
    async fn get_project_info_returns_name() {
        let (project, runner, resolver, _tmp) = make_ctx_fixture();
        let exec = GetProjectInfo;
        let out = exec
            .execute(
                &json!({}),
                ToolContext {
                    project: &project,
                    runner: &runner,
                    binary_resolver: &resolver,
                },
            )
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["name"], "t");
        assert_eq!(v["runs_count"], 0);
    }

    #[tokio::test]
    async fn list_project_inputs_and_samples_resolves_input_records() {
        let (project, runner, resolver, tmp) = make_ctx_fixture();
        let fastq = tmp.path().join("sample_R1.fastq");
        std::fs::write(&fastq, b"@r1\nACGT\n+\nIIII\n").unwrap();
        let input_id = {
            let mut proj = project.lock().await;
            let rec = proj.register_input(&fastq, None, None).unwrap();
            let sample = proj
                .create_sample(
                    "sample".into(),
                    Some("treated".into()),
                    None,
                    vec![rec.id.clone()],
                )
                .unwrap();
            assert_eq!(sample.inputs, vec![rec.id.clone()]);
            rec.id
        };

        let inputs_out = ListProjectInputs
            .execute(
                &json!({"kind":"Fastq"}),
                ToolContext {
                    project: &project,
                    runner: &runner,
                    binary_resolver: &resolver,
                },
            )
            .await
            .unwrap();
        let ToolOutput::Value(inputs_v) = inputs_out;
        assert_eq!(inputs_v["returned"], 1);
        assert_eq!(inputs_v["inputs"][0]["id"], input_id);

        let samples_out = ListProjectSamples
            .execute(
                &json!({}),
                ToolContext {
                    project: &project,
                    runner: &runner,
                    binary_resolver: &resolver,
                },
            )
            .await
            .unwrap();
        let ToolOutput::Value(samples_v) = samples_out;
        assert_eq!(samples_v["returned"], 1);
        assert_eq!(samples_v["samples"][0]["inputs"][0]["id"], input_id);
        assert_eq!(samples_v["samples"][0]["group"], "treated");
    }

    #[tokio::test]
    async fn summarize_run_returns_result_summary_and_log_preview() {
        let (project, runner, resolver, _tmp) = make_ctx_fixture();
        let run_id = {
            let mut proj = project.lock().await;
            let rec = proj.create_run("qc", json!({"input":"a.tsv"}));
            let run = proj.runs.iter_mut().find(|r| r.id == rec.id).unwrap();
            run.status = RunStatus::Done;
            run.result = Some(ModuleResult {
                output_files: vec![PathBuf::from("qc_report.json")],
                summary: json!({"pass": 3, "warn": 1}),
                log: "line1\nline2\n".into(),
            });
            rec.id
        };

        let out = SummarizeRun
            .execute(
                &json!({"run_id": run_id, "include_params": false}),
                ToolContext {
                    project: &project,
                    runner: &runner,
                    binary_resolver: &resolver,
                },
            )
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["module_id"], "qc");
        assert_eq!(v["result"]["summary"]["pass"], 3);
        assert_eq!(v["result"]["log_preview"], "line1\nline2");
        assert!(v.get("params").is_none());
    }
}

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::schema::{RiskLevel, ToolDef, ToolError};
use super::{ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry};

pub fn register_all(registry: &mut ToolRegistry) {
    registry.register(list_project_files_entry());
    registry.register(read_table_preview_entry());
    registry.register(get_project_info_entry());
    registry.register(get_run_status_entry());
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
        if !target.starts_with(&root) {
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

// ----- list_known_binaries -----

fn list_known_binaries_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "list_known_binaries".into(),
            description:
                "Which external tools (STAR, gffread-rs, cutadapt-rs, ...) are available to the app."
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
    use rb_core::project::Project;
    use rb_core::runner::Runner;
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
}

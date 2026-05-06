//! Memory-mutating tools the agent can invoke.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::memory::{
    crystallize::{long_term_update, Layer, LongTermBody},
    layers::{Scope, TodoEntry, WorkingCheckpoint},
    recall::{collect_candidates, Bm25Recaller, Recaller},
};
use crate::tools::{
    schema::{RiskLevel, ToolDef, ToolError},
    ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry,
};

pub fn register(reg: &mut ToolRegistry) {
    reg.register(entry(recall_def(), RecallExec));
    reg.register(entry(update_cp_def(), UpdateCpExec));
    reg.register(entry(long_term_def(), LongTermExec));
    reg.register(entry(task_done_def(), TaskDoneExec));
}

fn entry<E: ToolExecutor + 'static>(def: ToolDef, exec: E) -> ToolEntry {
    ToolEntry {
        def,
        executor: std::sync::Arc::new(exec),
    }
}

fn recall_def() -> ToolDef {
    ToolDef {
        name: "recall_memory".into(),
        description: "Search global+project memory for relevant skills, archives, insights.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "top_k": {"type": "integer", "default": 5}
            },
            "required": ["query"]
        }),
    }
}

fn update_cp_def() -> ToolDef {
    ToolDef {
        name: "update_working_checkpoint".into(),
        description: "Replace the in-progress todo list and progress note for this session.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {
                "todo": {"type": "array", "items": {"type":"object", "properties":{
                    "text":{"type":"string"},"done":{"type":"boolean"}
                }, "required":["text","done"]}},
                "perceive_snapshot_hash": {"type": "string"}
            },
            "required": ["todo"]
        }),
    }
}

fn long_term_def() -> ToolDef {
    ToolDef {
        name: "start_long_term_update".into(),
        description: "Write to L2 (facts) or L3 (skill SOP). Caller declares layer + scope.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {
                "layer": {"type": "string", "enum": ["l2", "l3"]},
                "scope": {"type": "string", "enum": ["global", "project"]},
                "section": {"type": "string"},
                "name": {"type": "string"},
                "triggers": {"type": "array", "items": {"type": "string"}},
                "markdown": {"type": "string"}
            },
            "required": ["layer", "scope", "markdown"]
        }),
    }
}

fn task_done_def() -> ToolDef {
    ToolDef {
        name: "task_done".into(),
        description: "Signal the current task is finished; triggers archive + insight crystallize."
            .into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {
                "headline": {"type": "string"},
                "tags": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["headline"]
        }),
    }
}

struct RecallExec;
#[async_trait]
impl ToolExecutor for RecallExec {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("query required".into()))?;
        let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
        let store = ctx
            .memory
            .ok_or_else(|| ToolError::Execution("memory not wired".into()))?;
        let cands = collect_candidates(store, ctx.project_root)
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let r = Bm25Recaller::new(top_k)
            .recall(query, cands, 4096)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(ToolOutput::Value(serde_json::to_value(r).unwrap()))
    }
}

struct UpdateCpExec;
#[async_trait]
impl ToolExecutor for UpdateCpExec {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let store = ctx
            .memory
            .ok_or_else(|| ToolError::Execution("memory not wired".into()))?;
        let project_root = ctx
            .project_root
            .ok_or_else(|| ToolError::Execution("project_root not wired".into()))?;
        let session_id = ctx
            .session_id
            .ok_or_else(|| ToolError::Execution("session_id not wired".into()))?;
        let todo: Vec<TodoEntry> = serde_json::from_value(
            args.get("todo")
                .cloned()
                .ok_or_else(|| ToolError::InvalidArgs("todo required".into()))?,
        )
        .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let snapshot_hash = args
            .get("perceive_snapshot_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let cp = WorkingCheckpoint {
            session_id: session_id.into(),
            project_root: project_root.display().to_string(),
            started_at: chrono::Utc::now(),
            last_step_at: chrono::Utc::now(),
            todo,
            message_count: 0,
            perceive_snapshot_hash: snapshot_hash,
        };
        store
            .write_checkpoint(project_root, &cp)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(ToolOutput::Value(json!({"ok": true})))
    }
}

struct LongTermExec;
#[async_trait]
impl ToolExecutor for LongTermExec {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let store = ctx
            .memory
            .ok_or_else(|| ToolError::Execution("memory not wired".into()))?;
        let layer = match args.get("layer").and_then(|v| v.as_str()).unwrap_or("") {
            "l2" => Layer::L2,
            "l3" => Layer::L3,
            other => return Err(ToolError::InvalidArgs(format!("layer={other}"))),
        };
        let scope = match args.get("scope").and_then(|v| v.as_str()).unwrap_or("") {
            "global" => Scope::Global,
            "project" => Scope::Project,
            other => return Err(ToolError::InvalidArgs(format!("scope={other}"))),
        };
        let body = LongTermBody {
            section: args
                .get("section")
                .and_then(|v| v.as_str())
                .map(|s| s.into()),
            name: args.get("name").and_then(|v| v.as_str()).map(|s| s.into()),
            triggers: args
                .get("triggers")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            markdown: args
                .get("markdown")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArgs("markdown required".into()))?
                .into(),
        };
        let r = long_term_update(store, ctx.project_root, layer, scope, body)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(ToolOutput::Value(serde_json::to_value(r).unwrap()))
    }
}

struct TaskDoneExec;
#[async_trait]
impl ToolExecutor for TaskDoneExec {
    async fn execute(&self, args: &Value, _ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        // task_done is a control-flow marker; agent_loop intercepts the call
        // and triggers crystallize_session. Here we just echo the intent.
        let headline = args
            .get("headline")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("headline required".into()))?;
        Ok(ToolOutput::Value(json!({
            "task_done": true,
            "headline": headline,
            "tags": args.get("tags").cloned().unwrap_or(json!([]))
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryStore;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn ctx_with_mem<'a>(
        store: &'a Arc<MemoryStore>,
        project: &'a Arc<tokio::sync::Mutex<rb_core::project::Project>>,
        runner: &'a Arc<rb_core::runner::Runner>,
        binres: &'a Arc<tokio::sync::Mutex<rb_core::binary::BinaryResolver>>,
        proot: &'a std::path::Path,
    ) -> ToolContext<'a> {
        ToolContext {
            project,
            runner,
            binary_resolver: binres,
            memory: Some(store),
            session_id: Some("sess1"),
            project_root: Some(proot),
            ask_user_tx: None,
        }
    }

    #[tokio::test]
    async fn update_checkpoint_writes_file() {
        let tmp = tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
        let proot = tmp.path().join("proj");
        store.ensure_project(&proot).unwrap();
        let project = Arc::new(tokio::sync::Mutex::new(
            rb_core::project::Project::create("t", &proot).unwrap(),
        ));
        let runner = Arc::new(rb_core::runner::Runner::new(project.clone()));
        let binres = Arc::new(tokio::sync::Mutex::new(
            rb_core::binary::BinaryResolver::with_defaults_at(proot.join("binaries.json")),
        ));
        let ctx = ctx_with_mem(&store, &project, &runner, &binres, &proot);
        let exec = UpdateCpExec;
        exec.execute(
            &json!({
                "todo": [{"text":"qc","done":false}],
                "perceive_snapshot_hash": "abc"
            }),
            ctx,
        )
        .await
        .unwrap();
        assert!(proot.join("agent/checkpoints/current.json").exists());
    }

    #[tokio::test]
    async fn task_done_echoes_headline() {
        let tmp = tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
        let proot = tmp.path().join("proj");
        store.ensure_project(&proot).unwrap();
        let project = Arc::new(tokio::sync::Mutex::new(
            rb_core::project::Project::create("t", &proot).unwrap(),
        ));
        let runner = Arc::new(rb_core::runner::Runner::new(project.clone()));
        let binres = Arc::new(tokio::sync::Mutex::new(
            rb_core::binary::BinaryResolver::with_defaults_at(proot.join("binaries.json")),
        ));
        let ctx = ctx_with_mem(&store, &project, &runner, &binres, &proot);
        let exec = TaskDoneExec;
        let out = exec
            .execute(&json!({"headline": "did rna-seq", "tags":["rna-seq"]}), ctx)
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["task_done"], true);
        assert_eq!(v["headline"], "did rna-seq");
    }
}

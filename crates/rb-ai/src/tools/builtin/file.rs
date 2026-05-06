//! Generic-path file tools.
//!
//! - `file_read` / `file_list` are read-only and run freely.
//! - `file_write` / `file_patch` are bucketed by SandboxPolicy upstream:
//!   sandbox paths run freely, in-project paths require approval, outside
//!   paths always ask. The tools themselves do raw IO and trust the
//!   agent_loop dispatcher to gate.

use std::path::Path;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::schema::{RiskLevel, ToolDef, ToolError};
use crate::tools::{ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry};

pub fn register(reg: &mut ToolRegistry) {
    reg.register(ToolEntry {
        def: file_read_def(),
        executor: std::sync::Arc::new(FileReadExec),
    });
    reg.register(ToolEntry {
        def: file_list_def(),
        executor: std::sync::Arc::new(FileListExec),
    });
    reg.register(ToolEntry {
        def: file_write_def(),
        executor: std::sync::Arc::new(FileWriteExec),
    });
    reg.register(ToolEntry {
        def: file_patch_def(),
        executor: std::sync::Arc::new(FilePatchExec),
    });
}

fn file_read_def() -> ToolDef {
    ToolDef {
        name: "file_read".into(),
        description: "Read a UTF-8 text file. Returns up to ~64KB.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "max_bytes": {"type": "integer", "default": 65536}
            },
            "required": ["path"]
        }),
    }
}

fn file_list_def() -> ToolDef {
    ToolDef {
        name: "file_list".into(),
        description: "List a directory; returns up to 200 entries.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"]
        }),
    }
}

fn file_write_def() -> ToolDef {
    ToolDef {
        name: "file_write".into(),
        description: "Write a text file. Risk depends on path: sandbox \
            paths run freely; project paths require approval; outside-project \
            paths always ask."
            .into(),
        risk: RiskLevel::RunLow,
        params: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["path", "content"]
        }),
    }
}

fn file_patch_def() -> ToolDef {
    ToolDef {
        name: "file_patch".into(),
        description: "Apply a unified diff to a single file.".into(),
        risk: RiskLevel::RunLow,
        params: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "diff": {"type": "string"}
            },
            "required": ["path", "diff"]
        }),
    }
}

struct FileReadExec;
#[async_trait]
impl ToolExecutor for FileReadExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let max = args
            .get("max_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(65536) as usize;
        let mut bytes = std::fs::read(path).map_err(|e| ToolError::Execution(e.to_string()))?;
        let truncated = bytes.len() > max;
        if truncated {
            bytes.truncate(max);
        }
        let body = String::from_utf8_lossy(&bytes).into_owned();
        Ok(ToolOutput::Value(json!({
            "path": path, "truncated": truncated, "content": body
        })))
    }
}

struct FileListExec;
#[async_trait]
impl ToolExecutor for FileListExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let mut entries = vec![];
        for ent in std::fs::read_dir(path)
            .map_err(|e| ToolError::Execution(e.to_string()))?
            .flatten()
            .take(200)
        {
            let n = ent.file_name().to_string_lossy().to_string();
            let kind = ent
                .file_type()
                .map(|t| if t.is_dir() { "dir" } else { "file" })
                .unwrap_or("?")
                .to_string();
            entries.push(json!({"name": n, "kind": kind}));
        }
        Ok(ToolOutput::Value(json!({"entries": entries})))
    }
}

pub struct FileWriteExec;
#[async_trait]
impl ToolExecutor for FileWriteExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("content required".into()))?;
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::Execution(e.to_string()))?;
        }
        std::fs::write(path, content).map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(ToolOutput::Value(json!({"path": path, "bytes": content.len()})))
    }
}

pub struct FilePatchExec;
#[async_trait]
impl ToolExecutor for FilePatchExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let diff = args
            .get("diff")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("diff required".into()))?;
        let original = std::fs::read_to_string(path).map_err(|e| ToolError::Execution(e.to_string()))?;
        let patched = apply_unified_diff(&original, diff)
            .map_err(|e| ToolError::Execution(format!("patch failed: {e}")))?;
        std::fs::write(path, &patched).map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(ToolOutput::Value(json!({"path": path})))
    }
}

/// Minimal unified-diff applier. Handles single-file diffs with one or more
/// `@@ -old,len +new,len @@` hunks; tolerates trailing newlines. Not a full
/// patch(1) replacement — sufficient for LLM-generated edits.
fn apply_unified_diff(original: &str, diff: &str) -> Result<String, String> {
    let lines: Vec<&str> = original.split_inclusive('\n').collect();
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    let mut pending: Vec<(usize, usize, Vec<String>, Vec<String>)> = vec![];

    let dlines: Vec<&str> = diff.lines().collect();
    let mut i = 0;
    while i < dlines.len() {
        let l = dlines[i];
        if l.starts_with("---") || l.starts_with("+++") {
            i += 1;
            continue;
        }
        if let Some(hdr) = l.strip_prefix("@@") {
            // Format: " -old_start,old_len +new_start,new_len @@..."
            let parts: Vec<&str> = hdr.split_whitespace().collect();
            let old = parts.iter().find(|p| p.starts_with('-')).copied().unwrap_or("-0,0");
            let old_start: usize = old
                .trim_start_matches('-')
                .split(',')
                .next()
                .unwrap()
                .parse()
                .map_err(|e| format!("bad hunk header: {e}"))?;
            let mut hunk_old: Vec<String> = vec![];
            let mut hunk_new: Vec<String> = vec![];
            i += 1;
            while i < dlines.len() && !dlines[i].starts_with("@@") {
                let h = dlines[i];
                if let Some(s) = h.strip_prefix(' ') {
                    hunk_old.push(format!("{s}\n"));
                    hunk_new.push(format!("{s}\n"));
                } else if let Some(s) = h.strip_prefix('-') {
                    hunk_old.push(format!("{s}\n"));
                } else if let Some(s) = h.strip_prefix('+') {
                    hunk_new.push(format!("{s}\n"));
                }
                i += 1;
            }
            pending.push((old_start.saturating_sub(1), hunk_old.len(), hunk_old, hunk_new));
            continue;
        }
        i += 1;
    }

    // Apply hunks in reverse so earlier offsets stay valid.
    pending.sort_by_key(|h| std::cmp::Reverse(h.0));
    for (start, len, old, new) in pending {
        if start + len > out.len() {
            return Err(format!("hunk start {start} len {len} > file len {}", out.len()));
        }
        let actual: Vec<String> = out[start..start + len].to_vec();
        if actual != old {
            return Err("context mismatch".into());
        }
        out.splice(start..start + len, new);
    }

    Ok(out.join(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn dummy_ctx(root: &std::path::Path) -> ToolContext<'static> {
        // Build leaks/Box::leak: only acceptable in tests for short-lived ctx.
        let project = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
            rb_core::project::Project::create("t", root).unwrap(),
        ))));
        let runner = Box::leak(Box::new(std::sync::Arc::new(rb_core::runner::Runner::new(
            project.clone(),
        ))));
        let binres = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
            rb_core::binary::BinaryResolver::with_defaults_at(root.join("binaries.json")),
        ))));
        ToolContext {
            project,
            runner,
            binary_resolver: binres,
            memory: None,
            session_id: None,
            project_root: None,
            ask_user_tx: None,
        }
    }

    #[tokio::test]
    async fn file_read_truncates_large_files() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("big.txt");
        std::fs::write(&p, "x".repeat(1000)).unwrap();
        let exec = FileReadExec;
        let out = exec
            .execute(
                &json!({"path": p.display().to_string(), "max_bytes": 50}),
                dummy_ctx(tmp.path()),
            )
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["truncated"], true);
        assert_eq!(v["content"].as_str().unwrap().len(), 50);
    }

    #[tokio::test]
    async fn file_write_writes_to_sandbox() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("foo.txt");
        let exec = FileWriteExec;
        exec.execute(
            &json!({"path": p.display().to_string(), "content": "hi"}),
            dummy_ctx(tmp.path()),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "hi");
    }

    #[tokio::test]
    async fn file_patch_applies_unified_diff() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("a.txt");
        std::fs::write(&p, "alpha\nbeta\ngamma\n").unwrap();
        let exec = FilePatchExec;
        let diff = "\
--- a/a.txt
+++ b/a.txt
@@ -1,3 +1,3 @@
 alpha
-beta
+BETA
 gamma
";
        exec.execute(
            &json!({"path": p.display().to_string(), "diff": diff}),
            dummy_ctx(tmp.path()),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "alpha\nBETA\ngamma\n"
        );
    }
}

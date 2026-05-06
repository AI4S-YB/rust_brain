//! Risk bucketing + path canonicalization for the agent sandbox.
//!
//! `classify` takes a tool call (name + args) and returns the bucket and
//! whether it can run immediately, requires a one-time approval, or always
//! asks. Full-permission mode bypasses approval gates.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::AiError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Bucket {
    ReadFs,
    SandboxWrite,
    ProjectModule { module: String },
    CodeRunSandbox,
    CodeRunOutOfSandbox,
    Web,
    MemoryWrite,
    DestructiveDelete,
    AskUser,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow,
    ApproveOnce,
    AlwaysAsk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyMode {
    Normal,
    FullPermission,
}

pub struct SandboxPolicy {
    pub mode: PolicyMode,
    pub project_root: PathBuf,
    pub sandbox_dir: PathBuf,
    pub approved: Mutex<HashSet<Bucket>>,
}

impl SandboxPolicy {
    pub fn new(project_root: PathBuf, sandbox_dirname: &str) -> Self {
        let sandbox_dir = project_root.join(sandbox_dirname);
        let _ = std::fs::create_dir_all(&sandbox_dir);
        Self {
            mode: PolicyMode::Normal,
            project_root,
            sandbox_dir,
            approved: Mutex::new(HashSet::new()),
        }
    }

    pub fn full_permission(mut self) -> Self {
        self.mode = PolicyMode::FullPermission;
        self
    }

    pub fn classify(&self, tool_name: &str, args: &serde_json::Value) -> (Bucket, Decision) {
        // Memory + control tools.
        if matches!(
            tool_name,
            "recall_memory" | "update_working_checkpoint" | "task_done"
        ) {
            return (Bucket::ReadFs, Decision::Allow);
        }
        if tool_name == "ask_user" {
            return (Bucket::AskUser, Decision::Allow);
        }
        if tool_name == "start_long_term_update" {
            return (Bucket::MemoryWrite, Decision::Allow);
        }

        // Read-only filesystem.
        if matches!(
            tool_name,
            "file_read" | "file_list" | "read_results_table" | "read_run_log" | "project_state"
        ) {
            return (Bucket::ReadFs, Decision::Allow);
        }

        // File writes / patches: depends on path.
        if matches!(tool_name, "file_write" | "file_patch") {
            let path_str = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            return match self.classify_write_path(Path::new(path_str)) {
                WritePath::InsideSandbox => (Bucket::SandboxWrite, Decision::Allow),
                WritePath::InsideProject => {
                    (Bucket::ProjectModule { module: "fs".into() }, Decision::ApproveOnce)
                }
                WritePath::OutsideProject => (Bucket::DestructiveDelete, Decision::AlwaysAsk),
            };
        }

        // Code run.
        if tool_name == "code_run" {
            let cwd = args
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .unwrap_or_else(|| self.sandbox_dir.clone());
            return if self.path_inside(&cwd, &self.sandbox_dir) {
                (Bucket::CodeRunSandbox, Decision::Allow)
            } else if self.path_inside(&cwd, &self.project_root) {
                (Bucket::CodeRunOutOfSandbox, Decision::ApproveOnce)
            } else {
                (Bucket::CodeRunOutOfSandbox, Decision::AlwaysAsk)
            };
        }

        // Web.
        if matches!(tool_name, "web_scan" | "web_execute_js") {
            return (Bucket::Web, Decision::Allow);
        }

        // Module-derived tools start with `run_`.
        if let Some(module) = tool_name.strip_prefix("run_") {
            return (
                Bucket::ProjectModule {
                    module: module.into(),
                },
                Decision::ApproveOnce,
            );
        }

        // Skill tools: `skill_<slug>` → first-call approval.
        if let Some(slug) = tool_name.strip_prefix("skill_") {
            return (
                Bucket::ProjectModule {
                    module: format!("skill:{slug}"),
                },
                Decision::ApproveOnce,
            );
        }

        // Default unknown: always ask.
        (Bucket::DestructiveDelete, Decision::AlwaysAsk)
    }

    /// Apply approval-cache + full-permission bypass. Returns whether the
    /// caller should run immediately (true), or wait for user approval (false).
    pub fn should_run(&self, bucket: &Bucket, decision: &Decision) -> bool {
        match self.mode {
            PolicyMode::FullPermission => true,
            PolicyMode::Normal => match decision {
                Decision::Allow => true,
                Decision::ApproveOnce => self.approved.lock().unwrap().contains(bucket),
                Decision::AlwaysAsk => false,
            },
        }
    }

    pub fn record_approval(&self, bucket: Bucket) {
        self.approved.lock().unwrap().insert(bucket);
    }

    fn classify_write_path(&self, p: &Path) -> WritePath {
        let abs = self.canonicalize_or_join(p);
        if self.path_inside(&abs, &self.sandbox_dir) {
            WritePath::InsideSandbox
        } else if self.path_inside(&abs, &self.project_root) {
            WritePath::InsideProject
        } else {
            WritePath::OutsideProject
        }
    }

    fn path_inside(&self, candidate: &Path, root: &Path) -> bool {
        let c = self.canonicalize_or_join(candidate);
        let r = self
            .canonicalize_or_join(root)
            .to_string_lossy()
            .to_string();
        c.to_string_lossy().starts_with(&r)
    }

    fn canonicalize_or_join(&self, p: &Path) -> PathBuf {
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.project_root.join(p)
        };
        let cleaned = path_clean::clean(&abs);
        std::fs::canonicalize(&cleaned).unwrap_or(cleaned)
    }
}

#[derive(Debug, PartialEq)]
enum WritePath {
    InsideSandbox,
    InsideProject,
    OutsideProject,
}

/// Fail-closed path validator for actual file writes (not classification).
/// Used by the file_write tool implementation in Phase 3.
pub fn require_inside(root: &Path, candidate: &Path) -> Result<PathBuf, AiError> {
    let abs = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let cleaned = path_clean::clean(&abs);
    if !cleaned.starts_with(root) {
        return Err(AiError::PathEscape(cleaned.display().to_string()));
    }
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn pol(tmp: &Path) -> SandboxPolicy {
        let p = tmp.to_path_buf();
        std::fs::create_dir_all(p.join("sandbox")).unwrap();
        SandboxPolicy::new(p, "sandbox")
    }

    #[test]
    fn read_tools_are_allow() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let (b, d) = p.classify("file_read", &json!({"path": "x"}));
        assert_eq!(b, Bucket::ReadFs);
        assert_eq!(d, Decision::Allow);
    }

    #[test]
    fn write_to_sandbox_is_allow() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let path = tmp.path().join("sandbox/foo.py");
        let (b, d) = p.classify("file_write", &json!({"path": path.display().to_string()}));
        assert_eq!(b, Bucket::SandboxWrite);
        assert_eq!(d, Decision::Allow);
    }

    #[test]
    fn write_inside_project_outside_sandbox_is_approve_once() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let path = tmp.path().join("results/out.tsv");
        let (_b, d) = p.classify("file_write", &json!({"path": path.display().to_string()}));
        assert_eq!(d, Decision::ApproveOnce);
    }

    #[test]
    fn write_outside_project_is_always_ask() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let outside = std::env::temp_dir().join("not_my_project").join("x");
        let (_, d) = p.classify("file_write", &json!({"path": outside.display().to_string()}));
        assert_eq!(d, Decision::AlwaysAsk);
    }

    #[test]
    fn run_module_tool_is_approve_once() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let (b, d) = p.classify("run_qc", &json!({}));
        match b {
            Bucket::ProjectModule { module } => assert_eq!(module, "qc"),
            _ => panic!(),
        }
        assert_eq!(d, Decision::ApproveOnce);
    }

    #[test]
    fn full_permission_bypasses_approval() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path()).full_permission();
        let (b, d) = p.classify("run_qc", &json!({}));
        assert!(p.should_run(&b, &d));
    }

    #[test]
    fn approve_once_caches_within_session() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let (b, d) = p.classify("run_qc", &json!({}));
        assert!(!p.should_run(&b, &d));
        p.record_approval(b.clone());
        assert!(p.should_run(&b, &d));
    }

    #[test]
    fn require_inside_rejects_dotdot_escape() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let bad = require_inside(&root, Path::new("../etc/passwd"));
        assert!(bad.is_err());
        let ok = require_inside(&root, Path::new("sub/file"));
        assert!(ok.is_ok());
    }
}

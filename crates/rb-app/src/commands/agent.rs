//! Tauri commands for the self-evolving agent.
//!
//! Pattern: each command takes the AppState (which embeds AgentRuntime),
//! does its work async, returns Result<T, String> (Tauri serializes errors
//! as strings). Heavy lifting (running provider, executing tools) happens
//! in a background tokio task spawned by agent_send; commands themselves
//! return quickly.

use std::path::PathBuf;
use std::sync::Arc;

use rb_ai::agent_loop::{AgentSession, ApprovalVerdict};
use rb_ai::sandbox::SandboxPolicy;
use rb_ai::tools::{builtin, ToolRegistry};
use rb_core::cancel::CancellationToken;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::{mpsc, Mutex};

use crate::agent_runtime::{AgentHandle, AgentRuntime};

#[derive(Debug, Serialize)]
pub struct StartSessionResp {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct StartSessionArgs {
    pub project_root: String,
    pub full_permission: Option<bool>,
}

#[tauri::command]
pub async fn agent_start_session(
    args: StartSessionArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<StartSessionResp, String> {
    if runtime.handle_for(&args.project_root).await.is_some() {
        return Err(format!(
            "agent already running for project {}",
            args.project_root
        ));
    }
    let project_root = PathBuf::from(&args.project_root);
    runtime
        .memory
        .ensure_project(&project_root)
        .map_err(|e| e.to_string())?;
    let session = Arc::new(Mutex::new(AgentSession::new(args.project_root.clone())));
    let session_id = session.lock().await.id.clone();
    let mut policy = SandboxPolicy::new(project_root.clone(), "sandbox");
    if args.full_permission.unwrap_or(false) {
        policy = policy.full_permission();
    }
    let policy = Arc::new(policy);
    let (approval_tx, _approval_rx_unused) = mpsc::channel::<(String, ApprovalVerdict)>(8);
    let (ask_user_tx, _ask_user_rx_unused) = mpsc::channel::<rb_ai::tools::AskUserRequest>(8);
    let handle = Arc::new(AgentHandle {
        session_id: session_id.clone(),
        session,
        policy,
        cancel: CancellationToken::new(),
        approval_tx,
        ask_user_tx,
        run_join: Mutex::new(None),
    });
    runtime.insert(args.project_root, handle).await;
    Ok(StartSessionResp { session_id })
}

/// Build a per-session ToolRegistry (builtin + module-derived + skill loader).
pub(crate) fn build_registry(
    modules: &[Arc<dyn rb_core::module::Module>],
    lang: &str,
    memory_global: &std::path::Path,
    project_root: &std::path::Path,
) -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    builtin::register_all(&mut reg);
    rb_ai::tools::module_derived::register_for_modules(&mut reg, modules, lang);
    let _ = rb_ai::tools::skill::register_dir(&mut reg, &memory_global.join("L3_skills"));
    let _ = rb_ai::tools::skill::register_dir(&mut reg, &project_root.join("agent/L3_local"));
    reg
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn build_registry_includes_builtin_and_skill_dirs() {
        let tmp = tempdir().unwrap();
        let global = tmp.path().join("global");
        std::fs::create_dir_all(global.join("L3_skills")).unwrap();
        std::fs::write(
            global.join("L3_skills/rna-seq-de.md"),
            "---\nname: rna-seq-de\ndescription: x\n---\nbody",
        )
        .unwrap();
        let proot = tmp.path().join("proj");
        std::fs::create_dir_all(proot.join("agent/L3_local")).unwrap();
        let reg = build_registry(&[], "en", &global, &proot);
        assert!(reg.get("file_read").is_some());
        assert!(reg.get("skill_rna_seq_de").is_some());
    }
}

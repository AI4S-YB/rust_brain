//! Tauri commands for the self-evolving agent.
//!
//! Pattern: each command takes the AppState (which embeds AgentRuntime),
//! does its work async, returns Result<T, String> (Tauri serializes errors
//! as strings). Heavy lifting (running provider, executing tools) happens
//! in a background tokio task spawned by agent_send; commands themselves
//! return quickly.

use std::collections::HashMap;
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
        approval_tx_slot: Mutex::new(approval_tx),
        ask_user_tx_slot: Mutex::new(ask_user_tx),
        pending_asks_slot: Mutex::new(None),
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

use rb_ai::agent_loop::{run_session, AgentEvent, RunConfig, RunSessionCtx};
use rb_ai::memory::Bm25Recaller;
use rb_ai::sandbox::NetLogger;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Deserialize)]
pub struct SendArgs {
    pub project_root: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct AskUserOutbound {
    pub session_id: String,
    pub call_id: String,
    pub prompt: String,
}

#[tauri::command]
pub async fn agent_send(
    args: SendArgs,
    app: AppHandle,
    runtime: State<'_, Arc<AgentRuntime>>,
    module_state: State<'_, crate::AppState>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or_else(|| format!("no agent session for project {}", args.project_root))?;

    // Refuse a second concurrent run for the same handle.
    {
        let mut join_slot = handle.run_join.lock().await;
        if let Some(j) = join_slot.take() {
            if !j.is_finished() {
                *join_slot = Some(j);
                return Err("a run is already in flight; cancel it or wait".into());
            }
        }
    }

    let session_id = handle.session_id.clone();
    let project_root_pb = PathBuf::from(&args.project_root);
    let memory = runtime.memory.clone();
    let ai_state = module_state.ai.clone();

    // Snapshot the runner Arc; agent_send requires a project to be open.
    let runner = {
        let opt = module_state.runner.lock().await;
        opt.as_ref()
            .cloned()
            .ok_or_else(|| "no project open; open a project before sending agent messages".to_string())?
    };

    // Build the registry per send.
    let modules: Vec<Arc<dyn rb_core::module::Module>> = {
        let reg = module_state.registry.lock().await;
        reg.list_all()
    };
    // AiConfig has no `lang` field today; default to "en". This can be wired
    // through UiConfig in a future task.
    let lang = "en".to_string();
    let registry = Arc::new(build_registry(
        &modules,
        &lang,
        &memory.global_root,
        &project_root_pb,
    ));

    // Provider.
    let provider = crate::commands::ai_provider::resolve_chat_provider(&ai_state)
        .await
        .map_err(|e| format!("provider init: {e}"))?;

    // Channels.
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(64);
    let (approval_tx, approval_rx) = mpsc::channel::<(String, ApprovalVerdict)>(8);
    let (ask_user_tx, mut ask_user_rx) = mpsc::channel::<rb_ai::tools::AskUserRequest>(8);
    let approval_rx = Arc::new(Mutex::new(approval_rx));

    // Replace handle's slots so agent_approve / agent_answer target the right channels.
    *handle.approval_tx_slot.lock().await = approval_tx.clone();
    *handle.ask_user_tx_slot.lock().await = ask_user_tx.clone();

    // Forwarder: AgentEvent → Tauri emit.
    let app_for_events = app.clone();
    let forwarder_events = tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            let _ = app_for_events.emit("agent-stream", &ev);
        }
    });

    // Forwarder: ask_user → emit + store responder.
    let pending_asks: Arc<Mutex<HashMap<String, mpsc::Sender<String>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    *handle.pending_asks_slot.lock().await = Some(pending_asks.clone());
    let app_for_ask = app.clone();
    let session_id_for_ask = session_id.clone();
    let pending_for_forwarder = pending_asks.clone();
    let forwarder_ask = tokio::spawn(async move {
        while let Some(req) = ask_user_rx.recv().await {
            pending_for_forwarder
                .lock()
                .await
                .insert(req.call_id.clone(), req.responder);
            let _ = app_for_ask.emit(
                "agent-ask-user",
                &AskUserOutbound {
                    session_id: session_id_for_ask.clone(),
                    call_id: req.call_id,
                    prompt: req.prompt,
                },
            );
        }
    });

    // Net log (disabled when policy = FullPermission).
    let net_log_enabled = !handle.policy.is_full_permission();
    let net_log = Arc::new(
        NetLogger::new(&project_root_pb, &session_id, net_log_enabled)
            .map_err(|e| e.to_string())?,
    );
    let recaller: Arc<dyn rb_ai::memory::Recaller> = Arc::new(Bm25Recaller::new(5));

    // Snapshot the send arguments we still need after moving into the task.
    let text = args.text;

    // Spawn run_session.
    let ctx = RunSessionCtx {
        project: runner.project_arc(),
        runner: runner.clone(),
        binary_resolver: module_state.binary_resolver.clone(),
        registry,
        policy: handle.policy.clone(),
        memory,
        recaller,
        provider,
        net_log,
        project_root: project_root_pb,
        config: RunConfig::default(),
    };
    let session_arc = handle.session.clone();
    let cancel = handle.cancel.clone();
    let join = tokio::spawn(async move {
        let r = run_session(
            ctx,
            text,
            session_arc,
            event_tx,
            ask_user_tx,
            approval_rx,
            cancel,
        )
        .await;
        forwarder_events.abort();
        forwarder_ask.abort();
        r
    });
    *handle.run_join.lock().await = Some(join);
    Ok(())
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

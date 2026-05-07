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

/// Build a per-session ToolRegistry (builtin + project_state + module-derived
/// + skill loader). The project/runner/binary_resolver are now passed in so
/// each tool that needs them carries its own `Arc<...>`.
pub(crate) fn build_registry(
    modules: &[Arc<dyn rb_core::module::Module>],
    runner: Arc<rb_core::runner::Runner>,
    project: Arc<Mutex<rb_core::project::Project>>,
    binary_resolver: Arc<Mutex<rb_core::binary::BinaryResolver>>,
    lang: &str,
    memory_global: &std::path::Path,
    project_root: &std::path::Path,
) -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    builtin::register_all(&mut reg);
    rb_ai_bio::project_state::register(&mut reg, project, binary_resolver);
    rb_ai_bio::module_derived::register_for_modules(&mut reg, modules, runner, lang);
    let _ = rb_ai::tools::skill::register_dir(&mut reg, &memory_global.join("L3_skills"));
    let _ = rb_ai::tools::skill::register_dir(&mut reg, &project_root.join("agent/L3_local"));
    reg
}

/// Build the system-prompt project summary the agent loop used to compute
/// inline. Moved here so rb-ai stays generic — the host (rb-app) owns the
/// rust_brain Project model and renders its preferred summary string.
pub(crate) async fn project_summary(project: &Arc<Mutex<rb_core::project::Project>>) -> String {
    let p = project.lock().await;
    format!(
        "Project: {}\nDefault view: {}\nRecent runs:\n{}",
        p.name,
        p.default_view.as_deref().unwrap_or("manual"),
        p.runs
            .iter()
            .rev()
            .take(10)
            .map(|r| format!("  {}: {} {:?}", r.id, r.module_id, r.status))
            .collect::<Vec<_>>()
            .join("\n")
    )
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
        opt.as_ref().cloned().ok_or_else(|| {
            "no project open; open a project before sending agent messages".to_string()
        })?
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
        runner.clone(),
        runner.project_arc(),
        module_state.binary_resolver.clone(),
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

    // Build the system-prompt project summary up-front (rb-ai is now host-
    // agnostic; we render the rust_brain-specific snapshot here).
    let system_context = project_summary(&runner.project_arc()).await;

    // Spawn run_session.
    let ctx = RunSessionCtx {
        registry,
        policy: handle.policy.clone(),
        memory,
        recaller,
        provider,
        net_log,
        project_root: project_root_pb,
        system_context,
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
        let project = Arc::new(Mutex::new(
            rb_core::project::Project::create("t", &proot).unwrap(),
        ));
        let runner = Arc::new(rb_core::runner::Runner::new(project.clone()));
        let binres = Arc::new(Mutex::new(
            rb_core::binary::BinaryResolver::with_defaults_at(proot.join("binaries.json")),
        ));
        let reg = build_registry(&[], runner, project, binres, "en", &global, &proot);
        assert!(reg.get("file_read").is_some());
        assert!(reg.get("skill_rna_seq_de").is_some());
    }
}

#[derive(Debug, Deserialize)]
pub struct ApproveArgs {
    pub project_root: String,
    pub call_id: String,
    pub edited_args: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn agent_approve(
    args: ApproveArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or("no agent session")?;
    let tx = handle.approval_tx_slot.lock().await.clone();
    tx.send((
        args.call_id,
        ApprovalVerdict::Approve {
            edited_args: args.edited_args,
        },
    ))
    .await
    .map_err(|e| format!("approve send: {e}"))
}

#[derive(Debug, Deserialize)]
pub struct RejectArgs {
    pub project_root: String,
    pub call_id: String,
    pub reason: Option<String>,
}

#[tauri::command]
pub async fn agent_reject(
    args: RejectArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or("no agent session")?;
    let tx = handle.approval_tx_slot.lock().await.clone();
    tx.send((
        args.call_id,
        ApprovalVerdict::Reject {
            reason: args.reason,
        },
    ))
    .await
    .map_err(|e| format!("reject send: {e}"))
}

#[derive(Debug, Deserialize)]
pub struct AnswerArgs {
    pub project_root: String,
    pub call_id: String,
    pub reply: String,
}

#[tauri::command]
pub async fn agent_answer(
    args: AnswerArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or("no agent session")?;
    let pending_lock = handle.pending_asks_slot.lock().await;
    let pending = pending_lock
        .as_ref()
        .ok_or("no in-flight run; ask_user only valid during agent_send")?;
    let mut p = pending.lock().await;
    let tx = p
        .remove(&args.call_id)
        .ok_or_else(|| format!("no pending ask_user with call_id {}", args.call_id))?;
    tx.send(args.reply)
        .await
        .map_err(|e| format!("answer send: {e}"))
}

#[derive(Debug, Deserialize)]
pub struct CancelArgs {
    pub project_root: String,
}

#[tauri::command]
pub async fn agent_cancel(
    args: CancelArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or("no agent session")?;
    handle.cancel.cancel();
    let mut slot = handle.run_join.lock().await;
    if let Some(j) = slot.take() {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), j).await;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct FullPermArgs {
    pub project_root: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn agent_set_full_permission(
    args: FullPermArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or("no agent session")?;
    handle.policy.set_full_permission(args.enabled);
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct ArchiveListEntry {
    pub id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub summary: String,
    pub outcome: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListArchivesArgs {
    pub project_root: String,
}

#[tauri::command]
pub async fn agent_list_archives(
    args: ListArchivesArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<Vec<ArchiveListEntry>, String> {
    let path = rb_ai::memory::MemoryStore::project_root(&PathBuf::from(&args.project_root))
        .join("L4_archives/_index.json");
    let entries = runtime
        .memory
        .read_index(&path)
        .map_err(|e| e.to_string())?;
    let mut out = vec![];
    for e in entries {
        if let rb_ai::memory::IndexEntry::Archive {
            id,
            started_at,
            ended_at,
            summary,
            outcome,
            tags,
        } = e
        {
            out.push(ArchiveListEntry {
                id,
                started_at: started_at.to_rfc3339(),
                ended_at: ended_at.map(|d| d.to_rfc3339()),
                summary,
                outcome: format!("{outcome:?}").to_lowercase(),
                tags,
            });
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
pub struct LoadArchiveArgs {
    pub project_root: String,
    pub archive_id: String,
}

#[tauri::command]
pub async fn agent_load_archive(
    args: LoadArchiveArgs,
    _runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<rb_ai::memory::Archive, String> {
    let path = rb_ai::memory::MemoryStore::project_root(&PathBuf::from(&args.project_root))
        .join("L4_archives")
        .join(format!("{}.json", args.archive_id));
    let bytes = std::fs::read(&path).map_err(|e| format!("read archive: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse archive: {e}"))
}

#[derive(Debug, Serialize)]
pub struct SkillsList {
    pub global: Vec<SkillSummary>,
    pub project: Vec<SkillSummary>,
}

#[derive(Debug, Serialize)]
pub struct SkillSummary {
    pub name: String,
    pub path: String,
    pub triggers: Vec<String>,
    pub hits: u64,
}

#[derive(Debug, Deserialize)]
pub struct ListSkillsArgs {
    pub project_root: String,
}

#[tauri::command]
pub async fn agent_list_skills(
    args: ListSkillsArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<SkillsList, String> {
    let global_idx = runtime
        .memory
        .read_index(&runtime.memory.global_root.join("L3_skills/_index.json"))
        .map_err(|e| e.to_string())?;
    let project_idx = runtime
        .memory
        .read_index(
            &rb_ai::memory::MemoryStore::project_root(&PathBuf::from(&args.project_root))
                .join("L3_local/_index.json"),
        )
        .map_err(|e| e.to_string())?;
    fn to_summary(entries: Vec<rb_ai::memory::IndexEntry>) -> Vec<SkillSummary> {
        entries
            .into_iter()
            .filter_map(|e| match e {
                rb_ai::memory::IndexEntry::Skill {
                    name,
                    path,
                    triggers,
                    hits,
                    ..
                } => Some(SkillSummary {
                    name,
                    path,
                    triggers,
                    hits,
                }),
                _ => None,
            })
            .collect()
    }
    Ok(SkillsList {
        global: to_summary(global_idx),
        project: to_summary(project_idx),
    })
}

#[derive(Debug, Deserialize)]
pub struct EditMemoryArgs {
    pub path: String,
    pub content: String,
}

#[tauri::command]
pub async fn agent_edit_memory(args: EditMemoryArgs) -> Result<(), String> {
    // Trust the frontend to pass a path returned by agent_list_skills or
    // resolved via well-known L0/L2 file names. Reject paths that escape
    // either the global memory root or any project's agent dir.
    let path = std::path::Path::new(&args.path);
    let canon = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let global = dirs::data_local_dir()
        .ok_or("no data_local_dir")?
        .join("rust_brain/agent");
    let allowed = canon.starts_with(&global) || canon.to_string_lossy().contains("/agent/");
    if !allowed {
        return Err(format!(
            "refused to write outside agent dirs: {}",
            canon.display()
        ));
    }
    std::fs::write(&canon, args.content).map_err(|e| e.to_string())
}

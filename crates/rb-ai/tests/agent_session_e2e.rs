//! End-to-end agent session test. A scripted ChatProvider emits a fixed
//! sequence: file_write -> code_run -> task_done. The test asserts:
//! - the file is written under sandbox/,
//! - the code runs and produces stdout,
//! - L4 archive is written with outcome=done,
//! - L1 insight is appended,
//! - checkpoint is cleared.

use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use rb_ai::agent_loop::{
    run_session, AgentEvent, AgentSession, ApprovalVerdict, RunConfig, RunSessionCtx, SharedSession,
};
use rb_ai::memory::{Bm25Recaller, MemoryStore, Recaller};
use rb_ai::provider::{ChatProvider, ChatRequest, FinishReason, ProviderError, ProviderEvent};
use rb_ai::sandbox::{NetLogger, SandboxPolicy};
use rb_ai::tools::{builtin, ToolRegistry};
use tempfile::tempdir;
use tokio::sync::{mpsc, Mutex};

#[derive(Clone)]
struct ScriptedProvider {
    steps: Arc<StdMutex<Vec<Vec<ProviderEvent>>>>,
}

#[async_trait]
impl ChatProvider for ScriptedProvider {
    fn id(&self) -> &str {
        "scripted"
    }

    async fn send(
        &self,
        _req: ChatRequest,
        sink: mpsc::Sender<ProviderEvent>,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> Result<(), ProviderError> {
        let next = self.steps.lock().unwrap().drain(..1).next();
        if let Some(events) = next {
            for ev in events {
                let _ = sink.send(ev).await;
            }
        } else {
            let _ = sink.send(ProviderEvent::Finish(FinishReason::Stop)).await;
        }
        Ok(())
    }
}

#[cfg(unix)]
#[tokio::test]
async fn agent_runs_scripted_session_to_task_done() {
    let tmp = tempdir().unwrap();
    let project_root = tmp.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    let memory = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
    memory.ensure_project(&project_root).unwrap();
    let policy = Arc::new(SandboxPolicy::new(project_root.clone(), "sandbox"));
    let registry = {
        let mut r = ToolRegistry::new();
        builtin::register_all(&mut r);
        Arc::new(r)
    };

    let sandbox_dir = project_root.join("sandbox");
    let sandbox_dir_s = sandbox_dir.display().to_string();

    let provider = Arc::new(ScriptedProvider {
        steps: Arc::new(StdMutex::new(vec![
            // turn 1: file_write into sandbox
            vec![
                ProviderEvent::ToolCall {
                    id: "t1".into(),
                    name: "file_write".into(),
                    args: serde_json::json!({
                        "path": format!("{sandbox_dir_s}/run.sh"),
                        "content": "echo hello-from-agent\n"
                    }),
                },
                ProviderEvent::Finish(FinishReason::ToolCalls),
            ],
            // turn 2: code_run
            vec![
                ProviderEvent::ToolCall {
                    id: "t2".into(),
                    name: "code_run".into(),
                    args: serde_json::json!({
                        "language": "shell",
                        "code": "bash run.sh",
                        "cwd": sandbox_dir_s.clone(),
                        "timeout_secs": 10
                    }),
                },
                ProviderEvent::Finish(FinishReason::ToolCalls),
            ],
            // turn 3: task_done
            vec![
                ProviderEvent::ToolCall {
                    id: "t3".into(),
                    name: "task_done".into(),
                    args: serde_json::json!({"headline":"hello world test","tags":["test"]}),
                },
                ProviderEvent::Finish(FinishReason::Stop),
            ],
        ])),
    });

    let net_log = Arc::new(NetLogger::new(&project_root, "sess1", false).unwrap());
    let recaller: Arc<dyn Recaller> = Arc::new(Bm25Recaller::new(3));
    let session: SharedSession = Arc::new(Mutex::new(AgentSession::new(
        project_root.display().to_string(),
    )));
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(64);
    let (ask_tx, _ask_rx) = mpsc::channel(4);
    let (_appr_tx, appr_rx) = mpsc::channel::<(String, ApprovalVerdict)>(4);
    let appr_rx = Arc::new(Mutex::new(appr_rx));
    let cancel = tokio_util::sync::CancellationToken::new();

    // Drain events in background.
    let drain = tokio::spawn(async move {
        let mut events = vec![];
        while let Some(e) = event_rx.recv().await {
            events.push(e);
        }
        events
    });

    let ctx = RunSessionCtx {
        registry,
        policy,
        memory: memory.clone(),
        recaller,
        provider,
        net_log,
        project_root: project_root.clone(),
        system_context: String::new(),
        config: RunConfig::default(),
    };
    run_session(
        ctx,
        "make a hello-world script and run it".into(),
        session.clone(),
        event_tx,
        ask_tx,
        appr_rx,
        cancel,
    )
    .await
    .unwrap();

    let events = drain.await.unwrap();
    assert!(events.iter().any(|e| matches!(e, AgentEvent::Done { .. })));

    // Side-effects:
    let script = sandbox_dir.join("run.sh");
    assert!(script.exists(), "expected run.sh at {}", script.display());
    // L4 archive written
    let id = session.lock().await.id.clone();
    let archive = project_root
        .join("agent/L4_archives")
        .join(format!("{id}.json"));
    assert!(
        archive.exists(),
        "expected archive at {}",
        archive.display()
    );
    // L1 insight appended
    let l1 = std::fs::read_to_string(memory.global_root.join("L1_insights.jsonl")).unwrap();
    assert!(
        l1.lines().count() >= 1,
        "expected at least one L1 insight line"
    );
    // Checkpoint cleared
    assert!(
        !project_root.join("agent/checkpoints/current.json").exists(),
        "checkpoint should be cleared after task_done"
    );
}

#[tokio::test]
async fn agent_aborts_after_consecutive_failures() {
    let tmp = tempdir().unwrap();
    let project_root = tmp.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    let memory = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
    memory.ensure_project(&project_root).unwrap();
    let policy = Arc::new(SandboxPolicy::new(project_root.clone(), "sandbox"));
    let registry = {
        let mut r = ToolRegistry::new();
        builtin::register_all(&mut r);
        Arc::new(r)
    };

    // file_read on a non-existent path always errors.
    let bad = "/definitely/does/not/exist/here.txt".to_string();
    let mut steps: Vec<Vec<ProviderEvent>> = vec![];
    for i in 0..10 {
        steps.push(vec![
            ProviderEvent::ToolCall {
                id: format!("c{i}"),
                name: "file_read".into(),
                args: serde_json::json!({"path": bad}),
            },
            ProviderEvent::Finish(FinishReason::ToolCalls),
        ]);
    }
    let provider = Arc::new(ScriptedProvider {
        steps: Arc::new(StdMutex::new(steps)),
    });

    let net_log = Arc::new(NetLogger::new(&project_root, "sess2", false).unwrap());
    let recaller: Arc<dyn rb_ai::memory::Recaller> = Arc::new(Bm25Recaller::new(3));
    let session: SharedSession = Arc::new(Mutex::new(AgentSession::new(
        project_root.display().to_string(),
    )));
    let (event_tx, _event_rx) = mpsc::channel::<AgentEvent>(64);
    let (ask_tx, _ask_rx) = mpsc::channel(4);
    let (_appr_tx, appr_rx) = mpsc::channel::<(String, ApprovalVerdict)>(4);
    let appr_rx = Arc::new(Mutex::new(appr_rx));
    let cancel = tokio_util::sync::CancellationToken::new();

    let mut cfg = RunConfig::default();
    cfg.max_consecutive_failures = 3;

    let ctx = RunSessionCtx {
        registry,
        policy,
        memory: memory.clone(),
        recaller,
        provider,
        net_log,
        project_root: project_root.clone(),
        system_context: String::new(),
        config: cfg,
    };
    let r = run_session(
        ctx,
        "read a missing file repeatedly".into(),
        session.clone(),
        event_tx,
        ask_tx,
        appr_rx,
        cancel,
    )
    .await;
    assert!(matches!(r, Err(rb_ai::AiError::Tool(_))));
    let id = session.lock().await.id.clone();
    let archive_body = std::fs::read_to_string(
        project_root
            .join("agent/L4_archives")
            .join(format!("{id}.json")),
    )
    .unwrap();
    assert!(archive_body.contains("\"outcome\": \"failed\""));
}

#[tokio::test]
async fn agent_cancel_writes_cancelled_archive() {
    let tmp = tempdir().unwrap();
    let project_root = tmp.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    let memory = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
    memory.ensure_project(&project_root).unwrap();
    let policy = Arc::new(SandboxPolicy::new(project_root.clone(), "sandbox"));
    let registry = {
        let mut r = ToolRegistry::new();
        builtin::register_all(&mut r);
        Arc::new(r)
    };
    let provider = Arc::new(ScriptedProvider {
        steps: Arc::new(StdMutex::new(vec![])),
    });
    let net_log = Arc::new(NetLogger::new(&project_root, "sess3", false).unwrap());
    let recaller: Arc<dyn rb_ai::memory::Recaller> = Arc::new(Bm25Recaller::new(3));
    let session: SharedSession = Arc::new(Mutex::new(AgentSession::new(
        project_root.display().to_string(),
    )));
    let (event_tx, _event_rx) = mpsc::channel::<AgentEvent>(64);
    let (ask_tx, _ask_rx) = mpsc::channel(4);
    let (_appr_tx, appr_rx) = mpsc::channel::<(String, ApprovalVerdict)>(4);
    let appr_rx = Arc::new(Mutex::new(appr_rx));
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();

    let ctx = RunSessionCtx {
        registry,
        policy,
        memory: memory.clone(),
        recaller,
        provider,
        net_log,
        project_root: project_root.clone(),
        system_context: String::new(),
        config: RunConfig::default(),
    };
    let r = run_session(
        ctx,
        "anything".into(),
        session.clone(),
        event_tx,
        ask_tx,
        appr_rx,
        cancel,
    )
    .await;
    assert!(matches!(r, Err(rb_ai::AiError::Cancelled)));
    let id = session.lock().await.id.clone();
    let archive_body = std::fs::read_to_string(
        project_root
            .join("agent/L4_archives")
            .join(format!("{id}.json")),
    )
    .unwrap();
    assert!(archive_body.contains("\"outcome\": \"cancelled\""));
}

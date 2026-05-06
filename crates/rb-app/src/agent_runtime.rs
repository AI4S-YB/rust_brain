//! Per-project AgentRuntime: holds the global MemoryStore + a map of
//! per-project AgentHandles. One AgentHandle = one in-flight or paused
//! agent session. The runtime is shared via Tauri State.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rb_ai::agent_loop::{ApprovalVerdict, SharedSession};
use rb_ai::memory::MemoryStore;
use rb_ai::sandbox::SandboxPolicy;
use rb_ai::AiError;
use rb_core::cancel::CancellationToken;
use tokio::sync::{mpsc, Mutex};

/// One active agent session. The map key is `project_root.display().to_string()`.
pub struct AgentHandle {
    pub session_id: String,
    pub session: SharedSession,
    pub policy: Arc<SandboxPolicy>,
    pub cancel: CancellationToken,
    /// Sender into the approval channel that execute_call awaits.
    pub approval_tx: mpsc::Sender<(String, ApprovalVerdict)>,
    /// Sender into the ask_user channel that AskUserExec writes into. The
    /// receiver is owned by a forwarder task that emits "agent-ask-user".
    pub ask_user_tx: mpsc::Sender<rb_ai::tools::AskUserRequest>,
    /// `Some` while a run_session task is active; `None` when the session
    /// is paused awaiting the next user turn.
    pub run_join: Mutex<Option<tokio::task::JoinHandle<Result<(), AiError>>>>,
}

pub struct AgentRuntime {
    pub memory: Arc<MemoryStore>,
    /// Per-project handles. Project root → handle.
    pub active: Mutex<HashMap<String, Arc<AgentHandle>>>,
}

impl AgentRuntime {
    pub fn new() -> Result<Self, AiError> {
        Ok(Self {
            memory: Arc::new(MemoryStore::open_default()?),
            active: Mutex::new(HashMap::new()),
        })
    }

    /// Test-only constructor that lets us point the global root at a tempdir.
    #[doc(hidden)]
    pub fn with_memory_root(root: PathBuf) -> Result<Self, AiError> {
        Ok(Self {
            memory: Arc::new(MemoryStore::open(root)?),
            active: Mutex::new(HashMap::new()),
        })
    }

    pub async fn handle_for(&self, project_root: &str) -> Option<Arc<AgentHandle>> {
        self.active.lock().await.get(project_root).cloned()
    }

    pub async fn insert(&self, project_root: String, handle: Arc<AgentHandle>) {
        self.active.lock().await.insert(project_root, handle);
    }

    pub async fn remove(&self, project_root: &str) -> Option<Arc<AgentHandle>> {
        self.active.lock().await.remove(project_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rb_ai::agent_loop::AgentSession;
    use tempfile::tempdir;

    #[tokio::test]
    async fn runtime_insert_and_lookup() {
        let tmp = tempdir().unwrap();
        let rt = AgentRuntime::with_memory_root(tmp.path().join("global")).unwrap();
        assert!(rt.handle_for("p").await.is_none());

        let session = Arc::new(tokio::sync::Mutex::new(AgentSession::new("p".into())));
        let policy = Arc::new(SandboxPolicy::new(tmp.path().to_path_buf(), "sandbox"));
        let cancel = CancellationToken::new();
        let (appr_tx, _) = mpsc::channel(1);
        let (ask_tx, _) = mpsc::channel(1);
        let h = Arc::new(AgentHandle {
            session_id: "s1".into(),
            session,
            policy,
            cancel,
            approval_tx: appr_tx,
            ask_user_tx: ask_tx,
            run_join: Mutex::new(None),
        });
        rt.insert("p".into(), h.clone()).await;
        let got = rt.handle_for("p").await.unwrap();
        assert_eq!(got.session_id, "s1");
        rt.remove("p").await;
        assert!(rt.handle_for("p").await.is_none());
    }
}

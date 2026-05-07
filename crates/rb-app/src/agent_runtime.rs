//! Per-project AgentRuntime: holds the global MemoryStore + a map of
//! per-project AgentHandles. One AgentHandle = one in-flight or paused
//! agent session. The runtime is shared via Tauri State.

use std::collections::HashMap;
#[cfg(test)]
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
    /// Replaced on every agent_send so the running task owns the fresh receiver.
    pub approval_tx_slot: Mutex<mpsc::Sender<(String, ApprovalVerdict)>>,
    pub ask_user_tx_slot: Mutex<mpsc::Sender<rb_ai::tools::AskUserRequest>>,
    /// Map of call_id -> responder for in-flight ask_user questions.
    /// Set by agent_send before spawning run_session; None when idle.
    pub pending_asks_slot:
        Mutex<Option<Arc<Mutex<HashMap<String, mpsc::Sender<String>>>>>>,
    /// `Some` while a run_session task is active.
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
    #[cfg(test)]
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

    /// Reserved for explicit session cleanup; currently sessions persist for
    /// the app lifetime, so this is unused outside tests.
    #[allow(dead_code)]
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
            approval_tx_slot: Mutex::new(appr_tx),
            ask_user_tx_slot: Mutex::new(ask_tx),
            pending_asks_slot: Mutex::new(None),
            run_join: Mutex::new(None),
        });
        rt.insert("p".into(), h.clone()).await;
        let got = rt.handle_for("p").await.unwrap();
        assert_eq!(got.session_id, "s1");
        rt.remove("p").await;
        assert!(rt.handle_for("p").await.is_none());
    }
}

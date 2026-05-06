//! Stage 4 of the loop: persist working checkpoint after each step and run
//! crystallize_session at end-of-task.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::error::AiError;
use crate::memory::layers::{ArchiveOutcome, WorkingCheckpoint};
use crate::memory::{crystallize::crystallize_session, MemoryStore, SessionSummaryInput};

use super::types::AgentSession;

pub async fn fsync_checkpoint(
    store: &MemoryStore,
    project_root: &Path,
    session: &AgentSession,
    perceive_snapshot: &str,
) -> Result<(), AiError> {
    let cp = WorkingCheckpoint {
        session_id: session.id.clone(),
        project_root: project_root.display().to_string(),
        started_at: session.started_at,
        last_step_at: Utc::now(),
        todo: session.todo.clone(),
        message_count: session.messages.len(),
        perceive_snapshot_hash: hash(perceive_snapshot),
    };
    store.write_checkpoint(project_root, &cp).await
}

pub async fn finalize(
    store: &Arc<MemoryStore>,
    project_root: &Path,
    session: &AgentSession,
    headline: String,
    tags: Vec<String>,
    outcome: ArchiveOutcome,
    net_log_path: Option<String>,
) -> Result<(), AiError> {
    crystallize_session(
        store,
        project_root,
        SessionSummaryInput {
            session_id: session.id.clone(),
            started_at: session.started_at,
            ended_at: Some(Utc::now()),
            outcome,
            messages: session.messages.clone(),
            headline,
            tags,
            net_log_path,
        },
    )
    .await?;
    store.clear_checkpoint(project_root)?;
    Ok(())
}

pub fn hash(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex(h.finalize().as_slice())
}

fn hex(b: &[u8]) -> String {
    static HEX: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for byte in b {
        s.push(HEX[(byte >> 4) as usize] as char);
        s.push(HEX[(byte & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn finalize_writes_archive_and_clears_checkpoint() {
        let tmp = tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
        let project = tmp.path().join("proj");
        store.ensure_project(&project).unwrap();
        let session = AgentSession::new(project.display().to_string());
        // Pre-populate a checkpoint to verify it's cleared.
        fsync_checkpoint(&store, &project, &session, "snap").await.unwrap();
        assert!(project.join("agent/checkpoints/current.json").exists());
        finalize(
            &store,
            &project,
            &session,
            "did the thing".into(),
            vec!["test".into()],
            ArchiveOutcome::Done,
            None,
        )
        .await
        .unwrap();
        assert!(!project.join("agent/checkpoints/current.json").exists());
        let archive = project
            .join("agent/L4_archives")
            .join(format!("{}.json", session.id));
        assert!(archive.exists());
    }
}

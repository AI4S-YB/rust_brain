use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::fs;
use uuid::Uuid;

use crate::error::AiError;

use super::{ChatSession, SessionMeta};

const CHATS_DIR: &str = "chats";
const INDEX_FILE: &str = "index.json";

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct SessionIndex {
    pub schema_version: u32,
    pub sessions: Vec<SessionMeta>,
}

#[derive(Clone)]
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    pub fn new(project_root: &Path) -> Self {
        Self {
            root: project_root.join(CHATS_DIR),
        }
    }

    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub async fn ensure_dir(&self) -> Result<(), AiError> {
        fs::create_dir_all(&self.root).await?;
        Ok(())
    }

    pub fn generate_session_id() -> String {
        format!("ses_{}", Uuid::new_v4().simple())
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }

    fn index_path(&self) -> PathBuf {
        self.root.join(INDEX_FILE)
    }

    pub async fn list(&self) -> Result<SessionIndex, AiError> {
        let p = self.index_path();
        if !p.exists() {
            return Ok(SessionIndex {
                schema_version: 1,
                sessions: vec![],
            });
        }
        let text = fs::read_to_string(&p).await?;
        Ok(serde_json::from_str(&text)?)
    }

    pub async fn save_session(&self, session: &ChatSession) -> Result<(), AiError> {
        self.ensure_dir().await?;
        atomic_write_json(&self.session_path(&session.id), session).await?;

        let mut index = self.list().await.unwrap_or_else(|_| SessionIndex {
            schema_version: 1,
            sessions: vec![],
        });
        let meta = session.meta();
        if let Some(existing) = index.sessions.iter_mut().find(|s| s.id == meta.id) {
            *existing = meta;
        } else {
            index.sessions.push(meta);
        }
        index
            .sessions
            .sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        atomic_write_json(&self.index_path(), &index).await?;
        Ok(())
    }

    pub async fn load_session(&self, id: &str) -> Result<ChatSession, AiError> {
        let text = fs::read_to_string(&self.session_path(id))
            .await
            .map_err(|_| AiError::InvalidState("legacy session".into()))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub async fn delete_session(&self, id: &str) -> Result<(), AiError> {
        let p = self.session_path(id);
        if p.exists() {
            fs::remove_file(&p).await?;
        }
        let mut index = self.list().await.unwrap_or_default();
        index.sessions.retain(|s| s.id != id);
        atomic_write_json(&self.index_path(), &index).await?;
        Ok(())
    }

    pub async fn rename_session(&self, id: &str, new_title: String) -> Result<(), AiError> {
        let mut s = self.load_session(id).await?;
        s.title = new_title;
        s.updated_at = chrono::Utc::now();
        self.save_session(&s).await
    }
}

async fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AiError> {
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(value)?;
    fs::write(&tmp, text).await?;
    fs::rename(&tmp, path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Message;
    use tempfile::tempdir;

    #[tokio::test]
    async fn roundtrip_save_list_load_delete() {
        let tmp = tempdir().unwrap();
        let store = SessionStore::new(tmp.path());
        let mut s = ChatSession::new("ses_1".into(), "t".into(), None);
        s.messages.push(Message::User {
            content: "hi".into(),
        });
        store.save_session(&s).await.unwrap();

        let idx = store.list().await.unwrap();
        assert_eq!(idx.sessions.len(), 1);
        assert_eq!(idx.sessions[0].message_count, 1);

        let loaded = store.load_session("ses_1").await.unwrap();
        assert_eq!(loaded.messages.len(), 1);

        store.delete_session("ses_1").await.unwrap();
        assert_eq!(store.list().await.unwrap().sessions.len(), 0);
    }

    #[tokio::test]
    async fn save_is_atomic_no_partial_file() {
        let tmp = tempdir().unwrap();
        let store = SessionStore::new(tmp.path());
        let mut s = ChatSession::new("ses_a".into(), "t".into(), None);
        s.messages.push(Message::User {
            content: "x".into(),
        });
        store.save_session(&s).await.unwrap();
        let entries: Vec<_> = std::fs::read_dir(tmp.path().join("chats"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(
            !entries.iter().any(|n| n.ends_with(".tmp")),
            "tmp file leaked: {entries:?}"
        );
    }

    #[tokio::test]
    async fn rename_updates_title_and_index() {
        let tmp = tempdir().unwrap();
        let store = SessionStore::new(tmp.path());
        let s = ChatSession::new("ses_r".into(), "original".into(), None);
        store.save_session(&s).await.unwrap();
        store
            .rename_session("ses_r", "renamed".into())
            .await
            .unwrap();
        let idx = store.list().await.unwrap();
        assert_eq!(idx.sessions[0].title, "renamed");
    }

    #[tokio::test]
    async fn load_missing_session_returns_session_not_found() {
        let tmp = tempdir().unwrap();
        let store = SessionStore::new(tmp.path());
        let err = store.load_session("ses_nope").await.unwrap_err();
        assert!(matches!(err, AiError::InvalidState(_)));
    }
}

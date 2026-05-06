//! Atomic, dual-root memory IO. `MemoryStore` knows where global vs project
//! memory lives and writes both atomically with file locks.
//!
//! All writes go through a temp-file + fsync + rename pattern; index files
//! use exclusive `fs2` locks to handle concurrent sessions.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fs2::FileExt;
use tokio::sync::Mutex;

use crate::error::AiError;
use crate::memory::layers::{Archive, IndexEntry, Insight, Scope};

const SHARD_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct MemoryStore {
    pub global_root: PathBuf,
    inner: Arc<Mutex<()>>, // serialize index writes within this process
}

impl MemoryStore {
    /// Resolve the global memory root (`~/.local/share/rust_brain/agent/`)
    /// and ensure its skeleton exists.
    pub fn open_default() -> Result<Self, AiError> {
        let base = dirs::data_local_dir()
            .ok_or_else(|| AiError::Config("no data_local_dir".into()))?
            .join("rust_brain")
            .join("agent");
        Self::open(base)
    }

    pub fn open(global_root: PathBuf) -> Result<Self, AiError> {
        std::fs::create_dir_all(global_root.join("L3_skills"))?;
        ensure_file(&global_root.join("L0_meta.md"), DEFAULT_L0)?;
        ensure_file(&global_root.join("L1_insights.jsonl"), "")?;
        ensure_file(&global_root.join("L2_facts.md"), "# Long-term facts\n\n")?;
        ensure_index(&global_root.join("L3_skills/_index.json"))?;
        Ok(Self {
            global_root,
            inner: Arc::new(Mutex::new(())),
        })
    }

    pub fn project_root(project_root: &Path) -> PathBuf {
        project_root.join("agent")
    }

    pub fn ensure_project(&self, project_root: &Path) -> Result<PathBuf, AiError> {
        let root = Self::project_root(project_root);
        std::fs::create_dir_all(root.join("L3_local"))?;
        std::fs::create_dir_all(root.join("L4_archives"))?;
        std::fs::create_dir_all(root.join("checkpoints"))?;
        ensure_index(&root.join("L3_local/_index.json"))?;
        ensure_index(&root.join("L4_archives/_index.json"))?;
        Ok(root)
    }

    pub async fn append_l1_insight(&self, insight: &Insight) -> Result<(), AiError> {
        let path = self.global_root.join("L1_insights.jsonl");
        let line = serde_json::to_string(insight)? + "\n";
        let _g = self.inner.lock().await;
        append_with_lock(&path, line.as_bytes())
    }

    /// Append an archive. Splits into `<id>.part2.json`, etc., when the
    /// in-progress file would exceed `SHARD_BYTES`.
    pub async fn append_l4_archive(
        &self,
        project_root: &Path,
        archive: &Archive,
    ) -> Result<PathBuf, AiError> {
        let root = Self::project_root(project_root).join("L4_archives");
        std::fs::create_dir_all(&root)?;
        let _g = self.inner.lock().await;
        let path = next_shard_path(&root, &archive.id, SHARD_BYTES)?;
        let bytes = serde_json::to_vec_pretty(archive)?;
        write_atomic(&path, &bytes)?;
        update_index(&root.join("_index.json"), |entries| {
            // Replace any existing entry with same id (last shard wins).
            entries.retain(|e| match e {
                IndexEntry::Archive { id, .. } => id != &archive.id,
                _ => true,
            });
            entries.push(IndexEntry::Archive {
                id: archive.id.clone(),
                started_at: archive.started_at,
                ended_at: archive.ended_at,
                summary: archive.summary.clone(),
                outcome: archive.outcome,
                tags: archive.tags.clone(),
            });
            Ok(())
        })?;
        Ok(path)
    }

    pub async fn upsert_skill_index(
        &self,
        scope: Scope,
        project_root: Option<&Path>,
        entry: IndexEntry,
    ) -> Result<(), AiError> {
        let dir = match (scope, project_root) {
            (Scope::Global, _) => self.global_root.join("L3_skills"),
            (Scope::Project, Some(p)) => Self::project_root(p).join("L3_local"),
            (Scope::Project, None) => {
                return Err(AiError::InvalidState(
                    "project scope requires project_root".into(),
                ))
            }
        };
        std::fs::create_dir_all(&dir)?;
        let _g = self.inner.lock().await;
        update_index(&dir.join("_index.json"), |entries| {
            // Replace by name.
            let name_of = |e: &IndexEntry| match e {
                IndexEntry::Skill { name, .. } => Some(name.clone()),
                _ => None,
            };
            let new_name = name_of(&entry);
            entries.retain(|e| name_of(e) != new_name);
            entries.push(entry);
            Ok(())
        })
    }

    pub fn read_l0(&self) -> Result<String, AiError> {
        Ok(std::fs::read_to_string(self.global_root.join("L0_meta.md"))?)
    }

    pub fn read_l2(&self) -> Result<String, AiError> {
        Ok(std::fs::read_to_string(self.global_root.join("L2_facts.md"))?)
    }

    pub fn read_index(&self, path: &Path) -> Result<Vec<IndexEntry>, AiError> {
        if !path.exists() {
            return Ok(vec![]);
        }
        let bytes = std::fs::read(path)?;
        if bytes.is_empty() {
            return Ok(vec![]);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub async fn write_checkpoint(
        &self,
        project_root: &Path,
        cp: &crate::memory::layers::WorkingCheckpoint,
    ) -> Result<(), AiError> {
        let path = Self::project_root(project_root)
            .join("checkpoints")
            .join("current.json");
        let bytes = serde_json::to_vec_pretty(cp)?;
        let _g = self.inner.lock().await;
        write_atomic(&path, &bytes)
    }

    pub fn read_checkpoint(
        &self,
        project_root: &Path,
    ) -> Result<Option<crate::memory::layers::WorkingCheckpoint>, AiError> {
        let path = Self::project_root(project_root)
            .join("checkpoints")
            .join("current.json");
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path)?;
        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    pub fn clear_checkpoint(&self, project_root: &Path) -> Result<(), AiError> {
        let path = Self::project_root(project_root)
            .join("checkpoints")
            .join("current.json");
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

fn ensure_file(path: &Path, default: &str) -> Result<(), AiError> {
    if !path.exists() {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(path, default)?;
    }
    Ok(())
}

fn ensure_index(path: &Path) -> Result<(), AiError> {
    if !path.exists() {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(path, b"[]")?;
    }
    Ok(())
}

fn append_with_lock(path: &Path, data: &[u8]) -> Result<(), AiError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    let f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.lock_exclusive()
        .map_err(|e| AiError::MemoryWrite(format!("lock {}: {e}", path.display())))?;
    let res = (&f).write_all(data).and_then(|_| f.sync_data());
    f.unlock().ok();
    res.map_err(|e| AiError::MemoryWrite(format!("append {}: {e}", path.display())))
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), AiError> {
    let parent = path
        .parent()
        .ok_or_else(|| AiError::MemoryWrite(format!("no parent: {}", path.display())))?;
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name().unwrap().to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&tmp, data)?;
    let f = std::fs::OpenOptions::new().write(true).open(&tmp)?;
    f.sync_data()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn update_index<F>(path: &Path, mutate: F) -> Result<(), AiError>
where
    F: FnOnce(&mut Vec<IndexEntry>) -> Result<(), AiError>,
{
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let lock_path = path.with_extension("lock");
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&lock_path)?;
    lock_file
        .lock_exclusive()
        .map_err(|e| AiError::MemoryWrite(format!("lock {}: {e}", path.display())))?;
    let res: Result<(), AiError> = (|| {
        let mut entries: Vec<IndexEntry> = if path.exists() {
            let bytes = std::fs::read(path)?;
            if bytes.is_empty() {
                vec![]
            } else {
                serde_json::from_slice(&bytes)?
            }
        } else {
            vec![]
        };
        mutate(&mut entries)?;
        let bytes = serde_json::to_vec_pretty(&entries)?;
        write_atomic(path, &bytes)
    })();
    lock_file.unlock().ok();
    res
}

fn next_shard_path(dir: &Path, id: &str, shard_bytes: u64) -> Result<PathBuf, AiError> {
    let main = dir.join(format!("{id}.json"));
    if !main.exists() {
        return Ok(main);
    }
    // If main is small, overwrite it; if it crosses threshold, find next part.
    let len = std::fs::metadata(&main)?.len();
    if len < shard_bytes {
        return Ok(main);
    }
    let mut n = 2;
    loop {
        let p = dir.join(format!("{id}.part{n}.json"));
        let exists = p.exists();
        let len = if exists { std::fs::metadata(&p)?.len() } else { 0 };
        if !exists || len < shard_bytes {
            return Ok(p);
        }
        n += 1;
        if n > 100 {
            return Err(AiError::MemoryWrite("too many shards".into()));
        }
    }
}

const DEFAULT_L0: &str = r#"# Agent meta-rules

- 第一性原则：在 `<project>/sandbox/` 内自由实验，写到项目结果区前先 ask_user 或调用对应 module。
- 任务分解：长任务先拆 todo，落到 working checkpoint。
- 失败处理：同一工具连续失败 ≥3 次时停下 ask_user，不要无脑重试。
- 记忆归类：项目特定细节写 project scope；可复用经验写 global scope。
- 透明度：每个工具调用前简短说明意图；调用后总结结果，不沉默。
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;

    fn store(tmp: &Path) -> MemoryStore {
        MemoryStore::open(tmp.join("global")).unwrap()
    }

    #[test]
    fn open_creates_skeleton_files() {
        let tmp = tempdir().unwrap();
        let s = store(tmp.path());
        assert!(s.global_root.join("L0_meta.md").exists());
        assert!(s.global_root.join("L1_insights.jsonl").exists());
        assert!(s.global_root.join("L2_facts.md").exists());
        assert!(s.global_root.join("L3_skills/_index.json").exists());
    }

    #[tokio::test]
    async fn append_l1_insight_writes_jsonl_lines() {
        let tmp = tempdir().unwrap();
        let s = store(tmp.path());
        for i in 0..3 {
            s.append_l1_insight(&Insight {
                id: format!("i{i}"),
                tag: "test".into(),
                summary: format!("s{i}"),
                evidence_archive_id: None,
                ts: Utc::now(),
            })
            .await
            .unwrap();
        }
        let body = std::fs::read_to_string(s.global_root.join("L1_insights.jsonl")).unwrap();
        assert_eq!(body.lines().count(), 3);
        for (i, line) in body.lines().enumerate() {
            let v: Insight = serde_json::from_str(line).unwrap();
            assert_eq!(v.id, format!("i{i}"));
        }
    }

    #[tokio::test]
    async fn archive_index_replaces_same_id() {
        let tmp = tempdir().unwrap();
        let s = store(tmp.path());
        let project = tmp.path().join("proj");
        s.ensure_project(&project).unwrap();
        let mut a = Archive {
            id: "a1".into(),
            started_at: Utc::now(),
            ended_at: None,
            summary: "draft".into(),
            outcome: super::super::layers::ArchiveOutcome::Done,
            tags: vec![],
            messages: vec![],
            net_log_path: None,
        };
        s.append_l4_archive(&project, &a).await.unwrap();
        a.summary = "final".into();
        s.append_l4_archive(&project, &a).await.unwrap();
        let idx = s
            .read_index(&MemoryStore::project_root(&project).join("L4_archives/_index.json"))
            .unwrap();
        assert_eq!(idx.len(), 1);
        match &idx[0] {
            IndexEntry::Archive { summary, .. } => assert_eq!(summary, "final"),
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn shards_appear_after_size_threshold() {
        let tmp = tempdir().unwrap();
        let s = store(tmp.path());
        let project = tmp.path().join("proj");
        s.ensure_project(&project).unwrap();
        // First archive — small.
        let a1 = Archive {
            id: "big".into(),
            started_at: Utc::now(),
            ended_at: None,
            summary: "x".into(),
            outcome: super::super::layers::ArchiveOutcome::Done,
            tags: vec![],
            messages: vec![],
            net_log_path: None,
        };
        s.append_l4_archive(&project, &a1).await.unwrap();
        // Manually inflate the existing archive file past 5MB.
        let main = MemoryStore::project_root(&project)
            .join("L4_archives")
            .join("big.json");
        let pad = vec![b' '; (SHARD_BYTES + 1) as usize];
        std::fs::write(&main, pad).unwrap();
        let part = s.append_l4_archive(&project, &a1).await.unwrap();
        assert!(part.file_name().unwrap().to_string_lossy().contains("part2"));
    }

    #[tokio::test]
    async fn checkpoint_round_trips_atomically() {
        use crate::memory::layers::{TodoEntry, WorkingCheckpoint};
        let tmp = tempdir().unwrap();
        let s = store(tmp.path());
        let project = tmp.path().join("proj");
        s.ensure_project(&project).unwrap();

        let cp = WorkingCheckpoint {
            session_id: "sess1".into(),
            project_root: project.display().to_string(),
            started_at: Utc::now(),
            last_step_at: Utc::now(),
            todo: vec![TodoEntry {
                text: "qc".into(),
                done: false,
            }],
            message_count: 3,
            perceive_snapshot_hash: "abc".into(),
        };
        s.write_checkpoint(&project, &cp).await.unwrap();
        let loaded = s.read_checkpoint(&project).unwrap().unwrap();
        assert_eq!(loaded.session_id, "sess1");
        assert_eq!(loaded.todo.len(), 1);
    }

    #[tokio::test]
    async fn checkpoint_returns_none_when_missing() {
        let tmp = tempdir().unwrap();
        let s = store(tmp.path());
        let project = tmp.path().join("proj");
        s.ensure_project(&project).unwrap();
        assert!(s.read_checkpoint(&project).unwrap().is_none());
    }
}

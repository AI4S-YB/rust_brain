//! Append-only network call logger. The agent's `web_scan` tool calls
//! `record_request` before issuing the HTTP request and `record_response`
//! after; the log lives at `<project>/agent/L4_archives/<session>.net.log`.
//!
//! Disabled when the active `AgentConfig.network.log_enabled` is false
//! (default in FullPermission mode).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::AiError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetEntry {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub session_id: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub bytes: Option<u64>,
    pub note: Option<String>,
}

pub struct NetLogger {
    enabled: bool,
    path: PathBuf,
    inner: Mutex<()>,
}

impl NetLogger {
    pub fn new(project_root: &Path, session_id: &str, enabled: bool) -> Result<Self, AiError> {
        let dir = project_root.join("agent").join("L4_archives");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{session_id}.net.log"));
        Ok(Self {
            enabled,
            path,
            inner: Mutex::new(()),
        })
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn path(&self) -> Option<&Path> {
        if self.enabled {
            Some(&self.path)
        } else {
            None
        }
    }

    pub fn record(&self, entry: &NetEntry) -> Result<(), AiError> {
        if !self.enabled {
            return Ok(());
        }
        let line = serde_json::to_string(entry)? + "\n";
        let _g = self.inner.lock().unwrap();
        // `.read(true)` is required for Windows: `LockFileEx` rejects file
        // handles opened with append-only access.
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&self.path)?;
        f.lock_exclusive()
            .map_err(|e| AiError::MemoryWrite(format!("net log lock: {e}")))?;
        let res = (&f)
            .write_all_at(line.as_bytes())
            .map_err(|e| AiError::MemoryWrite(format!("net log write: {e}")));
        f.unlock().ok();
        res
    }
}

trait WriteAll {
    fn write_all_at(self, buf: &[u8]) -> std::io::Result<()>;
}

impl WriteAll for &std::fs::File {
    fn write_all_at(self, buf: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        // Append-only file, ordinary write_all is fine.
        let mut f = self;
        f.write_all(buf)?;
        f.sync_data()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;

    #[test]
    fn record_appends_jsonl_line() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let log = NetLogger::new(&project, "sess1", true).unwrap();
        log.record(&NetEntry {
            ts: Utc::now(),
            session_id: "sess1".into(),
            method: "GET".into(),
            url: "https://example.org".into(),
            status: Some(200),
            bytes: Some(123),
            note: None,
        })
        .unwrap();
        let body = std::fs::read_to_string(log.path().unwrap()).unwrap();
        assert_eq!(body.lines().count(), 1);
        let v: NetEntry = serde_json::from_str(body.trim()).unwrap();
        assert_eq!(v.url, "https://example.org");
    }

    #[test]
    fn disabled_logger_is_silent() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let log = NetLogger::new(&project, "s", false).unwrap();
        assert!(log.path().is_none());
        log.record(&NetEntry {
            ts: Utc::now(),
            session_id: "s".into(),
            method: "GET".into(),
            url: "x".into(),
            status: None,
            bytes: None,
            note: None,
        })
        .unwrap();
        assert!(!project.join("agent/L4_archives/s.net.log").exists());
    }
}

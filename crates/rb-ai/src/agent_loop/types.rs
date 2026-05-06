//! Public types for the agent loop. Kept in their own file because both the
//! main loop and Tauri-facing rb-app need them, and we want a stable surface.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::memory::layers::TodoEntry;

/// One agent research session. Held in `Arc<Mutex<AgentSession>>` by the
/// run_session loop and accessed by tools via ToolContext.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub project_root: String,
    pub started_at: DateTime<Utc>,
    pub messages: Vec<serde_json::Value>, // raw provider message JSON
    pub todo: Vec<TodoEntry>,
    pub tool_failures: std::collections::HashMap<String, u32>,
}

impl AgentSession {
    pub fn new(project_root: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            project_root,
            started_at: Utc::now(),
            messages: vec![],
            todo: vec![],
            tool_failures: Default::default(),
        }
    }
}

/// Streaming events emitted up to rb-app (and onward to frontend in Plan 2).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    Text {
        session_id: String,
        delta: String,
    },
    Reasoning {
        session_id: String,
        delta: String,
    },
    ToolCall {
        session_id: String,
        call_id: String,
        name: String,
        bucket: String,
        decision: String,
        args: serde_json::Value,
    },
    ToolResult {
        session_id: String,
        call_id: String,
        result: serde_json::Value,
    },
    AskUser {
        session_id: String,
        call_id: String,
        prompt: String,
    },
    Memory {
        session_id: String,
        recalled: Vec<serde_json::Value>,
    },
    Checkpoint {
        session_id: String,
        todo: Vec<TodoEntry>,
    },
    Crystallize {
        session_id: String,
        layer: String,
        scope: String,
        path: String,
    },
    Done {
        session_id: String,
    },
    Error {
        session_id: String,
        message: String,
    },
}

pub type SharedSession = Arc<Mutex<AgentSession>>;

pub mod message;
pub mod store;

pub use message::{Message, ToolCall};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSnapshot {
    pub provider_id: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_snapshot: Option<ProviderSnapshot>,
    /// Reserved for Phase 3 summarization of long conversations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_snapshot: Option<ProviderSnapshot>,
}

impl ChatSession {
    pub fn new(id: String, title: String, provider_snapshot: Option<ProviderSnapshot>) -> Self {
        let now = Utc::now();
        Self {
            schema_version: 1,
            id,
            title,
            created_at: now,
            updated_at: now,
            provider_snapshot,
            summary: None,
            messages: vec![],
        }
    }

    pub fn meta(&self) -> SessionMeta {
        SessionMeta {
            id: self.id.clone(),
            title: self.title.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            message_count: self.messages.len(),
            provider_snapshot: self.provider_snapshot.clone(),
        }
    }
}

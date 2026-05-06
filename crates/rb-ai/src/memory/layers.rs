//! Layered memory types. Pure data + serialization, no IO.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// L1 entry: append-only insight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub id: String,
    pub tag: String,
    pub summary: String,
    pub evidence_archive_id: Option<String>,
    pub ts: DateTime<Utc>,
}

/// L3 skill metadata read from frontmatter; body kept separate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default = "SkillMeta::default_inputs_schema")]
    pub inputs_schema: serde_json::Value,
    #[serde(default = "SkillMeta::default_risk_tier")]
    pub risk_tier: String,
    #[serde(default)]
    pub crystallized_calls: Vec<serde_json::Value>,
}

impl SkillMeta {
    fn default_inputs_schema() -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    fn default_risk_tier() -> String {
        "run_mid".into()
    }
}

/// Index entry; written into `_index.json` for L3/L4 directories.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IndexEntry {
    Skill {
        name: String,
        path: String,
        scope: Scope,
        triggers: Vec<String>,
        hits: u64,
        last_used: Option<DateTime<Utc>>,
    },
    Archive {
        id: String,
        started_at: DateTime<Utc>,
        ended_at: Option<DateTime<Utc>>,
        summary: String,
        outcome: ArchiveOutcome,
        tags: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveOutcome {
    Done,
    Cancelled,
    Interrupted,
    Failed,
}

/// L4 archive: full agent session trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Archive {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub summary: String,
    pub outcome: ArchiveOutcome,
    pub tags: Vec<String>,
    pub messages: Vec<serde_json::Value>, // raw provider/tool messages
    pub net_log_path: Option<String>,
}

/// Single working checkpoint, written to `<project>/agent/checkpoints/current.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingCheckpoint {
    pub session_id: String,
    pub project_root: String,
    pub started_at: DateTime<Utc>,
    pub last_step_at: DateTime<Utc>,
    pub todo: Vec<TodoEntry>,
    pub message_count: usize,
    pub perceive_snapshot_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoEntry {
    pub text: String,
    pub done: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_meta_round_trips_with_defaults() {
        let yaml_like = serde_json::json!({
            "name": "human-rna-seq-de",
            "description": "Run a full RNA-seq DE pipeline",
        });
        let m: SkillMeta = serde_json::from_value(yaml_like).unwrap();
        assert_eq!(m.name, "human-rna-seq-de");
        assert_eq!(m.risk_tier, "run_mid");
        assert!(m.inputs_schema.get("type").is_some());
    }

    #[test]
    fn index_entry_serializes_with_kind_tag() {
        let entry = IndexEntry::Skill {
            name: "rna-seq".into(),
            path: "L3_skills/rna-seq.md".into(),
            scope: Scope::Global,
            triggers: vec!["rna-seq".into()],
            hits: 0,
            last_used: None,
        };
        let s = serde_json::to_string(&entry).unwrap();
        assert!(s.contains(r#""kind":"skill""#));
        assert!(s.contains(r#""scope":"global""#));
    }

    #[test]
    fn archive_outcome_is_snake_case() {
        let v = serde_json::to_value(ArchiveOutcome::Interrupted).unwrap();
        assert_eq!(v.as_str(), Some("interrupted"));
    }
}

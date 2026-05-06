//! Crystallize: fold a finished agent session into L1 insight + L4 archive,
//! and expose helpers for `start_long_term_update` (L2/L3 writeback).

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AiError;
use crate::memory::layers::{Archive, ArchiveOutcome, IndexEntry, Insight, Scope};
use crate::memory::store::MemoryStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummaryInput {
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub outcome: ArchiveOutcome,
    pub messages: Vec<serde_json::Value>,
    pub headline: String,
    pub tags: Vec<String>,
    pub net_log_path: Option<String>,
}

/// Append L4 archive + L1 insight summary in one shot.
pub async fn crystallize_session(
    store: &MemoryStore,
    project_root: &Path,
    input: SessionSummaryInput,
) -> Result<(), AiError> {
    let archive = Archive {
        id: input.session_id.clone(),
        started_at: input.started_at,
        ended_at: input.ended_at,
        summary: input.headline.clone(),
        outcome: input.outcome,
        tags: input.tags.clone(),
        messages: input.messages,
        net_log_path: input.net_log_path,
    };
    store.append_l4_archive(project_root, &archive).await?;

    let insight = Insight {
        id: Uuid::new_v4().simple().to_string(),
        tag: input
            .tags
            .first()
            .cloned()
            .unwrap_or_else(|| "session".into()),
        summary: input.headline,
        evidence_archive_id: Some(input.session_id),
        ts: Utc::now(),
    };
    store.append_l1_insight(&insight).await?;
    Ok(())
}

/// `start_long_term_update` writeback. Layer + scope are explicit per spec —
/// callers (the agent) declare them in tool args.
pub async fn long_term_update(
    store: &MemoryStore,
    project_root: Option<&Path>,
    layer: Layer,
    scope: Scope,
    body: LongTermBody,
) -> Result<UpdateResult, AiError> {
    match (layer, scope) {
        (Layer::L2, Scope::Global) => {
            let path = store.global_root.join("L2_facts.md");
            let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
            if !existing.ends_with('\n') {
                existing.push('\n');
            }
            existing.push_str(&format!(
                "\n## {}\n\n{}\n",
                body.section.unwrap_or_else(|| "untitled".into()),
                body.markdown
            ));
            super_write_text(&path, &existing)?;
            Ok(UpdateResult {
                path: path.display().to_string(),
            })
        }
        (Layer::L3, scope) => {
            let dir = match scope {
                Scope::Global => store.global_root.join("L3_skills"),
                Scope::Project => {
                    let p = project_root.ok_or_else(|| {
                        AiError::InvalidState("project scope requires project_root".into())
                    })?;
                    MemoryStore::project_root(p).join("L3_local")
                }
            };
            std::fs::create_dir_all(&dir)?;
            let slug = slugify(&body.name.clone().unwrap_or_else(|| "skill".into()));
            let path = dir.join(format!("{slug}.md"));
            super_write_text(&path, &body.markdown)?;
            let strip_base = match scope {
                Scope::Global => store.global_root.clone(),
                Scope::Project => MemoryStore::project_root(project_root.unwrap()),
            };
            store
                .upsert_skill_index(
                    scope,
                    project_root,
                    IndexEntry::Skill {
                        name: slug.clone(),
                        path: path
                            .strip_prefix(&strip_base)
                            .unwrap_or(&path)
                            .display()
                            .to_string(),
                        scope,
                        triggers: body.triggers.unwrap_or_default(),
                        hits: 0,
                        last_used: None,
                    },
                )
                .await?;
            Ok(UpdateResult {
                path: path.display().to_string(),
            })
        }
        (Layer::L2, Scope::Project) => Err(AiError::InvalidState(
            "L2 is global-only by convention".into(),
        )),
    }
}

fn super_write_text(path: &Path, text: &str) -> Result<(), AiError> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", Uuid::new_v4().simple()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .replace("--", "-")
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    L2,
    L3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermBody {
    /// For L2: section heading; for L3: ignored.
    pub section: Option<String>,
    /// For L3: skill name (used for filename + index entry).
    pub name: Option<String>,
    /// For L3: trigger keywords for retrieval.
    pub triggers: Option<Vec<String>>,
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResult {
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn crystallize_writes_archive_and_insight() {
        let tmp = tempdir().unwrap();
        let store = MemoryStore::open(tmp.path().join("global")).unwrap();
        let project = tmp.path().join("proj");
        store.ensure_project(&project).unwrap();
        crystallize_session(
            &store,
            &project,
            SessionSummaryInput {
                session_id: "s1".into(),
                started_at: Utc::now(),
                ended_at: Some(Utc::now()),
                outcome: ArchiveOutcome::Done,
                messages: vec![],
                headline: "did the thing".into(),
                tags: vec!["rna-seq".into()],
                net_log_path: None,
            },
        )
        .await
        .unwrap();

        // L4 written
        let archive = MemoryStore::project_root(&project).join("L4_archives/s1.json");
        assert!(archive.exists());
        // L1 written
        let l1 = std::fs::read_to_string(store.global_root.join("L1_insights.jsonl")).unwrap();
        assert_eq!(l1.lines().count(), 1);
        let v: Insight = serde_json::from_str(l1.trim()).unwrap();
        assert_eq!(v.tag, "rna-seq");
        assert_eq!(v.evidence_archive_id.as_deref(), Some("s1"));
    }

    #[tokio::test]
    async fn long_term_l3_global_writes_md_and_indexes() {
        let tmp = tempdir().unwrap();
        let store = MemoryStore::open(tmp.path().join("global")).unwrap();
        let r = long_term_update(
            &store,
            None,
            Layer::L3,
            Scope::Global,
            LongTermBody {
                section: None,
                name: Some("RNA-seq DE".into()),
                triggers: Some(vec!["rna-seq".into(), "de".into()]),
                markdown: "## SOP\n1. ...".into(),
            },
        )
        .await
        .unwrap();
        assert!(r.path.ends_with("rna-seq-de.md"));
        let body = std::fs::read_to_string(&r.path).unwrap();
        assert!(body.contains("SOP"));
        let idx = store
            .read_index(&store.global_root.join("L3_skills/_index.json"))
            .unwrap();
        assert_eq!(idx.len(), 1);
    }

    #[tokio::test]
    async fn long_term_l2_project_is_rejected() {
        let tmp = tempdir().unwrap();
        let store = MemoryStore::open(tmp.path().join("global")).unwrap();
        let project = tmp.path().join("proj");
        store.ensure_project(&project).unwrap();
        let err = long_term_update(
            &store,
            Some(&project),
            Layer::L2,
            Scope::Project,
            LongTermBody {
                section: Some("x".into()),
                name: None,
                triggers: None,
                markdown: "y".into(),
            },
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(err, AiError::InvalidState(_)));
    }
}

//! Build the per-turn system prompt: L0 + project snapshot + recalled memory.

use std::path::Path;
use std::sync::Arc;

use crate::error::AiError;
use crate::memory::recall::{collect_candidates, Recaller};
use crate::memory::MemoryStore;

pub struct PerceiveCtx {
    pub user_text: String,
    pub project_summary: String,
}

pub struct PerceiveOut {
    pub system_prompt: String,
    pub recalled: Vec<crate::memory::recall::RecallCandidate>,
}

pub async fn perceive(
    store: &MemoryStore,
    project_root: Option<&Path>,
    recaller: Arc<dyn Recaller>,
    ctx: &PerceiveCtx,
    budget_tokens: usize,
) -> Result<PerceiveOut, AiError> {
    let l0 = store.read_l0().unwrap_or_default();
    let l2 = store.read_l2().unwrap_or_default();
    let candidates = collect_candidates(store, project_root)?;
    let recall = recaller
        .recall(&ctx.user_text, candidates, budget_tokens)
        .await?;
    let mut sp = String::new();
    sp.push_str("# Meta rules\n\n");
    sp.push_str(&l0);
    sp.push_str("\n\n# Long-term facts\n\n");
    sp.push_str(&l2);
    sp.push_str("\n\n# Project snapshot\n\n");
    sp.push_str(&ctx.project_summary);
    if !recall.picked.is_empty() {
        sp.push_str("\n\n# Recalled memory (top matches)\n\n");
        for c in &recall.picked {
            sp.push_str(&format!("- [{}|{}] {}\n", c.scope, c.kind, c.text));
        }
    }
    Ok(PerceiveOut {
        system_prompt: sp,
        recalled: recall.picked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::recall::Bm25Recaller;
    use crate::memory::MemoryStore;
    use tempfile::tempdir;

    #[tokio::test]
    async fn perceive_includes_l0_project_and_recalled() {
        let tmp = tempdir().unwrap();
        let store = MemoryStore::open(tmp.path().join("global")).unwrap();
        let project = tmp.path().join("proj");
        store.ensure_project(&project).unwrap();
        // Seed a skill index entry so recall has something to match.
        store
            .upsert_skill_index(
                crate::memory::Scope::Global,
                None,
                crate::memory::IndexEntry::Skill {
                    name: "rna-seq".into(),
                    path: "L3_skills/rna-seq.md".into(),
                    scope: crate::memory::Scope::Global,
                    triggers: vec!["rna-seq".into(), "differential expression".into()],
                    hits: 0,
                    last_used: None,
                },
            )
            .await
            .unwrap();
        let recaller: Arc<dyn Recaller> = Arc::new(Bm25Recaller::new(3));
        let out = perceive(
            &store,
            Some(&project),
            recaller,
            &PerceiveCtx {
                user_text: "find DE genes in this rna-seq dataset".into(),
                project_summary: "Project: demo".into(),
            },
            4096,
        )
        .await
        .unwrap();
        assert!(out.system_prompt.contains("Meta rules"));
        assert!(out.system_prompt.contains("Project: demo"));
        assert!(out.system_prompt.contains("rna-seq"));
        assert!(!out.recalled.is_empty());
    }
}

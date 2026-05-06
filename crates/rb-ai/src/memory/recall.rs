//! Memory recall. BM25 over compact "candidate text" derived from index
//! entries + L1 insights. Flash-LLM-driven recall lives in `flash_recaller`
//! below but is wired in Phase 4.

use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AiError;
use crate::memory::layers::{IndexEntry, Insight};
use crate::memory::store::MemoryStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallCandidate {
    pub id: String,
    pub kind: String,  // "skill" | "archive" | "insight"
    pub scope: String, // "global" | "project"
    pub text: String,  // compact text used for matching
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallResult {
    pub picked: Vec<RecallCandidate>,
    pub rationale: Option<String>,
}

#[async_trait]
pub trait Recaller: Send + Sync {
    async fn recall(
        &self,
        query: &str,
        candidates: Vec<RecallCandidate>,
        budget_tokens: usize,
    ) -> Result<RecallResult, AiError>;
}

pub fn collect_candidates(
    store: &MemoryStore,
    project_root: Option<&Path>,
) -> Result<Vec<RecallCandidate>, AiError> {
    let mut out = vec![];

    // Global L3 skill index.
    let g_index = store.read_index(&store.global_root.join("L3_skills/_index.json"))?;
    for e in g_index {
        if let IndexEntry::Skill {
            name,
            path,
            triggers,
            ..
        } = e
        {
            out.push(RecallCandidate {
                id: format!("skill:global:{name}"),
                kind: "skill".into(),
                scope: "global".into(),
                text: format!("{name} {}", triggers.join(" ")),
                path: Some(path),
            });
        }
    }

    // Global L1 insights (last 200 lines).
    let l1 = store.global_root.join("L1_insights.jsonl");
    if l1.exists() {
        let body = std::fs::read_to_string(&l1)?;
        for line in body.lines().rev().take(200) {
            if let Ok(v) = serde_json::from_str::<Insight>(line) {
                out.push(RecallCandidate {
                    id: format!("insight:{}", v.id),
                    kind: "insight".into(),
                    scope: "global".into(),
                    text: format!("{} {}", v.tag, v.summary),
                    path: None,
                });
            }
        }
    }

    if let Some(p) = project_root {
        let proot = MemoryStore::project_root(p);
        // L3_local skills.
        let p_index = store.read_index(&proot.join("L3_local/_index.json"))?;
        for e in p_index {
            if let IndexEntry::Skill {
                name,
                path,
                triggers,
                ..
            } = e
            {
                out.push(RecallCandidate {
                    id: format!("skill:project:{name}"),
                    kind: "skill".into(),
                    scope: "project".into(),
                    text: format!("{name} {}", triggers.join(" ")),
                    path: Some(path),
                });
            }
        }
        // L4 archives.
        let a_index = store.read_index(&proot.join("L4_archives/_index.json"))?;
        for e in a_index {
            if let IndexEntry::Archive {
                id, summary, tags, ..
            } = e
            {
                out.push(RecallCandidate {
                    id: format!("archive:{id}"),
                    kind: "archive".into(),
                    scope: "project".into(),
                    text: format!("{} {}", summary, tags.join(" ")),
                    path: None,
                });
            }
        }
    }

    Ok(out)
}

// ---------- BM25 ----------

/// Lightweight BM25 over candidate texts. No stemming; lower-cased token split.
pub struct Bm25Recaller {
    pub top_k: usize,
}

impl Bm25Recaller {
    pub fn new(top_k: usize) -> Self {
        Self { top_k }
    }
}

#[async_trait]
impl Recaller for Bm25Recaller {
    async fn recall(
        &self,
        query: &str,
        candidates: Vec<RecallCandidate>,
        _budget_tokens: usize,
    ) -> Result<RecallResult, AiError> {
        if candidates.is_empty() {
            return Ok(RecallResult {
                picked: vec![],
                rationale: None,
            });
        }
        let q_terms: Vec<String> = tokenize(query);
        if q_terms.is_empty() {
            return Ok(RecallResult {
                picked: vec![],
                rationale: None,
            });
        }
        let docs: Vec<Vec<String>> = candidates.iter().map(|c| tokenize(&c.text)).collect();
        let avgdl = docs.iter().map(|d| d.len()).sum::<usize>() as f32 / docs.len() as f32;
        let n = docs.len();
        let mut df: HashMap<&str, usize> = HashMap::new();
        for d in &docs {
            let mut seen: HashMap<&str, ()> = HashMap::new();
            for t in d {
                if seen.insert(t.as_str(), ()).is_none() {
                    *df.entry(t.as_str()).or_insert(0) += 1;
                }
            }
        }
        let k1 = 1.5_f32;
        let b = 0.75_f32;
        let mut scored: Vec<(usize, f32)> = docs
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let dl = d.len() as f32;
                let mut score = 0.0;
                for q in &q_terms {
                    let f = d.iter().filter(|t| *t == q).count() as f32;
                    if f == 0.0 {
                        continue;
                    }
                    let n_q = *df.get(q.as_str()).unwrap_or(&0) as f32;
                    let idf = ((n as f32 - n_q + 0.5) / (n_q + 0.5) + 1.0).ln();
                    let denom = f + k1 * (1.0 - b + b * dl / avgdl.max(1.0));
                    score += idf * (f * (k1 + 1.0)) / denom;
                }
                (i, score)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let picked: Vec<RecallCandidate> = scored
            .into_iter()
            .filter(|(_, s)| *s > 0.0)
            .take(self.top_k)
            .map(|(i, _)| candidates[i].clone())
            .collect();
        Ok(RecallResult {
            picked,
            rationale: Some("bm25".into()),
        })
    }
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

// ---------- Composite (Flash primary, BM25 fallback) ----------

pub struct CompositeRecaller {
    pub primary: Option<std::sync::Arc<dyn Recaller>>,
    pub fallback: std::sync::Arc<Bm25Recaller>,
    pub timeout: std::time::Duration,
}

#[async_trait]
impl Recaller for CompositeRecaller {
    async fn recall(
        &self,
        query: &str,
        candidates: Vec<RecallCandidate>,
        budget_tokens: usize,
    ) -> Result<RecallResult, AiError> {
        if let Some(p) = &self.primary {
            let cands = candidates.clone();
            let q = query.to_string();
            let primary = p.clone();
            let res =
                tokio::time::timeout(self.timeout, primary.recall(&q, cands, budget_tokens)).await;
            if let Ok(Ok(r)) = res {
                return Ok(r);
            }
            // fall through to fallback
        }
        self.fallback.recall(query, candidates, budget_tokens).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cands() -> Vec<RecallCandidate> {
        vec![
            RecallCandidate {
                id: "skill:global:rna-seq".into(),
                kind: "skill".into(),
                scope: "global".into(),
                text: "human rna-seq differential expression deseq2".into(),
                path: None,
            },
            RecallCandidate {
                id: "skill:global:wgs".into(),
                kind: "skill".into(),
                scope: "global".into(),
                text: "whole genome sequencing variant calling".into(),
                path: None,
            },
            RecallCandidate {
                id: "insight:1".into(),
                kind: "insight".into(),
                scope: "global".into(),
                text: "low mapping rate often caused by adapter contamination".into(),
                path: None,
            },
        ]
    }

    #[tokio::test]
    async fn bm25_picks_topical_candidate() {
        let r = Bm25Recaller::new(2);
        let res = r
            .recall(
                "how do I find DE genes from RNA-seq data?",
                make_cands(),
                4096,
            )
            .await
            .unwrap();
        assert!(!res.picked.is_empty());
        assert_eq!(res.picked[0].id, "skill:global:rna-seq");
    }

    #[tokio::test]
    async fn bm25_returns_empty_for_empty_query() {
        let r = Bm25Recaller::new(2);
        let res = r.recall("???", make_cands(), 4096).await.unwrap();
        assert!(res.picked.is_empty());
    }

    #[tokio::test]
    async fn composite_falls_back_when_primary_times_out() {
        struct SlowPrimary;
        #[async_trait]
        impl Recaller for SlowPrimary {
            async fn recall(
                &self,
                _q: &str,
                _c: Vec<RecallCandidate>,
                _b: usize,
            ) -> Result<RecallResult, AiError> {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                Ok(RecallResult {
                    picked: vec![],
                    rationale: Some("primary".into()),
                })
            }
        }
        let c = CompositeRecaller {
            primary: Some(std::sync::Arc::new(SlowPrimary)),
            fallback: std::sync::Arc::new(Bm25Recaller::new(2)),
            timeout: std::time::Duration::from_millis(50),
        };
        let res = c.recall("rna-seq", make_cands(), 4096).await.unwrap();
        assert_eq!(res.rationale.as_deref(), Some("bm25"));
    }
}

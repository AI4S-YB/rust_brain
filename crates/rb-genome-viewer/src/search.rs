use crate::index::MemoryIndex;
use crate::tracks::TrackId;
use serde::Serialize;
use std::collections::HashMap;

/// Maps normalized feature name → (track_id, chrom, start, end).
#[derive(Default)]
pub struct SearchIndex {
    entries: HashMap<String, Vec<SearchEntry>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchEntry {
    pub track_id: TrackId,
    pub name: String,
    pub chrom: String,
    pub start: u64,
    pub end: u64,
}

impl SearchIndex {
    pub fn add_track(&mut self, track_id: &TrackId, memory: &MemoryIndex) {
        for f in memory.all_features() {
            let keys: Vec<String> = f.name.clone().into_iter()
                .chain(f.attrs.get("gene_id").cloned())
                .chain(f.attrs.get("gene_name").cloned())
                .chain(f.attrs.get("transcript_id").cloned())
                .chain(f.attrs.get("Name").cloned())
                .chain(f.attrs.get("ID").cloned())
                .collect();
            for k in keys {
                let lc = k.to_lowercase();
                self.entries.entry(lc).or_default().push(SearchEntry {
                    track_id: track_id.clone(),
                    name: k,
                    chrom: f.chrom.clone(),
                    start: f.start,
                    end: f.end,
                });
            }
        }
    }

    pub fn remove_track(&mut self, track_id: &TrackId) {
        for v in self.entries.values_mut() {
            v.retain(|e| e.track_id != *track_id);
        }
        self.entries.retain(|_, v| !v.is_empty());
    }

    /// Case-insensitive substring match. Returns at most `limit` entries.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&SearchEntry> {
        let q = query.to_lowercase();
        let mut out = Vec::new();
        for (key, entries) in &self.entries {
            if key.contains(&q) {
                for e in entries {
                    out.push(e);
                    if out.len() == limit {
                        return out;
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::MemoryIndex;
    use crate::tracks::TrackKind;
    use std::path::PathBuf;

    #[test]
    fn search_finds_gene_by_name() {
        let gtf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gtf");
        let mem = MemoryIndex::load(&gtf, TrackKind::Gtf).unwrap();
        let mut idx = SearchIndex::default();
        idx.add_track(&"t1".to_string(), &mem);
        let hits = idx.search("brca", 10);
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|e| e.chrom == "chr1" && e.start == 10));
    }

    #[test]
    fn search_case_insensitive() {
        let gtf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gtf");
        let mem = MemoryIndex::load(&gtf, TrackKind::Gtf).unwrap();
        let mut idx = SearchIndex::default();
        idx.add_track(&"t1".to_string(), &mem);
        let lower = idx.search("tp53", 10);
        let upper = idx.search("TP53", 10);
        assert_eq!(lower.len(), upper.len());
    }

    #[test]
    fn remove_track_clears_entries() {
        let gtf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gtf");
        let mem = MemoryIndex::load(&gtf, TrackKind::Gtf).unwrap();
        let mut idx = SearchIndex::default();
        idx.add_track(&"t1".to_string(), &mem);
        idx.remove_track(&"t1".to_string());
        assert!(idx.search("brca", 10).is_empty());
    }
}

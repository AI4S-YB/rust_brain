use crate::error::{Result, ViewerError};
use crate::index::SparseOffsetIndex;
use serde::Serialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct FastqRecord {
    pub id: String,
    pub seq: String,
    pub plus: String,
    pub qual: String,
}

#[derive(Debug, Serialize)]
pub struct OpenResult {
    pub total_records: usize,
    pub index_cached: bool,
    pub path: PathBuf,
}

pub struct FastqSession {
    pub path: PathBuf,
    pub index: SparseOffsetIndex,
}

impl FastqSession {
    pub fn open(path: &Path, cache_dir: &Path) -> Result<(Self, bool)> {
        let (index, cached) = SparseOffsetIndex::build_or_load(cache_dir, path)?;
        Ok((
            Self {
                path: path.to_path_buf(),
                index,
            },
            cached,
        ))
    }

    pub fn read_records(&self, start: usize, count: usize) -> Result<Vec<FastqRecord>> {
        if start >= self.index.total_records {
            return Ok(Vec::new());
        }
        let (_, offset) = self.index.anchor_for(start);
        let f = File::open(&self.path)?;
        let mut reader = BufReader::new(f);
        reader.seek(SeekFrom::Start(offset))?;

        let mut cursor = (start / self.index.spacing) * self.index.spacing;
        let mut skip_remaining = start - cursor;
        let mut out = Vec::with_capacity(count);
        let mut line = String::new();

        while out.len() < count && cursor < self.index.total_records {
            let mut rec = [String::new(), String::new(), String::new(), String::new()];
            for slot in &mut rec {
                line.clear();
                let n = reader.read_line(&mut line)?;
                if n == 0 {
                    return Err(ViewerError::Parse(format!(
                        "unexpected EOF at record {}",
                        cursor
                    )));
                }
                *slot = line.trim_end_matches(&['\n', '\r'][..]).to_string();
            }
            if skip_remaining > 0 {
                skip_remaining -= 1;
            } else {
                out.push(FastqRecord {
                    id: rec[0].clone(),
                    seq: rec[1].clone(),
                    plus: rec[2].clone(),
                    qual: rec[3].clone(),
                });
            }
            cursor += 1;
        }
        Ok(out)
    }

    pub fn seek_percent(&self, pct: f32) -> usize {
        let pct = pct.clamp(0.0, 1.0);
        ((self.index.total_records as f32) * pct) as usize
    }

    pub fn search_id(
        &self,
        query: &str,
        from: usize,
        limit: usize,
    ) -> Result<Vec<(usize, String)>> {
        let mut hits = Vec::new();
        let mut cursor = from;
        let chunk = 1000;
        while cursor < self.index.total_records && hits.len() < limit {
            let batch = self.read_records(cursor, chunk)?;
            for (i, rec) in batch.iter().enumerate() {
                if rec.id.contains(query) {
                    hits.push((cursor + i, rec.id.clone()));
                    if hits.len() == limit {
                        break;
                    }
                }
            }
            if batch.is_empty() {
                break;
            }
            cursor += batch.len();
        }
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.fastq")
    }

    #[test]
    fn reads_first_records() {
        let cache = tempfile::tempdir().unwrap();
        let (session, _) = FastqSession::open(&fixture(), cache.path()).unwrap();
        let recs = session.read_records(0, 3).unwrap();
        assert_eq!(recs.len(), 3);
        assert!(recs[0].id.starts_with("@read_0000"));
        assert_eq!(recs[1].id, "@read_0001 metadata");
        assert_eq!(recs[0].seq, "ACGTACGTACGTACGT");
    }

    #[test]
    fn reads_from_middle() {
        let cache = tempfile::tempdir().unwrap();
        let (session, _) = FastqSession::open(&fixture(), cache.path()).unwrap();
        let recs = session.read_records(42, 2).unwrap();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].id, "@read_0042 metadata");
        assert_eq!(recs[1].id, "@read_0043 metadata");
    }

    #[test]
    fn seek_percent_returns_record_number() {
        let cache = tempfile::tempdir().unwrap();
        let (session, _) = FastqSession::open(&fixture(), cache.path()).unwrap();
        assert_eq!(session.seek_percent(0.0), 0);
        assert_eq!(session.seek_percent(0.5), 50);
        assert_eq!(session.seek_percent(1.0), 100);
    }

    #[test]
    fn search_finds_matching_id() {
        let cache = tempfile::tempdir().unwrap();
        let (session, _) = FastqSession::open(&fixture(), cache.path()).unwrap();
        let hits = session.search_id("0042", 0, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 42);
    }

    #[test]
    fn read_past_end_returns_empty() {
        let cache = tempfile::tempdir().unwrap();
        let (session, _) = FastqSession::open(&fixture(), cache.path()).unwrap();
        let recs = session.read_records(5000, 10).unwrap();
        assert!(recs.is_empty());
    }
}

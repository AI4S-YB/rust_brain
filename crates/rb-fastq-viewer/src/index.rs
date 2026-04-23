use crate::error::{Result, ViewerError};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::fs::{File, Metadata};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub const ANCHOR_SPACING: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SparseOffsetIndex {
    pub anchors: Vec<u64>, // byte offset of record N where N = i * spacing
    pub total_records: usize,
    pub file_size: u64,
    pub mtime_unix: i64,
    pub spacing: usize,
}

impl SparseOffsetIndex {
    pub fn build(path: &Path) -> Result<Self> {
        Self::build_with_spacing(path, ANCHOR_SPACING)
    }

    pub fn build_with_spacing(path: &Path, spacing: usize) -> Result<Self> {
        if !path.exists() {
            return Err(ViewerError::NotFound(path.to_path_buf()));
        }
        let meta = std::fs::metadata(path)?;
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        let mut anchors = Vec::new();
        let mut offset: u64 = 0;
        let mut record_count: usize = 0;
        let mut line_buf = String::new();

        loop {
            if record_count % spacing == 0 {
                anchors.push(offset);
            }
            // A FASTQ record is exactly 4 lines.
            let mut bytes_in_record: u64 = 0;
            for line_idx in 0..4 {
                line_buf.clear();
                let n = reader.read_line(&mut line_buf)?;
                if n == 0 {
                    // EOF mid-record: if line_idx==0, clean end; else corrupt.
                    if line_idx == 0 {
                        return Ok(Self {
                            anchors,
                            total_records: record_count,
                            file_size: meta.len(),
                            mtime_unix: unix_mtime(&meta),
                            spacing,
                        });
                    }
                    return Err(ViewerError::Parse(format!(
                        "unexpected EOF inside record {}, line {}",
                        record_count, line_idx
                    )));
                }
                bytes_in_record += n as u64;
            }
            offset += bytes_in_record;
            record_count += 1;
        }
    }

    /// Byte offset to seek to when jumping to `record_n`. Returns the offset of the nearest
    /// preceding anchor; caller is responsible for scanning forward `(record_n - anchor_idx * self.spacing)`
    /// records after seeking.
    pub fn anchor_for(&self, record_n: usize) -> (usize, u64) {
        let anchor_idx = record_n / self.spacing;
        let offset = self.anchors.get(anchor_idx).copied().unwrap_or(0);
        (anchor_idx, offset)
    }
}

impl SparseOffsetIndex {
    pub fn cache_key(file_path: &Path) -> String {
        let abs = file_path
            .canonicalize()
            .unwrap_or_else(|_| file_path.to_path_buf());
        let mut hasher = Sha1::new();
        hasher.update(abs.to_string_lossy().as_bytes());
        let digest = hasher.finalize();
        digest.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn cache_path(cache_dir: &Path, file_path: &Path) -> PathBuf {
        cache_dir.join(format!("{}.idx", Self::cache_key(file_path)))
    }

    pub fn load_cached(cache_dir: &Path, file_path: &Path) -> Result<Option<Self>> {
        let cp = Self::cache_path(cache_dir, file_path);
        if !cp.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&cp)?;
        let idx: SparseOffsetIndex =
            bincode::deserialize(&bytes).map_err(|e| ViewerError::IndexCorrupt(e.to_string()))?;

        let meta = std::fs::metadata(file_path)?;
        let current_mtime = unix_mtime(&meta);
        if idx.file_size != meta.len() || idx.mtime_unix != current_mtime {
            return Ok(None); // stale
        }
        Ok(Some(idx))
    }

    pub fn save(&self, cache_dir: &Path, file_path: &Path) -> Result<()> {
        std::fs::create_dir_all(cache_dir)?;
        let cp = Self::cache_path(cache_dir, file_path);
        let bytes = bincode::serialize(self)?;
        std::fs::write(cp, bytes)?;
        Ok(())
    }

    pub fn build_or_load(cache_dir: &Path, file_path: &Path) -> Result<(Self, bool)> {
        if let Some(idx) = Self::load_cached(cache_dir, file_path)? {
            return Ok((idx, true));
        }
        let idx = Self::build(file_path)?;
        idx.save(cache_dir, file_path)?;
        Ok((idx, false))
    }
}

fn unix_mtime(meta: &Metadata) -> i64 {
    use std::time::UNIX_EPOCH;
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.fastq")
    }

    #[test]
    fn counts_records_correctly() {
        let idx = SparseOffsetIndex::build(&fixture()).unwrap();
        assert_eq!(idx.total_records, 100);
    }

    #[test]
    fn anchor_zero_is_file_start() {
        let idx = SparseOffsetIndex::build(&fixture()).unwrap();
        assert_eq!(idx.anchors[0], 0);
    }

    #[test]
    fn anchor_for_small_file_returns_zero() {
        let idx = SparseOffsetIndex::build(&fixture()).unwrap();
        let (anchor_idx, offset) = idx.anchor_for(50);
        assert_eq!(anchor_idx, 0);
        assert_eq!(offset, 0);
    }

    #[test]
    fn larger_file_has_multiple_anchors() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/larger.fastq");
        let idx = SparseOffsetIndex::build_with_spacing(&path, 1000).unwrap();
        assert_eq!(idx.total_records, 5000);
        // With spacing=1000 and 5000 records the loop pushes an anchor at record_count
        // 0,1000,2000,3000,4000 (5 data anchors) and one more at 5000 (the EOF boundary)
        // before the clean-exit path fires — 6 total.
        assert_eq!(idx.anchors.len(), 6);
        // Record 2500 sits in anchor bucket 2.
        let (anchor_idx, offset) = idx.anchor_for(2500);
        assert_eq!(anchor_idx, 2);
        assert!(offset > 0);
    }

    #[test]
    fn anchor_offsets_are_monotonic() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/larger.fastq");
        let idx = SparseOffsetIndex::build_with_spacing(&path, 1000).unwrap();
        for w in idx.anchors.windows(2) {
            assert!(w[0] < w[1], "anchors must be strictly increasing");
        }
    }

    #[test]
    fn cache_round_trip() {
        let cache = tempfile::tempdir().unwrap();
        let fp = fixture();
        let (idx1, hit1) = SparseOffsetIndex::build_or_load(cache.path(), &fp).unwrap();
        assert!(!hit1, "first call is a miss");
        let (idx2, hit2) = SparseOffsetIndex::build_or_load(cache.path(), &fp).unwrap();
        assert!(hit2, "second call hits cache");
        assert_eq!(idx1.total_records, idx2.total_records);
        assert_eq!(idx1.anchors, idx2.anchors);
    }

    #[test]
    fn cache_invalidates_on_mtime_change() {
        use std::fs::OpenOptions;
        use std::io::Write;
        let cache = tempfile::tempdir().unwrap();
        let tmp_fq = tempfile::NamedTempFile::new().unwrap();
        std::fs::copy(&fixture(), tmp_fq.path()).unwrap();
        let (_, _hit1) = SparseOffsetIndex::build_or_load(cache.path(), tmp_fq.path()).unwrap();

        // Append a record to change size+mtime.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let mut f = OpenOptions::new().append(true).open(tmp_fq.path()).unwrap();
        writeln!(f, "@new_read\nACGT\n+\nIIII").unwrap();
        drop(f);

        let (_, hit2) = SparseOffsetIndex::build_or_load(cache.path(), tmp_fq.path()).unwrap();
        assert!(!hit2, "cache must invalidate after file change");
    }
}

use crate::error::{Result, ViewerError};
use serde::{Deserialize, Serialize};
use std::fs::{File, Metadata};
use std::io::{BufRead, BufReader};
use std::path::Path;

pub const ANCHOR_SPACING: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SparseOffsetIndex {
    pub anchors: Vec<u64>,     // byte offset of record N where N = i * ANCHOR_SPACING
    pub total_records: usize,
    pub file_size: u64,
    pub mtime_unix: i64,
}

impl SparseOffsetIndex {
    pub fn build(path: &Path) -> Result<Self> {
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
            if record_count % ANCHOR_SPACING == 0 {
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
    /// preceding anchor; caller is responsible for scanning forward `(record_n - anchor_idx * ANCHOR_SPACING)`
    /// records after seeking.
    pub fn anchor_for(&self, record_n: usize) -> (usize, u64) {
        let anchor_idx = record_n / ANCHOR_SPACING;
        let offset = self.anchors.get(anchor_idx).copied().unwrap_or(0);
        (anchor_idx, offset)
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
}

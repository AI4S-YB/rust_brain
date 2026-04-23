use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use noodles_fasta as fasta;
use noodles_core::Region;
use serde::Serialize;

use crate::error::{Result, ViewerError};

/// Chromosome name and length extracted from a `.fai` index.
#[derive(Debug, Clone, Serialize)]
pub struct ChromMeta {
    pub name: String,
    pub length: u64,
}

/// Metadata returned when a reference FASTA is loaded.
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceMeta {
    pub path: PathBuf,
    pub chroms: Vec<ChromMeta>,
    /// `true` when the `.fai` was built during this call; `false` when it already existed.
    pub fai_built: bool,
}

/// A loaded reference FASTA with its index, ready for random-access queries.
pub struct ReferenceHandle {
    pub path: PathBuf,
    pub fai: fasta::fai::Index,
}

impl ReferenceHandle {
    /// Load a reference FASTA file.
    ///
    /// * Returns an error if `path` does not exist.
    /// * If `path + ".fai"` does not exist, builds and writes it; sets `fai_built: true`.
    /// * Otherwise reads the existing index; sets `fai_built: false`.
    /// * Returns both the handle (for queries) and metadata (chroms, lengths, build flag).
    pub fn load(path: &Path) -> Result<(Self, ReferenceMeta)> {
        if !path.exists() {
            return Err(ViewerError::NotFound(path.to_path_buf()));
        }

        // Derive the .fai path: append ".fai" to the FASTA path.
        let fai_path = {
            let p = path.to_path_buf();
            let mut os = p.into_os_string();
            os.push(".fai");
            PathBuf::from(os)
        };

        let (index, fai_built) = if fai_path.exists() {
            // Read existing index.
            let idx = fasta::fai::fs::read(&fai_path)?;
            (idx, false)
        } else {
            // Build the index from the FASTA file.
            let idx = fasta::fs::index(path)?;
            // Write it next to the FASTA.
            fasta::fai::fs::write(&fai_path, &idx)?;
            (idx, true)
        };

        // Extract chromosome names and lengths from the index records.
        let chroms: Vec<ChromMeta> = index
            .as_ref()
            .iter()
            .map(|rec| ChromMeta {
                name: String::from_utf8_lossy(rec.name()).into_owned(),
                length: rec.length(),
            })
            .collect();

        let meta = ReferenceMeta {
            path: path.to_path_buf(),
            chroms,
            fai_built,
        };

        let handle = ReferenceHandle {
            path: path.to_path_buf(),
            fai: index,
        };

        Ok((handle, meta))
    }

    /// Fetch a subsequence from the reference.
    ///
    /// `start` and `end` are 1-based, inclusive (matching noodles' `Region` interval convention).
    ///
    /// Returns the bases as a `String`.
    pub fn fetch_region(&self, chrom: &str, start: u64, end: u64) -> Result<String> {
        // Build a region string and parse it — noodles accepts "chr:start-end" with 1-based coords.
        let region_str = format!("{}:{}-{}", chrom, start, end);
        let region: Region = region_str
            .parse()
            .map_err(|e: noodles_core::region::ParseError| ViewerError::Parse(e.to_string()))?;

        let file = File::open(&self.path)?;
        let buf_reader = BufReader::new(file);
        let mut reader = fasta::io::Reader::new(buf_reader);

        let record = reader.query(&self.fai, &region)?;

        // The sequence bytes are returned as a Vec<u8> (via AsRef<[u8]>).
        let seq_bytes: &[u8] = record.sequence().as_ref();
        let seq_str = String::from_utf8(seq_bytes.to_vec())
            .map_err(|e| ViewerError::Parse(e.to_string()))?;

        Ok(seq_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fa() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.fa")
    }

    #[test]
    fn loads_two_chroms_and_builds_fai() {
        let fai_path = {
            let mut p = fa();
            p.as_mut_os_string().push(".fai");
            p
        };
        let _ = std::fs::remove_file(&fai_path);

        let (_handle, meta) = ReferenceHandle::load(&fa()).unwrap();
        assert!(meta.fai_built);
        assert_eq!(meta.chroms.len(), 2);
        assert_eq!(meta.chroms[0].name, "chr1");
        assert_eq!(meta.chroms[0].length, 128);
        assert_eq!(meta.chroms[1].name, "chr2");
        assert_eq!(meta.chroms[1].length, 128);
    }

    #[test]
    fn fetches_region_bytes() {
        // Ensure the .fai exists from the previous test run or build one.
        let _ = ReferenceHandle::load(&fa()).unwrap();
        let (handle, _) = ReferenceHandle::load(&fa()).unwrap();
        let seq = handle.fetch_region("chr1", 1, 16).unwrap();
        assert_eq!(seq, "ACGTACGTACGTACGT");
    }

    #[test]
    fn fetches_cross_chrom_distinctly() {
        let _ = ReferenceHandle::load(&fa()).unwrap();
        let (handle, _) = ReferenceHandle::load(&fa()).unwrap();
        let a = handle.fetch_region("chr1", 1, 4).unwrap();
        let b = handle.fetch_region("chr2", 1, 4).unwrap();
        assert_eq!(a, "ACGT");
        assert_eq!(b, "GGGG");
    }
}

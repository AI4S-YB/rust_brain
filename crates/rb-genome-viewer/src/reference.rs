use std::{
    fs::File,
    io::{BufReader, ErrorKind},
    path::{Path, PathBuf},
};

use noodles_core::Region;
use noodles_fasta as fasta;
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
    /// * If the existing index is invalid, rebuilds it; sets `fai_built: true`.
    /// * Returns both the handle (for queries) and metadata (chroms, lengths, build flag).
    pub fn load(path: &Path) -> Result<(Self, ReferenceMeta)> {
        if !path.exists() {
            return Err(ViewerError::NotFound(path.to_path_buf()));
        }

        let fai_path = Self::fai_path(path);

        let (index, fai_built) = if fai_path.exists() {
            match fasta::fai::fs::read(&fai_path) {
                Ok(idx) => (idx, false),
                Err(err) if err.kind() == ErrorKind::InvalidData => {
                    (Self::build_index(path, &fai_path)?, true)
                }
                Err(err) => return Err(err.into()),
            }
        } else {
            (Self::build_index(path, &fai_path)?, true)
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

    fn fai_path(path: &Path) -> PathBuf {
        let mut os = path.as_os_str().to_os_string();
        os.push(".fai");
        PathBuf::from(os)
    }

    fn build_index(path: &Path, fai_path: &Path) -> Result<fasta::fai::Index> {
        let index = fasta::fs::index(path)?;
        fasta::fai::fs::write(fai_path, &index)?;
        Ok(index)
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
        let seq_str =
            String::from_utf8(seq_bytes.to_vec()).map_err(|e| ViewerError::Parse(e.to_string()))?;

        Ok(seq_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn isolated_fa() -> (TempDir, PathBuf) {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.fa");
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("tiny.fa");
        std::fs::copy(&src, &dst).unwrap();
        (dir, dst)
    }

    #[test]
    fn loads_two_chroms_and_builds_fai() {
        let (_tmp, fa) = isolated_fa();
        let (_handle, meta) = ReferenceHandle::load(&fa).unwrap();
        assert!(meta.fai_built);
        assert_eq!(meta.chroms.len(), 2);
        assert_eq!(meta.chroms[0].name, "chr1");
        assert_eq!(meta.chroms[0].length, 128);
        assert_eq!(meta.chroms[1].name, "chr2");
        assert_eq!(meta.chroms[1].length, 128);
    }

    #[test]
    fn rebuilds_invalid_existing_fai() {
        let (_tmp, fa) = isolated_fa();
        let fai = ReferenceHandle::fai_path(&fa);
        std::fs::write(&fai, b"chr1\t128\t16\t64\t65\r\nchr2\t128\t162\t64\t65\r\n").unwrap();

        let (handle, meta) = ReferenceHandle::load(&fa).unwrap();

        assert!(meta.fai_built);
        assert_eq!(meta.chroms.len(), 2);
        assert_eq!(handle.fetch_region("chr2", 1, 4).unwrap(), "GGGG");
        assert!(!std::fs::read_to_string(&fai).unwrap().contains('\r'));
    }

    #[test]
    fn fetches_region_bytes() {
        let (_tmp, fa) = isolated_fa();
        let (handle, _) = ReferenceHandle::load(&fa).unwrap();
        let seq = handle.fetch_region("chr1", 1, 16).unwrap();
        assert_eq!(seq, "ACGTACGTACGTACGT");
    }

    #[test]
    fn fetches_cross_chrom_distinctly() {
        let (_tmp, fa) = isolated_fa();
        let (handle, _) = ReferenceHandle::load(&fa).unwrap();
        let a = handle.fetch_region("chr1", 1, 4).unwrap();
        let b = handle.fetch_region("chr2", 1, 4).unwrap();
        assert_eq!(a, "ACGT");
        assert_eq!(b, "GGGG");
    }
}

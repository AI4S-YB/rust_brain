use crate::error::{BamToolsError, Result};
use noodles_bam::{self as bam, bai};
use std::path::{Path, PathBuf};

/// Builds a `.bai` index for a coordinate-sorted BAM file and writes it to
/// `<src>.bai`. Returns the index path on success.
pub fn index_bam(src: &Path) -> Result<PathBuf> {
    if !src.exists() {
        return Err(BamToolsError::NotFound(src.to_path_buf()));
    }
    let index = bam::fs::index(src).map_err(|e| {
        // noodles returns InvalidData when the sort order isn't coordinate
        if e.kind() == std::io::ErrorKind::InvalidData
            && e.to_string().contains("invalid sort order")
        {
            BamToolsError::NotSorted
        } else {
            BamToolsError::Io(e)
        }
    })?;

    let dst = bai_path(src);
    bai::fs::write(&dst, &index)?;
    Ok(dst)
}

/// Returns the conventional `<src>.bai` path.
pub fn bai_path(src: &Path) -> PathBuf {
    let mut s = src.as_os_str().to_os_string();
    s.push(".bai");
    PathBuf::from(s)
}

use crate::error::{BamToolsError, Result};
use crate::index::bai_path;
use noodles_bam as bam;
use noodles_core::Region;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::str::FromStr;

/// Extract reads from `src` that intersect `region` (e.g. `chr1:10000-20000`)
/// and write them to a new BAM file at `dst`. Returns the number of records
/// written.
pub fn extract_region(src: &Path, region_str: &str, dst: &Path) -> Result<usize> {
    if !src.exists() {
        return Err(BamToolsError::NotFound(src.to_path_buf()));
    }
    let bai = bai_path(src);
    if !bai.exists() {
        return Err(BamToolsError::IndexMissing(src.to_path_buf()));
    }

    let region =
        Region::from_str(region_str).map_err(|e| BamToolsError::InvalidRegion(e.to_string()))?;

    let mut reader = bam::io::indexed_reader::Builder::default().build_from_path(src)?;
    let header = reader.read_header()?;

    // Validate that the region's reference exists in the BAM header so we can
    // give a clean error instead of an opaque "region not found".
    let ref_name = std::str::from_utf8(region.name())
        .map_err(|e| BamToolsError::InvalidRegion(e.to_string()))?;
    if !header.reference_sequences().contains_key(region.name()) {
        return Err(BamToolsError::UnknownReference(ref_name.to_string()));
    }

    if let Some(parent) = dst.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let out = File::create(dst)?;
    let mut writer = bam::io::Writer::new(BufWriter::new(out));
    writer.write_header(&header)?;

    let mut count = 0usize;
    let query = reader.query(&header, &region)?;
    for record in query.records() {
        let record = record?;
        writer.write_record(&header, &record)?;
        count += 1;
    }
    writer.try_finish()?;
    Ok(count)
}

/// A reference sequence entry from a BAM header.
#[derive(serde::Serialize)]
pub struct ReferenceEntry {
    pub name: String,
    pub length: usize,
}

/// Read a BAM header and return its reference sequences. Useful so the UI can
/// present a chromosome picker without needing `samtools view -H`.
pub fn header_references(src: &Path) -> Result<Vec<ReferenceEntry>> {
    if !src.exists() {
        return Err(BamToolsError::NotFound(src.to_path_buf()));
    }
    let mut reader = bam::io::reader::Builder.build_from_path(src)?;
    let header = reader.read_header()?;
    let refs = header
        .reference_sequences()
        .iter()
        .map(|(name, ref_seq)| ReferenceEntry {
            name: String::from_utf8_lossy(name).into_owned(),
            length: ref_seq.length().get(),
        })
        .collect();
    Ok(refs)
}

use crate::error::{Result, ViewerError};
use crate::tracks::TrackKind;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufRead;
use std::path::Path;

pub const MEMORY_INDEX_MAX_BYTES: u64 = 200 * 1024 * 1024; // 200 MB

#[derive(Debug, Clone, Serialize)]
pub struct Feature {
    pub chrom: String,
    pub start: u64, // 1-based inclusive (GFF/GTF convention); BED converted on load
    pub end: u64,   // inclusive
    pub name: Option<String>,
    pub strand: Option<char>,
    pub kind: String, // e.g., "gene", "exon", free-text
    pub attrs: HashMap<String, String>,
}

/// Simple per-chromosome feature list. For L1 we linear-scan per query — fast enough
/// for typical annotation file sizes (<100k features). If we ever need true interval
/// trees we can swap in `rust-lapper` behind this API.
#[derive(Default)]
pub struct MemoryIndex {
    by_chrom: HashMap<String, Vec<Feature>>,
}

impl MemoryIndex {
    pub fn load(path: &Path, kind: TrackKind) -> Result<Self> {
        match kind {
            TrackKind::Gff => Self::load_gff(path, false),
            TrackKind::Gtf => Self::load_gff(path, true), // same format; different attr syntax
            TrackKind::Bed => Self::load_bed(path),
        }
    }

    pub fn query(&self, chrom: &str, start: u64, end: u64) -> Vec<&Feature> {
        match self.by_chrom.get(chrom) {
            None => Vec::new(),
            Some(v) => v
                .iter()
                .filter(|f| f.end >= start && f.start <= end)
                .collect(),
        }
    }

    pub fn feature_count(&self) -> usize {
        self.by_chrom.values().map(|v| v.len()).sum()
    }

    pub fn all_features(&self) -> impl Iterator<Item = &Feature> {
        self.by_chrom.values().flat_map(|v| v.iter())
    }

    fn load_gff(path: &Path, is_gtf: bool) -> Result<Self> {
        let mut idx = Self::default();
        let file = File::open(path)?;
        // noodles-bgzf 0.46: constructor is noodles_bgzf::io::Reader::new; it
        // implements BufRead directly and must NOT be double-buffered.
        let lines: Box<dyn Iterator<Item = std::io::Result<String>>> = if has_gz_ext(path) {
            let reader = noodles_bgzf::io::Reader::new(file);
            Box::new(reader.lines())
        } else {
            let reader = std::io::BufReader::new(file);
            Box::new(reader.lines())
        };
        for line in lines {
            let line = line?;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 8 {
                continue;
            }
            let chrom = fields[0].to_string();
            let kind_str = fields[2].to_string();
            let start: u64 = fields[3]
                .parse()
                .map_err(|e| ViewerError::Parse(format!("start: {e}")))?;
            let end: u64 = fields[4]
                .parse()
                .map_err(|e| ViewerError::Parse(format!("end: {e}")))?;
            let strand = fields[6].chars().next();
            let attrs = if is_gtf {
                parse_gtf_attrs(fields.get(8).copied().unwrap_or(""))
            } else {
                parse_gff_attrs(fields.get(8).copied().unwrap_or(""))
            };
            let name = attrs
                .get("Name")
                .cloned()
                .or_else(|| attrs.get("gene_name").cloned())
                .or_else(|| attrs.get("ID").cloned())
                .or_else(|| attrs.get("gene_id").cloned())
                .or_else(|| attrs.get("transcript_id").cloned());
            idx.by_chrom
                .entry(chrom.clone())
                .or_default()
                .push(Feature {
                    chrom,
                    start,
                    end,
                    name,
                    strand,
                    kind: kind_str,
                    attrs,
                });
        }
        Ok(idx)
    }

    fn load_bed(path: &Path) -> Result<Self> {
        let mut idx = Self::default();
        let file = File::open(path)?;
        let lines: Box<dyn Iterator<Item = std::io::Result<String>>> = if has_gz_ext(path) {
            let reader = noodles_bgzf::io::Reader::new(file);
            Box::new(reader.lines())
        } else {
            let reader = std::io::BufReader::new(file);
            Box::new(reader.lines())
        };
        for line in lines {
            let line = line?;
            if line.is_empty()
                || line.starts_with('#')
                || line.starts_with("track")
                || line.starts_with("browser")
            {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 3 {
                continue;
            }
            let chrom = fields[0].to_string();
            let start: u64 = fields[1]
                .parse::<u64>()
                .map_err(|e| ViewerError::Parse(format!("bed start: {e}")))?
                + 1; // BED 0-based → internal 1-based inclusive
            let end: u64 = fields[2]
                .parse::<u64>()
                .map_err(|e| ViewerError::Parse(format!("bed end: {e}")))?;
            let name = fields.get(3).map(|s| s.to_string());
            let strand = fields.get(5).and_then(|s| s.chars().next());
            idx.by_chrom
                .entry(chrom.clone())
                .or_default()
                .push(Feature {
                    chrom,
                    start,
                    end,
                    name,
                    strand,
                    kind: "region".into(),
                    attrs: HashMap::new(),
                });
        }
        Ok(idx)
    }
}

fn has_gz_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s == "gz")
        .unwrap_or(false)
}

fn parse_gff_attrs(s: &str) -> HashMap<String, String> {
    s.split(';')
        .filter_map(|kv| {
            let mut it = kv.trim().splitn(2, '=');
            let k = it.next()?.trim().to_string();
            let v = it.next()?.trim().to_string();
            if k.is_empty() {
                None
            } else {
                Some((k, v))
            }
        })
        .collect()
}

fn parse_gtf_attrs(s: &str) -> HashMap<String, String> {
    s.split(';')
        .filter_map(|kv| {
            let kv = kv.trim();
            if kv.is_empty() {
                return None;
            }
            let mut it = kv.splitn(2, ' ');
            let k = it.next()?.trim().to_string();
            let v = it.next()?.trim().trim_matches('"').to_string();
            Some((k, v))
        })
        .collect()
}

pub fn file_is_large(path: &Path) -> Result<bool> {
    let meta = std::fs::metadata(path)?;
    Ok(meta.len() > MEMORY_INDEX_MAX_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn gff() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gff3")
    }
    fn gtf() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gtf")
    }
    fn bed() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.bed")
    }

    #[test]
    fn loads_gff3_and_counts_features() {
        let idx = MemoryIndex::load(&gff(), TrackKind::Gff).unwrap();
        assert_eq!(idx.feature_count(), 8);
    }

    #[test]
    fn gff_query_returns_overlap() {
        let idx = MemoryIndex::load(&gff(), TrackKind::Gff).unwrap();
        let hits = idx.query("chr1", 20, 40);
        // gene (10-50), mRNA (10-50), exon1 (10-25), exon2 (35-50)
        assert_eq!(hits.len(), 4);
    }

    #[test]
    fn gff_query_different_chrom_empty() {
        let idx = MemoryIndex::load(&gff(), TrackKind::Gff).unwrap();
        assert!(idx.query("chr1", 200, 300).is_empty());
        assert!(idx.query("chrX", 1, 1000).is_empty());
    }

    #[test]
    fn gtf_attrs_parsed() {
        let idx = MemoryIndex::load(&gtf(), TrackKind::Gtf).unwrap();
        let feats: Vec<&Feature> = idx.all_features().collect();
        let gene = feats
            .iter()
            .find(|f| f.kind == "gene" && f.chrom == "chr1")
            .unwrap();
        assert_eq!(gene.attrs.get("gene_id").map(|s| s.as_str()), Some("gene1"));
        assert_eq!(gene.name.as_deref(), Some("BRCA1-like"));
    }

    #[test]
    fn bed_converted_to_one_based() {
        let idx = MemoryIndex::load(&bed(), TrackKind::Bed).unwrap();
        let peak1 = idx
            .all_features()
            .find(|f| f.name.as_deref() == Some("peak1"))
            .unwrap();
        assert_eq!(peak1.start, 11); // BED 10 → internal 11 (1-based)
        assert_eq!(peak1.end, 50);
    }
}

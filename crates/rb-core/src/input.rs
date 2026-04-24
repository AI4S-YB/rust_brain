use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    Fastq,
    Fasta,
    Gtf,
    Gff,
    CountsMatrix,
    SampleSheet,
    Other,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InputRecord {
    pub id: String,
    pub path: PathBuf,
    pub display_name: String,
    pub kind: InputKind,
    pub size_bytes: u64,
    pub registered_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paired_with: Option<String>,
    #[serde(default)]
    pub missing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct InputPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<InputKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct InputScanReport {
    pub refreshed: u32,
    pub now_missing: u32,
    pub recovered: u32,
}

pub fn new_input_id() -> String {
    let short = Uuid::new_v4().to_string()[..8].to_string();
    format!("in_{}", short)
}

/// Infer the logical kind of an input file from its file name / extension.
/// `.gz` and `.bz2` suffixes are stripped before checking.
pub fn detect_kind(path: &Path) -> InputKind {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut stripped = name.as_str();
    for suf in [".gz", ".bz2", ".xz", ".zst"] {
        if let Some(rest) = stripped.strip_suffix(suf) {
            stripped = rest;
        }
    }
    if stripped.ends_with(".fastq") || stripped.ends_with(".fq") {
        InputKind::Fastq
    } else if stripped.ends_with(".fasta")
        || stripped.ends_with(".fa")
        || stripped.ends_with(".fna")
    {
        InputKind::Fasta
    } else if stripped.ends_with(".gtf") {
        InputKind::Gtf
    } else if stripped.ends_with(".gff") || stripped.ends_with(".gff3") {
        InputKind::Gff
    } else if stripped.ends_with(".tsv") || stripped.ends_with(".csv") {
        // Users often upload counts matrices as TSV/CSV; they can correct to
        // SampleSheet via update_input when the file is a sample sheet.
        InputKind::CountsMatrix
    } else {
        InputKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(s: &str) -> InputKind {
        detect_kind(Path::new(s))
    }

    #[test]
    fn detects_fastq_variants() {
        assert_eq!(kind("sample.fastq"), InputKind::Fastq);
        assert_eq!(kind("sample.fq"), InputKind::Fastq);
        assert_eq!(kind("sample_R1.fastq.gz"), InputKind::Fastq);
        assert_eq!(kind("/abs/path/reads.fq.bz2"), InputKind::Fastq);
    }

    #[test]
    fn detects_fasta_variants() {
        assert_eq!(kind("genome.fa"), InputKind::Fasta);
        assert_eq!(kind("genome.fasta"), InputKind::Fasta);
        assert_eq!(kind("genome.fna.gz"), InputKind::Fasta);
    }

    #[test]
    fn detects_annotations() {
        assert_eq!(kind("anno.gtf"), InputKind::Gtf);
        assert_eq!(kind("anno.gff"), InputKind::Gff);
        assert_eq!(kind("anno.gff3"), InputKind::Gff);
        assert_eq!(kind("anno.gff3.gz"), InputKind::Gff);
    }

    #[test]
    fn defaults_tabular_to_counts_matrix() {
        assert_eq!(kind("counts.tsv"), InputKind::CountsMatrix);
        assert_eq!(kind("counts.csv"), InputKind::CountsMatrix);
    }

    #[test]
    fn unknown_extensions_are_other() {
        assert_eq!(kind("mystery.bin"), InputKind::Other);
        assert_eq!(kind(""), InputKind::Other);
        assert_eq!(kind("README"), InputKind::Other);
    }

    #[test]
    fn new_id_has_prefix() {
        let id = new_input_id();
        assert!(id.starts_with("in_"));
        assert_eq!(id.len(), "in_".len() + 8);
    }
}

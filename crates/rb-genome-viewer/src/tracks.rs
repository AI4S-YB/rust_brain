use crate::error::{Result, ViewerError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub type TrackId = String;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Gff,
    Gtf,
    Bed,
    // L2 — reserved:
    // Bam,
}

impl TrackKind {
    pub fn detect(path: &Path, hint: Option<&str>) -> Result<Self> {
        if let Some(h) = hint {
            return match h.to_lowercase().as_str() {
                "gff" | "gff3" => Ok(Self::Gff),
                "gtf" => Ok(Self::Gtf),
                "bed" => Ok(Self::Bed),
                "bam" | "cram" => Err(ViewerError::UnsupportedKind(
                    "BAM/CRAM alignment tracks arrive in L2".into(),
                )),
                other => Err(ViewerError::UnsupportedKind(other.into())),
            };
        }
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        // Also handle `.gff3.gz`, `.gtf.gz`, `.bed.gz` by stripping .gz first.
        let effective = if ext == "gz" {
            path.file_stem()
                .and_then(|s| Path::new(s).extension())
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_default()
        } else {
            ext
        };
        match effective.as_str() {
            "gff" | "gff3" => Ok(Self::Gff),
            "gtf" => Ok(Self::Gtf),
            "bed" => Ok(Self::Bed),
            "bam" | "cram" => Err(ViewerError::UnsupportedKind(
                "BAM/CRAM alignment tracks arrive in L2".into(),
            )),
            other => Err(ViewerError::UnsupportedKind(other.into())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMeta {
    pub track_id: TrackId,
    pub kind: TrackKind,
    pub path: PathBuf,
    pub source: TrackSource,
    pub feature_count: usize,
    pub suggest_bgzip: bool,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackSource {
    Memory,
    Tabix,
}

pub fn new_track_id() -> TrackId {
    Uuid::new_v4().simple().to_string()[..12].to_string()
}

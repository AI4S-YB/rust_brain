use crate::error::Result;
use crate::index::MemoryIndex;
use crate::reference::{ReferenceHandle, ReferenceMeta};
use crate::search::SearchIndex;
use crate::tracks::{TrackId, TrackKind, TrackMeta};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenomicRegion {
    pub chrom: String,
    pub start: u64,
    pub end: u64,
}

#[derive(Default)]
pub struct GenomeSession {
    pub reference: Option<ReferenceHandle>,
    pub reference_meta: Option<ReferenceMeta>,
    pub tracks: HashMap<TrackId, TrackRuntime>,
    pub search: SearchIndex,
    pub position: Option<GenomicRegion>,
}

pub struct TrackRuntime {
    pub meta: TrackMeta,
    pub memory: Option<Arc<MemoryIndex>>,
    // tabix reader reconstructed on demand; see commands.rs fetch_track_features
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedSession {
    pub version: u32,
    pub reference: Option<SerializedReference>,
    pub tracks: Vec<SerializedTrack>,
    pub position: Option<GenomicRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedReference {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedTrack {
    pub path: PathBuf,
    pub kind: TrackKind,
    pub visible: bool,
}

impl GenomeSession {
    pub fn serialize(&self) -> SerializedSession {
        SerializedSession {
            version: 1,
            reference: self.reference_meta.as_ref().map(|m| SerializedReference {
                path: m.path.clone(),
            }),
            tracks: self
                .tracks
                .values()
                .map(|t| SerializedTrack {
                    path: t.meta.path.clone(),
                    kind: t.meta.kind,
                    visible: t.meta.visible,
                })
                .collect(),
            position: self.position.clone(),
        }
    }
}

pub fn load_session_from_disk(path: &Path) -> Result<Option<SerializedSession>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    let s: SerializedSession = serde_json::from_slice(&bytes)
        .map_err(|e| crate::error::ViewerError::Parse(format!("session parse: {e}")))?;
    Ok(Some(s))
}

pub fn save_session_to_disk(path: &Path, s: &SerializedSession) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(s)
        .map_err(|e| crate::error::ViewerError::Parse(format!("serde: {e}")))?;
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_empty_session() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("session.json");
        let s = SerializedSession {
            version: 1,
            reference: None,
            tracks: vec![],
            position: None,
        };
        save_session_to_disk(&p, &s).unwrap();
        let loaded = load_session_from_disk(&p).unwrap().unwrap();
        assert_eq!(loaded.version, 1);
        assert!(loaded.reference.is_none());
    }

    #[test]
    fn round_trip_with_position() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("session.json");
        let s = SerializedSession {
            version: 1,
            reference: Some(SerializedReference {
                path: PathBuf::from("/x/y.fa"),
            }),
            tracks: vec![SerializedTrack {
                path: PathBuf::from("/x/y.gff"),
                kind: TrackKind::Gff,
                visible: true,
            }],
            position: Some(GenomicRegion {
                chrom: "chr1".into(),
                start: 100,
                end: 200,
            }),
        };
        save_session_to_disk(&p, &s).unwrap();
        let loaded = load_session_from_disk(&p).unwrap().unwrap();
        assert_eq!(loaded.tracks.len(), 1);
        assert!(matches!(loaded.tracks[0].kind, TrackKind::Gff));
        let pos = loaded.position.unwrap();
        assert_eq!(pos.start, 100);
    }

    #[test]
    fn missing_file_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("nope.json");
        assert!(load_session_from_disk(&p).unwrap().is_none());
    }
}

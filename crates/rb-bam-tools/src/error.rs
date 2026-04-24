use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum BamToolsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("file not found: {0}")]
    NotFound(PathBuf),
    #[error("invalid region: {0}")]
    InvalidRegion(String),
    #[error("reference sequence not found in BAM header: {0}")]
    UnknownReference(String),
    #[error("BAM is not coordinate-sorted; sort before indexing")]
    NotSorted,
    #[error("index not found next to {0}; build an index first")]
    IndexMissing(PathBuf),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, BamToolsError>;

#[derive(Debug, Serialize)]
struct SerializedError {
    code: String,
    message: String,
    path: Option<PathBuf>,
}

impl BamToolsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::NotFound(_) => "not_found",
            Self::InvalidRegion(_) => "invalid_region",
            Self::UnknownReference(_) => "unknown_reference",
            Self::NotSorted => "not_sorted",
            Self::IndexMissing(_) => "index_missing",
            Self::Other(_) => "other",
        }
    }
}

impl Serialize for BamToolsError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let path = match self {
            Self::NotFound(p) | Self::IndexMissing(p) => Some(p.clone()),
            _ => None,
        };
        SerializedError {
            code: self.code().to_string(),
            message: self.to_string(),
            path,
        }
        .serialize(s)
    }
}

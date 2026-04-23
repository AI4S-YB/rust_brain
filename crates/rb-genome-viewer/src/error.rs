use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ViewerError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("file not found: {0}")]
    NotFound(PathBuf),
    #[error("unsupported format: {0}")]
    UnsupportedKind(String),
    #[error("track not found: {0}")]
    TrackNotFound(String),
    #[error("no reference loaded")]
    NoReference,
    #[error("index build failed: {0}")]
    IndexBuildFailed(String),
}

#[derive(Serialize)]
struct SerializedError {
    code: String,
    message: String,
    path: Option<PathBuf>,
}

impl ViewerError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Parse(_) => "parse",
            Self::NotFound(_) => "not_found",
            Self::UnsupportedKind(_) => "unsupported_kind",
            Self::TrackNotFound(_) => "track_not_found",
            Self::NoReference => "no_reference",
            Self::IndexBuildFailed(_) => "index_build_failed",
        }
    }
}

impl Serialize for ViewerError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let path = match self {
            Self::NotFound(p) => Some(p.clone()),
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

pub type Result<T> = std::result::Result<T, ViewerError>;

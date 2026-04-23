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
    #[error("out of range: requested record {requested}, total {total}")]
    OutOfRange { requested: usize, total: usize },
    #[error("index corrupt: {0}")]
    IndexCorrupt(String),
    #[error("bincode error: {0}")]
    Bincode(#[from] bincode::Error),
}

#[derive(Debug, Serialize)]
pub struct SerializedError {
    pub code: String,
    pub message: String,
    pub path: Option<PathBuf>,
}

impl ViewerError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Parse(_) => "parse",
            Self::NotFound(_) => "not_found",
            Self::OutOfRange { .. } => "out_of_range",
            Self::IndexCorrupt(_) => "index_corrupt",
            Self::Bincode(_) => "index_corrupt",
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

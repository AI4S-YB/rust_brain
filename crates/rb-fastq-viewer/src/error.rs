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
    #[error("no file open")]
    NoSession,
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
            Self::NoSession => "no_session",
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

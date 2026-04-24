use crate::error::BamToolsError;
use crate::extract::{extract_region, header_references, ReferenceEntry};
use crate::index::{bai_path, index_bam};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct IndexResult {
    pub bam: PathBuf,
    pub bai: PathBuf,
}

#[derive(Serialize)]
pub struct ExtractResult {
    pub output: PathBuf,
    pub records_written: usize,
}

#[tauri::command]
pub async fn bam_tools_index(path: PathBuf) -> std::result::Result<IndexResult, BamToolsError> {
    tokio::task::spawn_blocking(move || {
        let bai = index_bam(&path)?;
        Ok(IndexResult { bam: path, bai })
    })
    .await
    .map_err(|e| BamToolsError::Other(format!("join: {e}")))?
}

#[tauri::command]
pub async fn bam_tools_index_status(path: PathBuf) -> std::result::Result<bool, BamToolsError> {
    Ok(bai_path(&path).exists())
}

#[tauri::command]
pub async fn bam_tools_header_references(
    path: PathBuf,
) -> std::result::Result<Vec<ReferenceEntry>, BamToolsError> {
    tokio::task::spawn_blocking(move || header_references(&path))
        .await
        .map_err(|e| BamToolsError::Other(format!("join: {e}")))?
}

#[tauri::command]
pub async fn bam_tools_extract_region(
    path: PathBuf,
    region: String,
    output: PathBuf,
) -> std::result::Result<ExtractResult, BamToolsError> {
    tokio::task::spawn_blocking(move || {
        let n = extract_region(&path, &region, &output)?;
        Ok(ExtractResult {
            output,
            records_written: n,
        })
    })
    .await
    .map_err(|e| BamToolsError::Other(format!("join: {e}")))?
}

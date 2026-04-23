use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct OpenResultStub {
    pub placeholder: bool,
}

#[tauri::command]
pub async fn fastq_viewer_open(_path: PathBuf) -> std::result::Result<OpenResultStub, String> {
    Err("not implemented yet (Task 9)".into())
}

#[tauri::command]
pub async fn fastq_viewer_read_records(
    _start_record: usize,
    _count: usize,
) -> std::result::Result<Vec<String>, String> {
    Err("not implemented yet (Task 9)".into())
}

#[tauri::command]
pub async fn fastq_viewer_seek_percent(_pct: f32) -> std::result::Result<usize, String> {
    Err("not implemented yet (Task 9)".into())
}

#[tauri::command]
pub async fn fastq_viewer_search_id(
    _query: String,
    _from_record: usize,
    _limit: usize,
) -> std::result::Result<Vec<String>, String> {
    Err("not implemented yet (Task 9)".into())
}

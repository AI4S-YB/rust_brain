use crate::error::{Result, ViewerError};
use crate::session::{FastqRecord, FastqSession, OpenResult};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Default)]
pub struct FastqState {
    pub session: Mutex<Option<Arc<FastqSession>>>,
}

fn cache_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf> {
    let base = app
        .path()
        .app_cache_dir()
        .map_err(|e| ViewerError::Parse(format!("app_cache_dir: {e}")))?;
    Ok(base.join("fastq_idx"))
}

fn ensure_state<R: Runtime>(app: &AppHandle<R>) -> Arc<FastqState> {
    if app.try_state::<Arc<FastqState>>().is_none() {
        app.manage(Arc::new(FastqState::default()));
    }
    app.state::<Arc<FastqState>>().inner().clone()
}

#[tauri::command]
pub async fn fastq_viewer_open<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
) -> std::result::Result<OpenResult, ViewerError> {
    let cd = cache_dir(&app)?;
    let state = ensure_state(&app);
    tokio::task::spawn_blocking(move || {
        let (session, cached) = FastqSession::open(&path, &cd)?;
        let result = OpenResult {
            total_records: session.index.total_records,
            index_cached: cached,
            path: session.path.clone(),
        };
        *state.session.lock().unwrap() = Some(Arc::new(session));
        Ok::<_, ViewerError>(result)
    })
    .await
    .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

#[tauri::command]
pub async fn fastq_viewer_read_records<R: Runtime>(
    app: AppHandle<R>,
    start_record: usize,
    count: usize,
) -> std::result::Result<Vec<FastqRecord>, ViewerError> {
    let state = ensure_state(&app);
    let session = state
        .session
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| ViewerError::Parse("no file open".into()))?;
    tokio::task::spawn_blocking(move || session.read_records(start_record, count))
        .await
        .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

#[tauri::command]
pub async fn fastq_viewer_seek_percent<R: Runtime>(
    app: AppHandle<R>,
    pct: f32,
) -> std::result::Result<usize, ViewerError> {
    let state = ensure_state(&app);
    let session = state
        .session
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| ViewerError::Parse("no file open".into()))?;
    Ok(session.seek_percent(pct))
}

#[derive(Serialize)]
pub struct SearchHit {
    pub record_n: usize,
    pub id: String,
}

#[tauri::command]
pub async fn fastq_viewer_search_id<R: Runtime>(
    app: AppHandle<R>,
    query: String,
    from_record: usize,
    limit: usize,
) -> std::result::Result<Vec<SearchHit>, ViewerError> {
    let state = ensure_state(&app);
    let session = state
        .session
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| ViewerError::Parse("no file open".into()))?;
    tokio::task::spawn_blocking(move || {
        let hits = session.search_id(&query, from_record, limit)?;
        Ok::<_, ViewerError>(
            hits.into_iter()
                .map(|(record_n, id)| SearchHit { record_n, id })
                .collect(),
        )
    })
    .await
    .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

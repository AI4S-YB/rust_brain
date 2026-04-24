use crate::error::ViewerError;
use crate::session::{FastqSession, OpenResult, ReadResult, SearchResult, Status};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Default)]
pub struct FastqState {
    pub session: Mutex<Option<Arc<FastqSession>>>,
}

fn ensure_state<R: Runtime>(app: &AppHandle<R>) -> Arc<FastqState> {
    if app.try_state::<Arc<FastqState>>().is_none() {
        app.manage(Arc::new(FastqState::default()));
    }
    app.state::<Arc<FastqState>>().inner().clone()
}

fn current_session<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<FastqSession>, ViewerError> {
    let state = ensure_state(app);
    let guard = state.session.lock().unwrap();
    guard.clone().ok_or(ViewerError::NoSession)
}

#[tauri::command]
pub async fn fastq_viewer_open<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
) -> std::result::Result<OpenResult, ViewerError> {
    let state = ensure_state(&app);
    tokio::task::spawn_blocking(move || {
        let session = FastqSession::open(&path)?;
        let result = OpenResult {
            path: session.path.clone(),
            is_gzip: session.is_gzip,
            total_bytes: session.total_bytes,
        };
        *state.session.lock().unwrap() = Some(Arc::new(session));
        Ok::<_, ViewerError>(result)
    })
    .await
    .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

#[tauri::command]
pub async fn fastq_viewer_close<R: Runtime>(
    app: AppHandle<R>,
) -> std::result::Result<(), ViewerError> {
    let state = ensure_state(&app);
    *state.session.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
pub async fn fastq_viewer_status<R: Runtime>(
    app: AppHandle<R>,
) -> std::result::Result<Status, ViewerError> {
    let session = current_session(&app)?;
    Ok(session.status())
}

#[tauri::command]
pub async fn fastq_viewer_read<R: Runtime>(
    app: AppHandle<R>,
    from: usize,
    count: usize,
) -> std::result::Result<ReadResult, ViewerError> {
    let session = current_session(&app)?;
    tokio::task::spawn_blocking(move || session.read(from, count))
        .await
        .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

#[tauri::command]
pub async fn fastq_viewer_search_id<R: Runtime>(
    app: AppHandle<R>,
    query: String,
    from: usize,
    limit: usize,
    max_scan: Option<usize>,
) -> std::result::Result<SearchResult, ViewerError> {
    let session = current_session(&app)?;
    let max = max_scan.unwrap_or(200_000);
    tokio::task::spawn_blocking(move || session.search_id(&query, from, limit, max))
        .await
        .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

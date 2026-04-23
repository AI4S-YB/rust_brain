use crate::bgzip;
use crate::error::ViewerError;
use crate::index::{file_is_large, Feature, MemoryIndex};
use crate::reference::ReferenceMeta;
use crate::search::SearchEntry;
use crate::session::{
    load_session_from_disk, save_session_to_disk, GenomeSession, SerializedSession, TrackRuntime,
};
use crate::tracks::{new_track_id, TrackId, TrackKind, TrackMeta, TrackSource};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

#[derive(Default)]
pub struct GenomeState {
    pub session: Mutex<GenomeSession>,
}

fn ensure_state<R: Runtime>(app: &AppHandle<R>) -> Arc<GenomeState> {
    if app.try_state::<Arc<GenomeState>>().is_none() {
        app.manage(Arc::new(GenomeState::default()));
    }
    app.state::<Arc<GenomeState>>().inner().clone()
}

fn session_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, ViewerError> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| ViewerError::Parse(format!("app_data_dir: {e}")))?;
    Ok(base.join("genome_viewer_session.json"))
}

#[tauri::command]
pub async fn genome_viewer_load_reference<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
) -> std::result::Result<ReferenceMeta, ViewerError> {
    let state = ensure_state(&app);
    tokio::task::spawn_blocking(move || {
        let (handle, meta) = crate::reference::ReferenceHandle::load(&path)?;
        let mut s = state.session.lock().unwrap();
        s.reference = Some(handle);
        s.reference_meta = Some(meta.clone());
        Ok::<_, ViewerError>(meta)
    })
    .await
    .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

#[tauri::command]
pub async fn genome_viewer_add_track<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
    kind_hint: Option<String>,
) -> std::result::Result<TrackMeta, ViewerError> {
    let state = ensure_state(&app);
    tokio::task::spawn_blocking(move || {
        let kind = TrackKind::detect(&path, kind_hint.as_deref())?;
        if !path.exists() {
            return Err(ViewerError::NotFound(path));
        }
        let large = file_is_large(&path)?;
        // L1: always memory index unless file is already bgzipped (tabix path wired in a future task).
        let mem = MemoryIndex::load(&path, kind)?;
        let feature_count = mem.feature_count();
        let track_id = new_track_id();
        let meta = TrackMeta {
            track_id: track_id.clone(),
            kind,
            path: path.clone(),
            source: TrackSource::Memory,
            feature_count,
            suggest_bgzip: large,
            visible: true,
        };
        let mem = Arc::new(mem);
        {
            let mut s = state.session.lock().unwrap();
            s.search.add_track(&track_id, &mem);
            s.tracks.insert(
                track_id.clone(),
                TrackRuntime {
                    meta: meta.clone(),
                    memory: Some(mem),
                },
            );
        }
        Ok::<_, ViewerError>(meta)
    })
    .await
    .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

#[tauri::command]
pub async fn genome_viewer_remove_track<R: Runtime>(
    app: AppHandle<R>,
    track_id: TrackId,
) -> std::result::Result<(), ViewerError> {
    let state = ensure_state(&app);
    let mut s = state.session.lock().unwrap();
    s.tracks.remove(&track_id);
    s.search.remove_track(&track_id);
    Ok(())
}

#[tauri::command]
pub async fn genome_viewer_list_tracks<R: Runtime>(
    app: AppHandle<R>,
) -> std::result::Result<Vec<TrackMeta>, ViewerError> {
    let state = ensure_state(&app);
    let s = state.session.lock().unwrap();
    Ok(s.tracks.values().map(|t| t.meta.clone()).collect())
}

#[tauri::command]
pub async fn genome_viewer_fetch_reference_region<R: Runtime>(
    app: AppHandle<R>,
    chrom: String,
    start: u64,
    end: u64,
) -> std::result::Result<String, ViewerError> {
    let state = ensure_state(&app);
    let s = state.session.lock().unwrap();
    let handle = s.reference.as_ref().ok_or(ViewerError::NoReference)?;
    handle.fetch_region(&chrom, start, end)
}

#[tauri::command]
pub async fn genome_viewer_fetch_track_features<R: Runtime>(
    app: AppHandle<R>,
    track_id: TrackId,
    chrom: String,
    start: u64,
    end: u64,
) -> std::result::Result<Vec<Feature>, ViewerError> {
    let state = ensure_state(&app);
    let s = state.session.lock().unwrap();
    let track = s
        .tracks
        .get(&track_id)
        .ok_or_else(|| ViewerError::TrackNotFound(track_id.clone()))?;
    let mem = track.memory.as_ref().ok_or_else(|| {
        ViewerError::Parse("track has no memory index (tabix not yet wired)".into())
    })?;
    Ok(mem.query(&chrom, start, end).into_iter().cloned().collect())
}

#[tauri::command]
pub async fn genome_viewer_search_feature<R: Runtime>(
    app: AppHandle<R>,
    query: String,
    limit: usize,
) -> std::result::Result<Vec<SearchEntry>, ViewerError> {
    let state = ensure_state(&app);
    let s = state.session.lock().unwrap();
    Ok(s.search
        .search(&query, limit)
        .into_iter()
        .cloned()
        .collect())
}

#[derive(Serialize)]
pub struct BgzipResult {
    pub new_path: PathBuf,
}

#[tauri::command]
pub async fn genome_viewer_bgzip_and_tabix<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
) -> std::result::Result<BgzipResult, ViewerError> {
    let kind = TrackKind::detect(&path, None)?;
    let app_for_emit = app.clone();
    let p = path.clone();
    let new_path = tokio::task::spawn_blocking(move || {
        bgzip::bgzip_and_tabix(&p, kind, |done, total| {
            let _ = app_for_emit.emit(
                "genome_viewer_index_progress",
                serde_json::json!({ "path": p.clone(), "done": done, "total": total }),
            );
        })
    })
    .await
    .map_err(|e| ViewerError::Parse(format!("join: {e}")))??;
    Ok(BgzipResult { new_path })
}

#[tauri::command]
pub async fn genome_viewer_get_session_state<R: Runtime>(
    app: AppHandle<R>,
) -> std::result::Result<Option<SerializedSession>, ViewerError> {
    let p = session_path(&app)?;
    load_session_from_disk(&p)
}

#[tauri::command]
pub async fn genome_viewer_save_session_state<R: Runtime>(
    app: AppHandle<R>,
    state: SerializedSession,
) -> std::result::Result<(), ViewerError> {
    let p = session_path(&app)?;
    save_session_to_disk(&p, &state)?;
    Ok(())
}

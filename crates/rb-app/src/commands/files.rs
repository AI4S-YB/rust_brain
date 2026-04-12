use serde::Serialize;

#[derive(Serialize)]
pub struct TablePreview {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[tauri::command]
pub async fn select_files(filters: Option<String>) -> Result<Vec<String>, String> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(filter_str) = filters {
        dialog = dialog.add_filter("Files", &[filter_str.as_str()]);
    }
    let paths = dialog.pick_files();
    Ok(paths
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

#[tauri::command]
pub async fn select_directory() -> Result<Option<String>, String> {
    let path = rfd::FileDialog::new().pick_folder();
    Ok(path.map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn read_table_preview(path: String, n_rows: usize) -> Result<TablePreview, String> {
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut lines = content.lines();

    let headers = lines
        .next()
        .map(|line| line.split('\t').map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let rows: Vec<Vec<String>> = lines
        .take(n_rows)
        .map(|line| line.split('\t').map(|s| s.to_string()).collect())
        .collect();

    Ok(TablePreview { headers, rows })
}

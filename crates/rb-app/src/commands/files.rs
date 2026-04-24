use serde::Serialize;

#[derive(Serialize)]
pub struct TablePreview {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

fn split_tsv_row(line: &str, col_limit: usize) -> Vec<String> {
    line.split('\t')
        .take(col_limit)
        .map(|s| s.to_string())
        .collect()
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
pub async fn read_table_preview(
    path: String,
    n_rows: Option<usize>,
    max_rows: Option<usize>,
    max_cols: Option<usize>,
    has_header: Option<bool>,
) -> Result<TablePreview, String> {
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut lines = content.lines();
    let row_limit = max_rows.or(n_rows).unwrap_or(50);
    let col_limit = max_cols.unwrap_or(usize::MAX);

    let headers = if has_header.unwrap_or(true) {
        lines
            .next()
            .map(|line| split_tsv_row(line, col_limit))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let rows: Vec<Vec<String>> = lines
        .take(row_limit)
        .map(|line| split_tsv_row(line, col_limit))
        .collect();

    Ok(TablePreview { headers, rows })
}

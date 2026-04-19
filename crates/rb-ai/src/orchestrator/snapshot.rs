use std::sync::Arc;

use rb_core::project::Project;
use tokio::sync::Mutex;

/// Build a compact, LLM-friendly summary of project state.
/// Target ≤ ~500 tokens (~2000 chars). Keeps system-prompt overhead bounded
/// even for very active projects.
pub async fn build(project: &Arc<Mutex<Project>>) -> String {
    let (name, default_view, created_at, root, runs_tail) = {
        let proj = project.lock().await;
        let runs_tail: Vec<_> = proj
            .runs
            .iter()
            .rev()
            .take(10)
            .map(|r| {
                (
                    r.id.clone(),
                    format!("{:?}", r.status),
                    r.finished_at
                        .map(|d| d.format("%H:%M").to_string())
                        .unwrap_or_else(|| "-".into()),
                )
            })
            .collect();
        (
            proj.name.clone(),
            proj.default_view.clone().unwrap_or_else(|| "manual".into()),
            proj.created_at,
            proj.root_dir.clone(),
            runs_tail,
        )
    };

    let mut out = String::new();
    out.push_str(&format!("Project: {name}\n"));
    out.push_str(&format!("Default view: {default_view}\n"));
    out.push_str(&format!("Created: {}\n\n", created_at.format("%Y-%m-%d")));

    out.push_str("Top-level files:\n");
    match std::fs::read_dir(&root) {
        Ok(rd) => {
            let mut shown = 0;
            for ent in rd.flatten() {
                if shown >= 20 {
                    out.push_str("  ...\n");
                    break;
                }
                let name = ent.file_name().to_string_lossy().to_string();
                let kind = match ent.file_type() {
                    Ok(t) if t.is_dir() => "/",
                    _ => "",
                };
                out.push_str(&format!("  {name}{kind}\n"));
                shown += 1;
            }
        }
        Err(e) => {
            out.push_str(&format!("  (read_dir failed: {e})\n"));
        }
    }
    out.push('\n');

    out.push_str("Recent runs:\n");
    if runs_tail.is_empty() {
        out.push_str("  (none yet)\n");
    } else {
        for (id, status, finished) in runs_tail {
            out.push_str(&format!("  {id}: {status} {finished}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rb_core::project::Project;
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn snapshot_includes_project_name_and_runs_header() {
        let tmp = tempdir().unwrap();
        let project = Arc::new(Mutex::new(Project::create("demo", tmp.path()).unwrap()));
        let s = build(&project).await;
        assert!(s.contains("Project: demo"));
        assert!(s.contains("Recent runs:"));
        assert!(s.contains("(none yet)"));
    }

    #[tokio::test]
    async fn snapshot_lists_top_level_entries_and_marks_dirs() {
        let tmp = tempdir().unwrap();
        let project = Arc::new(Mutex::new(Project::create("t", tmp.path()).unwrap()));
        std::fs::write(tmp.path().join("data.fastq.gz"), b"").unwrap();
        std::fs::create_dir_all(tmp.path().join("refs")).unwrap();
        let s = build(&project).await;
        assert!(s.contains("data.fastq.gz"));
        assert!(
            s.contains("refs/"),
            "directories must carry a trailing slash"
        );
    }
}

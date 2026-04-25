use std::sync::Arc;

use rb_core::project::Project;
use tokio::sync::Mutex;

/// Build a compact, LLM-friendly summary of project state.
/// Target ≤ ~500 tokens (~2000 chars). Keeps system-prompt overhead bounded
/// even for very active projects.
pub async fn build(project: &Arc<Mutex<Project>>) -> String {
    let (name, default_view, created_at, root, inputs, samples, assets, runs_tail) = {
        let proj = project.lock().await;
        let runs_tail: Vec<_> = proj
            .runs
            .iter()
            .rev()
            .take(10)
            .map(|r| {
                (
                    r.id.clone(),
                    r.module_id.clone(),
                    format!("{:?}", r.status),
                    r.finished_at
                        .map(|d| d.format("%H:%M").to_string())
                        .unwrap_or_else(|| "-".into()),
                    r.error.clone(),
                )
            })
            .collect();
        let inputs: Vec<_> = proj
            .inputs
            .iter()
            .take(8)
            .map(|i| {
                (
                    i.id.clone(),
                    i.display_name.clone(),
                    format!("{:?}", i.kind),
                    i.missing,
                    i.sample_id.clone(),
                )
            })
            .collect();
        let samples: Vec<_> = proj
            .samples
            .iter()
            .take(8)
            .map(|s| {
                (
                    s.id.clone(),
                    s.name.clone(),
                    s.group.clone(),
                    s.condition.clone(),
                    s.paired,
                    s.inputs.len(),
                )
            })
            .collect();
        let assets: Vec<_> = proj
            .assets
            .iter()
            .rev()
            .take(8)
            .map(|a| {
                (
                    a.id.clone(),
                    a.display_name.clone(),
                    format!("{:?}", a.kind),
                    a.produced_by_run_id.clone(),
                )
            })
            .collect();
        (
            proj.name.clone(),
            proj.default_view.clone().unwrap_or_else(|| "manual".into()),
            proj.created_at,
            proj.root_dir.clone(),
            inputs,
            samples,
            assets,
            runs_tail,
        )
    };

    let mut out = String::new();
    out.push_str(&format!("Project: {name}\n"));
    out.push_str(&format!("Default view: {default_view}\n"));
    out.push_str(&format!("Created: {}\n\n", created_at.format("%Y-%m-%d")));

    out.push_str("Registered inputs:\n");
    if inputs.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for (id, name, kind, missing, sample_id) in inputs {
            let missing = if missing { " missing" } else { "" };
            let sample = sample_id
                .map(|s| format!(" sample={s}"))
                .unwrap_or_default();
            out.push_str(&format!("  {id}: {name} [{kind}]{missing}{sample}\n"));
        }
    }
    out.push('\n');

    out.push_str("Samples:\n");
    if samples.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for (id, name, group, condition, paired, input_count) in samples {
            let group = group.map(|s| format!(" group={s}")).unwrap_or_default();
            let condition = condition
                .map(|s| format!(" condition={s}"))
                .unwrap_or_default();
            let layout = if paired { "paired" } else { "single" };
            out.push_str(&format!(
                "  {id}: {name} {layout} inputs={input_count}{group}{condition}\n"
            ));
        }
    }
    out.push('\n');

    out.push_str("Derived assets:\n");
    if assets.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for (id, name, kind, run_id) in assets {
            out.push_str(&format!("  {id}: {name} [{kind}] from {run_id}\n"));
        }
    }
    out.push('\n');

    out.push_str("Top-level files:\n");
    match std::fs::read_dir(&root) {
        Ok(rd) => {
            let entries: Vec<_> = rd.flatten().take(21).collect();
            for ent in entries.iter().take(20) {
                let name = ent.file_name().to_string_lossy().to_string();
                let kind = match ent.file_type() {
                    Ok(t) if t.is_dir() => "/",
                    _ => "",
                };
                out.push_str(&format!("  {name}{kind}\n"));
            }
            if entries.len() > 20 {
                out.push_str("  ...\n");
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
        for (id, module_id, status, finished, error) in runs_tail {
            let error = error
                .map(|e| {
                    format!(
                        " error={}",
                        e.lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(80)
                            .collect::<String>()
                    )
                })
                .unwrap_or_default();
            out.push_str(&format!("  {id}: {module_id} {status} {finished}{error}\n"));
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

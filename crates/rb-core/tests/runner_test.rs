use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rb_core::module::{Module, ModuleError, ModuleResult, Progress, ValidationError};
use rb_core::project::{Project, RunStatus};
use rb_core::runner::Runner;
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::mpsc;

struct MockModule;

#[async_trait]
impl Module for MockModule {
    fn id(&self) -> &str {
        "mock"
    }

    fn name(&self) -> &str {
        "Mock Module"
    }

    fn validate(&self, _params: &Value) -> Vec<ValidationError> {
        vec![]
    }

    async fn run(
        &self,
        _params: &Value,
        _project_dir: &Path,
        progress_tx: mpsc::Sender<Progress>,
    ) -> Result<ModuleResult, ModuleError> {
        progress_tx
            .send(Progress {
                fraction: 0.5,
                message: "halfway".to_string(),
            })
            .await
            .ok();

        progress_tx
            .send(Progress {
                fraction: 1.0,
                message: "done".to_string(),
            })
            .await
            .ok();

        Ok(ModuleResult {
            output_files: vec![],
            summary: serde_json::json!({"test": true}),
            log: String::new(),
        })
    }
}

#[tokio::test]
async fn runner_executes_mock_module() {
    // 1. Create temp dir, create Project
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("test_project");
    let project = Project::create("Test Runner Project", &root).unwrap();

    // 2. Wrap project in Arc<Mutex>
    let project_arc = Arc::new(tokio::sync::Mutex::new(project));

    // Collect progress messages
    let progress_log: Arc<Mutex<Vec<(String, Progress)>>> = Arc::new(Mutex::new(vec![]));
    let progress_log_clone = Arc::clone(&progress_log);

    // 3. Create Runner with progress callback
    let runner =
        Runner::new(Arc::clone(&project_arc)).on_progress(Box::new(move |run_id, progress| {
            progress_log_clone
                .lock()
                .unwrap()
                .push((run_id.to_string(), progress));
        }));

    // 4. Spawn MockModule
    let module = Arc::new(MockModule);
    let params = serde_json::json!({});
    let run_id = runner
        .spawn(module, params)
        .await
        .expect("spawn should succeed");

    // 5. Wait ~200ms for completion
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // 6. Assert run record status is Done
    let proj = project_arc.lock().await;
    let run = proj
        .runs
        .iter()
        .find(|r| r.id == run_id)
        .expect("run record must exist");
    assert_eq!(
        run.status,
        RunStatus::Done,
        "run status should be Done, got {:?}",
        run.status
    );

    // 7. Assert result is Some
    assert!(run.result.is_some(), "run result should be populated");
    let result = run.result.as_ref().unwrap();
    assert_eq!(result.summary["test"], true);

    // Bonus: check progress messages were received
    let log = progress_log.lock().unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].1.fraction, 0.5);
    assert_eq!(log[0].1.message, "halfway");
    assert_eq!(log[1].1.fraction, 1.0);
    assert_eq!(log[1].1.message, "done");
}

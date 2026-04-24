use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};

use crate::cancel::CancellationToken;
use crate::module::{Module, ModuleResult, Progress};
use crate::project::{Project, RunStatus};
use crate::run_event::{LogStream, RunEvent};

pub type ProgressCallback = Box<dyn Fn(&str, Progress) + Send + Sync>;
pub type LogCallback = Box<dyn Fn(&str, String, LogStream) + Send + Sync>;
pub type CompletionCallback = Box<dyn Fn(&str, Result<ModuleResult, String>) + Send + Sync>;

struct ActiveRun {
    handle: tokio::task::JoinHandle<()>,
    cancel: CancellationToken,
}

pub struct Runner {
    project: Arc<Mutex<Project>>,
    on_progress: Option<Arc<ProgressCallback>>,
    on_log: Option<Arc<LogCallback>>,
    on_complete: Option<Arc<CompletionCallback>>,
    active_runs: Arc<Mutex<HashMap<String, ActiveRun>>>,
}

impl Runner {
    pub fn new(project: Arc<Mutex<Project>>) -> Self {
        Runner {
            project,
            on_progress: None,
            on_log: None,
            on_complete: None,
            active_runs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn on_progress(mut self, cb: ProgressCallback) -> Self {
        self.on_progress = Some(Arc::new(cb));
        self
    }

    pub fn on_log(mut self, cb: LogCallback) -> Self {
        self.on_log = Some(Arc::new(cb));
        self
    }

    pub fn on_complete(mut self, cb: CompletionCallback) -> Self {
        self.on_complete = Some(Arc::new(cb));
        self
    }

    pub fn project(&self) -> &Arc<Mutex<Project>> {
        &self.project
    }

    pub async fn spawn(&self, module: Arc<dyn Module>, params: Value) -> Result<String, String> {
        self.spawn_with_lineage(module, params, Vec::new(), Vec::new())
            .await
    }

    /// Like `spawn`, but also records which registered InputRecord ids and
    /// AssetRecord ids this run consumes. Used by the frontend when the user
    /// picks from the Inputs / Assets registry in a module form so the
    /// Tasks / Assets views can show lineage.
    pub async fn spawn_with_lineage(
        &self,
        module: Arc<dyn Module>,
        params: Value,
        inputs_used: Vec<String>,
        assets_used: Vec<String>,
    ) -> Result<String, String> {
        let run_id = {
            let mut proj = self.project.lock().await;
            proj.create_run(module.id(), params.clone()).id
        };

        {
            let mut proj = self.project.lock().await;
            if let Some(run) = proj.runs.iter_mut().find(|r| r.id == run_id) {
                run.status = RunStatus::Running;
                run.started_at = Some(Utc::now());
                run.inputs_used = inputs_used;
                run.assets_used = assets_used;
            }
            proj.save().map_err(|e| e.to_string())?;
        }

        let project_dir = {
            let proj = self.project.lock().await;
            proj.root_dir.clone()
        };

        let (events_tx, mut events_rx) = mpsc::channel::<RunEvent>(64);
        let cancel_token = CancellationToken::new();

        let project_arc = Arc::clone(&self.project);
        let active_runs_arc = Arc::clone(&self.active_runs);
        let on_progress_arc = self.on_progress.clone();
        let on_log_arc = self.on_log.clone();
        let on_complete_arc = self.on_complete.clone();
        let rid = run_id.clone();
        let rid_for_events = run_id.clone();
        let rid_for_complete = run_id.clone();
        let cancel_for_module = cancel_token.clone();

        // Event forwarding task: split RunEvent into progress vs log callbacks
        tokio::task::spawn(async move {
            while let Some(event) = events_rx.recv().await {
                match event {
                    RunEvent::Progress { fraction, message } => {
                        if let Some(cb) = &on_progress_arc {
                            cb(&rid_for_events, Progress { fraction, message });
                        }
                    }
                    RunEvent::Log { line, stream } => {
                        if let Some(cb) = &on_log_arc {
                            cb(&rid_for_events, line, stream);
                        }
                    }
                }
            }
        });

        let handle = tokio::task::spawn(async move {
            let run_dir = {
                let proj = project_arc.lock().await;
                proj.run_dir(&rid).unwrap_or_else(|| project_dir.clone())
            };

            let result = module
                .run(&params, &run_dir, events_tx, cancel_for_module)
                .await;

            let (status, module_result_opt, error_opt) = match &result {
                Ok(mr) => (RunStatus::Done, Some(mr.clone()), None),
                Err(crate::module::ModuleError::Cancelled) => (RunStatus::Cancelled, None, None),
                Err(e) => (RunStatus::Failed, None, Some(e.to_string())),
            };

            // Compute declared assets from the module *before* locking the
            // project, so the module doesn't race with UI reads.
            let declared_assets = match &result {
                Ok(mr) => module.produced_assets(mr),
                _ => Vec::new(),
            };

            {
                let mut proj = project_arc.lock().await;
                if let Some(run) = proj.runs.iter_mut().find(|r| r.id == rid) {
                    run.status = status;
                    run.finished_at = Some(Utc::now());
                    run.result = module_result_opt;
                    run.error = error_opt;
                }
                // Auto-register assets declared by the module. Errors are
                // logged but do not fail the run — lineage is advisory.
                if !declared_assets.is_empty() {
                    if let Err(e) = proj.register_declared_assets(&rid, &declared_assets) {
                        eprintln!("asset auto-register for run {rid} failed: {e}");
                    }
                }
                let _ = proj.save();
            }

            {
                let mut active = active_runs_arc.lock().await;
                active.remove(&rid);
            }

            if let Some(cb) = &on_complete_arc {
                let cb_result = result.map_err(|e| e.to_string());
                cb(&rid_for_complete, cb_result);
            }
        });

        {
            let mut active = self.active_runs.lock().await;
            active.insert(
                run_id.clone(),
                ActiveRun {
                    handle,
                    cancel: cancel_token,
                },
            );
        }

        Ok(run_id)
    }

    pub async fn cancel(&self, run_id: &str) {
        let entry = {
            let mut active = self.active_runs.lock().await;
            active.remove(run_id)
        };

        if let Some(ActiveRun { handle, cancel }) = entry {
            cancel.cancel();
            // Give cooperative cancellation a brief window, then abort as a safety net.
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            handle.abort();

            let mut proj = self.project.lock().await;
            if let Some(run) = proj.runs.iter_mut().find(|r| r.id == run_id) {
                // Only mark as Cancelled if the task hasn't already transitioned
                // to a terminal state (Done / Failed) during the grace period.
                if matches!(run.status, RunStatus::Running) {
                    run.status = RunStatus::Cancelled;
                    run.finished_at = Some(Utc::now());
                }
            }
            let _ = proj.save();
        }
    }
}

#[cfg(test)]
mod runner_tests {
    use super::*;
    use crate::cancel::CancellationToken;
    use crate::module::{Module, ModuleError, ModuleResult, ValidationError};
    use crate::project::{Project, RunStatus};
    use crate::run_event::{LogStream, RunEvent};
    use std::sync::Arc;
    use tokio::sync::{mpsc, Mutex};

    struct EmitsLogModule;

    #[async_trait::async_trait]
    impl Module for EmitsLogModule {
        fn id(&self) -> &str {
            "emitslog"
        }
        fn name(&self) -> &str {
            "EmitsLog"
        }
        fn validate(&self, _p: &serde_json::Value) -> Vec<ValidationError> {
            vec![]
        }
        async fn run(
            &self,
            _p: &serde_json::Value,
            _d: &std::path::Path,
            events_tx: mpsc::Sender<RunEvent>,
            _c: CancellationToken,
        ) -> Result<ModuleResult, ModuleError> {
            events_tx
                .send(RunEvent::Log {
                    line: "hello".into(),
                    stream: LogStream::Stderr,
                })
                .await
                .ok();
            events_tx
                .send(RunEvent::Progress {
                    fraction: 1.0,
                    message: "done".into(),
                })
                .await
                .ok();
            Ok(ModuleResult {
                output_files: vec![],
                summary: serde_json::json!({}),
                log: "".into(),
            })
        }
    }

    #[tokio::test]
    async fn runner_routes_log_and_progress_separately() {
        let tmp = tempfile::tempdir().unwrap();
        let project = Project::create("t", tmp.path()).unwrap();
        let got_log = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let got_prog = Arc::new(std::sync::Mutex::new(Vec::<f64>::new()));
        let log_for_cb = got_log.clone();
        let prog_for_cb = got_prog.clone();
        let runner = Runner::new(Arc::new(Mutex::new(project)))
            .on_progress(Box::new(move |_id, p| {
                prog_for_cb.lock().unwrap().push(p.fraction);
            }))
            .on_log(Box::new(move |_id, line, _stream| {
                log_for_cb.lock().unwrap().push(line);
            }));
        let id = runner
            .spawn(Arc::new(EmitsLogModule), serde_json::json!({}))
            .await
            .unwrap();
        // Poll until the run finishes (status leaves Running)
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let done = runner
                .project()
                .lock()
                .await
                .runs
                .iter()
                .any(|r| r.id == id && matches!(r.status, RunStatus::Done));
            if done {
                break;
            }
        }
        assert_eq!(got_log.lock().unwrap().as_slice(), &["hello".to_string()]);
        assert_eq!(got_prog.lock().unwrap().as_slice(), &[1.0]);
    }

    struct FailingModule;

    #[async_trait::async_trait]
    impl Module for FailingModule {
        fn id(&self) -> &str {
            "failer"
        }
        fn name(&self) -> &str {
            "Failer"
        }
        fn validate(&self, _p: &serde_json::Value) -> Vec<ValidationError> {
            vec![]
        }
        async fn run(
            &self,
            _p: &serde_json::Value,
            _d: &std::path::Path,
            _tx: mpsc::Sender<RunEvent>,
            _c: CancellationToken,
        ) -> Result<ModuleResult, ModuleError> {
            Err(ModuleError::ToolError("boom: something went wrong".into()))
        }
    }

    #[tokio::test]
    async fn runner_persists_error_on_failed_run() {
        let tmp = tempfile::tempdir().unwrap();
        let project = Project::create("t", tmp.path()).unwrap();
        let runner = Runner::new(Arc::new(Mutex::new(project)));
        let id = runner
            .spawn(Arc::new(FailingModule), serde_json::json!({}))
            .await
            .unwrap();

        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let done = runner
                .project()
                .lock()
                .await
                .runs
                .iter()
                .any(|r| r.id == id && matches!(r.status, RunStatus::Failed));
            if done {
                break;
            }
        }

        let proj = runner.project().lock().await;
        let run = proj.runs.iter().find(|r| r.id == id).expect("run exists");
        assert!(matches!(run.status, RunStatus::Failed));
        let err = run.error.as_deref().expect("error persisted");
        assert!(
            err.contains("boom: something went wrong"),
            "expected tool error in persisted message, got: {err:?}"
        );
    }
}

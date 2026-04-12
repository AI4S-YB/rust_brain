use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};

use crate::module::{Module, ModuleResult, Progress};
use crate::project::{Project, RunStatus};

pub type ProgressCallback = Box<dyn Fn(&str, Progress) + Send + Sync>;
pub type CompletionCallback = Box<dyn Fn(&str, Result<ModuleResult, String>) + Send + Sync>;

pub struct Runner {
    project: Arc<Mutex<Project>>,
    on_progress: Option<Arc<ProgressCallback>>,
    on_complete: Option<Arc<CompletionCallback>>,
    active_runs: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl Runner {
    pub fn new(project: Arc<Mutex<Project>>) -> Self {
        Runner {
            project,
            on_progress: None,
            on_complete: None,
            active_runs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn on_progress(mut self, cb: ProgressCallback) -> Self {
        self.on_progress = Some(Arc::new(cb));
        self
    }

    pub fn on_complete(mut self, cb: CompletionCallback) -> Self {
        self.on_complete = Some(Arc::new(cb));
        self
    }

    pub fn project(&self) -> &Arc<Mutex<Project>> {
        &self.project
    }

    pub async fn spawn(
        &self,
        module: Arc<dyn Module>,
        params: Value,
    ) -> Result<String, String> {
        // 1. Create run record in the project
        let run_id = {
            let mut proj = self.project.lock().await;
            let record = proj.create_run(module.id(), params.clone());
            record.id
        };

        // 2. Set status to Running, save project
        {
            let mut proj = self.project.lock().await;
            if let Some(run) = proj.runs.iter_mut().find(|r| r.id == run_id) {
                run.status = RunStatus::Running;
                run.started_at = Some(Utc::now());
            }
            proj.save().map_err(|e| e.to_string())?;
        }

        // Get the project root dir for module.run()
        let project_dir = {
            let proj = self.project.lock().await;
            proj.root_dir.clone()
        };

        // 3. Create progress channel
        let (progress_tx, mut progress_rx) = mpsc::channel::<Progress>(64);

        // Clone everything needed for the spawned tasks
        let project_arc = Arc::clone(&self.project);
        let active_runs_arc = Arc::clone(&self.active_runs);
        let on_progress_arc = self.on_progress.clone();
        let on_complete_arc = self.on_complete.clone();
        let rid = run_id.clone();
        let rid_for_complete = run_id.clone();

        // 4. Spawn progress forwarding task
        let rid_for_progress = run_id.clone();
        let on_progress_for_fwd = on_progress_arc.clone();
        tokio::task::spawn(async move {
            while let Some(progress) = progress_rx.recv().await {
                if let Some(cb) = &on_progress_for_fwd {
                    cb(&rid_for_progress, progress);
                }
            }
        });

        // 5 & 6. Spawn the module.run() task and handle completion
        let handle = tokio::task::spawn(async move {
            let run_dir = {
                let proj = project_arc.lock().await;
                proj.run_dir(&rid).unwrap_or_else(|| project_dir.clone())
            };

            let result = module.run(&params, &run_dir, progress_tx).await;

            // progress_tx dropped here, so progress forwarding task will end

            // 6. Update run record status, save project, call on_complete
            let (status, module_result_opt) = match &result {
                Ok(mr) => (RunStatus::Done, Some(mr.clone())),
                Err(_) => (RunStatus::Failed, None),
            };

            {
                let mut proj = project_arc.lock().await;
                if let Some(run) = proj.runs.iter_mut().find(|r| r.id == rid) {
                    run.status = status;
                    run.finished_at = Some(Utc::now());
                    run.result = module_result_opt;
                }
                let _ = proj.save();
            }

            // Remove from active_runs
            {
                let mut active = active_runs_arc.lock().await;
                active.remove(&rid);
            }

            // Call completion callback
            if let Some(cb) = &on_complete_arc {
                let cb_result = result.map_err(|e| e.to_string());
                cb(&rid_for_complete, cb_result);
            }
        });

        // Store the handle
        {
            let mut active = self.active_runs.lock().await;
            active.insert(run_id.clone(), handle);
        }

        // 7. Return run_id immediately (non-blocking)
        Ok(run_id)
    }

    pub async fn cancel(&self, run_id: &str) {
        let handle = {
            let mut active = self.active_runs.lock().await;
            active.remove(run_id)
        };

        if let Some(h) = handle {
            h.abort();
        }

        // Set status to Cancelled and save
        let mut proj = self.project.lock().await;
        if let Some(run) = proj.runs.iter_mut().find(|r| r.id == run_id) {
            run.status = RunStatus::Cancelled;
            run.finished_at = Some(Utc::now());
        }
        let _ = proj.save();
    }
}

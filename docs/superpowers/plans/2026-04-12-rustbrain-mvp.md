# RustBrain MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a desktop transcriptomics analysis tool integrating fastqc-rs, cutadapt-rs, and DESeq2_rs behind a Tauri v2 + WebView UI with ECharts visualizations and project-based data management.

**Architecture:** Cargo workspace with 6 crates. `rb-core` defines the Module trait, project model, and async runner. `rb-qc`, `rb-trimming`, `rb-deseq2` are thin adapters implementing Module for each tool. `rb-app` is the Tauri binary wiring commands and events. The existing vanilla HTML/CSS/JS frontend connects via `invoke()` and `listen()`.

**Tech Stack:** Rust, Tauri v2, Tokio, serde, async-trait, ECharts, vanilla JS

**Spec:** `docs/superpowers/specs/2026-04-12-rustbrain-mvp-design.md`

---

## File Map

### New Rust files

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Workspace root defining all member crates |
| `crates/rb-core/Cargo.toml` | Core lib dependencies |
| `crates/rb-core/src/lib.rs` | Re-exports all public types |
| `crates/rb-core/src/module.rs` | Module trait, Progress, ModuleResult, ValidationError, ModuleError |
| `crates/rb-core/src/project.rs` | Project, RunRecord, RunStatus, create/load/save logic |
| `crates/rb-core/src/runner.rs` | Async task runner, spawns modules, routes progress |
| `crates/rb-core/src/config.rs` | AppConfig struct with defaults |
| `crates/rb-app/Cargo.toml` | Tauri app dependencies |
| `crates/rb-app/build.rs` | tauri-build script |
| `crates/rb-app/tauri.conf.json` | Tauri configuration |
| `crates/rb-app/src/main.rs` | Tauri entry point |
| `crates/rb-app/src/state.rs` | AppState, ModuleRegistry |
| `crates/rb-app/src/commands/mod.rs` | Command module re-exports |
| `crates/rb-app/src/commands/project.rs` | create/open/list_recent project commands |
| `crates/rb-app/src/commands/modules.rs` | run_module, validate_params, cancel_run, get_run_result, list_runs |
| `crates/rb-app/src/commands/files.rs` | select_files, select_directory, read_table_preview |
| `crates/rb-qc/Cargo.toml` | QC adapter dependencies |
| `crates/rb-qc/src/lib.rs` | Module impl wrapping fastqc-rs |
| `crates/rb-trimming/Cargo.toml` | Trimming adapter dependencies |
| `crates/rb-trimming/src/lib.rs` | Module impl wrapping cutadapt-rs |
| `crates/rb-deseq2/Cargo.toml` | DESeq2 adapter dependencies |
| `crates/rb-deseq2/src/lib.rs` | Module impl wrapping DESeq2_rs |

### Modified frontend files

| File | Changes |
|------|---------|
| `frontend/index.html` | Replace Plotly CDN with ECharts CDN, add Tauri API script |
| `frontend/js/app.js` | Replace Plotly chart code with ECharts, replace mock API with Tauri invoke, add event listeners, add project management UI, add custom plot tab |

### Test files

| File | Tests |
|------|-------|
| `crates/rb-core/tests/project_test.rs` | Project create/load/save roundtrip |
| `crates/rb-core/tests/runner_test.rs` | Runner with mock module |

---

## Task 1: Workspace scaffolding + rb-core types

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/rb-core/Cargo.toml`
- Create: `crates/rb-core/src/lib.rs`
- Create: `crates/rb-core/src/module.rs`
- Create: `crates/rb-core/src/config.rs`

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
# Cargo.toml (workspace root)
[workspace]
resolver = "2"
members = [
    "crates/rb-core",
    "crates/rb-app",
    "crates/rb-qc",
    "crates/rb-trimming",
    "crates/rb-deseq2",
]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync"] }
async-trait = "0.1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
thiserror = "2"
```

- [ ] **Step 2: Create rb-core crate**

```toml
# crates/rb-core/Cargo.toml
[package]
name = "rb-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
async-trait.workspace = true
chrono.workspace = true
uuid.workspace = true
thiserror.workspace = true
```

- [ ] **Step 3: Write module.rs with all core types**

```rust
// crates/rb-core/src/module.rs
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub fraction: f64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleResult {
    pub output_files: Vec<PathBuf>,
    pub summary: serde_json::Value,
    pub log: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("invalid parameters: {0:?}")]
    InvalidParams(Vec<ValidationError>),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("tool error: {0}")]
    ToolError(String),
    #[error("cancelled")]
    Cancelled,
}

#[async_trait::async_trait]
pub trait Module: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError>;
    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        progress_tx: mpsc::Sender<Progress>,
    ) -> Result<ModuleResult, ModuleError>;
}
```

- [ ] **Step 4: Write config.rs**

```rust
// crates/rb-core/src/config.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub default_output_dir: Option<PathBuf>,
    pub default_threads: u32,
    pub temp_dir: Option<PathBuf>,
    pub reference_genome_dir: Option<PathBuf>,
    pub annotation_file: Option<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_output_dir: None,
            default_threads: 4,
            temp_dir: None,
            reference_genome_dir: None,
            annotation_file: None,
        }
    }
}
```

- [ ] **Step 5: Write lib.rs re-exports**

```rust
// crates/rb-core/src/lib.rs
pub mod config;
pub mod module;
pub mod project;
pub mod runner;
```

Create placeholder files so the crate compiles:

```rust
// crates/rb-core/src/project.rs
// Implemented in Task 2
```

```rust
// crates/rb-core/src/runner.rs
// Implemented in Task 3
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p rb-core`
Expected: compiles with no errors (may have unused warnings, that's fine)

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/rb-core/
git commit -m "feat(rb-core): scaffold workspace and define Module trait types"
```

---

## Task 2: Project persistence

**Files:**
- Create: `crates/rb-core/src/project.rs`
- Create: `crates/rb-core/tests/project_test.rs`

- [ ] **Step 1: Write the project model and persistence logic**

```rust
// crates/rb-core/src/project.rs
use crate::module::ModuleResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RunStatus {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub module_id: String,
    pub params: serde_json::Value,
    pub status: RunStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub result: Option<ModuleResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip)]
    pub root_dir: PathBuf,
    pub runs: Vec<RunRecord>,
}

impl Project {
    /// Create a new project with the given name in the given directory.
    /// Creates the directory structure: root/project.json, root/input/, root/runs/
    pub fn create(name: &str, root_dir: &Path) -> Result<Self, std::io::Error> {
        fs::create_dir_all(root_dir)?;
        fs::create_dir_all(root_dir.join("input"))?;
        fs::create_dir_all(root_dir.join("runs"))?;

        let project = Self {
            name: name.to_string(),
            created_at: Utc::now(),
            root_dir: root_dir.to_path_buf(),
            runs: Vec::new(),
        };
        project.save()?;
        Ok(project)
    }

    /// Load a project from a directory containing project.json
    pub fn load(root_dir: &Path) -> Result<Self, std::io::Error> {
        let json_path = root_dir.join("project.json");
        let contents = fs::read_to_string(&json_path)?;
        let mut project: Project = serde_json::from_str(&contents)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        project.root_dir = root_dir.to_path_buf();
        Ok(project)
    }

    /// Persist project metadata to project.json
    pub fn save(&self) -> Result<(), std::io::Error> {
        let json_path = self.root_dir.join("project.json");
        let contents = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(&json_path, contents)
    }

    /// Create a run directory and return the run record.
    /// Directory name: {module_id}_{short_uuid}
    pub fn create_run(&mut self, module_id: &str, params: serde_json::Value) -> RunRecord {
        let id = uuid::Uuid::new_v4().to_string();
        let short_id = &id[..8];
        let run_dir = self.root_dir.join("runs").join(format!("{module_id}_{short_id}"));
        let _ = fs::create_dir_all(&run_dir);

        // Save params snapshot
        let params_path = run_dir.join("params.json");
        let _ = fs::write(&params_path, serde_json::to_string_pretty(&params).unwrap_or_default());

        let record = RunRecord {
            id,
            module_id: module_id.to_string(),
            params,
            status: RunStatus::Pending,
            started_at: None,
            finished_at: None,
            result: None,
        };
        self.runs.push(record.clone());
        record
    }

    /// Get the run directory path for a given run ID
    pub fn run_dir(&self, run_id: &str) -> Option<PathBuf> {
        let record = self.runs.iter().find(|r| r.id == run_id)?;
        let short_id = &run_id[..8];
        Some(self.root_dir.join("runs").join(format!("{}_{}", record.module_id, short_id)))
    }
}
```

- [ ] **Step 2: Write the test**

```rust
// crates/rb-core/tests/project_test.rs
use rb_core::project::{Project, RunStatus};
use std::path::Path;
use tempfile::TempDir;

#[test]
fn create_and_load_project() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("test_project");

    // Create
    let mut project = Project::create("Test Experiment", &root).unwrap();
    assert_eq!(project.name, "Test Experiment");
    assert!(root.join("project.json").exists());
    assert!(root.join("input").is_dir());
    assert!(root.join("runs").is_dir());

    // Create a run
    let params = serde_json::json!({"threads": 4});
    let run = project.create_run("qc", params.clone());
    assert_eq!(run.module_id, "qc");
    assert_eq!(run.status, RunStatus::Pending);
    project.save().unwrap();

    // Verify run directory exists
    let run_dir = project.run_dir(&run.id).unwrap();
    assert!(run_dir.is_dir());
    assert!(run_dir.join("params.json").exists());

    // Load
    let loaded = Project::load(&root).unwrap();
    assert_eq!(loaded.name, "Test Experiment");
    assert_eq!(loaded.runs.len(), 1);
    assert_eq!(loaded.runs[0].module_id, "qc");
}
```

Add `tempfile` as a dev dependency:

```toml
# Add to crates/rb-core/Cargo.toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Run the test**

Run: `cargo test -p rb-core`
Expected: `create_and_load_project ... ok`

- [ ] **Step 4: Commit**

```bash
git add crates/rb-core/
git commit -m "feat(rb-core): implement Project model with create/load/save"
```

---

## Task 3: Async task runner

**Files:**
- Create: `crates/rb-core/src/runner.rs`
- Create: `crates/rb-core/tests/runner_test.rs`

- [ ] **Step 1: Write the runner**

```rust
// crates/rb-core/src/runner.rs
use crate::module::{Module, ModuleError, ModuleResult, Progress};
use crate::project::{Project, RunStatus};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// Callback invoked on each progress update from a running module
pub type ProgressCallback = Box<dyn Fn(&str, Progress) + Send + Sync>;

/// Callback invoked when a run completes or fails
pub type CompletionCallback = Box<dyn Fn(&str, Result<ModuleResult, String>) + Send + Sync>;

pub struct Runner {
    project: Arc<Mutex<Project>>,
    on_progress: Option<Arc<ProgressCallback>>,
    on_complete: Option<Arc<CompletionCallback>>,
    active_runs: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl Runner {
    pub fn new(project: Arc<Mutex<Project>>) -> Self {
        Self {
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

    /// Spawn a module run in a background tokio task. Returns the run ID immediately.
    pub async fn spawn(
        &self,
        module: Arc<dyn Module>,
        params: serde_json::Value,
    ) -> Result<String, String> {
        let run_id;
        let run_dir;
        {
            let mut proj = self.project.lock().await;
            let record = proj.create_run(module.id(), params.clone());
            run_id = record.id.clone();
            run_dir = proj.run_dir(&run_id).unwrap_or_else(|| PathBuf::from("."));
            // Mark as running
            if let Some(r) = proj.runs.iter_mut().find(|r| r.id == run_id) {
                r.status = RunStatus::Running;
                r.started_at = Some(chrono::Utc::now());
            }
            let _ = proj.save();
        }

        let (progress_tx, mut progress_rx) = mpsc::channel::<Progress>(64);
        let project = self.project.clone();
        let on_progress = self.on_progress.clone();
        let on_complete = self.on_complete.clone();
        let rid = run_id.clone();

        // Progress forwarding task
        let rid_progress = rid.clone();
        let progress_handle = tokio::spawn(async move {
            while let Some(p) = progress_rx.recv().await {
                if let Some(ref cb) = on_progress {
                    cb(&rid_progress, p);
                }
            }
        });

        // Module execution task
        let handle = tokio::spawn(async move {
            let result = module.run(&params, &run_dir, progress_tx).await;
            progress_handle.abort(); // stop progress listener

            let mut proj = project.lock().await;
            if let Some(r) = proj.runs.iter_mut().find(|r| r.id == rid) {
                r.finished_at = Some(chrono::Utc::now());
                match &result {
                    Ok(res) => {
                        r.status = RunStatus::Done;
                        r.result = Some(res.clone());
                    }
                    Err(_) => {
                        r.status = RunStatus::Failed;
                    }
                }
            }
            let _ = proj.save();

            if let Some(ref cb) = on_complete {
                cb(&rid, result.map_err(|e| e.to_string()));
            }
        });

        self.active_runs.lock().await.insert(run_id.clone(), handle);
        Ok(run_id)
    }

    /// Access the shared project reference
    pub fn project(&self) -> &Arc<Mutex<Project>> {
        &self.project
    }

    /// Cancel a running task
    pub async fn cancel(&self, run_id: &str) {
        let mut active = self.active_runs.lock().await;
        if let Some(handle) = active.remove(run_id) {
            handle.abort();
            let mut proj = self.project.lock().await;
            if let Some(r) = proj.runs.iter_mut().find(|r| r.id == run_id) {
                r.status = RunStatus::Cancelled;
                r.finished_at = Some(chrono::Utc::now());
            }
            let _ = proj.save();
        }
    }
}
```

**Note:** The runner exposes `project()` so Tauri commands can access the project without storing a separate copy.

- [ ] **Step 2: Write the test with a mock module**

```rust
// crates/rb-core/tests/runner_test.rs
use rb_core::module::{Module, ModuleError, ModuleResult, Progress, ValidationError};
use rb_core::project::Project;
use rb_core::runner::Runner;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

struct MockModule;

#[async_trait::async_trait]
impl Module for MockModule {
    fn id(&self) -> &str { "mock" }
    fn name(&self) -> &str { "Mock Module" }
    fn validate(&self, _params: &serde_json::Value) -> Vec<ValidationError> { vec![] }

    async fn run(
        &self,
        _params: &serde_json::Value,
        _project_dir: &Path,
        progress_tx: mpsc::Sender<Progress>,
    ) -> Result<ModuleResult, ModuleError> {
        let _ = progress_tx.send(Progress { fraction: 0.5, message: "halfway".into() }).await;
        let _ = progress_tx.send(Progress { fraction: 1.0, message: "done".into() }).await;
        Ok(ModuleResult {
            output_files: vec![],
            summary: serde_json::json!({"test": true}),
            log: "mock run complete".into(),
        })
    }
}

#[tokio::test]
async fn runner_executes_mock_module() {
    let tmp = tempfile::TempDir::new().unwrap();
    let project = Project::create("runner_test", tmp.path()).unwrap();
    let project = Arc::new(Mutex::new(project));

    let progress_log = Arc::new(Mutex::new(Vec::<Progress>::new()));
    let progress_log_clone = progress_log.clone();

    let runner = Runner::new(project.clone())
        .on_progress(Box::new(move |_run_id, p| {
            let log = progress_log_clone.clone();
            // Note: this is a sync callback, so we just store
            // In production, Tauri emit is sync-safe
            let _ = log.try_lock().map(|mut v| v.push(p));
        }));

    let module = Arc::new(MockModule);
    let params = serde_json::json!({"key": "value"});
    let run_id = runner.spawn(module, params).await.unwrap();

    // Wait for completion
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let proj = project.lock().await;
    let record = proj.runs.iter().find(|r| r.id == run_id).unwrap();
    assert_eq!(record.status, rb_core::project::RunStatus::Done);
    assert!(record.result.is_some());
}
```

Add dev dependencies:

```toml
# Add to crates/rb-core/Cargo.toml [dev-dependencies]
async-trait = "0.1"
```

- [ ] **Step 3: Fix compile errors and make tests pass**

Fix the `r_id_for_cb` typo in runner.rs — replace with `rid`. Adjust any borrow/lifetime issues the compiler reports. The runner uses `Arc<Mutex<Project>>` extensively — ensure all locks are dropped before awaiting.

Run: `cargo test -p rb-core`
Expected: both `create_and_load_project` and `runner_executes_mock_module` pass

- [ ] **Step 4: Commit**

```bash
git add crates/rb-core/
git commit -m "feat(rb-core): implement async task runner with progress callbacks"
```

---

## Task 4: Tauri app scaffold

**Files:**
- Create: `crates/rb-app/Cargo.toml`
- Create: `crates/rb-app/build.rs`
- Create: `crates/rb-app/tauri.conf.json`
- Create: `crates/rb-app/src/main.rs`
- Create: `crates/rb-app/src/state.rs`
- Create: `crates/rb-app/src/commands/mod.rs`

- [ ] **Step 1: Create rb-app Cargo.toml**

```toml
# crates/rb-app/Cargo.toml
[package]
name = "rb-app"
version = "0.1.0"
edition = "2021"

[dependencies]
tauri = { version = "2", features = [] }
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
rb-core = { path = "../rb-core" }

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

- [ ] **Step 2: Create build.rs**

```rust
// crates/rb-app/build.rs
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 3: Create tauri.conf.json**

```json
{
  "productName": "RustBrain",
  "version": "0.1.0",
  "identifier": "com.rustbrain.app",
  "build": {
    "frontendDist": "../../frontend"
  },
  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "title": "RustBrain - Transcriptomics Analysis",
        "width": 1400,
        "height": 900,
        "resizable": true,
        "center": true
      }
    ],
    "security": {
      "csp": "default-src 'self'; script-src 'self' 'unsafe-inline' https://unpkg.com https://cdn.jsdelivr.net; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com; connect-src 'self' ipc: http://ipc.localhost https://unpkg.com https://cdn.jsdelivr.net; img-src 'self' data:"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

- [ ] **Step 4: Create state.rs**

```rust
// crates/rb-app/src/state.rs
use rb_core::module::Module;
use rb_core::project::Project;
use rb_core::runner::Runner;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ModuleRegistry {
    modules: HashMap<String, Arc<dyn Module>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self { modules: HashMap::new() }
    }

    pub fn register(&mut self, module: Arc<dyn Module>) {
        self.modules.insert(module.id().to_string(), module);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Module>> {
        self.modules.get(id).cloned()
    }

    pub fn list_ids(&self) -> Vec<String> {
        self.modules.keys().cloned().collect()
    }
}

pub struct AppState {
    pub registry: Arc<ModuleRegistry>,
    pub runner: Arc<Mutex<Option<Runner>>>,
    pub recent_projects: Arc<Mutex<Vec<PathBuf>>>,
}

impl AppState {
    pub fn new(registry: ModuleRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
            runner: Arc::new(Mutex::new(None)),
            recent_projects: Arc::new(Mutex::new(Vec::new())),
        }
    }
}
```

- [ ] **Step 5: Create main.rs and commands/mod.rs stubs**

```rust
// crates/rb-app/src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::{AppState, ModuleRegistry};

fn main() {
    let registry = ModuleRegistry::new();
    // Modules will be registered here in Task 8-10

    tauri::Builder::default()
        .manage(AppState::new(registry))
        .invoke_handler(tauri::generate_handler![
            // Commands will be registered here in Task 5-6
        ])
        .run(tauri::generate_context!())
        .expect("error while running RustBrain");
}
```

```rust
// crates/rb-app/src/commands/mod.rs
pub mod project;
pub mod modules;
pub mod files;
```

Create empty placeholder files:

```rust
// crates/rb-app/src/commands/project.rs
// Implemented in Task 5
```

```rust
// crates/rb-app/src/commands/modules.rs
// Implemented in Task 6
```

```rust
// crates/rb-app/src/commands/files.rs
// Implemented in Task 6
```

- [ ] **Step 6: Generate placeholder icons**

Run: `mkdir -p crates/rb-app/icons`

Create a minimal placeholder icon (or copy from Tauri template). The app will compile without real icons in dev mode. For now, remove the icon array from tauri.conf.json or use the Tauri CLI to generate icons later: `cargo tauri icon frontend/favicon.svg`

- [ ] **Step 7: Verify compilation**

Run: `cargo check -p rb-app`
Expected: compiles (with dead_code warnings, that's fine)

- [ ] **Step 8: Commit**

```bash
git add crates/rb-app/
git commit -m "feat(rb-app): scaffold Tauri v2 app with state and module registry"
```

---

## Task 5: Tauri project commands

**Files:**
- Modify: `crates/rb-app/src/commands/project.rs`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: Implement project commands**

```rust
// crates/rb-app/src/commands/project.rs
use crate::state::AppState;
use rb_core::project::Project;
use rb_core::runner::Runner;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Serialize)]
pub struct ProjectInfo {
    pub name: String,
    pub root_dir: String,
    pub run_count: usize,
}

impl From<&Project> for ProjectInfo {
    fn from(p: &Project) -> Self {
        Self {
            name: p.name.clone(),
            root_dir: p.root_dir.to_string_lossy().to_string(),
            run_count: p.runs.len(),
        }
    }
}

#[tauri::command]
pub async fn create_project(
    name: String,
    dir: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ProjectInfo, String> {
    let root = std::path::PathBuf::from(&dir).join(&name);
    let project = Project::create(&name, &root).map_err(|e| e.to_string())?;
    let info = ProjectInfo::from(&project);

    // Set up runner with Tauri event emission
    let project_arc = Arc::new(Mutex::new(project));
    let app_handle = app.clone();
    let runner = Runner::new(project_arc)
        .on_progress(Box::new(move |run_id, progress| {
            let _ = app_handle.emit("run-progress", serde_json::json!({
                "runId": run_id,
                "fraction": progress.fraction,
                "message": progress.message,
            }));
        }));

    *state.runner.lock().await = Some(runner);
    state.recent_projects.lock().await.push(root);

    Ok(info)
}

#[tauri::command]
pub async fn open_project(
    dir: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ProjectInfo, String> {
    let root = std::path::PathBuf::from(&dir);
    let project = Project::load(&root).map_err(|e| e.to_string())?;
    let info = ProjectInfo::from(&project);

    let project_arc = Arc::new(Mutex::new(project));
    let app_handle = app.clone();
    let runner = Runner::new(project_arc)
        .on_progress(Box::new(move |run_id, progress| {
            let _ = app_handle.emit("run-progress", serde_json::json!({
                "runId": run_id,
                "fraction": progress.fraction,
                "message": progress.message,
            }));
        }));

    *state.runner.lock().await = Some(runner);

    let mut recent = state.recent_projects.lock().await;
    if !recent.contains(&root) {
        recent.push(root);
    }

    Ok(info)
}

#[tauri::command]
pub async fn list_recent_projects(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let recent = state.recent_projects.lock().await;
    Ok(recent.iter().map(|p| p.to_string_lossy().to_string()).collect())
}
```

- [ ] **Step 2: Register commands in main.rs**

```rust
// crates/rb-app/src/main.rs — update invoke_handler
.invoke_handler(tauri::generate_handler![
    commands::project::create_project,
    commands::project::open_project,
    commands::project::list_recent_projects,
])
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rb-app`
Expected: compiles successfully

- [ ] **Step 4: Commit**

```bash
git add crates/rb-app/src/
git commit -m "feat(rb-app): add Tauri project management commands"
```

---

## Task 6: Tauri module execution + file commands

**Files:**
- Modify: `crates/rb-app/src/commands/modules.rs`
- Modify: `crates/rb-app/src/commands/files.rs`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: Implement module execution commands**

```rust
// crates/rb-app/src/commands/modules.rs
use crate::state::AppState;
use rb_core::module::ValidationError;
use rb_core::project::RunRecord;
use serde::Serialize;

#[tauri::command]
pub async fn validate_params(
    module_id: String,
    params: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ValidationError>, String> {
    let module = state.registry.get(&module_id)
        .ok_or_else(|| format!("module not found: {module_id}"))?;
    Ok(module.validate(&params))
}

#[tauri::command]
pub async fn run_module(
    module_id: String,
    params: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let module = state.registry.get(&module_id)
        .ok_or_else(|| format!("module not found: {module_id}"))?;

    let runner = state.runner.lock().await;
    let runner = runner.as_ref().ok_or("no project open")?;

    runner.spawn(module, params).await
}

#[tauri::command]
pub async fn cancel_run(
    run_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let runner = state.runner.lock().await;
    if let Some(runner) = runner.as_ref() {
        runner.cancel(&run_id).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_run_result(
    run_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<RunRecord>, String> {
    let runner = state.runner.lock().await;
    let runner = runner.as_ref().ok_or("no project open")?;
    let project = runner.project().lock().await;
    Ok(project.runs.iter().find(|r| r.id == run_id).cloned())
}

#[tauri::command]
pub async fn list_runs(
    module_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RunRecord>, String> {
    let runner = state.runner.lock().await;
    let runner = runner.as_ref().ok_or("no project open")?;
    let project = runner.project().lock().await;
    Ok(match module_id {
        Some(mid) => project.runs.iter().filter(|r| r.module_id == mid).cloned().collect(),
        None => project.runs.clone(),
    })
}
```

- [ ] **Step 2: Implement file operation commands**

```rust
// crates/rb-app/src/commands/files.rs
use serde::Serialize;
use std::io::BufRead;

#[tauri::command]
pub async fn select_files(filters: Option<String>) -> Result<Vec<String>, String> {
    use tauri::api::dialog::blocking::FileDialogBuilder;
    // Note: Tauri v2 may use tauri_plugin_dialog instead.
    // If so, the plugin must be added to Cargo.toml and configured.
    // For now, use rfd (raw file dialog) as a simpler alternative:
    let mut dialog = rfd::FileDialog::new();
    if let Some(f) = &filters {
        for ext in f.split(',') {
            let ext = ext.trim().trim_start_matches('.');
            dialog = dialog.add_filter("Files", &[ext]);
        }
    }
    let files = dialog.pick_files().unwrap_or_default();
    Ok(files.into_iter().map(|p| p.to_string_lossy().to_string()).collect())
}

#[tauri::command]
pub async fn select_directory() -> Result<Option<String>, String> {
    let dir = rfd::FileDialog::new().pick_folder();
    Ok(dir.map(|p| p.to_string_lossy().to_string()))
}

#[derive(Serialize)]
pub struct TablePreview {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[tauri::command]
pub async fn read_table_preview(path: String, n_rows: usize) -> Result<TablePreview, String> {
    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let reader = std::io::BufReader::new(file);
    let mut lines = reader.lines();

    let headers = lines.next()
        .ok_or("empty file")?
        .map_err(|e| e.to_string())?
        .split('\t')
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let mut rows = Vec::new();
    for line in lines.take(n_rows) {
        let line = line.map_err(|e| e.to_string())?;
        rows.push(line.split('\t').map(|s| s.to_string()).collect());
    }

    Ok(TablePreview { headers, rows })
}
```

Add `rfd` dependency:

```toml
# Add to crates/rb-app/Cargo.toml [dependencies]
rfd = "0.15"
```

- [ ] **Step 3: Register all commands in main.rs**

Update the `invoke_handler` in main.rs to include all commands:

```rust
.invoke_handler(tauri::generate_handler![
    commands::project::create_project,
    commands::project::open_project,
    commands::project::list_recent_projects,
    commands::modules::validate_params,
    commands::modules::run_module,
    commands::modules::cancel_run,
    commands::modules::get_run_result,
    commands::modules::list_runs,
    commands::files::select_files,
    commands::files::select_directory,
    commands::files::read_table_preview,
])
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p rb-app`
Expected: compiles successfully

- [ ] **Step 5: Commit**

```bash
git add crates/rb-app/
git commit -m "feat(rb-app): add module execution and file operation commands"
```

---

## Task 7: Git submodules + adapter crate scaffolds

**Files:**
- Create: `deps/` directory with git submodules
- Create: `crates/rb-qc/Cargo.toml`, `crates/rb-qc/src/lib.rs`
- Create: `crates/rb-trimming/Cargo.toml`, `crates/rb-trimming/src/lib.rs`
- Create: `crates/rb-deseq2/Cargo.toml`, `crates/rb-deseq2/src/lib.rs`

- [ ] **Step 1: Add git submodules**

```bash
git submodule add https://github.com/AI4S-YB/fastqc-rs.git deps/fastqc-rs
git submodule add https://github.com/AI4S-YB/cutadapt-rs.git deps/cutadapt-rs
git submodule add https://github.com/AI4S-YB/DESeq2_rs.git deps/DESeq2_rs
```

- [ ] **Step 2: Inspect each tool's library API**

Before writing adapters, read each tool's `src/lib.rs` to confirm the public API:

```bash
head -50 deps/DESeq2_rs/src/lib.rs
head -50 deps/fastqc-rs/src/lib.rs
ls deps/cutadapt-rs/  # Find the library crate (likely cutadapt-core/)
```

- [ ] **Step 3: Create rb-deseq2 scaffold**

```toml
# crates/rb-deseq2/Cargo.toml
[package]
name = "rb-deseq2"
version = "0.1.0"
edition = "2021"

[dependencies]
rb-core = { path = "../rb-core" }
deseq2-rs = { path = "../../deps/DESeq2_rs" }
async-trait.workspace = true
serde_json.workspace = true
tokio.workspace = true
```

```rust
// crates/rb-deseq2/src/lib.rs
// Stub — implemented in Task 8
pub struct DeseqModule;
```

- [ ] **Step 4: Create rb-qc scaffold**

```toml
# crates/rb-qc/Cargo.toml
[package]
name = "rb-qc"
version = "0.1.0"
edition = "2021"

[dependencies]
rb-core = { path = "../rb-core" }
fastqc-rs = { path = "../../deps/fastqc-rs" }
async-trait.workspace = true
serde_json.workspace = true
tokio.workspace = true
```

```rust
// crates/rb-qc/src/lib.rs
pub struct QcModule;
```

- [ ] **Step 5: Create rb-trimming scaffold**

Inspect `deps/cutadapt-rs/` to find the library crate name. It is a workspace with `cutadapt-core` as the library crate:

```toml
# crates/rb-trimming/Cargo.toml
[package]
name = "rb-trimming"
version = "0.1.0"
edition = "2021"

[dependencies]
rb-core = { path = "../rb-core" }
cutadapt-core = { path = "../../deps/cutadapt-rs/cutadapt-core" }
async-trait.workspace = true
serde_json.workspace = true
tokio.workspace = true
```

```rust
// crates/rb-trimming/src/lib.rs
pub struct TrimmingModule;
```

**Note:** The exact path to `cutadapt-core` depends on the workspace layout of `deps/cutadapt-rs/`. Verify with `cat deps/cutadapt-rs/Cargo.toml` and adjust the path dependency accordingly.

- [ ] **Step 6: Verify compilation**

Run: `cargo check --workspace`
Expected: all crates compile. If a dep path is wrong, fix based on actual repo structure.

- [ ] **Step 7: Commit**

```bash
git add .gitmodules deps/ crates/rb-qc/ crates/rb-trimming/ crates/rb-deseq2/
git commit -m "feat: add tool submodules and adapter crate scaffolds"
```

---

## Task 8: rb-deseq2 adapter

**Files:**
- Modify: `crates/rb-deseq2/src/lib.rs`

**Prerequisite:** Read `deps/DESeq2_rs/src/pipeline.rs` and `deps/DESeq2_rs/src/data.rs` to confirm the API. Based on documented behavior, the API is:

```rust
use deseq2_rs::pipeline::DESeqDataSet;
use deseq2_rs::data::{Contrast, DESeqResult};
```

- [ ] **Step 1: Implement the DESeq2 adapter**

```rust
// crates/rb-deseq2/src/lib.rs
use rb_core::module::{Module, ModuleError, ModuleResult, Progress, ValidationError};
use std::path::Path;
use tokio::sync::mpsc;

pub struct DeseqModule;

#[async_trait::async_trait]
impl Module for DeseqModule {
    fn id(&self) -> &str { "deseq2" }
    fn name(&self) -> &str { "Differential Expression (DESeq2)" }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        if params.get("counts_path").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            errors.push(ValidationError { field: "counts_path".into(), message: "counts matrix file is required".into() });
        }
        if params.get("coldata_path").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            errors.push(ValidationError { field: "coldata_path".into(), message: "sample info file is required".into() });
        }
        if params.get("design").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            errors.push(ValidationError { field: "design".into(), message: "design variable is required".into() });
        }
        if params.get("reference").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            errors.push(ValidationError { field: "reference".into(), message: "reference level is required".into() });
        }
        errors
    }

    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        progress_tx: mpsc::Sender<Progress>,
    ) -> Result<ModuleResult, ModuleError> {
        let counts_path = params["counts_path"].as_str().unwrap().to_string();
        let coldata_path = params["coldata_path"].as_str().unwrap().to_string();
        let design = params["design"].as_str().unwrap().to_string();
        let reference = params["reference"].as_str().unwrap().to_string();
        let output_file = params.get("output_file")
            .and_then(|v| v.as_str())
            .unwrap_or("deseq2_results.tsv")
            .to_string();
        let padj_cutoff: f64 = params.get("padj_cutoff")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.01);
        let lfc_cutoff: f64 = params.get("lfc_cutoff")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);

        let dir = project_dir.to_path_buf();

        // DESeq2 is CPU-bound, run in blocking task
        let result = tokio::task::spawn_blocking(move || {
            use deseq2_rs::pipeline::DESeqDataSet;
            use deseq2_rs::data::Contrast;

            let mut log = String::new();
            log.push_str(&format!("[INFO] DESeq2_rs\n"));
            log.push_str(&format!("[INFO] Counts: {}\n", counts_path));
            log.push_str(&format!("[INFO] Design: ~{}, Reference: {}\n", design, reference));

            let dds = DESeqDataSet::from_csv(&counts_path, &coldata_path, &design, &reference)
                .map_err(|e| ModuleError::ToolError(format!("Failed to load data: {e}")))?;

            log.push_str("[INFO] Running DESeq2 pipeline...\n");
            let dds = dds.run()
                .map_err(|e| ModuleError::ToolError(format!("Pipeline failed: {e}")))?;

            let results = dds.results(Contrast::LastCoefficient);
            log.push_str(&format!("[INFO] {} genes tested\n", results.len()));

            // Count significant genes
            let sig_count = results.iter()
                .filter(|r| r.p_adjusted < padj_cutoff && r.log2_fold_change.abs() > lfc_cutoff)
                .count();
            let up_count = results.iter()
                .filter(|r| r.p_adjusted < padj_cutoff && r.log2_fold_change > lfc_cutoff)
                .count();
            let down_count = results.iter()
                .filter(|r| r.p_adjusted < padj_cutoff && r.log2_fold_change < -lfc_cutoff)
                .count();

            log.push_str(&format!("[DONE] {} significant (padj < {}, |log2FC| > {})\n",
                sig_count, padj_cutoff, lfc_cutoff));

            // Write results TSV
            let output_path = dir.join(&output_file);
            let mut wtr = std::fs::File::create(&output_path)
                .map_err(|e| ModuleError::IoError(e))?;
            use std::io::Write;
            writeln!(wtr, "gene\tbaseMean\tlog2FoldChange\tlfcSE\tstat\tpvalue\tpadj")
                .map_err(|e| ModuleError::IoError(e))?;
            for r in &results {
                writeln!(wtr, "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    r.gene, r.base_mean, r.log2_fold_change,
                    r.lfc_se, r.stat, r.p_value, r.p_adjusted)
                    .map_err(|e| ModuleError::IoError(e))?;
            }

            // Build summary JSON for frontend
            let summary = serde_json::json!({
                "total_genes": results.len(),
                "significant": sig_count,
                "up_regulated": up_count,
                "down_regulated": down_count,
                "padj_cutoff": padj_cutoff,
                "lfc_cutoff": lfc_cutoff,
                "results": results.iter().map(|r| serde_json::json!({
                    "gene": r.gene,
                    "baseMean": r.base_mean,
                    "log2FoldChange": r.log2_fold_change,
                    "lfcSE": r.lfc_se,
                    "stat": r.stat,
                    "pvalue": r.p_value,
                    "padj": r.p_adjusted,
                })).collect::<Vec<_>>(),
            });

            Ok::<_, ModuleError>((vec![output_path], summary, log))
        }).await.map_err(|e| ModuleError::ToolError(format!("task join error: {e}")))?;

        let (output_files, summary, log) = result?;
        Ok(ModuleResult { output_files, summary, log })
    }
}
```

**Note:** The exact field names on `DESeqResult` (e.g., `r.p_adjusted` vs `r.padj`) must be verified by reading `deps/DESeq2_rs/src/data.rs`. Adjust if the actual struct uses different names.

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p rb-deseq2`
Expected: compiles. If field names don't match, fix based on `data.rs`.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-deseq2/
git commit -m "feat(rb-deseq2): implement Module trait adapter for DESeq2_rs"
```

---

## Task 9: rb-qc adapter

**Files:**
- Modify: `crates/rb-qc/src/lib.rs`

**Prerequisite:** Read `deps/fastqc-rs/src/analysis.rs` and `deps/fastqc-rs/src/modules/` to understand how to run analysis programmatically. The library exports `analysis`, `modules`, `report`, and `config` modules.

- [ ] **Step 1: Implement the QC adapter**

The exact API depends on what `fastqc-rs` exposes. The pattern below is based on the library structure (analysis module that processes files and returns module results). **Verify by reading the source and adjust accordingly.**

```rust
// crates/rb-qc/src/lib.rs
use rb_core::module::{Module, ModuleError, ModuleResult, Progress, ValidationError};
use std::path::Path;
use tokio::sync::mpsc;

pub struct QcModule;

#[async_trait::async_trait]
impl Module for QcModule {
    fn id(&self) -> &str { "qc" }
    fn name(&self) -> &str { "Quality Control (FastQC)" }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let files = params.get("input_files").and_then(|v| v.as_array());
        if files.map_or(true, |f| f.is_empty()) {
            errors.push(ValidationError {
                field: "input_files".into(),
                message: "at least one input file is required".into(),
            });
        }
        errors
    }

    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        progress_tx: mpsc::Sender<Progress>,
    ) -> Result<ModuleResult, ModuleError> {
        let input_files: Vec<String> = params["input_files"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let threads: usize = params.get("threads")
            .and_then(|v| v.as_u64())
            .unwrap_or(4) as usize;

        let dir = project_dir.to_path_buf();
        let total = input_files.len();

        // fastqc-rs is CPU-bound
        let result = tokio::task::spawn_blocking(move || {
            let mut log = String::new();
            let mut output_files = Vec::new();
            let mut all_summaries = Vec::new();

            log.push_str(&format!("[INFO] fastqc-rs v{}\n", fastqc_rs::VERSION));
            log.push_str(&format!("[INFO] Processing {} file(s) with {} threads\n", total, threads));

            // Process each input file
            // NOTE: The exact API calls below must be verified by reading
            // deps/fastqc-rs/src/analysis.rs. The library likely exposes
            // a function like `analyze_file(path, config) -> AnalysisResult`
            // or similar. Adjust this code to match the actual API.
            for (i, input_path) in input_files.iter().enumerate() {
                log.push_str(&format!("[INFO] Analyzing {}...\n", input_path));

                // --- BEGIN: Verify and adjust this section ---
                // Expected pattern based on library structure:
                // let config = fastqc_rs::config::Config::default();
                // let result = fastqc_rs::analysis::analyze_file(input_path, &config)?;
                // let summary = result.summary();
                // fastqc_rs::report::write_report(&result, &output_dir)?;
                // --- END ---

                // For now, invoke the CLI as a subprocess fallback
                // until the library API is confirmed:
                let output_dir = dir.join(format!("fastqc_{}", i));
                std::fs::create_dir_all(&output_dir)
                    .map_err(|e| ModuleError::IoError(e))?;

                let status = std::process::Command::new("fastqc-rs")
                    .arg(input_path)
                    .arg("-o")
                    .arg(output_dir.to_str().unwrap())
                    .arg("-t")
                    .arg(threads.to_string())
                    .status()
                    .map_err(|e| ModuleError::IoError(e))?;

                if !status.success() {
                    return Err(ModuleError::ToolError(
                        format!("fastqc-rs failed on {}", input_path)));
                }

                output_files.push(output_dir.clone());

                all_summaries.push(serde_json::json!({
                    "file": input_path,
                    "output_dir": output_dir.to_string_lossy(),
                }));

                log.push_str(&format!("[INFO] Done ({}/{})\n", i + 1, total));
            }

            log.push_str("[DONE] QC analysis complete\n");

            let summary = serde_json::json!({
                "total_files": total,
                "results": all_summaries,
            });

            Ok::<_, ModuleError>((output_files, summary, log))
        }).await.map_err(|e| ModuleError::ToolError(format!("task join error: {e}")))?;

        let (output_files, summary, log) = result?;
        Ok(ModuleResult { output_files, summary, log })
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p rb-qc`
Expected: compiles. Adjust API calls based on actual fastqc-rs source.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-qc/
git commit -m "feat(rb-qc): implement Module trait adapter for fastqc-rs"
```

---

## Task 10: rb-trimming adapter

**Files:**
- Modify: `crates/rb-trimming/src/lib.rs`

**Prerequisite:** Read `deps/cutadapt-rs/cutadapt-core/src/lib.rs` to confirm the pipeline API. The library has a `pipeline` module.

- [ ] **Step 1: Implement the trimming adapter**

```rust
// crates/rb-trimming/src/lib.rs
use rb_core::module::{Module, ModuleError, ModuleResult, Progress, ValidationError};
use std::path::Path;
use tokio::sync::mpsc;

pub struct TrimmingModule;

#[async_trait::async_trait]
impl Module for TrimmingModule {
    fn id(&self) -> &str { "trimming" }
    fn name(&self) -> &str { "Adapter Trimming (cutadapt)" }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let files = params.get("input_files").and_then(|v| v.as_array());
        if files.map_or(true, |f| f.is_empty()) {
            errors.push(ValidationError {
                field: "input_files".into(),
                message: "at least one input file is required".into(),
            });
        }
        errors
    }

    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        progress_tx: mpsc::Sender<Progress>,
    ) -> Result<ModuleResult, ModuleError> {
        let input_files: Vec<String> = params["input_files"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let adapter: String = params.get("adapter")
            .and_then(|v| v.as_str())
            .unwrap_or("AGATCGGAAGAGC")
            .to_string();
        let quality_cutoff: u32 = params.get("quality_cutoff")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as u32;
        let min_length: u32 = params.get("min_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as u32;
        let threads: usize = params.get("threads")
            .and_then(|v| v.as_u64())
            .unwrap_or(4) as usize;

        let dir = project_dir.to_path_buf();

        let result = tokio::task::spawn_blocking(move || {
            let mut log = String::new();
            let mut output_files = Vec::new();

            log.push_str("[INFO] cutadapt-rs\n");
            log.push_str(&format!("[INFO] Adapter: {} (3' regular)\n", adapter));
            log.push_str(&format!("[INFO] Quality cutoff: {}, Min length: {}\n", quality_cutoff, min_length));

            for input_path in &input_files {
                let input = std::path::Path::new(input_path);
                let stem = input.file_stem().unwrap_or_default().to_string_lossy();
                let output_path = dir.join(format!("trimmed_{}.fastq.gz", stem));

                log.push_str(&format!("[INFO] Processing {}...\n", input_path));

                // NOTE: The exact cutadapt-core API must be verified by reading
                // deps/cutadapt-rs/cutadapt-core/src/lib.rs
                // The CLI equivalent is:
                //   cutadapt -a ADAPTER -q QUALITY -m MINLEN -o output input
                // Use subprocess fallback until library API is confirmed:
                let status = std::process::Command::new("cutadapt-rs")
                    .arg("-a").arg(&adapter)
                    .arg("-q").arg(quality_cutoff.to_string())
                    .arg("-m").arg(min_length.to_string())
                    .arg("-j").arg(threads.to_string())
                    .arg("-o").arg(output_path.to_str().unwrap())
                    .arg(input_path)
                    .status()
                    .map_err(|e| ModuleError::IoError(e))?;

                if !status.success() {
                    return Err(ModuleError::ToolError(
                        format!("cutadapt-rs failed on {}", input_path)));
                }

                output_files.push(output_path);
            }

            log.push_str("[DONE] Trimming complete\n");

            let summary = serde_json::json!({
                "total_files": input_files.len(),
                "adapter": adapter,
                "quality_cutoff": quality_cutoff,
                "min_length": min_length,
            });

            Ok::<_, ModuleError>((output_files, summary, log))
        }).await.map_err(|e| ModuleError::ToolError(format!("task join error: {e}")))?;

        let (output_files, summary, log) = result?;
        Ok(ModuleResult { output_files, summary, log })
    }
}
```

- [ ] **Step 2: Register all modules in main.rs**

```rust
// crates/rb-app/src/main.rs — update module registration
use std::sync::Arc;

fn main() {
    let mut registry = ModuleRegistry::new();
    registry.register(Arc::new(rb_qc::QcModule));
    registry.register(Arc::new(rb_trimming::TrimmingModule));
    registry.register(Arc::new(rb_deseq2::DeseqModule));

    tauri::Builder::default()
        // ...
}
```

Add dependencies to rb-app:

```toml
# Add to crates/rb-app/Cargo.toml [dependencies]
rb-qc = { path = "../rb-qc" }
rb-trimming = { path = "../rb-trimming" }
rb-deseq2 = { path = "../rb-deseq2" }
```

- [ ] **Step 3: Verify full workspace compilation**

Run: `cargo check --workspace`
Expected: all 6 crates compile

- [ ] **Step 4: Commit**

```bash
git add crates/
git commit -m "feat: implement trimming adapter and register all modules in Tauri app"
```

---

## Task 11: Frontend — ECharts migration

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/js/app.js`

- [ ] **Step 1: Replace Plotly CDN with ECharts in index.html**

In `frontend/index.html`, replace the Plotly script tag:

```html
<!-- Remove this line: -->
<script src="https://cdn.plot.ly/plotly-2.35.0.min.js" charset="utf-8"></script>

<!-- Add this line: -->
<script src="https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js"></script>
```

Also add the Tauri API script (after ECharts):

```html
<script>
  // Tauri API will be available as window.__TAURI__ when running in Tauri
  // In browser dev mode, we provide a mock fallback
  if (!window.__TAURI__) {
    window.__TAURI__ = {
      core: { invoke: (cmd, args) => { console.log('[mock invoke]', cmd, args); return Promise.resolve(null); } },
      event: { listen: (evt, cb) => { console.log('[mock listen]', evt); return Promise.resolve(() => {}); } },
    };
  }
</script>
```

- [ ] **Step 2: Rewrite chart constants and base config in app.js**

Replace `PLOTLY_LAYOUT_BASE` and `PLOTLY_CONFIG` with ECharts equivalents:

```javascript
// Replace PLOTLY_LAYOUT_BASE and PLOTLY_CONFIG with:
const ECHART_THEME = {
  backgroundColor: '#faf8f4',
  textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
  title: { textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' } },
  grid: { left: 60, right: 24, top: 44, bottom: 50, borderColor: '#e8e2d8' },
  xAxis: { splitLine: { lineStyle: { color: '#e8e2d8' } }, axisLine: { lineStyle: { color: '#ddd6ca' } } },
  yAxis: { splitLine: { lineStyle: { color: '#e8e2d8' } }, axisLine: { lineStyle: { color: '#ddd6ca' } } },
};

function createChart(container) {
  return echarts.init(container, null, { renderer: 'canvas' });
}
```

- [ ] **Step 3: Rewrite renderQCCharts()**

Replace the Plotly-based `renderQCCharts` function:

```javascript
function renderQCCharts() {
  const el = document.getElementById('qc-quality-chart');
  if (!el) return;
  const chart = createChart(el);

  const pos = Array.from({ length: 150 }, (_, i) => i + 1);
  const mean = pos.map(p => p < 5 ? 32 + Math.random() * 3 : p < 120 ? 34 + Math.random() * 2 : 34 - (p - 120) * 0.15 + Math.random() * 2);
  const lo = mean.map(q => q - 4 - Math.random() * 2);
  const hi = mean.map(q => q + 2 + Math.random());

  chart.setOption({
    ...ECHART_THEME,
    title: { text: 'Per Base Sequence Quality', ...ECHART_THEME.title },
    tooltip: { trigger: 'axis' },
    xAxis: { type: 'category', data: pos, name: 'Position (bp)', ...ECHART_THEME.xAxis },
    yAxis: { type: 'value', name: 'Phred Score', min: 0, max: 42, ...ECHART_THEME.yAxis },
    visualMap: { show: false, pieces: [
      { lte: 20, color: 'rgba(201,80,60,0.04)' },
      { gt: 20, lte: 28, color: 'rgba(184,134,11,0.06)' },
      { gt: 28, color: 'rgba(45,134,89,0.04)' },
    ], dimension: 1, seriesIndex: 2 },
    series: [
      { name: 'Upper', type: 'line', data: hi, lineStyle: { width: 0 }, symbol: 'none', stack: 'band', areaStyle: { opacity: 0 } },
      { name: 'Range', type: 'line', data: lo, lineStyle: { width: 0 }, symbol: 'none', stack: 'band', areaStyle: { color: 'rgba(13,115,119,0.08)', opacity: 1 } },
      { name: 'Mean Quality', type: 'line', data: mean, lineStyle: { color: '#0d7377', width: 2.5 }, symbol: 'none', smooth: true },
    ],
    grid: ECHART_THEME.grid,
  });
  window.addEventListener('resize', () => chart.resize());
}
```

- [ ] **Step 4: Rewrite renderTrimmingCharts()**

```javascript
function renderTrimmingCharts() {
  const el = document.getElementById('trim-length-chart');
  if (!el) return;
  const chart = createChart(el);

  const lens = Array.from({ length: 131 }, (_, i) => i + 20);
  const counts = lens.map(l => Math.floor(80000 * Math.exp(-0.5 * ((l - 148) / 8) ** 2) + Math.random() * 1000));

  chart.setOption({
    ...ECHART_THEME,
    title: { text: 'Read Length Distribution After Trimming', ...ECHART_THEME.title },
    tooltip: { trigger: 'axis', formatter: p => `Length: ${p[0].name} bp<br>Count: ${p[0].value}` },
    xAxis: { type: 'category', data: lens, name: 'Read Length (bp)', ...ECHART_THEME.xAxis },
    yAxis: { type: 'value', name: 'Count', ...ECHART_THEME.yAxis },
    series: [{
      type: 'bar', data: counts,
      itemStyle: { color: p => p.dataIndex < 30 ? 'rgba(184,134,11,0.6)' : 'rgba(59,110,165,0.5)' },
      barWidth: '60%',
    }],
    grid: ECHART_THEME.grid,
  });
  window.addEventListener('resize', () => chart.resize());
}
```

- [ ] **Step 5: Rewrite renderDESeq2Charts()**

```javascript
function renderDESeq2Charts() {
  const volcEl = document.getElementById('deseq-volcano-chart');
  const maEl = document.getElementById('deseq-ma-chart');
  const tbody = document.querySelector('#deseq-results-table tbody');

  // Generate mock data (same as before)
  const n = 2000;
  const genes = [];
  for (let i = 0; i < n; i++) {
    const lfc = (Math.random() - 0.5) * 8;
    const bm = Math.pow(10, 1 + Math.random() * 4);
    const pv = Math.pow(10, -(Math.abs(lfc) * (1 + Math.random() * 3) + Math.random() * 2));
    const pa = Math.min(1, pv * n / (i + 1));
    genes.push({ name: `Gene_${String(i+1).padStart(5,'0')}`, log2FC: lfc, baseMean: bm, pvalue: pv, padj: pa, nlp: -Math.log10(Math.max(pa, 1e-300)) });
  }

  if (volcEl) {
    const chart = createChart(volcEl);
    const ns = genes.filter(g => g.padj >= 0.01 || Math.abs(g.log2FC) <= 1).map(g => [g.log2FC, g.nlp, g.name]);
    const up = genes.filter(g => g.padj < 0.01 && g.log2FC > 1).map(g => [g.log2FC, g.nlp, g.name]);
    const dn = genes.filter(g => g.padj < 0.01 && g.log2FC < -1).map(g => [g.log2FC, g.nlp, g.name]);

    chart.setOption({
      ...ECHART_THEME,
      title: { text: 'Volcano Plot', ...ECHART_THEME.title },
      tooltip: { formatter: p => `${p.value[2]}<br>log2FC: ${p.value[0].toFixed(2)}<br>-log10(padj): ${p.value[1].toFixed(1)}` },
      xAxis: { type: 'value', name: 'log2 Fold Change', ...ECHART_THEME.xAxis },
      yAxis: { type: 'value', name: '-log10(padj)', ...ECHART_THEME.yAxis },
      series: [
        { name: 'Not Sig.', type: 'scatter', data: ns, symbolSize: 4, itemStyle: { color: 'rgba(168,162,158,0.35)' }, large: true },
        { name: 'Up', type: 'scatter', data: up, symbolSize: 5, itemStyle: { color: '#c9503c' } },
        { name: 'Down', type: 'scatter', data: dn, symbolSize: 5, itemStyle: { color: '#3b6ea5' } },
      ],
      legend: { data: ['Not Sig.', 'Up', 'Down'], top: 10, right: 10, textStyle: { fontSize: 11 } },
      grid: ECHART_THEME.grid,
    });
    // Add threshold lines via markLine
    chart.setOption({
      series: [{}, {
        markLine: { silent: true, lineStyle: { color: '#ddd6ca', type: 'dashed' }, data: [
          { xAxis: -1 }, { xAxis: 1 }, { yAxis: 2 },
        ]}
      }, {}]
    });
    window.addEventListener('resize', () => chart.resize());
  }

  if (maEl) {
    const chart = createChart(maEl);
    const sig = genes.filter(g => g.padj < 0.01 && Math.abs(g.log2FC) > 1).map(g => [Math.log10(g.baseMean), g.log2FC]);
    const ns = genes.filter(g => g.padj >= 0.01 || Math.abs(g.log2FC) <= 1).map(g => [Math.log10(g.baseMean), g.log2FC]);

    chart.setOption({
      ...ECHART_THEME,
      title: { text: 'MA Plot', ...ECHART_THEME.title },
      xAxis: { type: 'value', name: 'log10(Mean Expression)', ...ECHART_THEME.xAxis },
      yAxis: { type: 'value', name: 'log2 Fold Change', ...ECHART_THEME.yAxis },
      series: [
        { name: 'Not Sig.', type: 'scatter', data: ns, symbolSize: 4, itemStyle: { color: 'rgba(168,162,158,0.3)' }, large: true },
        { name: 'Significant', type: 'scatter', data: sig, symbolSize: 5, itemStyle: { color: '#c9503c', opacity: 0.6 },
          markLine: { silent: true, lineStyle: { color: '#c8bfb0' }, data: [{ yAxis: 0 }] }
        },
      ],
      legend: { data: ['Not Sig.', 'Significant'], top: 10, right: 10 },
      grid: ECHART_THEME.grid,
    });
    window.addEventListener('resize', () => chart.resize());
  }

  // Table rendering stays the same (no chart library dependency)
  if (tbody) {
    const sorted = genes.sort((a, b) => a.padj - b.padj).slice(0, 30);
    tbody.innerHTML = sorted.map(g => {
      const sc = g.padj < 0.01 && Math.abs(g.log2FC) > 1 ? 'significant' : '';
      const fc = g.log2FC > 0 ? 'positive' : 'negative';
      return `<tr><td class="gene-name">${g.name}</td><td class="${fc}">${g.log2FC.toFixed(3)}</td><td>${g.pvalue.toExponential(2)}</td><td class="${sc}">${g.padj.toExponential(2)}</td></tr>`;
    }).join('');
  }
}
```

- [ ] **Step 6: Rewrite renderWGCNACharts()**

```javascript
function renderWGCNACharts() {
  const modEl = document.getElementById('wgcna-module-chart');
  const traitEl = document.getElementById('wgcna-trait-chart');

  if (modEl) {
    const chart = createChart(modEl);
    const names = ['turquoise','blue','brown','green','yellow','red','black','pink','magenta','purple','greenyellow','grey'];
    const sizes = [820,650,520,410,380,310,270,240,190,160,130,920];
    const colors = ['#40E0D0','#4169E1','#8B6914','#228B22','#DAA520','#DC143C','#444','#FF69B4','#C71585','#7B68EE','#7CCD7C','#999'];

    chart.setOption({
      ...ECHART_THEME,
      title: { text: 'Module Sizes', ...ECHART_THEME.title },
      tooltip: { trigger: 'axis', formatter: p => `${p[0].name}<br>${p[0].value} genes` },
      xAxis: { type: 'category', data: names, name: 'Module', axisLabel: { rotate: 30 }, ...ECHART_THEME.xAxis },
      yAxis: { type: 'value', name: 'Gene Count', ...ECHART_THEME.yAxis },
      series: [{
        type: 'bar', data: sizes.map((s, i) => ({ value: s, itemStyle: { color: colors[i] + 'CC' } })),
        barWidth: '55%',
      }],
      grid: ECHART_THEME.grid,
    });
    window.addEventListener('resize', () => chart.resize());
  }

  if (traitEl) {
    const chart = createChart(traitEl);
    const mods = ['turquoise','blue','brown','green','yellow','red'];
    const traits = ['Treatment','Time','Batch','Age'];
    const data = [];
    mods.forEach((m, mi) => traits.forEach((t, ti) => {
      data.push([ti, mi, +((Math.random() - 0.5) * 2).toFixed(2)]);
    }));

    chart.setOption({
      ...ECHART_THEME,
      title: { text: 'Module-Trait Correlation', ...ECHART_THEME.title },
      tooltip: { formatter: p => `${mods[p.value[1]]} vs ${traits[p.value[0]]}<br>r = ${p.value[2]}` },
      xAxis: { type: 'category', data: traits, ...ECHART_THEME.xAxis },
      yAxis: { type: 'category', data: mods, ...ECHART_THEME.yAxis },
      visualMap: { min: -1, max: 1, calculable: true, orient: 'horizontal', left: 'center', bottom: 0,
        inRange: { color: ['#3b6ea5', '#faf8f4', '#c9503c'] } },
      series: [{ type: 'heatmap', data, label: { show: true, formatter: p => p.value[2].toFixed(2), fontSize: 11 } }],
      grid: { ...ECHART_THEME.grid, bottom: 80 },
    });
    window.addEventListener('resize', () => chart.resize());
  }
}
```

- [ ] **Step 7: Add result export (ECharts toolbox + TSV download)**

Enable the ECharts built-in save-as-image toolbox on all charts by adding `toolbox` to each chart option:

```javascript
// Add to ECHART_THEME:
ECHART_THEME.toolbox = {
  feature: {
    saveAsImage: { title: 'Save PNG', pixelRatio: 2 },
    dataZoom: { title: { zoom: 'Zoom', back: 'Reset' } },
  },
  right: 20, top: 10,
};
```

Add a TSV export helper function:

```javascript
function exportTableAsTSV(tableId, filename) {
  const table = document.getElementById(tableId);
  if (!table) return;
  const rows = Array.from(table.querySelectorAll('tr'));
  const tsv = rows.map(row =>
    Array.from(row.querySelectorAll('th, td')).map(cell => cell.textContent.trim()).join('\t')
  ).join('\n');
  const blob = new Blob([tsv], { type: 'text/tab-separated-values' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url; a.download = filename || 'export.tsv';
  a.click();
  URL.revokeObjectURL(url);
}
```

Add export buttons to result panels. For example, in the DESeq2 results table tab:

```html
<button class="btn btn-ghost btn-sm" onclick="exportTableAsTSV('deseq-results-table', 'deseq2_results.tsv')">
  <i data-lucide="download"></i> Export TSV
</button>
```

- [ ] **Step 8: Open in browser and verify all charts render**

Run: `cd frontend && python3 -m http.server 8090`
Navigate to each module page and verify charts display correctly.

- [ ] **Step 8: Commit**

```bash
git add frontend/
git commit -m "feat(frontend): migrate all charts from Plotly.js to ECharts"
```

---

## Task 12: Frontend — Tauri API integration + progress

**Files:**
- Modify: `frontend/js/app.js`

- [ ] **Step 1: Replace the mock API object**

Replace the existing `api` object in app.js:

```javascript
// Replace the existing api object with:
const api = {
  invoke(command, args) {
    return window.__TAURI__.core.invoke(command, args);
  },
  async listen(event, callback) {
    return window.__TAURI__.event.listen(event, callback);
  }
};
```

- [ ] **Step 2: Replace file drop zone click handler with native dialog**

In the `setupEvents()` function, replace the file input click handler:

```javascript
// Replace the click handler for .file-drop-zone:
document.addEventListener('click', async e => {
  const z = e.target.closest('.file-drop-zone');
  if (z && !e.target.closest('.file-item-remove')) {
    try {
      const files = await api.invoke('select_files', { filters: z.dataset.accept || null });
      if (files && files.length > 0) {
        const mid = z.dataset.module;
        if (!state.files[mid]) state.files[mid] = [];
        files.forEach(f => {
          const name = f.split('/').pop().split('\\').pop();
          state.files[mid].push({ name, size: 0, path: f });
        });
        const list = document.getElementById(`${mid}-file-list`);
        if (list) renderFileList(list, mid);
      }
    } catch (err) {
      console.error('File selection failed:', err);
    }
  }
});
```

- [ ] **Step 3: Wire up progress events**

Add event listeners at the end of the `init()` function:

```javascript
// Add to init() function:
api.listen('run-progress', (event) => {
  const { runId, fraction, message } = event.payload;
  const statusText = document.getElementById('statusText');
  if (statusText) statusText.textContent = message;
  // Update progress in log panel if visible
  const logOutputs = document.querySelectorAll('.log-output');
  logOutputs.forEach(log => {
    log.textContent += `\n${message}`;
    log.scrollTop = log.scrollHeight;
  });
});

api.listen('run-completed', (event) => {
  const { runId, result } = event.payload;
  document.getElementById('statusText').textContent = 'Ready';
  document.getElementById('jobStatus').textContent = 'No active jobs';
  // Re-render current view to show results
  if (state.currentView !== 'dashboard') {
    navigate(state.currentView);
  }
});

api.listen('run-failed', (event) => {
  const { runId, error } = event.payload;
  document.getElementById('statusText').textContent = 'Error';
  document.getElementById('jobStatus').textContent = 'No active jobs';
  console.error('Run failed:', error);
});
```

- [ ] **Step 4: Update runModule to use real API**

Replace the `window.runModule` function:

```javascript
window.runModule = async function (moduleId) {
  const mod = MODULES.find(m => m.id === moduleId);
  const statusText = document.getElementById('statusText');
  const jobStatus = document.getElementById('jobStatus');

  // Collect params from the form inputs in the current view
  const params = collectModuleParams(moduleId);

  // Validate first
  try {
    const errors = await api.invoke('validate_params', { moduleId, params });
    if (errors && errors.length > 0) {
      alert(errors.map(e => `${e.field}: ${e.message}`).join('\n'));
      return;
    }
  } catch (e) {
    console.warn('Validation skipped:', e);
  }

  statusText.textContent = `Running ${mod?.name || moduleId}...`;
  jobStatus.textContent = '1 active job';

  const badge = document.querySelector(`.nav-item[data-view="${moduleId}"] .nav-badge`);
  if (badge) { badge.className = 'nav-badge running'; badge.textContent = 'Running'; }

  try {
    const runId = await api.invoke('run_module', { moduleId, params });
    console.log('Run started:', runId);
  } catch (e) {
    statusText.textContent = 'Error';
    jobStatus.textContent = 'No active jobs';
    if (badge) { badge.className = 'nav-badge ready'; badge.textContent = 'Ready'; }
    console.error('Run failed to start:', e);
  }
};

// Helper to collect form params from the current view
function collectModuleParams(moduleId) {
  const params = {};
  // Collect input files
  params.input_files = (state.files[moduleId] || []).map(f => f.path || f.name);

  // Collect form values from the current view
  const inputs = document.querySelectorAll('.module-panel .form-input, .module-panel .form-select');
  inputs.forEach(input => {
    const label = input.closest('.form-group')?.querySelector('.form-label');
    if (label) {
      const key = label.textContent.trim().toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/_$/, '');
      if (input.type === 'number') params[key] = parseFloat(input.value);
      else params[key] = input.value;
    }
  });

  // Collect checkboxes
  const checkboxes = document.querySelectorAll('.module-panel .form-checkbox input[type="checkbox"]');
  checkboxes.forEach(cb => {
    const label = cb.closest('.form-checkbox')?.textContent?.trim();
    if (label) {
      const key = label.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/_$/, '');
      params[key] = cb.checked;
    }
  });

  return params;
}
```

- [ ] **Step 5: Test in Tauri dev mode**

Run: `cd crates/rb-app && cargo tauri dev`
Expected: app window opens showing the frontend, sidebar navigation works, clicking "Run" buttons triggers invoke calls (visible in console).

- [ ] **Step 6: Commit**

```bash
git add frontend/
git commit -m "feat(frontend): integrate Tauri API for file selection, module execution, and progress events"
```

---

## Task 13: Frontend — Project management UI + Custom Plot

**Files:**
- Modify: `frontend/js/app.js`

- [ ] **Step 1: Add project management to Dashboard**

In the `renderDashboard()` function, add project management section before the pipeline flow:

```javascript
// Add this block at the start of the dashboard-hero section, after the subtitle:
const projectUI = `
  <div class="card animate-slide-up" style="animation-delay:40ms;margin-bottom:24px">
    <div class="card-header">
      <span class="card-title">Project</span>
      <span class="badge badge-${state.projectOpen ? 'green' : 'muted'}">${state.projectOpen ? 'Open' : 'No Project'}</span>
    </div>
    <div id="projectName-display" style="font-family:var(--font-mono);font-size:0.88rem;color:var(--text-secondary);margin-bottom:16px;">
      ${state.projectName || 'No project open. Create or open a project to begin analysis.'}
    </div>
    <div style="display:flex;gap:10px">
      <button class="btn btn-primary btn-sm" onclick="createProjectDialog()"><i data-lucide="plus"></i> New Project</button>
      <button class="btn btn-secondary btn-sm" onclick="openProjectDialog()"><i data-lucide="folder-open"></i> Open Project</button>
    </div>
  </div>`;
```

Insert `${projectUI}` in the dashboard HTML between the hero and pipeline flow sections.

- [ ] **Step 2: Add project dialog functions**

```javascript
// Add to global scope:
state.projectOpen = false;
state.projectName = '';

window.createProjectDialog = async function() {
  const name = prompt('Project name:');
  if (!name) return;
  try {
    const dir = await api.invoke('select_directory');
    if (!dir) return;
    const project = await api.invoke('create_project', { name, dir });
    state.projectOpen = true;
    state.projectName = project.name;
    document.getElementById('projectName').textContent = project.name;
    navigate('dashboard');
  } catch (e) {
    alert('Failed to create project: ' + e);
  }
};

window.openProjectDialog = async function() {
  try {
    const dir = await api.invoke('select_directory');
    if (!dir) return;
    const project = await api.invoke('open_project', { dir });
    state.projectOpen = true;
    state.projectName = project.name;
    document.getElementById('projectName').textContent = project.name;
    navigate('dashboard');
  } catch (e) {
    alert('Failed to open project: ' + e);
  }
};
```

- [ ] **Step 3: Add Custom Plot tab to each module's results**

In each module's results panel (renderQC, renderTrimming, renderDifferential, renderNetwork), add a new tab:

```html
<div class="tab" data-tab="MODULE-custom">Custom Plot</div>
```

And a corresponding tab content:

```html
<div class="tab-content" data-tab="MODULE-custom">
  <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:12px;margin-bottom:16px">
    <div class="form-group"><label class="form-label">X Axis</label>
      <select class="form-select custom-plot-x"><option>Select column...</option></select></div>
    <div class="form-group"><label class="form-label">Y Axis</label>
      <select class="form-select custom-plot-y"><option>Select column...</option></select></div>
    <div class="form-group"><label class="form-label">Chart Type</label>
      <select class="form-select custom-plot-type">
        <option value="scatter">Scatter</option>
        <option value="bar">Bar</option>
        <option value="boxplot">Box Plot</option>
        <option value="histogram">Histogram</option>
      </select></div>
  </div>
  <button class="btn btn-primary btn-sm" onclick="renderCustomPlot('MODULE')"><i data-lucide="bar-chart-3"></i> Draw</button>
  <div class="chart-container" id="MODULE-custom-chart" style="height:380px;margin-top:16px;"></div>
</div>
```

Replace `MODULE` with the actual module ID (e.g., `qc`, `deseq`, `trim`, `wgcna`).

- [ ] **Step 4: Add custom plot rendering function**

```javascript
window.renderCustomPlot = function(moduleId) {
  const container = document.getElementById(`${moduleId}-custom-chart`);
  if (!container) return;

  const xSelect = document.querySelector(`.custom-plot-x`);
  const ySelect = document.querySelector(`.custom-plot-y`);
  const typeSelect = document.querySelector(`.custom-plot-type`);

  if (!xSelect?.value || !ySelect?.value || xSelect.value === 'Select column...') {
    return;
  }

  const chart = createChart(container);
  const chartType = typeSelect?.value || 'scatter';

  // In production, data comes from ModuleResult.summary
  // For now, generate demo data
  const n = 100;
  const xData = Array.from({length: n}, () => Math.random() * 10);
  const yData = Array.from({length: n}, () => Math.random() * 10);

  const option = {
    ...ECHART_THEME,
    title: { text: `${xSelect.value} vs ${ySelect.value}`, ...ECHART_THEME.title },
    xAxis: { type: 'value', name: xSelect.value, ...ECHART_THEME.xAxis },
    yAxis: { type: 'value', name: ySelect.value, ...ECHART_THEME.yAxis },
    grid: ECHART_THEME.grid,
  };

  if (chartType === 'scatter') {
    option.series = [{ type: 'scatter', data: xData.map((x, i) => [x, yData[i]]), symbolSize: 6, itemStyle: { color: '#0d7377' } }];
  } else if (chartType === 'bar') {
    option.xAxis.type = 'category';
    option.xAxis.data = xData.map((_, i) => i);
    option.series = [{ type: 'bar', data: yData, itemStyle: { color: '#3b6ea5' } }];
  }

  chart.setOption(option);
  window.addEventListener('resize', () => chart.resize());
};
```

- [ ] **Step 5: Test in browser**

Run: `cd frontend && python3 -m http.server 8090`
Verify: project buttons appear on dashboard, custom plot tab renders in each module.

- [ ] **Step 6: Commit**

```bash
git add frontend/
git commit -m "feat(frontend): add project management UI and custom plot tab"
```

---

## Task 14: End-to-end integration test

- [ ] **Step 1: Build and launch the full app**

```bash
cd crates/rb-app && cargo tauri dev
```

- [ ] **Step 2: Test project workflow**

1. Click "New Project" on Dashboard → enter name → select directory
2. Verify project.json created in selected directory
3. Verify header shows project name
4. Close and reopen via "Open Project"

- [ ] **Step 3: Test module workflow (with a sample file)**

1. Navigate to QC module
2. Click file drop zone → native file dialog opens
3. Select a FASTQ file
4. Click "Run QC"
5. Verify progress events appear in log
6. Verify results render after completion

- [ ] **Step 4: Test each chart type**

Navigate to DESeq2 → verify volcano plot and MA plot render with ECharts.
Navigate to WGCNA (demo) → verify module sizes and trait heatmap render.
Test custom plot tab → select columns and chart type → click Draw.

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "chore: end-to-end integration verification"
```

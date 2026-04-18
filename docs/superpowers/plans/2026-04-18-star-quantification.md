# STAR Quantification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two new analysis modules — `rb-star-index` (genome indexing) and `rb-star-align` (alignment + quantification) — that wrap the STAR_rs CLI to produce BAM files, per-gene counts, and a merged counts matrix ready for DESeq2.

**Architecture:** Extend rb-core with a `RunEvent` enum (replacing the flat `Progress` channel) so adapters can stream stderr/stdout logs alongside progress updates, plus a `CancellationToken` so subprocess-based adapters can kill child processes on user cancel. Add a `BinaryResolver` with JSON settings file for tool-path discovery (settings override → PATH fallback → structured NotFound error). Both STAR modules invoke the `star` binary as a subprocess, forward stderr lines as `RunEvent::Log`, and poll the cancel token each tick. `rb-star-align` also parses `Log.final.out` per sample and merges `ReadsPerGene.out.tab` files into a single `counts_matrix.tsv` for downstream DESeq2 use. rb-trimming is upgraded to the new resolver + cancel pattern in the same breaking change. Frontend gains a shared log panel, a Settings view for binary paths, two new STAR views, and a "Use this matrix in DESeq2" handoff button.

**Tech Stack:** Rust 2021, tokio async runtime, `tokio-util::sync::CancellationToken` for cooperative cancel, `directories` crate for cross-platform config paths, `which` crate for PATH lookup, existing Tauri v2 event system (`emit("run-progress")` + new `emit("run-log")`), vanilla JS + ECharts 5 for frontend.

---

## File Structure

### New files

| File | Responsibility |
|---|---|
| `crates/rb-core/src/run_event.rs` | `RunEvent` enum + `LogStream` enum + serde impls |
| `crates/rb-core/src/binary.rs` | `BinaryResolver`, `BinaryError`, `BinaryStatus`, `KNOWN_BINARIES` registry, settings.json I/O |
| `crates/rb-core/src/cancel.rs` | Thin re-export of `tokio_util::sync::CancellationToken` with `ModuleError::Cancelled` helper |
| `crates/rb-star-index/Cargo.toml` | crate manifest |
| `crates/rb-star-index/src/lib.rs` | `StarIndexModule` (Module trait impl) |
| `crates/rb-star-index/src/subprocess.rs` | `run_star_streaming` helper: spawn child, forward stderr as `RunEvent::Log`, poll cancel token |
| `crates/rb-star-align/Cargo.toml` | crate manifest |
| `crates/rb-star-align/src/lib.rs` | `StarAlignModule` (Module trait impl) — orchestrates per-sample runs |
| `crates/rb-star-align/src/subprocess.rs` | (initially copy of rb-star-index's helper; refactor opportunity noted) |
| `crates/rb-star-align/src/log_final.rs` | Parse `Log.final.out` → stats struct |
| `crates/rb-star-align/src/counts.rs` | Merge per-sample `ReadsPerGene.out.tab` into a single TSV matrix |
| `crates/rb-star-align/tests/fixtures/Log.final.out` | Golden STAR log fixture |
| `crates/rb-star-align/tests/fixtures/ReadsPerGene.sample1.out.tab` | Golden counts fixture |
| `crates/rb-star-align/tests/fixtures/ReadsPerGene.sample2.out.tab` | Golden counts fixture (different gene set for union test) |
| `crates/rb-app/src/commands/settings.rs` | Tauri commands: `get_binary_paths`, `set_binary_path`, `clear_binary_path` |

### Modified files

| File | Change |
|---|---|
| `Cargo.toml` (workspace root) | Add new members, add workspace deps (`tokio-util`, `directories`, `which`) |
| `crates/rb-core/Cargo.toml` | Add deps `tokio-util`, `directories`, `which`, `serde_json` |
| `crates/rb-core/src/lib.rs` | Export `run_event`, `binary`, `cancel` |
| `crates/rb-core/src/module.rs` | Update `Module::run` signature: `events_tx` (RunEvent) + `cancel: CancellationToken` |
| `crates/rb-core/src/runner.rs` | Channel type = `mpsc::Sender<RunEvent>`; forwarder splits Progress vs Log; add cancel token per run; `cancel()` calls `token.cancel()` then aborts |
| `crates/rb-qc/src/lib.rs` | Signature migration (mechanical) |
| `crates/rb-trimming/src/lib.rs` | Signature migration + BinaryResolver + child kill on cancel |
| `crates/rb-deseq2/src/lib.rs` | Signature migration (mechanical) |
| `crates/rb-app/src/state.rs` | Add `Arc<Mutex<BinaryResolver>>` to `AppState` |
| `crates/rb-app/src/main.rs` | Register `StarIndexModule`, `StarAlignModule`; register settings commands |
| `crates/rb-app/src/commands/mod.rs` | Declare `pub mod settings;` |
| `crates/rb-app/src/commands/project.rs` | Runner callback also handles `run-log` events |
| `frontend/js/app.js` | Log panel component, Settings view, STAR views, mapping chart, DESeq2 handoff |
| `frontend/index.html` | Browser mock shim adds star_index / star_align / settings fixtures |
| `frontend/css/style.css` | Log panel + Settings table styles |
| `README.md` | STAR install notes |
| `CLAUDE.md` | Add star_index + star_align to module list |

---

## Phase 1 — rb-core infrastructure (Tasks 1–8)

### Task 1: Add workspace dependencies and register new crate placeholders

**Files:**
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Update the root Cargo.toml**

Replace the contents of `/home/xzg/project/rust_brain/Cargo.toml` with:

```toml
[workspace]
resolver = "2"
members = [
    "crates/rb-core",
    "crates/rb-app",
    "crates/rb-qc",
    "crates/rb-trimming",
    "crates/rb-deseq2",
    "crates/rb-star-index",
    "crates/rb-star-align",
]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "process", "io-util"] }
tokio-util = { version = "0.7", features = ["rt"] }
async-trait = "0.1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
thiserror = "2"
directories = "5"
which = "6"
```

- [ ] **Step 2: Create crate directory skeletons**

Run:

```bash
mkdir -p crates/rb-star-index/src crates/rb-star-align/src crates/rb-star-align/tests/fixtures
```

Write `crates/rb-star-index/Cargo.toml`:

```toml
[package]
name = "rb-star-index"
version = "0.1.0"
edition = "2021"

[dependencies]
rb-core = { path = "../rb-core" }
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tokio-util.workspace = true
thiserror.workspace = true
```

Write `crates/rb-star-index/src/lib.rs`:

```rust
// placeholder — implemented in Task 11
```

Write `crates/rb-star-align/Cargo.toml`:

```toml
[package]
name = "rb-star-align"
version = "0.1.0"
edition = "2021"

[dependencies]
rb-core = { path = "../rb-core" }
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tokio-util.workspace = true
thiserror.workspace = true
```

Write `crates/rb-star-align/src/lib.rs`:

```rust
// placeholder — implemented in Task 17
```

- [ ] **Step 3: Verify workspace resolves**

Run: `cargo check --workspace`
Expected: success (the two new crates compile as empty libraries).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/rb-star-index crates/rb-star-align
git commit -m "chore: register rb-star-index and rb-star-align crates"
```

---

### Task 2: Add RunEvent enum to rb-core

**Files:**
- Create: `crates/rb-core/src/run_event.rs`
- Modify: `crates/rb-core/src/lib.rs`
- Modify: `crates/rb-core/Cargo.toml`

- [ ] **Step 1: Update rb-core Cargo.toml**

Replace `/home/xzg/project/rust_brain/crates/rb-core/Cargo.toml` contents with:

```toml
[package]
name = "rb-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tokio-util.workspace = true
async-trait.workspace = true
chrono.workspace = true
uuid.workspace = true
thiserror.workspace = true
directories.workspace = true
which.workspace = true
```

- [ ] **Step 2: Write failing test in a new file**

Create `crates/rb-core/src/run_event.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEvent {
    Progress { fraction: f64, message: String },
    Log { line: String, stream: LogStream },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_round_trip() {
        let ev = RunEvent::Progress {
            fraction: 0.5,
            message: "halfway".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let back: RunEvent = serde_json::from_str(&json).unwrap();
        match back {
            RunEvent::Progress { fraction, message } => {
                assert_eq!(fraction, 0.5);
                assert_eq!(message, "halfway");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn log_stderr_round_trip() {
        let ev = RunEvent::Log {
            line: "STAR starting".into(),
            stream: LogStream::Stderr,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"stream\":\"stderr\""));
        let back: RunEvent = serde_json::from_str(&json).unwrap();
        match back {
            RunEvent::Log { line, stream } => {
                assert_eq!(line, "STAR starting");
                assert_eq!(stream, LogStream::Stderr);
            }
            _ => panic!("wrong variant"),
        }
    }
}
```

Update `crates/rb-core/src/lib.rs`:

```rust
pub mod config;
pub mod module;
pub mod project;
pub mod run_event;
pub mod runner;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p rb-core run_event`
Expected: both tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-core/Cargo.toml crates/rb-core/src/run_event.rs crates/rb-core/src/lib.rs
git commit -m "feat(rb-core): add RunEvent enum with Progress and Log variants"
```

---

### Task 3: Add cancel-token helper module to rb-core

**Files:**
- Create: `crates/rb-core/src/cancel.rs`
- Modify: `crates/rb-core/src/lib.rs`

- [ ] **Step 1: Write the helper**

Create `crates/rb-core/src/cancel.rs`:

```rust
pub use tokio_util::sync::CancellationToken;

/// Convenience: check token and return Cancelled error if tripped.
#[macro_export]
macro_rules! bail_if_cancelled {
    ($token:expr) => {
        if $token.is_cancelled() {
            return Err($crate::module::ModuleError::Cancelled);
        }
    };
}
```

Update `crates/rb-core/src/lib.rs`:

```rust
pub mod cancel;
pub mod config;
pub mod module;
pub mod project;
pub mod run_event;
pub mod runner;
```

- [ ] **Step 2: Verify compile**

Run: `cargo check -p rb-core`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-core/src/cancel.rs crates/rb-core/src/lib.rs
git commit -m "feat(rb-core): expose CancellationToken + bail_if_cancelled macro"
```

---

### Task 4: Migrate Module trait signature (RunEvent + CancellationToken)

**Files:**
- Modify: `crates/rb-core/src/module.rs`

- [ ] **Step 1: Write failing test**

Append to `crates/rb-core/src/module.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cancel::CancellationToken;
    use crate::run_event::RunEvent;

    struct DummyModule;

    #[async_trait::async_trait]
    impl Module for DummyModule {
        fn id(&self) -> &str { "dummy" }
        fn name(&self) -> &str { "Dummy" }
        fn validate(&self, _params: &serde_json::Value) -> Vec<ValidationError> { vec![] }
        async fn run(
            &self,
            _params: &serde_json::Value,
            _project_dir: &std::path::Path,
            events_tx: tokio::sync::mpsc::Sender<RunEvent>,
            _cancel: CancellationToken,
        ) -> Result<ModuleResult, ModuleError> {
            let _ = events_tx
                .send(RunEvent::Progress { fraction: 1.0, message: "done".into() })
                .await;
            Ok(ModuleResult {
                output_files: vec![],
                summary: serde_json::json!({}),
                log: String::new(),
            })
        }
    }

    #[tokio::test]
    async fn module_trait_accepts_run_event_and_cancel() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<RunEvent>(4);
        let token = CancellationToken::new();
        let m = DummyModule;
        let res = m
            .run(&serde_json::json!({}), std::path::Path::new("/tmp"), tx, token)
            .await
            .unwrap();
        assert!(res.output_files.is_empty());
        match rx.recv().await.unwrap() {
            RunEvent::Progress { fraction, .. } => assert_eq!(fraction, 1.0),
            _ => panic!("expected Progress"),
        }
    }
}
```

- [ ] **Step 2: Run test (expected to fail)**

Run: `cargo test -p rb-core module::tests`
Expected: compile error — `Module::run` still takes `mpsc::Sender<Progress>`.

- [ ] **Step 3: Update Module trait**

Replace `/home/xzg/project/rust_brain/crates/rb-core/src/module.rs` with:

```rust
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use crate::cancel::CancellationToken;
use crate::run_event::RunEvent;

// Retained for backwards-compat of RunRecord/ModuleResult consumers;
// Progress is now a shape carried inside RunEvent::Progress.
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
        events_tx: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cancel::CancellationToken;
    use crate::run_event::RunEvent;

    struct DummyModule;

    #[async_trait::async_trait]
    impl Module for DummyModule {
        fn id(&self) -> &str { "dummy" }
        fn name(&self) -> &str { "Dummy" }
        fn validate(&self, _params: &serde_json::Value) -> Vec<ValidationError> { vec![] }
        async fn run(
            &self,
            _params: &serde_json::Value,
            _project_dir: &std::path::Path,
            events_tx: tokio::sync::mpsc::Sender<RunEvent>,
            _cancel: CancellationToken,
        ) -> Result<ModuleResult, ModuleError> {
            let _ = events_tx
                .send(RunEvent::Progress { fraction: 1.0, message: "done".into() })
                .await;
            Ok(ModuleResult {
                output_files: vec![],
                summary: serde_json::json!({}),
                log: String::new(),
            })
        }
    }

    #[tokio::test]
    async fn module_trait_accepts_run_event_and_cancel() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<RunEvent>(4);
        let token = CancellationToken::new();
        let m = DummyModule;
        let res = m
            .run(&serde_json::json!({}), std::path::Path::new("/tmp"), tx, token)
            .await
            .unwrap();
        assert!(res.output_files.is_empty());
        match rx.recv().await.unwrap() {
            RunEvent::Progress { fraction, .. } => assert_eq!(fraction, 1.0),
            _ => panic!("expected Progress"),
        }
    }
}
```

- [ ] **Step 4: Run test to verify it compiles but other crates fail**

Run: `cargo check -p rb-core`
Expected: rb-core passes; `cargo check --workspace` fails in rb-qc, rb-trimming, rb-deseq2 (signatures don't match). That's fine — fixed in Task 5.

Run: `cargo test -p rb-core module::tests`
Expected: test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-core/src/module.rs
git commit -m "feat(rb-core)!: migrate Module::run to RunEvent + CancellationToken

BREAKING CHANGE: all adapters must accept events_tx: mpsc::Sender<RunEvent>
and cancel: CancellationToken instead of the previous Sender<Progress>."
```

---

### Task 5: Migrate existing adapters (qc, trimming, deseq2) to the new signature

**Files:**
- Modify: `crates/rb-qc/src/lib.rs`
- Modify: `crates/rb-trimming/src/lib.rs`
- Modify: `crates/rb-deseq2/src/lib.rs`

- [ ] **Step 1: Update rb-qc signature**

In `crates/rb-qc/src/lib.rs`:
- Change imports top line from:
  ```rust
  use rb_core::module::{Module, ModuleError, ModuleResult, Progress, ValidationError};
  ```
  to:
  ```rust
  use rb_core::cancel::CancellationToken;
  use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
  use rb_core::run_event::RunEvent;
  ```
- Change the `run` signature parameter list from:
  ```rust
  progress_tx: mpsc::Sender<Progress>,
  ```
  to:
  ```rust
  events_tx: mpsc::Sender<RunEvent>,
  _cancel: CancellationToken,
  ```
- Replace every `progress_tx.send(Progress { fraction, message }).await` with `events_tx.send(RunEvent::Progress { fraction, message }).await`. There are two such sites (inside the loop and at the end).

- [ ] **Step 2: Update rb-deseq2 the same way**

Apply the identical pattern to `crates/rb-deseq2/src/lib.rs`. (Open the file and search for `progress_tx` / `Progress {` — same replacements as Step 1.)

- [ ] **Step 3: Update rb-trimming (signature only for now)**

In `crates/rb-trimming/src/lib.rs`, apply the same two replacements. Cancel support + BinaryResolver come in Task 9.

- [ ] **Step 4: Run workspace build**

Run: `cargo check --workspace`
Expected: the remaining compile failure is in `crates/rb-core/src/runner.rs` and in `crates/rb-app/src/commands/project.rs` (they still use the old channel type). Fixed in Task 6.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-qc crates/rb-trimming crates/rb-deseq2
git commit -m "refactor: migrate qc/trimming/deseq2 adapters to RunEvent signature"
```

---

### Task 6: Update Runner to forward RunEvent + per-run cancel token

**Files:**
- Modify: `crates/rb-core/src/runner.rs`
- Modify: `crates/rb-app/src/commands/project.rs`

- [ ] **Step 1: Write failing integration test**

Append to `crates/rb-core/src/runner.rs` a module-level test (after the existing impl):

```rust
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
        fn id(&self) -> &str { "emitslog" }
        fn name(&self) -> &str { "EmitsLog" }
        fn validate(&self, _p: &serde_json::Value) -> Vec<ValidationError> { vec![] }
        async fn run(
            &self,
            _p: &serde_json::Value,
            _d: &std::path::Path,
            events_tx: mpsc::Sender<RunEvent>,
            _c: CancellationToken,
        ) -> Result<ModuleResult, ModuleError> {
            events_tx.send(RunEvent::Log {
                line: "hello".into(), stream: LogStream::Stderr,
            }).await.ok();
            events_tx.send(RunEvent::Progress {
                fraction: 1.0, message: "done".into(),
            }).await.ok();
            Ok(ModuleResult { output_files: vec![], summary: serde_json::json!({}), log: "".into() })
        }
    }

    #[tokio::test]
    async fn runner_routes_log_and_progress_separately() {
        let tmp = tempfile::tempdir().unwrap();
        let project = Project::create("t", tmp.path()).unwrap();
        let got_log = Arc::new(Mutex::new(Vec::<String>::new()));
        let got_prog = Arc::new(Mutex::new(Vec::<f64>::new()));
        let log_for_cb = got_log.clone();
        let prog_for_cb = got_prog.clone();
        let runner = Runner::new(Arc::new(Mutex::new(project)))
            .on_progress(Box::new(move |_id, p| {
                prog_for_cb.blocking_lock().push(p.fraction);
            }))
            .on_log(Box::new(move |_id, line, _stream| {
                log_for_cb.blocking_lock().push(line);
            }));
        let id = runner
            .spawn(Arc::new(EmitsLogModule), serde_json::json!({}))
            .await
            .unwrap();
        // Poll until the run finishes (status leaves Running)
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let done = runner.project().lock().await.runs.iter()
                .any(|r| r.id == id && matches!(r.status, RunStatus::Done));
            if done { break; }
        }
        assert_eq!(got_log.lock().await.as_slice(), &["hello".to_string()]);
        assert_eq!(got_prog.lock().await.as_slice(), &[1.0]);
    }
}
```

Add to `crates/rb-core/Cargo.toml` under `[dev-dependencies]`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rb-core runner_tests`
Expected: compile errors — `on_log` doesn't exist, channel type is still `Progress`.

- [ ] **Step 3: Rewrite runner.rs**

Replace `/home/xzg/project/rust_brain/crates/rb-core/src/runner.rs` with:

```rust
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
        let run_id = {
            let mut proj = self.project.lock().await;
            proj.create_run(module.id(), params.clone()).id
        };

        {
            let mut proj = self.project.lock().await;
            if let Some(run) = proj.runs.iter_mut().find(|r| r.id == run_id) {
                run.status = RunStatus::Running;
                run.started_at = Some(Utc::now());
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

            let result = module.run(&params, &run_dir, events_tx, cancel_for_module).await;

            let (status, module_result_opt) = match &result {
                Ok(mr) => (RunStatus::Done, Some(mr.clone())),
                Err(crate::module::ModuleError::Cancelled) => (RunStatus::Cancelled, None),
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
            active.insert(run_id.clone(), ActiveRun { handle, cancel: cancel_token });
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
        }

        let mut proj = self.project.lock().await;
        if let Some(run) = proj.runs.iter_mut().find(|r| r.id == run_id) {
            run.status = RunStatus::Cancelled;
            run.finished_at = Some(Utc::now());
        }
        let _ = proj.save();
    }
}
```

Keep the `runner_tests` module from Step 1 below this impl.

- [ ] **Step 4: Update rb-app project.rs to register the new on_log callback**

In `crates/rb-app/src/commands/project.rs`, replace `setup_runner` with:

```rust
fn setup_runner(project: Project, app: &AppHandle) -> Runner {
    let project_arc = Arc::new(tokio::sync::Mutex::new(project));
    let app_for_prog = app.clone();
    let app_for_log = app.clone();
    Runner::new(project_arc)
        .on_progress(Box::new(move |run_id, progress| {
            let _ = app_for_prog.emit(
                "run-progress",
                serde_json::json!({
                    "runId": run_id,
                    "fraction": progress.fraction,
                    "message": progress.message,
                }),
            );
        }))
        .on_log(Box::new(move |run_id, line, stream| {
            let _ = app_for_log.emit(
                "run-log",
                serde_json::json!({
                    "runId": run_id,
                    "line": line,
                    "stream": match stream {
                        rb_core::run_event::LogStream::Stdout => "stdout",
                        rb_core::run_event::LogStream::Stderr => "stderr",
                    },
                }),
            );
        }))
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p rb-core runner_tests`
Expected: the new test passes.

Run: `cargo check --workspace`
Expected: success.

- [ ] **Step 6: Commit**

```bash
git add crates/rb-core/src/runner.rs crates/rb-core/Cargo.toml crates/rb-app/src/commands/project.rs
git commit -m "feat(rb-core): route RunEvent via separate progress/log callbacks + per-run cancel token"
```

---

### Task 7: Implement BinaryResolver

**Files:**
- Create: `crates/rb-core/src/binary.rs`
- Modify: `crates/rb-core/src/lib.rs`

- [ ] **Step 1: Write failing tests first**

Create `crates/rb-core/src/binary.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Registry of binaries that rust_brain knows about.
/// Each entry has an id used as both the CLI name (what's looked up on PATH)
/// and the settings.json key, plus a human-readable install hint.
pub struct KnownBinary {
    pub id: &'static str,
    pub display_name: &'static str,
    pub install_hint: &'static str,
}

pub const KNOWN_BINARIES: &[KnownBinary] = &[
    KnownBinary {
        id: "star",
        display_name: "STAR (STAR_rs)",
        install_hint: "Build from https://github.com/AI4S-YB/STAR_rs and set the path in Settings, or add the `star` binary to PATH.",
    },
    KnownBinary {
        id: "cutadapt-rs",
        display_name: "cutadapt-rs",
        install_hint: "Build from https://github.com/AI4S-YB/cutadapt-rs and set the path in Settings, or add the `cutadapt-rs` binary to PATH.",
    },
];

#[derive(Debug, thiserror::Error)]
pub enum BinaryError {
    #[error("binary '{name}' not found. Searched: {searched:?}. {hint}")]
    NotFound { name: String, searched: Vec<String>, hint: String },
    #[error("path '{0}' is not an executable file")]
    NotExecutable(PathBuf),
    #[error("settings I/O error: {0}")]
    SettingsIo(#[from] std::io::Error),
    #[error("settings parse error: {0}")]
    SettingsParse(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsFile {
    #[serde(default)]
    pub binary_paths: HashMap<String, Option<PathBuf>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BinaryStatus {
    pub id: String,
    pub display_name: String,
    pub configured_path: Option<PathBuf>,
    pub detected_on_path: Option<PathBuf>,
    pub install_hint: String,
}

pub struct BinaryResolver {
    settings_path: PathBuf,
    settings: SettingsFile,
}

impl BinaryResolver {
    /// Cross-platform settings path using the `directories` crate.
    pub fn default_settings_path() -> PathBuf {
        if let Some(pd) = directories::ProjectDirs::from("", "", "rust_brain") {
            return pd.config_dir().join("settings.json");
        }
        PathBuf::from("settings.json")
    }

    pub fn load() -> Result<Self, BinaryError> {
        Self::load_from(Self::default_settings_path())
    }

    pub fn load_from(path: PathBuf) -> Result<Self, BinaryError> {
        let settings = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            serde_json::from_str(&text)?
        } else {
            SettingsFile::default()
        };
        Ok(Self { settings_path: path, settings })
    }

    pub fn resolve(&self, name: &str) -> Result<PathBuf, BinaryError> {
        // Configured override takes precedence
        if let Some(Some(p)) = self.settings.binary_paths.get(name) {
            if is_executable(p) {
                return Ok(p.clone());
            }
            return Err(BinaryError::NotExecutable(p.clone()));
        }
        // Fall back to PATH
        if let Ok(found) = which::which(name) {
            return Ok(found);
        }
        let hint = KNOWN_BINARIES
            .iter()
            .find(|k| k.id == name)
            .map(|k| k.install_hint.to_string())
            .unwrap_or_else(|| format!("No install hint registered for '{}'.", name));
        Err(BinaryError::NotFound {
            name: name.to_string(),
            searched: vec!["settings.json override".into(), "$PATH".into()],
            hint,
        })
    }

    pub fn set(&mut self, name: &str, path: PathBuf) -> Result<(), BinaryError> {
        if !is_executable(&path) {
            return Err(BinaryError::NotExecutable(path));
        }
        self.settings.binary_paths.insert(name.to_string(), Some(path));
        self.save()
    }

    pub fn clear(&mut self, name: &str) -> Result<(), BinaryError> {
        self.settings.binary_paths.remove(name);
        self.save()
    }

    fn save(&self) -> Result<(), BinaryError> {
        if let Some(parent) = self.settings_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&self.settings)?;
        std::fs::write(&self.settings_path, text)?;
        Ok(())
    }

    pub fn list_known(&self) -> Vec<BinaryStatus> {
        KNOWN_BINARIES
            .iter()
            .map(|k| {
                let configured = self.settings.binary_paths.get(k.id).and_then(|o| o.clone());
                let detected = which::which(k.id).ok();
                BinaryStatus {
                    id: k.id.to_string(),
                    display_name: k.display_name.to_string(),
                    configured_path: configured,
                    detected_on_path: detected,
                    install_hint: k.install_hint.to_string(),
                }
            })
            .collect()
    }
}

fn is_executable(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_exec(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\necho hi\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(&p).unwrap().permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&p, perm).unwrap();
        }
        p
    }

    #[test]
    fn override_takes_precedence_over_path() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = write_exec(tmp.path(), "star");
        let settings = tmp.path().join("settings.json");
        let mut r = BinaryResolver::load_from(settings).unwrap();
        r.set("star", fake.clone()).unwrap();
        let resolved = r.resolve("star").unwrap();
        assert_eq!(resolved, fake);
    }

    #[test]
    fn not_found_includes_install_hint() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = tmp.path().join("settings.json");
        let r = BinaryResolver::load_from(settings).unwrap();
        // Use a name guaranteed to be missing from PATH and registered.
        let err = r.resolve("star").unwrap_err();
        match err {
            BinaryError::NotFound { hint, .. } => {
                assert!(hint.contains("STAR_rs"), "hint should reference STAR_rs: {}", hint);
            }
            _ => {
                // On CI a real `star` may exist on PATH; in that case, the test is inapplicable.
                // Accept success too.
            }
        }
    }

    #[test]
    fn set_rejects_non_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("not-exec.txt");
        std::fs::write(&p, "hi").unwrap();
        let settings = tmp.path().join("settings.json");
        let mut r = BinaryResolver::load_from(settings).unwrap();
        let res = r.set("star", p);
        assert!(matches!(res, Err(BinaryError::NotExecutable(_))));
    }

    #[test]
    fn clear_removes_override() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = write_exec(tmp.path(), "star");
        let settings = tmp.path().join("settings.json");
        let mut r = BinaryResolver::load_from(settings).unwrap();
        r.set("star", fake).unwrap();
        r.clear("star").unwrap();
        let statuses = r.list_known();
        let s = statuses.iter().find(|b| b.id == "star").unwrap();
        assert!(s.configured_path.is_none());
    }

    #[test]
    fn list_known_contains_all_registered() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = tmp.path().join("settings.json");
        let r = BinaryResolver::load_from(settings).unwrap();
        let ids: Vec<_> = r.list_known().into_iter().map(|b| b.id).collect();
        assert!(ids.contains(&"star".to_string()));
        assert!(ids.contains(&"cutadapt-rs".to_string()));
    }
}
```

Update `crates/rb-core/src/lib.rs`:

```rust
pub mod binary;
pub mod cancel;
pub mod config;
pub mod module;
pub mod project;
pub mod run_event;
pub mod runner;
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p rb-core binary`
Expected: all 5 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-core/src/binary.rs crates/rb-core/src/lib.rs
git commit -m "feat(rb-core): add BinaryResolver with settings.json + PATH fallback"
```

---

### Task 8: Add Tauri settings commands and wire into AppState

**Files:**
- Create: `crates/rb-app/src/commands/settings.rs`
- Modify: `crates/rb-app/src/commands/mod.rs`
- Modify: `crates/rb-app/src/state.rs`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: Write the settings commands module**

Create `crates/rb-app/src/commands/settings.rs`:

```rust
use std::path::PathBuf;

use rb_core::binary::{BinaryResolver, BinaryStatus};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn get_binary_paths(state: State<'_, AppState>) -> Result<Vec<BinaryStatus>, String> {
    let resolver = state.binary_resolver.lock().await;
    Ok(resolver.list_known())
}

#[tauri::command]
pub async fn set_binary_path(
    name: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut resolver = state.binary_resolver.lock().await;
    resolver
        .set(&name, PathBuf::from(path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_binary_path(
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut resolver = state.binary_resolver.lock().await;
    resolver.clear(&name).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Declare the module in commands/mod.rs**

Replace `crates/rb-app/src/commands/mod.rs` with:

```rust
pub mod files;
pub mod modules;
pub mod project;
pub mod settings;
```

- [ ] **Step 3: Add the resolver to AppState**

Replace the `AppState` struct/impl block in `crates/rb-app/src/state.rs` with:

```rust
pub struct AppState {
    pub registry: Arc<ModuleRegistry>,
    pub runner: Arc<Mutex<Option<Runner>>>,
    pub recent_projects: Arc<Mutex<Vec<PathBuf>>>,
    pub binary_resolver: Arc<Mutex<rb_core::binary::BinaryResolver>>,
}

impl AppState {
    pub fn new(registry: ModuleRegistry) -> Self {
        let resolver = rb_core::binary::BinaryResolver::load()
            .expect("failed to load binary resolver settings");
        Self {
            registry: Arc::new(registry),
            runner: Arc::new(Mutex::new(None)),
            recent_projects: Arc::new(Mutex::new(Vec::new())),
            binary_resolver: Arc::new(Mutex::new(resolver)),
        }
    }
}
```

- [ ] **Step 4: Register the new commands in main.rs**

In `crates/rb-app/src/main.rs`, update the `invoke_handler!` macro to include the three settings commands. Final `main.rs` body becomes:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::{AppState, ModuleRegistry};
use std::sync::Arc;

fn main() {
    let mut registry = ModuleRegistry::new();
    registry.register(Arc::new(rb_deseq2::DeseqModule));
    registry.register(Arc::new(rb_qc::QcModule));
    registry.register(Arc::new(rb_trimming::TrimmingModule));
    // star_index and star_align registered in Tasks 13 and 18

    tauri::Builder::default()
        .manage(AppState::new(registry))
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
            commands::settings::get_binary_paths,
            commands::settings::set_binary_path,
            commands::settings::clear_binary_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RustBrain");
}
```

- [ ] **Step 5: Verify build**

Run: `cargo check --workspace`
Expected: success.

- [ ] **Step 6: Commit**

```bash
git add crates/rb-app/src/commands/settings.rs crates/rb-app/src/commands/mod.rs crates/rb-app/src/state.rs crates/rb-app/src/main.rs
git commit -m "feat(rb-app): add Tauri settings commands for binary paths"
```

---

### Task 9: Upgrade rb-trimming to use BinaryResolver + cooperative cancellation

**Files:**
- Modify: `crates/rb-trimming/src/lib.rs`

- [ ] **Step 1: Rewrite rb-trimming/src/lib.rs**

Replace `/home/xzg/project/rust_brain/crates/rb-trimming/src/lib.rs` with:

```rust
use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::{LogStream, RunEvent};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub struct TrimmingModule;

#[async_trait::async_trait]
impl Module for TrimmingModule {
    fn id(&self) -> &str { "trimming" }
    fn name(&self) -> &str { "Cutadapt Adapter Trimming" }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        match params.get("input_files") {
            None => errors.push(ValidationError {
                field: "input_files".into(),
                message: "input_files must be a non-empty array".into(),
            }),
            Some(v) => {
                if v.as_array().map_or(true, |a| a.is_empty()) {
                    errors.push(ValidationError {
                        field: "input_files".into(),
                        message: "input_files must be a non-empty array".into(),
                    });
                }
            }
        }
        // Surface resolver errors at validate time for UI feedback.
        if let Ok(resolver) = BinaryResolver::load() {
            if let Err(e) = resolver.resolve("cutadapt-rs") {
                errors.push(ValidationError {
                    field: "binary".into(),
                    message: e.to_string(),
                });
            }
        }
        errors
    }

    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        events_tx: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        let errors = self.validate(params);
        if !errors.is_empty() {
            return Err(ModuleError::InvalidParams(errors));
        }

        let resolver = BinaryResolver::load().map_err(|e| ModuleError::ToolError(e.to_string()))?;
        let bin = resolver
            .resolve("cutadapt-rs")
            .map_err(|e| ModuleError::ToolError(e.to_string()))?;

        let input_files: Vec<PathBuf> = params["input_files"]
            .as_array().unwrap().iter()
            .filter_map(|v| v.as_str().map(PathBuf::from))
            .collect();

        let adapter = params.get("adapter").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let quality_cutoff = params.get("quality_cutoff").and_then(|v| v.as_u64()).unwrap_or(20);
        let min_length = params.get("min_length").and_then(|v| v.as_u64()).unwrap_or(20);

        let output_dir = project_dir.join("trimmed");
        std::fs::create_dir_all(&output_dir)?;

        let total = input_files.len();
        let mut output_files = Vec::new();
        let mut file_summaries = Vec::new();
        let mut log_lines = Vec::new();

        for (idx, input_path) in input_files.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(ModuleError::Cancelled);
            }
            let fraction = idx as f64 / total as f64;
            let _ = events_tx
                .send(RunEvent::Progress {
                    fraction,
                    message: format!("Trimming {} ({}/{})", input_path.display(), idx + 1, total),
                })
                .await;

            let input_str = input_path.to_string_lossy().to_string();
            let file_name = input_path
                .file_name().map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("output_{}.fastq.gz", idx));
            let output_path = output_dir.join(&file_name);
            let output_str = output_path.to_string_lossy().to_string();

            let mut cmd = Command::new(&bin);
            cmd.arg("-o").arg(&output_str);
            cmd.arg("-q").arg(quality_cutoff.to_string());
            cmd.arg("-m").arg(min_length.to_string());
            if !adapter.is_empty() { cmd.arg("-a").arg(&adapter); }
            cmd.arg(&input_str);
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

            match cmd.spawn() {
                Ok(mut child) => {
                    let stdout = child.stdout.take().expect("piped");
                    let stderr = child.stderr.take().expect("piped");
                    let tx_out = events_tx.clone();
                    let tx_err = events_tx.clone();
                    tokio::spawn(async move {
                        let mut r = BufReader::new(stdout).lines();
                        while let Ok(Some(line)) = r.next_line().await {
                            let _ = tx_out.send(RunEvent::Log { line, stream: LogStream::Stdout }).await;
                        }
                    });
                    tokio::spawn(async move {
                        let mut r = BufReader::new(stderr).lines();
                        while let Ok(Some(line)) = r.next_line().await {
                            let _ = tx_err.send(RunEvent::Log { line, stream: LogStream::Stderr }).await;
                        }
                    });
                    let status_or_cancel = tokio::select! {
                        s = child.wait() => Ok(s),
                        _ = cancel.cancelled() => {
                            let _ = child.kill().await;
                            Err(ModuleError::Cancelled)
                        }
                    };
                    match status_or_cancel {
                        Err(e) => return Err(e),
                        Ok(Ok(status)) => {
                            if status.success() {
                                if output_path.exists() { output_files.push(output_path.clone()); }
                                file_summaries.push(serde_json::json!({
                                    "file": input_str,
                                    "output": output_str,
                                    "status": "ok",
                                }));
                                log_lines.push(format!("OK: {} -> {}", input_str, output_str));
                            } else {
                                file_summaries.push(serde_json::json!({
                                    "file": input_str,
                                    "status": "error",
                                    "exit_code": status.code(),
                                }));
                                log_lines.push(format!("ERROR: {} exit={}", input_str, status.code().unwrap_or(-1)));
                            }
                        }
                        Ok(Err(e)) => {
                            file_summaries.push(serde_json::json!({
                                "file": input_str, "status": "error", "error": e.to_string(),
                            }));
                            log_lines.push(format!("ERROR waiting for child: {}", e));
                        }
                    }
                }
                Err(e) => {
                    file_summaries.push(serde_json::json!({
                        "file": input_str, "status": "error", "error": e.to_string(),
                    }));
                    log_lines.push(format!("ERROR spawning: {}", e));
                }
            }
        }

        let _ = events_tx
            .send(RunEvent::Progress { fraction: 1.0, message: "Done".into() })
            .await;

        let ok_count = file_summaries.iter().filter(|v| v["status"] == "ok").count();
        let summary = serde_json::json!({
            "total_files": total,
            "trimmed_ok": ok_count,
            "output_directory": output_dir.display().to_string(),
            "adapter": adapter,
            "quality_cutoff": quality_cutoff,
            "min_length": min_length,
            "files": file_summaries,
        });

        Ok(ModuleResult {
            output_files,
            summary,
            log: log_lines.join("\n"),
        })
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cargo check -p rb-trimming`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-trimming/src/lib.rs
git commit -m "refactor(rb-trimming): use BinaryResolver + streaming logs + cancel on kill"
```

---

## Phase 2 — rb-star-index module (Tasks 10–13)

### Task 10: Add rb-star-index subprocess helper

**Files:**
- Create: `crates/rb-star-index/src/subprocess.rs`

- [ ] **Step 1: Write the helper**

Create `crates/rb-star-index/src/subprocess.rs`:

```rust
use rb_core::cancel::CancellationToken;
use rb_core::module::ModuleError;
use rb_core::run_event::{LogStream, RunEvent};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Spawn `bin` with the given args, stream both stdout/stderr back via events_tx,
/// and honour cooperative cancellation by killing the child.
pub async fn run_star_streaming(
    bin: &PathBuf,
    args: &[String],
    events_tx: mpsc::Sender<RunEvent>,
    cancel: CancellationToken,
) -> Result<std::process::ExitStatus, ModuleError> {
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| ModuleError::ToolError(format!(
        "failed to spawn {}: {}", bin.display(), e,
    )))?;

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let tx_out = events_tx.clone();
    let tx_err = events_tx.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx_out.send(RunEvent::Log { line, stream: LogStream::Stdout }).await;
        }
    });
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx_err.send(RunEvent::Log { line, stream: LogStream::Stderr }).await;
        }
    });

    tokio::select! {
        status = child.wait() => status.map_err(|e| ModuleError::ToolError(e.to_string())),
        _ = cancel.cancelled() => {
            let _ = child.kill().await;
            Err(ModuleError::Cancelled)
        }
    }
}
```

- [ ] **Step 2: Verify compile**

Run: `cargo check -p rb-star-index`
Expected: success. (`lib.rs` still has its placeholder and doesn't import this file yet.)

- [ ] **Step 3: Commit**

```bash
git add crates/rb-star-index/src/subprocess.rs
git commit -m "feat(rb-star-index): add subprocess runner with streaming logs and cancel"
```

---

### Task 11: Implement StarIndexModule (validate + run)

**Files:**
- Modify: `crates/rb-star-index/src/lib.rs`

- [ ] **Step 1: Write validate() tests first**

Replace `crates/rb-star-index/src/lib.rs` with the following (tests at the bottom; implementation above):

```rust
mod subprocess;

use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::sync::mpsc;

pub struct StarIndexModule;

#[async_trait::async_trait]
impl Module for StarIndexModule {
    fn id(&self) -> &str { "star_index" }
    fn name(&self) -> &str { "STAR Genome Index" }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let require_path = |field: &str, errors: &mut Vec<ValidationError>| {
            match params.get(field).and_then(|v| v.as_str()) {
                None => errors.push(ValidationError {
                    field: field.into(),
                    message: format!("{} is required", field),
                }),
                Some(s) => {
                    if !Path::new(s).exists() {
                        errors.push(ValidationError {
                            field: field.into(),
                            message: format!("{} does not exist: {}", field, s),
                        });
                    }
                }
            }
        };
        require_path("genome_fasta", &mut errors);
        require_path("gtf_file", &mut errors);

        if let Some(v) = params.get("extra_args") {
            if !v.is_array() || !v.as_array().unwrap().iter().all(|x| x.is_string()) {
                errors.push(ValidationError {
                    field: "extra_args".into(),
                    message: "extra_args must be an array of strings".into(),
                });
            }
        }

        if let Ok(resolver) = BinaryResolver::load() {
            if let Err(e) = resolver.resolve("star") {
                errors.push(ValidationError { field: "binary".into(), message: e.to_string() });
            }
        }

        errors
    }

    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        events_tx: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        let errors = self.validate(params);
        if !errors.is_empty() {
            return Err(ModuleError::InvalidParams(errors));
        }

        let resolver = BinaryResolver::load().map_err(|e| ModuleError::ToolError(e.to_string()))?;
        let bin = resolver.resolve("star").map_err(|e| ModuleError::ToolError(e.to_string()))?;

        let genome_fasta = params["genome_fasta"].as_str().unwrap();
        let gtf_file = params["gtf_file"].as_str().unwrap();
        let threads = params.get("threads").and_then(|v| v.as_u64()).unwrap_or(4);
        let sjdb_overhang = params.get("sjdb_overhang").and_then(|v| v.as_u64()).unwrap_or(100);
        let sa_nbases = params
            .get("genome_sa_index_nbases").and_then(|v| v.as_u64()).unwrap_or(14);
        let extra: Vec<String> = params.get("extra_args")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        // project_dir here is the run directory prepared by Runner.
        let out_dir = project_dir.to_path_buf();
        std::fs::create_dir_all(&out_dir)?;

        let _ = events_tx.send(RunEvent::Progress {
            fraction: 0.0,
            message: "Starting genome generation".into(),
        }).await;

        let mut args: Vec<String> = vec![
            "--runMode".into(), "genomeGenerate".into(),
            "--genomeDir".into(), out_dir.display().to_string(),
            "--genomeFastaFiles".into(), genome_fasta.into(),
            "--sjdbGTFfile".into(), gtf_file.into(),
            "--runThreadN".into(), threads.to_string(),
            "--sjdbOverhang".into(), sjdb_overhang.to_string(),
            "--genomeSAindexNbases".into(), sa_nbases.to_string(),
        ];
        args.extend(extra.iter().cloned());

        let started = Instant::now();
        let status = subprocess::run_star_streaming(&bin, &args, events_tx.clone(), cancel).await?;
        let elapsed = started.elapsed().as_secs();

        if !status.success() {
            return Err(ModuleError::ToolError(format!(
                "STAR genomeGenerate exited with code {}",
                status.code().unwrap_or(-1),
            )));
        }

        // Verify key artifacts.
        let required = ["SA", "SAindex", "Genome", "chrNameLength.txt", "geneInfo.tab"];
        let mut output_files: Vec<PathBuf> = Vec::new();
        for name in required {
            let p = out_dir.join(name);
            if !p.exists() {
                return Err(ModuleError::ToolError(format!("missing expected artifact: {}", p.display())));
            }
            output_files.push(p);
        }
        let log_out = out_dir.join("Log.out");
        if log_out.exists() { output_files.push(log_out); }

        let index_size = dir_size(&out_dir).unwrap_or(0);

        let _ = events_tx.send(RunEvent::Progress { fraction: 1.0, message: "Done".into() }).await;

        let summary = serde_json::json!({
            "genome_dir": out_dir.display().to_string(),
            "genome_fasta": genome_fasta,
            "gtf_file": gtf_file,
            "threads": threads,
            "sjdb_overhang": sjdb_overhang,
            "genome_sa_index_nbases": sa_nbases,
            "index_size_bytes": index_size,
            "generation_seconds": elapsed,
        });

        Ok(ModuleResult { output_files, summary, log: String::new() })
    }
}

fn dir_size(p: &Path) -> std::io::Result<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(p)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() { total += meta.len(); }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_requires_genome_fasta_and_gtf() {
        let m = StarIndexModule;
        let errs = m.validate(&serde_json::json!({}));
        let fields: Vec<_> = errs.iter().map(|e| e.field.clone()).collect();
        assert!(fields.iter().any(|f| f == "genome_fasta"));
        assert!(fields.iter().any(|f| f == "gtf_file"));
    }

    #[test]
    fn validate_rejects_missing_files() {
        let m = StarIndexModule;
        let errs = m.validate(&serde_json::json!({
            "genome_fasta": "/nonexistent/genome.fa",
            "gtf_file": "/nonexistent/anno.gtf",
        }));
        assert!(errs.iter().any(|e| e.field == "genome_fasta"));
        assert!(errs.iter().any(|e| e.field == "gtf_file"));
    }

    #[test]
    fn validate_rejects_bad_extra_args() {
        let m = StarIndexModule;
        let tmp = tempfile::tempdir().unwrap();
        let fa = tmp.path().join("g.fa");
        let gtf = tmp.path().join("a.gtf");
        std::fs::write(&fa, "").unwrap();
        std::fs::write(&gtf, "").unwrap();
        let errs = m.validate(&serde_json::json!({
            "genome_fasta": fa,
            "gtf_file": gtf,
            "extra_args": "not-an-array",
        }));
        assert!(errs.iter().any(|e| e.field == "extra_args"));
    }
}
```

- [ ] **Step 2: Add dev-dep to rb-star-index Cargo.toml**

Append to `crates/rb-star-index/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
tokio = { workspace = true, features = ["rt-multi-thread", "macros"] }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rb-star-index`
Expected: 3 tests pass. (The resolver-not-found validation error is tolerated because tests don't set STAR; the test only checks specific required field errors.)

- [ ] **Step 4: Commit**

```bash
git add crates/rb-star-index/src/lib.rs crates/rb-star-index/Cargo.toml
git commit -m "feat(rb-star-index): implement StarIndexModule with validate + run"
```

---

### Task 12: Register rb-star-index in rb-app

**Files:**
- Modify: `crates/rb-app/Cargo.toml`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: Add dep**

In `crates/rb-app/Cargo.toml`, under `[dependencies]`, add (preserving existing entries):

```toml
rb-star-index = { path = "../rb-star-index" }
```

- [ ] **Step 2: Register module**

In `crates/rb-app/src/main.rs`, add one line after the other `registry.register(...)` calls:

```rust
registry.register(Arc::new(rb_star_index::StarIndexModule));
```

- [ ] **Step 3: Build**

Run: `cargo check --workspace`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-app/Cargo.toml crates/rb-app/src/main.rs
git commit -m "feat(rb-app): register StarIndexModule"
```

---

## Phase 3 — rb-star-align module (Tasks 13–18)

### Task 13: Add Log.final.out parser with fixture tests

**Files:**
- Create: `crates/rb-star-align/src/log_final.rs`
- Create: `crates/rb-star-align/tests/fixtures/Log.final.out`

- [ ] **Step 1: Create the fixture**

Write `crates/rb-star-align/tests/fixtures/Log.final.out`:

```
                                 Started job on |	Apr 18 10:00:00
                             Started mapping on |	Apr 18 10:00:05
                                    Finished on |	Apr 18 10:05:00
       Mapping speed, Million of reads per hour |	120.00

                          Number of input reads |	10000000
                      Average input read length |	100

                                    UNIQUE READS:
                   Uniquely mapped reads number |	9000000
                        Uniquely mapped reads % |	90.00%
                          Average mapped length |	99.50

                             MULTI-MAPPING READS:
        Number of reads mapped to multiple loci |	500000
             % of reads mapped to multiple loci |	5.00%
        Number of reads mapped to too many loci |	0
             % of reads mapped to too many loci |	0.00%

                                 UNMAPPED READS:
 Number of reads unmapped: too many mismatches |	100000
      % of reads unmapped: too many mismatches |	1.00%
           Number of reads unmapped: too short |	300000
                % of reads unmapped: too short |	3.00%
               Number of reads unmapped: other |	100000
                    % of reads unmapped: other |	1.00%
```

- [ ] **Step 2: Write the parser + tests**

Create `crates/rb-star-align/src/log_final.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct LogFinalStats {
    pub input_reads: Option<u64>,
    pub uniquely_mapped: Option<u64>,
    pub uniquely_mapped_pct: Option<f64>,
    pub multi_mapped: Option<u64>,
    pub multi_mapped_pct: Option<f64>,
    pub unmapped: Option<u64>,
    pub unmapped_pct: Option<f64>,
}

pub fn parse(text: &str) -> LogFinalStats {
    let mut s = LogFinalStats::default();
    let mut unmapped_sum: u64 = 0;
    let mut unmapped_pct_sum: f64 = 0.0;
    let mut saw_unmapped = false;

    for line in text.lines() {
        let Some((key, val)) = line.split_once('|') else { continue; };
        let key = key.trim();
        let val = val.trim();
        match key {
            "Number of input reads" => s.input_reads = parse_u64(val),
            "Uniquely mapped reads number" => s.uniquely_mapped = parse_u64(val),
            "Uniquely mapped reads %" => s.uniquely_mapped_pct = parse_pct(val),
            "Number of reads mapped to multiple loci" => s.multi_mapped = parse_u64(val),
            "% of reads mapped to multiple loci" => s.multi_mapped_pct = parse_pct(val),
            k if k.starts_with("Number of reads unmapped:") => {
                if let Some(n) = parse_u64(val) {
                    unmapped_sum += n;
                    saw_unmapped = true;
                }
            }
            k if k.starts_with("% of reads unmapped:") => {
                if let Some(p) = parse_pct(val) {
                    unmapped_pct_sum += p;
                    saw_unmapped = true;
                }
            }
            _ => {}
        }
    }
    if saw_unmapped {
        s.unmapped = Some(unmapped_sum);
        s.unmapped_pct = Some(unmapped_pct_sum);
    }
    s
}

fn parse_u64(v: &str) -> Option<u64> {
    v.split_whitespace().next()?.replace(',', "").parse().ok()
}

fn parse_pct(v: &str) -> Option<f64> {
    v.trim_end_matches('%').trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../tests/fixtures/Log.final.out");

    #[test]
    fn parses_key_counts() {
        let s = parse(FIXTURE);
        assert_eq!(s.input_reads, Some(10_000_000));
        assert_eq!(s.uniquely_mapped, Some(9_000_000));
        assert_eq!(s.uniquely_mapped_pct, Some(90.0));
        assert_eq!(s.multi_mapped, Some(500_000));
        assert_eq!(s.multi_mapped_pct, Some(5.0));
    }

    #[test]
    fn sums_unmapped_across_categories() {
        let s = parse(FIXTURE);
        // too_many_mismatches (100k) + too_short (300k) + other (100k) = 500k
        assert_eq!(s.unmapped, Some(500_000));
        assert!((s.unmapped_pct.unwrap() - 5.0).abs() < 0.001);
    }

    #[test]
    fn tolerates_missing_fields() {
        let s = parse("    Number of input reads |\t100\n");
        assert_eq!(s.input_reads, Some(100));
        assert!(s.uniquely_mapped.is_none());
    }
}
```

Add `pub mod log_final;` to the top of `crates/rb-star-align/src/lib.rs` (replace the placeholder comment):

```rust
pub mod log_final;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rb-star-align log_final`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-star-align/src/log_final.rs crates/rb-star-align/src/lib.rs crates/rb-star-align/tests/fixtures/Log.final.out
git commit -m "feat(rb-star-align): parse Log.final.out stats with fixture tests"
```

---

### Task 14: Add counts matrix merger with fixture tests

**Files:**
- Create: `crates/rb-star-align/src/counts.rs`
- Create: `crates/rb-star-align/tests/fixtures/ReadsPerGene.sample1.out.tab`
- Create: `crates/rb-star-align/tests/fixtures/ReadsPerGene.sample2.out.tab`

- [ ] **Step 1: Create fixtures**

`crates/rb-star-align/tests/fixtures/ReadsPerGene.sample1.out.tab` (columns are tab-separated: geneId, unstranded, forward, reverse):

```
N_unmapped	0	0	0
N_multimapping	12	12	12
N_noFeature	34	100	20
N_ambiguous	5	5	5
GENE_A	100	90	10
GENE_B	200	180	20
GENE_C	0	0	0
```

`crates/rb-star-align/tests/fixtures/ReadsPerGene.sample2.out.tab`:

```
N_unmapped	0	0	0
N_multimapping	7	7	7
N_noFeature	20	80	10
N_ambiguous	3	3	3
GENE_A	50	40	10
GENE_D	300	250	50
```

(Note: sample2 is missing GENE_B and GENE_C, has extra GENE_D — tests the union behaviour.)

- [ ] **Step 2: Write the merger + tests**

Create `crates/rb-star-align/src/counts.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Strand { Unstranded, Forward, Reverse }

impl Strand {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "unstranded" => Some(Self::Unstranded),
            "forward"    => Some(Self::Forward),
            "reverse"    => Some(Self::Reverse),
            _ => None,
        }
    }
    /// Column index in ReadsPerGene.out.tab (0=geneId, 1=unstranded, 2=forward, 3=reverse)
    pub fn column_index(self) -> usize {
        match self { Self::Unstranded => 1, Self::Forward => 2, Self::Reverse => 3 }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SampleSummary {
    pub n_unmapped: u64,
    pub n_multimapping: u64,
    pub n_nofeature: u64,
    pub n_ambiguous: u64,
}

#[derive(Debug)]
pub struct SampleCounts {
    pub summary: SampleSummary,
    pub genes: BTreeMap<String, u64>,
}

pub fn read_reads_per_gene(path: &Path, strand: Strand) -> std::io::Result<SampleCounts> {
    let f = std::fs::File::open(path)?;
    let reader = BufReader::new(f);
    let col = strand.column_index();
    let mut summary = SampleSummary::default();
    let mut genes: BTreeMap<String, u64> = BTreeMap::new();

    for line in reader.lines() {
        let line = line?;
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 4 { continue; }
        let id = fields[0];
        let count: u64 = fields[col].parse().unwrap_or(0);
        match id {
            "N_unmapped"     => summary.n_unmapped = count,
            "N_multimapping" => summary.n_multimapping = count,
            "N_noFeature"    => summary.n_nofeature = count,
            "N_ambiguous"    => summary.n_ambiguous = count,
            _ => { genes.insert(id.to_string(), count); }
        }
    }
    Ok(SampleCounts { summary, genes })
}

/// Merge per-sample counts into a single matrix: rows=geneId (sorted union), cols=samples (input order).
pub fn write_counts_matrix(
    out_path: &Path,
    sample_names: &[String],
    per_sample: &[SampleCounts],
) -> std::io::Result<()> {
    let mut all_genes: BTreeMap<String, ()> = BTreeMap::new();
    for s in per_sample {
        for g in s.genes.keys() {
            all_genes.insert(g.clone(), ());
        }
    }

    let mut f = std::fs::File::create(out_path)?;
    write!(f, "gene_id")?;
    for name in sample_names {
        write!(f, "\t{}", name)?;
    }
    writeln!(f)?;

    for gene in all_genes.keys() {
        write!(f, "{}", gene)?;
        for s in per_sample {
            let c = s.genes.get(gene).copied().unwrap_or(0);
            write!(f, "\t{}", c)?;
        }
        writeln!(f)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    #[test]
    fn strand_from_str() {
        assert_eq!(Strand::from_str("unstranded"), Some(Strand::Unstranded));
        assert_eq!(Strand::from_str("forward"),    Some(Strand::Forward));
        assert_eq!(Strand::from_str("reverse"),    Some(Strand::Reverse));
        assert_eq!(Strand::from_str("junk"),       None);
    }

    #[test]
    fn reads_summary_and_genes_unstranded() {
        let s = read_reads_per_gene(&fixture("ReadsPerGene.sample1.out.tab"), Strand::Unstranded).unwrap();
        assert_eq!(s.summary.n_multimapping, 12);
        assert_eq!(s.summary.n_nofeature, 34);
        assert_eq!(s.genes.get("GENE_A"), Some(&100));
        assert_eq!(s.genes.get("GENE_B"), Some(&200));
    }

    #[test]
    fn reads_forward_column_selects_col_2() {
        let s = read_reads_per_gene(&fixture("ReadsPerGene.sample1.out.tab"), Strand::Forward).unwrap();
        assert_eq!(s.genes.get("GENE_A"), Some(&90));
        assert_eq!(s.summary.n_nofeature, 100);
    }

    #[test]
    fn merge_unions_genes_and_zero_fills() {
        let s1 = read_reads_per_gene(&fixture("ReadsPerGene.sample1.out.tab"), Strand::Unstranded).unwrap();
        let s2 = read_reads_per_gene(&fixture("ReadsPerGene.sample2.out.tab"), Strand::Unstranded).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("counts.tsv");
        write_counts_matrix(&out, &["S1".into(), "S2".into()], &[s1, s2]).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "gene_id\tS1\tS2");
        // Alphabetical: GENE_A, GENE_B, GENE_C, GENE_D
        assert_eq!(lines[1], "GENE_A\t100\t50");
        assert_eq!(lines[2], "GENE_B\t200\t0");  // S2 missing → 0
        assert_eq!(lines[3], "GENE_C\t0\t0");
        assert_eq!(lines[4], "GENE_D\t0\t300");  // S1 missing → 0
    }
}
```

Update `crates/rb-star-align/src/lib.rs` (keep `log_final` from Task 13):

```rust
pub mod counts;
pub mod log_final;
```

- [ ] **Step 3: Add dev-dep**

Append to `crates/rb-star-align/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
tokio = { workspace = true, features = ["rt-multi-thread", "macros"] }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p rb-star-align counts`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-star-align/src/counts.rs crates/rb-star-align/src/lib.rs crates/rb-star-align/Cargo.toml crates/rb-star-align/tests/fixtures/ReadsPerGene.sample*.out.tab
git commit -m "feat(rb-star-align): per-sample counts parser and matrix merger with fixtures"
```

---

### Task 15: Add rb-star-align subprocess helper

**Files:**
- Create: `crates/rb-star-align/src/subprocess.rs`

- [ ] **Step 1: Write the helper**

Create `crates/rb-star-align/src/subprocess.rs` with the same contents as `crates/rb-star-index/src/subprocess.rs` (Task 10). The file body is identical:

```rust
use rb_core::cancel::CancellationToken;
use rb_core::module::ModuleError;
use rb_core::run_event::{LogStream, RunEvent};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub async fn run_star_streaming(
    bin: &PathBuf,
    args: &[String],
    events_tx: mpsc::Sender<RunEvent>,
    cancel: CancellationToken,
) -> Result<std::process::ExitStatus, ModuleError> {
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| ModuleError::ToolError(format!(
        "failed to spawn {}: {}", bin.display(), e,
    )))?;

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let tx_out = events_tx.clone();
    let tx_err = events_tx.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx_out.send(RunEvent::Log { line, stream: LogStream::Stdout }).await;
        }
    });
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx_err.send(RunEvent::Log { line, stream: LogStream::Stderr }).await;
        }
    });

    tokio::select! {
        status = child.wait() => status.map_err(|e| ModuleError::ToolError(e.to_string())),
        _ = cancel.cancelled() => {
            let _ = child.kill().await;
            Err(ModuleError::Cancelled)
        }
    }
}
```

Update `crates/rb-star-align/src/lib.rs`:

```rust
pub mod counts;
pub mod log_final;
mod subprocess;
```

- [ ] **Step 2: Verify compile**

Run: `cargo check -p rb-star-align`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-star-align/src/subprocess.rs crates/rb-star-align/src/lib.rs
git commit -m "feat(rb-star-align): add subprocess helper (copy of rb-star-index's)"
```

---

### Task 16: Implement StarAlignModule validate()

**Files:**
- Modify: `crates/rb-star-align/src/lib.rs`

- [ ] **Step 1: Append the module + validate() tests**

Replace `crates/rb-star-align/src/lib.rs` with:

```rust
pub mod counts;
pub mod log_final;
mod subprocess;

use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

pub struct StarAlignModule;

fn sample_name_from_r1(r1: &str) -> String {
    let p = Path::new(r1);
    let mut name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    for ext in [".gz", ".fastq", ".fq", ".txt"] {
        if let Some(stripped) = name.strip_suffix(ext) {
            name = stripped.to_string();
        }
    }
    for suffix in ["_R1", "_1"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    name
}

#[async_trait::async_trait]
impl Module for StarAlignModule {
    fn id(&self) -> &str { "star_align" }
    fn name(&self) -> &str { "STAR Alignment & Quantification" }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        match params.get("genome_dir").and_then(|v| v.as_str()) {
            None => errors.push(ValidationError {
                field: "genome_dir".into(), message: "genome_dir is required".into(),
            }),
            Some(s) => {
                let p = Path::new(s);
                if !p.is_dir() {
                    errors.push(ValidationError {
                        field: "genome_dir".into(),
                        message: format!("genome_dir does not exist or is not a directory: {}", s),
                    });
                } else if !p.join("SA").exists() {
                    errors.push(ValidationError {
                        field: "genome_dir".into(),
                        message: format!("genome_dir does not look like a STAR index (missing SA): {}", s),
                    });
                }
            }
        }

        let r1 = params.get("reads_1").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if r1.is_empty() {
            errors.push(ValidationError {
                field: "reads_1".into(), message: "reads_1 must be a non-empty array".into(),
            });
        }
        for (i, v) in r1.iter().enumerate() {
            match v.as_str() {
                None => errors.push(ValidationError {
                    field: format!("reads_1[{}]", i),
                    message: "must be a string path".into(),
                }),
                Some(p) => if !Path::new(p).exists() {
                    errors.push(ValidationError {
                        field: format!("reads_1[{}]", i),
                        message: format!("file does not exist: {}", p),
                    });
                }
            }
        }

        if let Some(r2) = params.get("reads_2").and_then(|v| v.as_array()) {
            if !r2.is_empty() && r2.len() != r1.len() {
                errors.push(ValidationError {
                    field: "reads_2".into(),
                    message: format!("reads_2 length ({}) must match reads_1 length ({})", r2.len(), r1.len()),
                });
            }
            for (i, v) in r2.iter().enumerate() {
                if let Some(p) = v.as_str() {
                    if !Path::new(p).exists() {
                        errors.push(ValidationError {
                            field: format!("reads_2[{}]", i),
                            message: format!("file does not exist: {}", p),
                        });
                    }
                }
            }
        }

        if let Some(names) = params.get("sample_names").and_then(|v| v.as_array()) {
            if names.len() != r1.len() {
                errors.push(ValidationError {
                    field: "sample_names".into(),
                    message: format!("sample_names length ({}) must match reads_1 length ({})", names.len(), r1.len()),
                });
            }
            let mut seen = std::collections::HashSet::new();
            for (i, v) in names.iter().enumerate() {
                let s = v.as_str().unwrap_or("");
                if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c)) {
                    errors.push(ValidationError {
                        field: format!("sample_names[{}]", i),
                        message: "must be non-empty and match [A-Za-z0-9_.-]+".into(),
                    });
                }
                if !seen.insert(s) {
                    errors.push(ValidationError {
                        field: format!("sample_names[{}]", i),
                        message: format!("duplicate sample name: {}", s),
                    });
                }
            }
        }

        match params.get("strand").and_then(|v| v.as_str()).unwrap_or("unstranded") {
            "unstranded" | "forward" | "reverse" => {}
            other => errors.push(ValidationError {
                field: "strand".into(),
                message: format!("strand must be unstranded/forward/reverse, got '{}'", other),
            }),
        }

        if let Some(v) = params.get("extra_args") {
            if !v.is_array() || !v.as_array().unwrap().iter().all(|x| x.is_string()) {
                errors.push(ValidationError {
                    field: "extra_args".into(),
                    message: "extra_args must be an array of strings".into(),
                });
            }
        }

        if let Ok(resolver) = BinaryResolver::load() {
            if let Err(e) = resolver.resolve("star") {
                errors.push(ValidationError { field: "binary".into(), message: e.to_string() });
            }
        }

        errors
    }

    async fn run(
        &self,
        _params: &serde_json::Value,
        _project_dir: &Path,
        _events_tx: mpsc::Sender<RunEvent>,
        _cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        // Implemented in Task 17
        Err(ModuleError::ToolError("run() not implemented yet".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_name_strips_common_suffixes() {
        assert_eq!(sample_name_from_r1("/x/S1_R1.fastq.gz"), "S1");
        assert_eq!(sample_name_from_r1("/x/S2_1.fq"),       "S2");
        assert_eq!(sample_name_from_r1("/x/raw.fastq"),     "raw");
        assert_eq!(sample_name_from_r1("/x/odd.name.fq.gz"), "odd.name");
    }

    #[test]
    fn validate_requires_genome_and_reads() {
        let m = StarAlignModule;
        let errs = m.validate(&serde_json::json!({}));
        let fields: Vec<_> = errs.iter().map(|e| e.field.clone()).collect();
        assert!(fields.iter().any(|f| f == "genome_dir"));
        assert!(fields.iter().any(|f| f == "reads_1"));
    }

    #[test]
    fn validate_rejects_length_mismatch_between_r1_and_r2() {
        let tmp = tempfile::tempdir().unwrap();
        let gd = tmp.path().join("gdir");
        std::fs::create_dir_all(&gd).unwrap();
        std::fs::write(gd.join("SA"), "").unwrap();
        let r1 = tmp.path().join("a_R1.fq"); std::fs::write(&r1, "").unwrap();
        let r2a = tmp.path().join("a_R2.fq"); std::fs::write(&r2a, "").unwrap();
        let r2b = tmp.path().join("b_R2.fq"); std::fs::write(&r2b, "").unwrap();
        let m = StarAlignModule;
        let errs = m.validate(&serde_json::json!({
            "genome_dir": gd,
            "reads_1": [r1],
            "reads_2": [r2a, r2b],
        }));
        assert!(errs.iter().any(|e| e.field == "reads_2" && e.message.contains("length")));
    }

    #[test]
    fn validate_rejects_bad_strand() {
        let tmp = tempfile::tempdir().unwrap();
        let gd = tmp.path().join("gdir");
        std::fs::create_dir_all(&gd).unwrap();
        std::fs::write(gd.join("SA"), "").unwrap();
        let r1 = tmp.path().join("a_R1.fq"); std::fs::write(&r1, "").unwrap();
        let m = StarAlignModule;
        let errs = m.validate(&serde_json::json!({
            "genome_dir": gd, "reads_1": [r1], "strand": "weird",
        }));
        assert!(errs.iter().any(|e| e.field == "strand"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p rb-star-align --lib tests`
Expected: 4 tests pass (plus counts/log_final tests from earlier).

- [ ] **Step 3: Commit**

```bash
git add crates/rb-star-align/src/lib.rs
git commit -m "feat(rb-star-align): add StarAlignModule validate() with tests"
```

---

### Task 17: Implement StarAlignModule run()

**Files:**
- Modify: `crates/rb-star-align/src/lib.rs`

- [ ] **Step 1: Replace the `run()` stub with a full implementation**

In `crates/rb-star-align/src/lib.rs`, locate the `async fn run(...)` stub and replace it (keep everything else) with:

```rust
    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        events_tx: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        let errors = self.validate(params);
        if !errors.is_empty() {
            return Err(ModuleError::InvalidParams(errors));
        }

        let resolver = BinaryResolver::load().map_err(|e| ModuleError::ToolError(e.to_string()))?;
        let bin = resolver.resolve("star").map_err(|e| ModuleError::ToolError(e.to_string()))?;

        let genome_dir = params["genome_dir"].as_str().unwrap().to_string();
        let reads_1: Vec<String> = params["reads_1"].as_array().unwrap().iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
        let reads_2: Vec<String> = params.get("reads_2").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let sample_names: Vec<String> = match params.get("sample_names").and_then(|v| v.as_array()) {
            Some(a) => a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
            None => reads_1.iter().map(|r| sample_name_from_r1(r)).collect(),
        };
        let threads = params.get("threads").and_then(|v| v.as_u64()).unwrap_or(4);
        let strand_str = params.get("strand").and_then(|v| v.as_str()).unwrap_or("unstranded").to_string();
        let strand = counts::Strand::from_str(&strand_str).unwrap();
        let extra: Vec<String> = params.get("extra_args")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        let run_dir = project_dir.to_path_buf();
        std::fs::create_dir_all(&run_dir)?;

        let total = reads_1.len();
        let mut per_sample_counts: Vec<counts::SampleCounts> = Vec::with_capacity(total);
        let mut samples_summary: Vec<serde_json::Value> = Vec::with_capacity(total);
        let mut output_files: Vec<PathBuf> = Vec::new();
        let mut combined_log = String::new();

        for i in 0..total {
            if cancel.is_cancelled() { return Err(ModuleError::Cancelled); }
            let name = &sample_names[i];
            let r1 = &reads_1[i];
            let r2 = reads_2.get(i);
            let fraction = i as f64 / total as f64;
            let _ = events_tx.send(RunEvent::Progress {
                fraction,
                message: format!("Aligning {} ({}/{})", name, i + 1, total),
            }).await;

            let sample_out = run_dir.join(name);
            std::fs::create_dir_all(&sample_out)?;
            let prefix = format!("{}/", sample_out.display());

            let is_gz = r1.ends_with(".gz") || r2.map(|p| p.ends_with(".gz")).unwrap_or(false);

            let mut args: Vec<String> = vec![
                "--runMode".into(),       "alignReads".into(),
                "--genomeDir".into(),     genome_dir.clone(),
                "--readFilesIn".into(),   r1.clone(),
            ];
            if let Some(r2v) = r2 { args.push(r2v.clone()); }
            if is_gz {
                args.push("--readFilesCommand".into());
                args.push("zcat".into());
            }
            args.push("--outFileNamePrefix".into()); args.push(prefix);
            args.push("--runThreadN".into());        args.push(threads.to_string());
            args.push("--quantMode".into());         args.push("GeneCounts".into());
            args.push("--outSAMtype".into()); args.push("BAM".into()); args.push("Unsorted".into());
            args.extend(extra.iter().cloned());

            let status = subprocess::run_star_streaming(&bin, &args, events_tx.clone(), cancel.clone()).await?;

            let log_final_path  = sample_out.join("Log.final.out");
            let reads_per_gene  = sample_out.join("ReadsPerGene.out.tab");
            let bam             = sample_out.join("Aligned.out.bam");

            if !status.success() {
                samples_summary.push(serde_json::json!({
                    "name": name, "r1": r1, "r2": r2,
                    "status": "error",
                    "exit_code": status.code(),
                }));
                combined_log.push_str(&format!("\n[{}] STAR exited with code {}\n", name, status.code().unwrap_or(-1)));
                // Insert an empty counts entry so matrix alignment stays consistent,
                // otherwise the sample would be missing from the matrix columns.
                per_sample_counts.push(counts::SampleCounts {
                    summary: counts::SampleSummary::default(),
                    genes: std::collections::BTreeMap::new(),
                });
                continue;
            }

            let log_stats = std::fs::read_to_string(&log_final_path).ok().map(|t| log_final::parse(&t));
            let sample_counts = counts::read_reads_per_gene(&reads_per_gene, strand)
                .map_err(|e| ModuleError::ToolError(format!("parse {}: {}", reads_per_gene.display(), e)))?;

            let stats_json = log_stats.as_ref().map(|s| serde_json::json!({
                "input_reads":           s.input_reads,
                "uniquely_mapped":       s.uniquely_mapped,
                "uniquely_mapped_pct":   s.uniquely_mapped_pct,
                "multi_mapped":          s.multi_mapped,
                "multi_mapped_pct":      s.multi_mapped_pct,
                "unmapped":              s.unmapped,
                "unmapped_pct":          s.unmapped_pct,
                "n_unmapped":            sample_counts.summary.n_unmapped,
                "n_multimapping":        sample_counts.summary.n_multimapping,
                "n_nofeature":           sample_counts.summary.n_nofeature,
                "n_ambiguous":           sample_counts.summary.n_ambiguous,
            })).unwrap_or(serde_json::Value::Null);

            samples_summary.push(serde_json::json!({
                "name": name, "r1": r1, "r2": r2,
                "status": "ok",
                "bam": bam.display().to_string(),
                "reads_per_gene": reads_per_gene.display().to_string(),
                "log_final":      log_final_path.display().to_string(),
                "stats": stats_json,
            }));

            if bam.exists()            { output_files.push(bam); }
            if reads_per_gene.exists() { output_files.push(reads_per_gene); }
            if log_final_path.exists() { output_files.push(log_final_path); }

            per_sample_counts.push(sample_counts);
        }

        let _ = events_tx.send(RunEvent::Progress {
            fraction: 1.0, message: "Merging counts matrix".into(),
        }).await;

        let matrix_path = run_dir.join("counts_matrix.tsv");
        counts::write_counts_matrix(&matrix_path, &sample_names, &per_sample_counts)?;
        output_files.push(matrix_path.clone());

        let _ = events_tx.send(RunEvent::Progress { fraction: 1.0, message: "Done".into() }).await;

        let summary = serde_json::json!({
            "run_dir": run_dir.display().to_string(),
            "counts_matrix": matrix_path.display().to_string(),
            "strand": strand_str,
            "genome_dir": genome_dir,
            "samples": samples_summary,
        });

        Ok(ModuleResult { output_files, summary, log: combined_log })
    }
```

- [ ] **Step 2: Verify build**

Run: `cargo check -p rb-star-align`
Expected: success.

Run: `cargo test -p rb-star-align`
Expected: all tests from Tasks 13, 14, 16 pass (run() is not covered by unit tests — integration test in Task 21 exercises it).

- [ ] **Step 3: Commit**

```bash
git add crates/rb-star-align/src/lib.rs
git commit -m "feat(rb-star-align): implement per-sample alignment + counts matrix merge"
```

---

### Task 18: Register rb-star-align in rb-app

**Files:**
- Modify: `crates/rb-app/Cargo.toml`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: Add dep**

Append to the `[dependencies]` table of `crates/rb-app/Cargo.toml`:

```toml
rb-star-align = { path = "../rb-star-align" }
```

- [ ] **Step 2: Register the module**

In `crates/rb-app/src/main.rs`, add after the `rb_star_index` registration:

```rust
registry.register(Arc::new(rb_star_align::StarAlignModule));
```

- [ ] **Step 3: Build**

Run: `cargo check --workspace`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-app/Cargo.toml crates/rb-app/src/main.rs
git commit -m "feat(rb-app): register StarAlignModule"
```

---

## Phase 4 — Frontend (Tasks 19–23)

### Task 19: Shared streaming log panel + run-log listener

**Files:**
- Modify: `frontend/js/app.js`
- Modify: `frontend/css/style.css`

- [ ] **Step 1: Add log-panel state and listener**

In `frontend/js/app.js`, find the block where other Tauri event listeners are registered (search for `'run-progress'` in the file). Near that same place, add:

```javascript
// --- run-log streaming support (shared across all modules) ---
const LOG_BUFFER_MAX = 500;
state.logsByRun = state.logsByRun || {};

function appendRunLog(runId, line, stream) {
  const buf = (state.logsByRun[runId] = state.logsByRun[runId] || []);
  buf.push({ line, stream });
  while (buf.length > LOG_BUFFER_MAX) buf.shift();
  const panel = document.querySelector(`[data-log-panel="${runId}"] pre`);
  if (panel) {
    const prefix = stream === 'stderr' ? '' : '[out] ';
    panel.textContent += prefix + line + '\n';
    if (!panel.dataset.userScrolled) {
      panel.scrollTop = panel.scrollHeight;
    }
  }
}

function renderLogPanel(runId) {
  const existing = state.logsByRun[runId] || [];
  const text = existing.map(e => (e.stream === 'stderr' ? '' : '[out] ') + e.line).join('\n');
  return `<details class="log-panel" data-log-panel="${runId}">
    <summary>Log</summary>
    <pre>${text}</pre>
  </details>`;
}

// Attach pre-scroll-watch so auto-scroll respects user intent
document.addEventListener('scroll', (e) => {
  const pre = e.target;
  if (pre.tagName !== 'PRE' || !pre.closest('[data-log-panel]')) return;
  const nearBottom = pre.scrollHeight - pre.scrollTop - pre.clientHeight < 20;
  if (nearBottom) delete pre.dataset.userScrolled;
  else pre.dataset.userScrolled = '1';
}, true);

// Wire up the Tauri event (use the same invoke style as run-progress)
if (window.__TAURI__?.event) {
  window.__TAURI__.event.listen('run-log', (e) => {
    const { runId, line, stream } = e.payload || {};
    if (runId) appendRunLog(runId, line, stream);
  });
}
```

- [ ] **Step 2: Render the log panel in the per-run views**

Find the function that renders a running run's card (search for `run-progress` emit usage in app.js, or the function that displays progress bars). Add a call to `renderLogPanel(runId)` inside that card's HTML template.

Example — in the progress-card template (exact location depends on existing structure), after the progress bar `<div>`, insert:

```javascript
html += renderLogPanel(runId);
```

If multiple call sites exist, update each. (Search for `progress-bar` class or equivalent to locate them.)

- [ ] **Step 3: Add CSS**

Append to `frontend/css/style.css`:

```css
.log-panel {
  margin-top: 0.75rem;
  background: var(--surface-2, #fafaf6);
  border: 1px solid var(--border, #e4decf);
  border-radius: 6px;
  padding: 0.5rem 0.75rem;
  font-family: 'JetBrains Mono', 'SF Mono', monospace;
  font-size: 0.8rem;
}
.log-panel summary { cursor: pointer; font-weight: 600; }
.log-panel pre {
  max-height: 280px;
  overflow-y: auto;
  margin: 0.5rem 0 0;
  white-space: pre-wrap;
  word-break: break-all;
}
```

- [ ] **Step 4: Manual smoke test**

Open `frontend/index.html` in a browser (or via `cd frontend && python3 -m http.server 8090`) and ensure the app still loads with no console errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/js/app.js frontend/css/style.css
git commit -m "feat(frontend): add shared streaming log panel with run-log listener"
```

---

### Task 20: Settings view for binary paths

**Files:**
- Modify: `frontend/js/app.js`
- Modify: `frontend/index.html` (mock fixture only)

- [ ] **Step 1: Add the Settings view**

In `frontend/js/app.js`, find the `navigate(view)` function and its `switch`/`if` chain. Add a new case `"settings"` that renders:

```javascript
async function renderSettings() {
  let statuses = [];
  try {
    statuses = await window.__TAURI__.core.invoke('get_binary_paths');
  } catch (e) {
    return `<div class="error">Failed to load settings: ${e}</div>`;
  }
  const rows = statuses.map(s => `
    <tr>
      <td>${s.display_name}</td>
      <td class="path">${s.configured_path ?? '<em>(not set)</em>'}</td>
      <td class="path">${s.detected_on_path ?? '<em>(not on PATH)</em>'}</td>
      <td>${s.configured_path || s.detected_on_path ? '<span class="ok">OK</span>' : '<span class="warn">Missing</span>'}</td>
      <td>
        <button data-act="browse" data-id="${s.id}">Browse…</button>
        ${s.configured_path ? `<button data-act="clear" data-id="${s.id}">Clear</button>` : ''}
      </td>
    </tr>
  `).join('');
  return `
    <h2>Settings — Binary Paths</h2>
    <p>When a binary is not on PATH, configure its full path here. Configured paths override PATH.</p>
    <table class="settings-table">
      <thead><tr><th>Tool</th><th>Configured</th><th>Detected on PATH</th><th>Status</th><th>Actions</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
  `;
}
```

Register case `'settings'` in `navigate()`:

```javascript
case 'settings': content = await renderSettings(); break;
```

(If `navigate()` is synchronous, adapt: use a two-phase render that first shows a spinner then swaps in the HTML after the invoke resolves. Look at how `list_recent_projects` is already handled in this codebase — follow that pattern.)

Add a delegated click handler near the existing navigation handlers:

```javascript
document.addEventListener('click', async (e) => {
  const btn = e.target.closest('[data-act="browse"]');
  if (btn) {
    const picked = await window.__TAURI__.core.invoke('select_files', { multiple: false });
    if (picked && picked[0]) {
      try {
        await window.__TAURI__.core.invoke('set_binary_path', { name: btn.dataset.id, path: picked[0] });
        navigate('settings');
      } catch (err) { alert('Failed: ' + err); }
    }
  }
  const clr = e.target.closest('[data-act="clear"]');
  if (clr) {
    try {
      await window.__TAURI__.core.invoke('clear_binary_path', { name: clr.dataset.id });
      navigate('settings');
    } catch (err) { alert('Failed: ' + err); }
  }
});
```

- [ ] **Step 2: Add a sidebar entry**

Find the sidebar nav list (search for existing items like `data-view="qc"` in app.js or index.html). Add a gear-icon link:

```html
<a href="#settings" data-view="settings">⚙ Settings</a>
```

Style it at the bottom of the sidebar via whatever class the sidebar uses.

- [ ] **Step 3: Extend the browser mock shim**

In `frontend/index.html`, find the `window.__TAURI__` mock (search for `invoke:` inside a script block). Extend the switch/if chain that maps command names to mock responses:

```javascript
if (cmd === 'get_binary_paths') {
  return Promise.resolve([
    { id: 'star', display_name: 'STAR (STAR_rs)', configured_path: null, detected_on_path: null, install_hint: '...' },
    { id: 'cutadapt-rs', display_name: 'cutadapt-rs', configured_path: null, detected_on_path: null, install_hint: '...' },
  ]);
}
if (cmd === 'set_binary_path' || cmd === 'clear_binary_path') return Promise.resolve(null);
```

- [ ] **Step 4: Add table CSS**

Append to `frontend/css/style.css`:

```css
.settings-table { border-collapse: collapse; width: 100%; }
.settings-table th, .settings-table td {
  text-align: left; padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #e4decf);
}
.settings-table td.path {
  font-family: 'JetBrains Mono', monospace; font-size: 0.85rem;
  max-width: 24rem; overflow: hidden; text-overflow: ellipsis;
}
.settings-table .ok   { color: var(--accent-teal, #1f6b5e); }
.settings-table .warn { color: var(--accent-coral, #c46b4b); }
```

- [ ] **Step 5: Smoke test**

In a browser, navigate to `#settings` and verify the table renders (mock data in browser mode). No console errors.

- [ ] **Step 6: Commit**

```bash
git add frontend/js/app.js frontend/css/style.css frontend/index.html
git commit -m "feat(frontend): add Settings view for binary paths"
```

---

### Task 21: STAR Index view + form

**Files:**
- Modify: `frontend/js/app.js`
- Modify: `frontend/index.html` (mock fixture)

- [ ] **Step 1: Add renderer**

In `frontend/js/app.js`, add after the existing render functions (e.g., after `renderQc`):

```javascript
function renderStarIndex() {
  return `
    <h2>STAR Genome Index</h2>
    <p>Build a STAR index from a reference genome FASTA and GTF annotation. Required before any alignment run.</p>
    <form id="form-star-index">
      <label>Genome FASTA
        <input type="text" name="genome_fasta" data-pick="file" placeholder="/path/to/genome.fa" required />
        <button type="button" data-pick-for="genome_fasta">Browse…</button>
      </label>
      <label>GTF annotation
        <input type="text" name="gtf_file" data-pick="file" placeholder="/path/to/annotation.gtf" required />
        <button type="button" data-pick-for="gtf_file">Browse…</button>
      </label>
      <label>Threads <input type="number" name="threads" value="4" min="1" /></label>
      <label>sjdbOverhang <input type="number" name="sjdb_overhang" value="100" min="1" /></label>
      <label>genomeSAindexNbases <input type="number" name="genome_sa_index_nbases" value="14" min="1" max="18" /></label>
      <details><summary>Advanced</summary>
        <label>Extra args (one per line)
          <textarea name="extra_args" placeholder="--limitGenomeGenerateRAM 31000000000"></textarea>
        </label>
      </details>
      <button type="submit">Build Index</button>
    </form>
    <div id="star-index-runs"></div>
  `;
}

async function submitStarIndex(form) {
  const fd = new FormData(form);
  const extra_args = (fd.get('extra_args') || '').toString().split('\n').map(s => s.trim()).filter(Boolean);
  const params = {
    genome_fasta: fd.get('genome_fasta'),
    gtf_file:     fd.get('gtf_file'),
    threads:      parseInt(fd.get('threads'), 10) || 4,
    sjdb_overhang: parseInt(fd.get('sjdb_overhang'), 10) || 100,
    genome_sa_index_nbases: parseInt(fd.get('genome_sa_index_nbases'), 10) || 14,
    extra_args,
  };
  try {
    const runId = await window.__TAURI__.core.invoke('run_module', { moduleId: 'star_index', params });
    navigate('star-index');
    // The run card will appear via the existing runs list mechanism
  } catch (err) { alert('Failed to start run: ' + err); }
}
```

Register `case 'star-index': content = renderStarIndex(); break;` inside `navigate()`.

Add a submit handler:

```javascript
document.addEventListener('submit', (e) => {
  if (e.target.id === 'form-star-index') { e.preventDefault(); submitStarIndex(e.target); }
});
```

Add a generic file-pick handler (if one doesn't already exist — search for `data-pick-for`):

```javascript
document.addEventListener('click', async (e) => {
  const btn = e.target.closest('[data-pick-for]');
  if (!btn) return;
  const field = btn.dataset.pickFor;
  const picked = await window.__TAURI__.core.invoke('select_files', { multiple: false });
  if (picked && picked[0]) {
    const input = btn.parentElement.querySelector(`input[name="${field}"]`);
    if (input) input.value = picked[0];
  }
});
```

- [ ] **Step 2: Add sidebar entry**

Near the existing sidebar items, add under a new "Alignment & Quantification" heading:

```html
<h4>Alignment & Quantification</h4>
<a href="#star-index" data-view="star-index">STAR Index</a>
<a href="#star-align" data-view="star-align">STAR Alignment</a>
```

- [ ] **Step 3: Mock fixture in index.html**

Extend the `invoke` mock:

```javascript
if (cmd === 'run_module' && args.moduleId === 'star_index') {
  return Promise.resolve('mock-run-' + Date.now());
}
```

- [ ] **Step 4: Smoke test**

Visit `#star-index` in the browser (mock mode); the form renders; clicking "Build Index" returns a mock run id.

- [ ] **Step 5: Commit**

```bash
git add frontend/js/app.js frontend/index.html
git commit -m "feat(frontend): add STAR Index view and form"
```

---

### Task 22: STAR Alignment view + form + results (mapping chart, counts preview, DESeq2 handoff)

**Files:**
- Modify: `frontend/js/app.js`
- Modify: `frontend/index.html` (mock fixture)

- [ ] **Step 1: Add renderer**

In `frontend/js/app.js`:

```javascript
function renderStarAlign() {
  return `
    <h2>STAR Alignment & Quantification</h2>
    <p>Map FASTQ reads to a pre-built STAR index and produce per-sample BAM, gene counts, and a merged counts matrix.</p>
    <form id="form-star-align">
      <label>Genome index directory
        <input type="text" name="genome_dir" required placeholder="/path/to/star_index" />
        <button type="button" data-pick-for="genome_dir" data-pick-mode="dir">Browse…</button>
      </label>
      <label>R1 FASTQ files (one per sample)
        <input type="text" name="reads_1" required placeholder="/path/to/S1_R1.fq.gz /path/to/S2_R1.fq.gz" />
        <button type="button" data-pick-for="reads_1" data-pick-mode="multi">Browse…</button>
      </label>
      <label>R2 FASTQ files (optional, paired-end)
        <input type="text" name="reads_2" placeholder="/path/to/S1_R2.fq.gz /path/to/S2_R2.fq.gz" />
        <button type="button" data-pick-for="reads_2" data-pick-mode="multi">Browse…</button>
      </label>
      <label>Sample names (optional, one per line; defaults from R1 filename)
        <textarea name="sample_names" placeholder="S1&#10;S2"></textarea>
      </label>
      <label>Threads <input type="number" name="threads" value="4" min="1" /></label>
      <fieldset>
        <legend>Strand</legend>
        <label><input type="radio" name="strand" value="unstranded" checked /> unstranded</label>
        <label><input type="radio" name="strand" value="forward" /> forward</label>
        <label><input type="radio" name="strand" value="reverse" /> reverse</label>
      </fieldset>
      <details><summary>Advanced</summary>
        <label>Extra args (one per line)
          <textarea name="extra_args" placeholder="--outFilterMultimapNmax 10"></textarea>
        </label>
      </details>
      <button type="submit">Run Alignment</button>
    </form>
    <div id="star-align-runs"></div>
  `;
}

async function submitStarAlign(form) {
  const fd = new FormData(form);
  const splitPaths = (s) => (s || '').toString().split(/\s+/).map(x => x.trim()).filter(Boolean);
  const splitLines = (s) => (s || '').toString().split('\n').map(x => x.trim()).filter(Boolean);
  const params = {
    genome_dir:    fd.get('genome_dir'),
    reads_1:       splitPaths(fd.get('reads_1')),
    reads_2:       splitPaths(fd.get('reads_2')),
    sample_names:  splitLines(fd.get('sample_names')),
    threads:       parseInt(fd.get('threads'), 10) || 4,
    strand:        fd.get('strand') || 'unstranded',
    extra_args:    splitLines(fd.get('extra_args')),
  };
  if (params.sample_names.length === 0) delete params.sample_names;
  if (params.reads_2.length === 0)     delete params.reads_2;
  try {
    const runId = await window.__TAURI__.core.invoke('run_module', { moduleId: 'star_align', params });
    state.currentRunId = runId;
    navigate('star-align');
  } catch (err) { alert('Failed to start run: ' + err); }
}

function renderStarAlignResult(result) {
  const samples = (result.summary && result.summary.samples) || [];
  const matrixPath = result.summary && result.summary.counts_matrix;
  const data = {
    names: samples.map(s => s.name),
    uniq:  samples.map(s => (s.stats && s.stats.uniquely_mapped) || 0),
    multi: samples.map(s => (s.stats && s.stats.multi_mapped) || 0),
    unmap: samples.map(s => (s.stats && s.stats.unmapped) || 0),
  };
  setTimeout(() => renderMappingRateChart('star-align-chart', data), 0);

  let previewHtml = '<p><em>No counts matrix produced</em></p>';
  if (matrixPath) {
    window.__TAURI__.core.invoke('read_table_preview', { path: matrixPath, max_rows: 50, max_cols: 10 })
      .then(rows => {
        const el = document.getElementById('star-align-preview');
        if (!el || !rows || rows.length === 0) return;
        const header = rows[0].map(c => `<th>${c}</th>`).join('');
        const body = rows.slice(1).map(r => '<tr>' + r.map(c => `<td>${c}</td>`).join('') + '</tr>').join('');
        el.innerHTML = `<table class="preview-table"><thead><tr>${header}</tr></thead><tbody>${body}</tbody></table>`;
      }).catch(() => {});
  }

  return `
    <h3>Mapping rate</h3>
    <div id="star-align-chart" style="width: 100%; height: 320px;"></div>
    <h3>Counts matrix preview (first 50 × 10)</h3>
    <div id="star-align-preview">Loading…</div>
    ${matrixPath ? `<button id="star-to-deseq" data-matrix="${matrixPath}">Use this matrix in DESeq2</button>` : ''}
    ${previewHtml}
  `;
}

function renderMappingRateChart(elId, data) {
  const el = document.getElementById(elId);
  if (!el || !window.echarts) return;
  const chart = window.echarts.init(el, ECHART_THEME);
  chart.setOption({
    tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
    legend: { data: ['Unique', 'Multi', 'Unmapped'] },
    grid: { left: 60, right: 20, top: 40, bottom: 50 },
    xAxis: { type: 'category', data: data.names },
    yAxis: { type: 'value', name: 'Reads' },
    series: [
      { name: 'Unique',   type: 'bar', stack: 'total', data: data.uniq },
      { name: 'Multi',    type: 'bar', stack: 'total', data: data.multi },
      { name: 'Unmapped', type: 'bar', stack: 'total', data: data.unmap },
    ],
  });
}
```

Register in `navigate()`: `case 'star-align': content = renderStarAlign(); break;`

Add the submit handler near other `submit` listeners:

```javascript
document.addEventListener('submit', (e) => {
  if (e.target.id === 'form-star-align') { e.preventDefault(); submitStarAlign(e.target); }
});
```

Add the DESeq2 handoff:

```javascript
document.addEventListener('click', (e) => {
  const btn = e.target.closest('#star-to-deseq');
  if (!btn) return;
  state.prefill = state.prefill || {};
  state.prefill.differential = { counts_matrix: btn.dataset.matrix };
  navigate('differential');
});
```

In `renderDifferential()` (existing function for the DESeq2 view), look for the counts_matrix input and prefill from `state.prefill?.differential?.counts_matrix`. Example patch inside `renderDifferential`:

```javascript
const prefill = (state.prefill && state.prefill.differential) || {};
// then use prefill.counts_matrix as the default value on the input
```

In the run-results display (where other modules show their `ModuleResult.summary`), add a branch for `moduleId === 'star_align'` that calls `renderStarAlignResult(result)` to inject the chart + preview.

- [ ] **Step 2: Add directory/multi picker support**

If the existing pick handler only supports single-file, extend it:

```javascript
document.addEventListener('click', async (e) => {
  const btn = e.target.closest('[data-pick-for]');
  if (!btn) return;
  const mode = btn.dataset.pickMode || 'file';
  let picked;
  if (mode === 'dir') {
    picked = await window.__TAURI__.core.invoke('select_directory');
  } else {
    picked = await window.__TAURI__.core.invoke('select_files', { multiple: mode === 'multi' });
  }
  const field = btn.dataset.pickFor;
  const input = btn.parentElement.querySelector(`[name="${field}"]`);
  if (!input) return;
  if (Array.isArray(picked)) input.value = picked.join(' ');
  else if (picked) input.value = picked;
});
```

- [ ] **Step 3: Browser mock extensions in index.html**

```javascript
if (cmd === 'run_module' && args.moduleId === 'star_align') {
  return Promise.resolve('mock-run-align-' + Date.now());
}
if (cmd === 'read_table_preview') {
  return Promise.resolve([
    ['gene_id', 'S1', 'S2'],
    ['GENE_A', '100', '50'],
    ['GENE_B', '200', '0'],
    ['GENE_D', '0', '300'],
  ]);
}
```

- [ ] **Step 4: Smoke test**

Browser: visit `#star-align`, verify form renders; submit; verify navigation. Manually inject a synthetic result JSON into the runs view to see the chart (or just confirm `renderStarAlignResult` doesn't throw when called with the mock schema).

- [ ] **Step 5: Commit**

```bash
git add frontend/js/app.js frontend/index.html
git commit -m "feat(frontend): add STAR Alignment view with mapping chart and DESeq2 handoff"
```

---

### Task 23: Wire star-align result into runs view

**Files:**
- Modify: `frontend/js/app.js`

- [ ] **Step 1: Locate the run-results rendering**

Search `frontend/js/app.js` for `renderRunResult` or wherever per-module result HTML is selected by `module_id`. The existing code has something like:

```javascript
switch (moduleId) {
  case 'qc':           /* qc html */ break;
  case 'trimming':     /* trimming html */ break;
  case 'differential': /* deseq2 html */ break;
}
```

- [ ] **Step 2: Add a case for star_align**

Inside that switch, add:

```javascript
case 'star_align': html = renderStarAlignResult(result); break;
case 'star_index': html = `<pre>${JSON.stringify(result.summary, null, 2)}</pre>`; break;
```

- [ ] **Step 3: Smoke test (browser mock)**

Confirm no runtime errors when viewing a mock star_align run.

- [ ] **Step 4: Commit**

```bash
git add frontend/js/app.js
git commit -m "feat(frontend): route star_align/star_index results to their renderers"
```

---

## Phase 5 — Tests + docs (Tasks 24–26)

### Task 24: Cancellation integration test for subprocess adapter

**Files:**
- Create: `crates/rb-star-index/tests/cancel.rs`

- [ ] **Step 1: Write the test using a fake `star` binary**

Create `crates/rb-star-index/tests/cancel.rs`:

```rust
//! Cancellation test using /bin/sleep as a fake STAR binary via resolver override.
//! This verifies that cancel actually kills the child process rather than waiting.

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_kills_subprocess() {
    use rb_core::binary::BinaryResolver;
    use rb_core::cancel::CancellationToken;
    use rb_core::run_event::RunEvent;
    use std::path::PathBuf;
    use tokio::sync::mpsc;

    // Point `star` at /bin/sleep via settings override.
    let tmp = tempfile::tempdir().unwrap();
    let settings = tmp.path().join("settings.json");
    let mut r = BinaryResolver::load_from(settings.clone()).unwrap();
    r.set("star", PathBuf::from("/bin/sleep")).unwrap();

    // We call the subprocess helper directly (bypasses the Module trait).
    // Re-export the helper module for tests.
    use rb_star_index::subprocess::run_star_streaming;

    let (tx, mut rx) = mpsc::channel::<RunEvent>(16);
    let token = CancellationToken::new();
    let bin = r.resolve("star").unwrap();
    let args = vec!["30".to_string()];

    let cancel_clone = token.clone();
    let handle = tokio::spawn(async move {
        run_star_streaming(&bin, &args, tx, cancel_clone).await
    });

    // Cancel after 200 ms; a well-behaved implementation kills /bin/sleep in well under 1s.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    token.cancel();

    let start = std::time::Instant::now();
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), handle)
        .await
        .expect("cancel did not complete in time")
        .expect("task panicked");
    assert!(matches!(result, Err(rb_core::module::ModuleError::Cancelled)));
    assert!(start.elapsed() < std::time::Duration::from_secs(3));

    // Drain any pending events (make sure the forwarder exits cleanly)
    drop(rx);
}
```

- [ ] **Step 2: Expose the subprocess module for the integration test**

In `crates/rb-star-index/src/lib.rs`, change the `mod subprocess;` declaration at the top to:

```rust
pub mod subprocess;
```

- [ ] **Step 3: Run the test**

Run: `cargo test -p rb-star-index --test cancel`
Expected: passes on Linux/macOS. (Test is gated to `#[cfg(unix)]`.)

- [ ] **Step 4: Commit**

```bash
git add crates/rb-star-index/src/lib.rs crates/rb-star-index/tests/cancel.rs
git commit -m "test(rb-star-index): verify cancel kills subprocess via /bin/sleep"
```

---

### Task 25: Optional integration test gated on STAR_BIN

**Files:**
- Create: `crates/rb-star-align/tests/integration_smoke.rs`
- Create: `crates/rb-star-align/tests/data/chr.fa` (small FASTA)
- Create: `crates/rb-star-align/tests/data/anno.gtf` (hand-written GTF)
- Create: `crates/rb-star-align/tests/data/reads.fq` (small FASTQ)

- [ ] **Step 1: Create minimal fixture data**

Write `crates/rb-star-align/tests/data/chr.fa`:

```
>chr1
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT
```

Write `crates/rb-star-align/tests/data/anno.gtf` (single-exon gene on chr1):

```
chr1	handwritten	gene	1	100	.	+	.	gene_id "GENE_TEST";
chr1	handwritten	transcript	1	100	.	+	.	gene_id "GENE_TEST"; transcript_id "T1";
chr1	handwritten	exon	1	100	.	+	.	gene_id "GENE_TEST"; transcript_id "T1";
```

Write `crates/rb-star-align/tests/data/reads.fq` (two tiny reads):

```
@read1
ACGTACGTACGTACGTACGTACGTACGTACGT
+
IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII
@read2
ACGTACGTACGTACGTACGTACGTACGTACGT
+
IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII
```

- [ ] **Step 2: Write the integration test**

Create `crates/rb-star-align/tests/integration_smoke.rs`:

```rust
//! Optional end-to-end test: requires `STAR_BIN` env var pointing to a real STAR_rs binary.
//! Skipped silently when STAR_BIN is unset — CI default doesn't provide it.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn end_to_end_index_then_align() {
    let star_bin = match std::env::var("STAR_BIN") { Ok(v) => v, Err(_) => { eprintln!("STAR_BIN not set; skipping"); return; } };
    let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let tmp = tempfile::tempdir().unwrap();

    // Point the resolver at the user-supplied binary.
    let settings = tmp.path().join("settings.json");
    let mut r = rb_core::binary::BinaryResolver::load_from(settings).unwrap();
    r.set("star", std::path::PathBuf::from(&star_bin)).unwrap();

    // --- Build index ---
    let idx_dir = tmp.path().join("run_idx");
    std::fs::create_dir_all(&idx_dir).unwrap();
    let (tx, mut _rx) = tokio::sync::mpsc::channel::<rb_core::run_event::RunEvent>(64);
    let token = rb_core::cancel::CancellationToken::new();
    use rb_core::module::Module;
    let idx_mod = rb_star_index::StarIndexModule;
    let idx_params = serde_json::json!({
        "genome_fasta": data.join("chr.fa"),
        "gtf_file":     data.join("anno.gtf"),
        "threads": 2,
        "sjdb_overhang": 29,
        "genome_sa_index_nbases": 4,
    });
    let idx_result = idx_mod.run(&idx_params, &idx_dir, tx, token).await
        .expect("index build failed");
    assert!(idx_result.output_files.iter().any(|p| p.ends_with("SA")));

    // --- Align ---
    let align_dir = tmp.path().join("run_align");
    std::fs::create_dir_all(&align_dir).unwrap();
    let (tx2, mut _rx2) = tokio::sync::mpsc::channel::<rb_core::run_event::RunEvent>(64);
    let token2 = rb_core::cancel::CancellationToken::new();
    let align_mod = rb_star_align::StarAlignModule;
    let align_params = serde_json::json!({
        "genome_dir": idx_dir,
        "reads_1": [ data.join("reads.fq") ],
        "threads": 2,
        "strand": "unstranded",
    });
    let align_result = align_mod.run(&align_params, &align_dir, tx2, token2).await
        .expect("alignment failed");
    let matrix = align_result.summary["counts_matrix"].as_str().unwrap();
    let text = std::fs::read_to_string(matrix).unwrap();
    assert!(text.lines().count() >= 1, "counts matrix empty");
    assert!(text.lines().next().unwrap().starts_with("gene_id"));
}
```

- [ ] **Step 3: Run the test (skips without STAR_BIN)**

Run: `cargo test -p rb-star-align --test integration_smoke`
Expected: prints "STAR_BIN not set; skipping" and passes.

- [ ] **Step 4: Document the opt-in**

Append to `crates/rb-star-align/README.md` (create if missing):

```markdown
# rb-star-align

Run the optional integration test with:

```bash
STAR_BIN=/path/to/star cargo test -p rb-star-align --test integration_smoke
```
```

- [ ] **Step 5: Commit**

```bash
git add crates/rb-star-align/tests crates/rb-star-align/README.md
git commit -m "test(rb-star-align): add opt-in integration test gated on STAR_BIN"
```

---

### Task 26: Update CLAUDE.md and README

**Files:**
- Modify: `CLAUDE.md`
- Modify: `README.md`

- [ ] **Step 1: Update CLAUDE.md**

In `/home/xzg/project/rust_brain/CLAUDE.md`, update the "Cargo Workspace" section to list 7 crates instead of 5. Add two new bullets after the existing `rb-trimming` bullet:

```markdown
**rb-star-index** — Adapter invoking STAR_rs `star --runMode genomeGenerate` as a subprocess. Uses `BinaryResolver` for tool discovery (falls back to PATH). Streams stderr lines as `RunEvent::Log`.

**rb-star-align** — Adapter invoking STAR_rs `star --runMode alignReads` per sample. Parses `Log.final.out` for mapping stats and merges per-sample `ReadsPerGene.out.tab` files into a single `counts_matrix.tsv` ready for DESeq2. Streams stderr and honours cooperative cancellation.
```

Under "Key Patterns", add:

```markdown
- **Adding a subprocess-based module**: Use `rb_core::binary::BinaryResolver` to discover the tool (never hardcode `Command::new("toolname")`); register the binary id + install hint in `KNOWN_BINARIES` in `rb-core/src/binary.rs` so it shows up in Settings.
- **RunEvent channel**: modules emit `RunEvent::Progress` (progress bar) or `RunEvent::Log` (streaming stderr/stdout shown to user). The Runner forwards both to Tauri as `run-progress` and `run-log` events.
- **Cancellation**: modules receive a `CancellationToken` and must honour it. Subprocess-based modules use `tokio::select!` on `child.wait()` vs `cancel.cancelled()` and call `child.kill().await` on cancel.
```

- [ ] **Step 2: Update README**

In `/home/xzg/project/rust_brain/README.md`, add a section after the existing install notes:

```markdown
## STAR_rs dependency

`rb-star-index` and `rb-star-align` invoke the `star` binary from
https://github.com/AI4S-YB/STAR_rs. Build it and either put it on your PATH or
configure its full path in the app's Settings view (⚙ in the sidebar):

```bash
git clone https://github.com/AI4S-YB/STAR_rs.git
cd STAR_rs && cargo build --release
# then either add target/release to PATH, or use the Settings view in RustBrain
```

## cutadapt-rs dependency

Similarly, `rb-trimming` invokes the `cutadapt-rs` binary from
https://github.com/AI4S-YB/cutadapt-rs. Same discovery mechanism: PATH or
Settings-configured path.
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md README.md
git commit -m "docs: document star_index/star_align modules and STAR install notes"
```

---

## Wrap-up checklist

After Task 26 is merged:

- [ ] `cargo test --workspace` passes (all unit tests + cancel integration test).
- [ ] Manual golden-path test: open the app, run STAR index → STAR align → click "Use this matrix in DESeq2" → verify DESeq2 form pre-fills.
- [ ] Manual cancel test: start a long-running STAR alignment, cancel mid-run, run `pgrep star` and verify no orphan process remains.
- [ ] Settings view: browse to a binary, save, restart app, verify the setting persists.

If any of these fail, open a follow-up task — do not declare the feature done.

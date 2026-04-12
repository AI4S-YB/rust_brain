# RustBrain MVP Design Spec

> Transcriptomics analysis desktop tool — full Rust stack with Tauri + WebView

## Overview

RustBrain is a desktop application for end-to-end RNA-seq transcriptomics analysis. It integrates existing Rust bioinformatics tools (fastqc-rs, cutadapt-rs, DESeq2_rs) behind a unified UI, with interactive ECharts visualizations and project-based data management.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Desktop framework | Tauri v2 | Mature Rust + WebView, built-in IPC, native dialogs, small bundle |
| Tool integration | Mixed: library calls for own tools, subprocess for external | Best performance for own tools, flexibility for external |
| Execution model | Free module mode (MVP); pipeline orchestration later | Ship fast, add orchestration when modules are stable |
| Data management | Full project-based with independent work directories | Reproducibility, organization, easy sharing |
| Visualization | ECharts (primary) + D3.js (supplementary, as needed) | WebGL perf for 60k+ gene scatter, brush linking, small bundle |
| Code organization | Monorepo Cargo workspace | Unified compilation, seamless cross-crate iteration |
| MVP scope | QC + Trimming + DESeq2 | Three mature Rust tools, covers core RNA-seq workflow |

## Project Structure

```
rust_brain/
├── Cargo.toml                    # workspace root
├── frontend/                     # existing HTML/CSS/JS (Tauri distDir)
│   ├── index.html
│   ├── css/style.css
│   └── js/app.js
├── crates/
│   ├── rb-app/                   # Tauri main binary
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── main.rs           # Tauri entry point
│   │   │   ├── commands/         # Tauri commands (frontend invoke targets)
│   │   │   │   ├── mod.rs
│   │   │   │   ├── project.rs    # create/open/save project
│   │   │   │   ├── qc.rs         # QC module commands
│   │   │   │   ├── trimming.rs   # Trimming module commands
│   │   │   │   └── deseq2.rs     # DESeq2 module commands
│   │   │   └── state.rs          # AppState (project, module registry)
│   │   ├── tauri.conf.json
│   │   └── icons/
│   │
│   ├── rb-core/                  # core business logic (lib)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── project.rs        # Project model: create/load/save
│   │       ├── module.rs         # Module trait: unified analysis interface
│   │       ├── runner.rs         # Task runner: async execution, progress
│   │       └── config.rs         # Global config (tool paths, defaults)
│   │
│   ├── rb-qc/                    # QC adapter (lib)
│   │   ├── Cargo.toml            # depends on fastqc-rs, rb-core
│   │   └── src/lib.rs            # implements Module trait
│   │
│   ├── rb-trimming/              # Trimming adapter (lib)
│   │   ├── Cargo.toml            # depends on cutadapt-rs, rb-core
│   │   └── src/lib.rs
│   │
│   └── rb-deseq2/               # DESeq2 adapter (lib)
│       ├── Cargo.toml            # depends on deseq2-rs, rb-core
│       └── src/lib.rs
│
└── deps/                         # external tool sources (git submodule)
    ├── fastqc-rs/
    ├── cutadapt-rs/
    └── DESeq2_rs/
```

## Core Interfaces

### Module Trait

All analysis modules implement a unified interface defined in `rb-core`:

```rust
pub trait Module: Send + Sync {
    /// Unique module identifier: "qc", "trimming", "deseq2"
    fn id(&self) -> &str;

    /// Human-readable display name
    fn name(&self) -> &str;

    /// Validate parameters, return list of errors (empty = valid)
    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError>;

    /// Run the analysis asynchronously
    /// progress_tx pushes real-time progress to the frontend
    fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        progress_tx: tokio::sync::mpsc::Sender<Progress>,
    ) -> impl Future<Output = Result<ModuleResult, ModuleError>> + Send;
}

pub struct Progress {
    pub fraction: f64,       // 0.0 ~ 1.0
    pub message: String,     // "Processing read 5,000,000 / 10,000,000"
}

pub struct ModuleResult {
    pub output_files: Vec<PathBuf>,   // generated files
    pub summary: serde_json::Value,   // structured summary for frontend
    pub log: String,                  // full log text
}

pub struct ValidationError {
    pub field: String,        // parameter field name
    pub message: String,      // human-readable error
}

pub enum ModuleError {
    InvalidParams(Vec<ValidationError>),
    IoError(std::io::Error),
    ToolError(String),        // upstream tool returned an error
    Cancelled,
}
```

### Project Model

```rust
pub struct Project {
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub root_dir: PathBuf,
    pub runs: Vec<RunRecord>,
}

pub struct RunRecord {
    pub id: String,                 // UUID
    pub module_id: String,          // "qc" / "trimming" / "deseq2"
    pub params: serde_json::Value,  // parameter snapshot
    pub status: RunStatus,          // Pending / Running / Done / Failed / Cancelled
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub result: Option<ModuleResult>,
}

pub enum RunStatus {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}
```

### Project Directory Layout

Run directories use `{module_id}_{run_id_short}` to avoid same-day collisions:

```
~/rnaseq_projects/my_experiment/
├── project.json              # project metadata + run records
├── input/                    # raw files (or symlinks)
├── runs/
│   ├── qc_a3f8b2c1/         # each run uses UUID prefix (8 chars)
│   │   ├── params.json
│   │   ├── fastqc_output/
│   │   └── run.log
│   ├── trimming_e7d4f091/
│   └── deseq2_c2b19a5e/
```

## Frontend-Backend Communication

### Tauri Commands (frontend → backend)

```typescript
// Project management
invoke('create_project', { name, dir })        → Project
invoke('open_project', { dir })                → Project
invoke('list_recent_projects')                 → Project[]

// Module execution
invoke('validate_params', { moduleId, params }) → ValidationError[]
invoke('run_module', { moduleId, params })      → string (runId)
invoke('cancel_run', { runId })                 → void

// Result queries
invoke('get_run_result', { runId })             → RunRecord
invoke('list_runs', { moduleId? })              → RunRecord[]

// File operations
invoke('select_files', { filters })             → string[] (file paths)
invoke('select_directory')                      → string
invoke('read_table_preview', { path, nRows })   → { headers, rows }
```

### Tauri Events (backend → frontend)

```typescript
listen('run-progress',  (e) => { /* { runId, fraction, message } */ })
listen('run-completed', (e) => { /* { runId, result: ModuleResult } */ })
listen('run-failed',    (e) => { /* { runId, error: string } */ })
```

### Execution Flow

1. Frontend calls `invoke('run_module', { moduleId, params })`
2. `rb-app` command handler spawns a tokio task, returns `runId` immediately
3. Task calls `rb-core` runner, which invokes the module's `run()` method
4. Module sends `Progress` messages through `progress_tx`
5. Runner forwards progress as `emit('run-progress', ...)` events
6. On completion, runner emits `run-completed` with `ModuleResult`
7. Frontend receives event, switches to Results tab, renders charts with real data

## Frontend Adaptations

### Changes from current state

1. **Replace Plotly.js with ECharts** — rewrite chart rendering functions, keep same data flow
2. **Replace mock API with Tauri invoke** — swap `api.invoke()` stub for `window.__TAURI__.core.invoke()`
3. **Native file dialogs** — click on file drop zones calls `invoke('select_files')` instead of `<input type="file">`
4. **Real-time progress** — listen to Tauri events, update progress bar + log panel
5. **Project management UI** — add New/Open Project dialogs to Dashboard, persist recent projects
6. **Custom Plot tab** — new tab in each module's results: user picks X/Y columns, chart type, color grouping; ECharts renders dynamically

### Unchanged

- Overall layout, sidebar navigation, hash routing
- CSS design system (Warm Botanical Lab theme)
- Tab switching, collapsible sections, form controls
- Responsive layout

## Tech Stack

| Layer | Choice |
|-------|--------|
| Desktop framework | Tauri v2 |
| Frontend | Vanilla HTML/CSS/JS |
| Visualization | ECharts (primary), D3.js (supplementary) |
| Fonts | Zilla Slab + Karla + Fira Code |
| Backend runtime | Rust + Tokio |
| Serialization | serde + serde_json |
| Project persistence | JSON files |
| Tool integration | Library calls (fastqc-rs, cutadapt-rs, DESeq2_rs) |
| Build system | Cargo workspace + Tauri CLI |

## MVP Scope

### Included

1. **Project management** — create, open, recent projects list
2. **QC module** — file select → params → run fastqc-rs → quality distribution chart + summary table
3. **Trimming module** — file select → adapter/quality params → run cutadapt-rs → stats table + length distribution chart
4. **DESeq2 module** — counts matrix + coldata → design/reference params → run DESeq2_rs → volcano plot + MA plot + results table
5. **Custom plotting** — select columns from results to plot scatter/box/histogram/heatmap
6. **Real-time progress** — live log output + progress bar during runs
7. **Result export** — charts as PNG/SVG, tables as TSV

### Excluded (future iterations)

- Pipeline orchestration (multi-module auto-chaining)
- WGCNA, Enrichment, Alignment, Quantification modules
- External tool subprocess integration (HISAT2, StringTie)
- Batch sample management
- i18n / multi-language
- Auto-update

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Tool library APIs are not uniform; adapter effort unknown | Investigate each tool's public API first; keep adapter layers thin |
| fastqc-rs output is HTML reports; need structured data | Extract from internal data structures, not by parsing HTML |
| DESeq2_rs depends on faer linear algebra; long compile times | Enable incremental compilation at workspace level; only recompile changed crates |
| ECharts heatmap memory with large gene matrices | Backend does data sampling/pagination; frontend renders only visible region |

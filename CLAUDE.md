# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```bash
# Clone with submodules (required — tools are git submodules in deps/)
git clone --recurse-submodules https://github.com/AI4S-YB/rust_brain.git

# Check entire workspace compiles
cargo check --workspace

# Run all tests
cargo test --workspace

# Run a single test
cargo test -p rb-core -- create_and_load_project

# Lint (our crates only, deps capped to warn)
RUSTFLAGS="--cap-lints=warn" cargo clippy -p rb-core -p rb-app -p rb-qc -p rb-trimming -p rb-deseq2 -- -D warnings

# Format (our crates only, excludes submodule deps)
cargo fmt -p rb-core -p rb-app -p rb-qc -p rb-trimming -p rb-deseq2

# Run desktop app (requires: cargo install tauri-cli --locked)
cd crates/rb-app && cargo tauri dev

# Frontend-only preview (no Rust backend, uses mock API shim)
cd frontend && python3 -m http.server 8090

# Linux system deps for Tauri/WebView
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libgtk-3-dev
```

## Architecture

### Cargo Workspace (5 crates)

**rb-core** — Core library with no tool dependencies. Defines:
- `Module` trait (`#[async_trait]`) — the central abstraction all analysis modules implement: `id()`, `name()`, `validate()`, `async run()`
- `Project` model — project directory management with `project.json`, per-run directories (`runs/{module}_{uuid8}/`)
- `Runner` — async task executor that spawns modules via tokio, routes progress through mpsc channels, manages cancellation via JoinHandle abort

**rb-app** — Tauri v2 binary. Wires everything together:
- `AppState` holds `ModuleRegistry` (HashMap of `Arc<dyn Module>`) and `Runner`
- 9 Tauri commands in `commands/`: project (create/open/list), modules (validate/run/cancel/get_result/list_runs), files (select_files/select_directory/read_table_preview)
- Runner's progress callback emits `"run-progress"` Tauri events to frontend
- Runner is created when a project is opened; accessing project goes through `runner.project()`

**rb-qc** — Adapter wrapping fastqc-rs. Calls `fastqc_rs::analysis::process_file()` directly in `spawn_blocking`.

**rb-deseq2** — Adapter wrapping DESeq2_rs. Calls `DESeqDataSet::from_csv()` → `.run()` → `.results(Contrast::LastCoefficient)` in `spawn_blocking`.

**rb-trimming** — Adapter calling cutadapt-rs CLI as subprocess (`std::process::Command`). Cannot use library dep because cutadapt-core uses workspace dependency inheritance incompatible with external path deps.

### Tool Submodules (deps/)

Three git submodules in `deps/`: `fastqc-rs`, `cutadapt-rs`, `DESeq2_rs`. These are AI4S-YB org repos. rb-qc and rb-deseq2 reference them as path dependencies; rb-trimming uses the CLI binary.

### Frontend (frontend/)

Vanilla HTML/CSS/JS single-page app — no build step, no framework.

- **Routing**: hash-based (`#qc`, `#differential`, etc.), `navigate()` function renders views into `#content`
- **Charts**: ECharts 5 (replaced Plotly). Each module has chart render functions. `ECHART_THEME` constant for consistent styling.
- **Tauri integration**: `window.__TAURI__.core.invoke()` for commands, `window.__TAURI__.event.listen()` for progress events. Browser-mode shim in `index.html` provides mock fallback for development without Rust backend.
- **Design**: "Warm Botanical Lab" theme — light cream background, Zilla Slab headings, Karla body text, teal/coral/green accent palette.

### Data Flow

```
Frontend invoke('run_module') → rb-app command → Runner.spawn()
  → creates RunRecord (Pending→Running) → spawns tokio task
  → adapter.run() calls tool library/subprocess
  → Progress sent via mpsc → Runner forwards as Tauri emit('run-progress')
  → completion updates RunRecord (Done/Failed) → emit('run-completed')
```

## Key Patterns

- **Adding a new module**: Create `crates/rb-{name}/` implementing `Module` trait, register in `rb-app/src/main.rs` via `registry.register(Arc::new(...))`, add frontend view in `app.js`
- **CPU-bound work**: Always wrap in `tokio::task::spawn_blocking` (adapters do this for tool calls)
- **Params**: Passed as `serde_json::Value` — each adapter deserializes what it needs in `run()` and validates in `validate()`
- **Project state**: Shared via `Arc<tokio::sync::Mutex<Project>>` between Runner and commands

## CI/CD

Workflow at `.github/workflows/ci.yml` triggers on `v*` tags only. Creates GitHub Release with platform artifacts (.deb, .AppImage, .dmg, .msi).

```bash
# Release a version
git tag v0.2 -m "v0.2 — description"
git push origin v0.2
```

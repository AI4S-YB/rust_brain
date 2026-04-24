# Project Inputs (P1) — Implementation Plan

> **For agentic workers:** Steps use `- [ ]` checkbox syntax. Complete each task end-to-end (edit → build/test → commit) before moving on.

**Goal:** Ship Phase 1 of the Project Asset Management design: a first-class Input registry inside each project so users can register FASTQ / FASTA / GTF / GFF files once and stop re-dragging them. No module form changes yet — this phase is pure inventory.

**Architecture:** Extend `rb_core::project::Project` with an `inputs: Vec<InputRecord>` field (with `#[serde(default)]` for backward compat). Add 5 Tauri commands (`list_inputs`, `register_input`, `register_inputs_batch`, `update_input`, `delete_input`, `scan_inputs`). Add a new frontend view at `#inputs` following the `#tasks` pattern we just built. Move Tasks from the System sidebar section up into a new **Project** section that also contains Inputs.

**Tech Stack:** Rust (rb-core lib, rb-app Tauri commands), vanilla JS frontend.

**Upstream spec:** `docs/superpowers/specs/2026-04-24-project-asset-management-design.md`

---

## File Structure

**Created:**
- `crates/rb-core/src/input.rs` — `InputRecord`, `InputKind`, `InputScanReport`, detection helper, tests
- `crates/rb-app/src/commands/inputs.rs` — 6 Tauri handlers
- `frontend/js/modules/inputs/view.js` — Inputs view
- `frontend/js/api/inputs.js` — JS API wrapper
- `frontend/css/views/inputs.css` — table styles (mostly shared with tasks.css)

**Modified:**
- `crates/rb-core/src/lib.rs` — `pub mod input;`
- `crates/rb-core/src/project.rs` — add `inputs: Vec<InputRecord>` with `#[serde(default)]`; add `register_input`, `register_inputs_batch`, `update_input`, `delete_input`, `scan_inputs` methods; tests
- `crates/rb-app/src/commands/mod.rs` — `pub mod inputs;`
- `crates/rb-app/src/main.rs` — register 6 new commands in `invoke_handler!`
- `frontend/js/core/constants.js` — add `"inputs"` to `KNOWN_VIEWS`
- `frontend/js/core/router.js` — route `#inputs` to `renderInputsView`
- `frontend/js/core/events.js` — dispatch `delete-input`, `register-inputs`, `scan-inputs`, etc.
- `frontend/index.html` — new **Project** sidebar section; move Tasks up; add Inputs entry; mock-mode shim stubs
- `frontend/js/i18n.js` — `nav.project`, `nav.inputs`, `inputs.*` (EN + ZH)
- `frontend/css/style.css` — `@import url('views/inputs.css');`

---

## Task 1: Define `InputRecord` and kind detection (rb-core)

**Files:**
- Create: `crates/rb-core/src/input.rs`
- Modify: `crates/rb-core/src/lib.rs`

- [ ] **Step 1: Create `crates/rb-core/src/input.rs`**

Define:
- `InputKind` enum: `Fastq, Fasta, Gtf, Gff, CountsMatrix, SampleSheet, Other`, derive serde + PartialEq.
- `InputRecord` struct with fields per spec (id, path, display_name, kind, size_bytes, registered_at, sample_id, paired_with, missing, notes). `#[serde(default)]` on everything nullable.
- `detect_kind(path: &Path) -> InputKind` — strip `.gz` / `.bz2`, then check extensions per spec.
- `pub fn new_input_id() -> String` using uuid's 8-char shortcut (same pattern as `create_run`), prefixed `"in_"`.
- `InputScanReport { refreshed: u32, now_missing: u32, recovered: u32 }`.
- `InputPatch { display_name: Option<String>, kind: Option<InputKind>, notes: Option<String> }`.

Unit tests for `detect_kind` covering: .fastq, .fq.gz, .fasta, .fa, .fna, .gtf, .gff3, .tsv (→ CountsMatrix), random extension (→ Other).

- [ ] **Step 2: Expose the module in `crates/rb-core/src/lib.rs`**

Add `pub mod input;` alongside the existing `pub mod project;` etc.

- [ ] **Step 3: Verify**

`cargo check -p rb-core` → clean. `cargo test -p rb-core input::` → all kind-detection tests pass.

- [ ] **Step 4: Commit**

`feat(rb-core): introduce InputRecord and kind detection for P1 inputs registry`

---

## Task 2: Project methods for inputs CRUD + scan (rb-core)

**Files:** Modify: `crates/rb-core/src/project.rs`

- [ ] **Step 1: Add the `inputs` field**

In the `Project` struct, after `runs: Vec<RunRecord>`, add:

```rust
#[serde(default)]
pub inputs: Vec<crate::input::InputRecord>,
```

Add `inputs: Vec::new()` to the `Project::create` initializer.

- [ ] **Step 2: Add methods on `Project`**

- `register_input(&mut self, path: &Path, kind: Option<InputKind>, display_name: Option<String>) -> io::Result<InputRecord>`
  - Canonicalize the path (if `canonicalize` fails for not-found → return `io::Error::new(NotFound, ...)`)
  - If an existing record has the same canonical path → return it (idempotent; do NOT create a duplicate).
  - Read size with `fs::metadata`.
  - Build an `InputRecord` with `id = new_input_id()`, `kind = kind.unwrap_or_else(|| detect_kind(&path))`, `display_name = display_name.or(path.file_name().str)`, `registered_at = Utc::now()`, `missing = false`.
  - Push + `save()`.
- `register_inputs_batch(&mut self, paths: &[PathBuf]) -> (Vec<InputRecord>, Vec<(PathBuf, String)>)` — loop + collect successes and `(path, err)` pairs. Single `save()` at the end.
- `update_input(&mut self, id: &str, patch: InputPatch) -> io::Result<InputRecord>` — find by id, apply non-None patch fields, `save()`, return clone.
- `delete_input(&mut self, id: &str) -> io::Result<()>` — remove record by id, `save()`. NEVER touch the file on disk.
- `scan_inputs(&mut self) -> io::Result<InputScanReport>` — iterate, re-stat each path, update `size_bytes` + `missing` flag, count transitions. `save()` once at the end.

- [ ] **Step 3: Tests — add a new `mod input_registry_tests`**

Cover:
- `register_input` assigns an id with the `"in_"` prefix and records size from fs metadata.
- `register_input` is idempotent on duplicate absolute paths (same id returned).
- `register_input` rejects a non-existent path with `NotFound`.
- `delete_input` removes the record but leaves the file on disk untouched.
- `update_input` updates only the non-None patch fields.
- `scan_inputs` sets `missing = true` after the file disappears and clears it when restored.

- [ ] **Step 4: Verify**

`cargo test -p rb-core` → all green (existing + new tests).

- [ ] **Step 5: Commit**

`feat(rb-core): project-level input registry with scan and idempotent register`

---

## Task 3: Tauri commands (rb-app)

**Files:**
- Create: `crates/rb-app/src/commands/inputs.rs`
- Modify: `crates/rb-app/src/commands/mod.rs`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: Create `crates/rb-app/src/commands/inputs.rs`**

Implement 6 Tauri commands following the `modules.rs::list_runs` pattern (acquire runner, lock project, mutate, return):

- `list_inputs() -> Result<Vec<InputRecord>, String>`
- `register_input(path: String, kind: Option<InputKind>, display_name: Option<String>) -> Result<InputRecord, String>`
- `register_inputs_batch(paths: Vec<String>) -> Result<BatchRegisterResult, String>` where `BatchRegisterResult { registered: Vec<InputRecord>, errors: Vec<(String, String)> }` (Serialize)
- `update_input(id: String, patch: InputPatch) -> Result<InputRecord, String>`
- `delete_input(id: String) -> Result<(), String>`
- `scan_inputs() -> Result<InputScanReport, String>`

All handlers `.map_err(|e| e.to_string())` the `io::Error`.

- [ ] **Step 2: Wire the module**

In `crates/rb-app/src/commands/mod.rs` add `pub mod inputs;`.

In `crates/rb-app/src/main.rs`, register all 6 in the `invoke_handler![ ... ]` list. Place next to the other `commands::modules::*` entries.

- [ ] **Step 3: Verify**

`cargo check -p rb-app` → clean.
`cargo clippy -p rb-core -p rb-app` → 0 errors in our code.

- [ ] **Step 4: Commit**

`feat(rb-app): Tauri commands for project inputs registry`

---

## Task 4: Frontend API wrapper

**Files:** Create: `frontend/js/api/inputs.js`

- [ ] **Step 1: Create the wrapper**

Following `api/modules.js` style:

```js
import { api } from '../core/tauri.js';

export const inputsApi = {
  list()                           { return api.invoke('list_inputs'); },
  register(path, opts = {})        { return api.invoke('register_input', { path, kind: opts.kind ?? null, displayName: opts.displayName ?? null }); },
  registerBatch(paths)             { return api.invoke('register_inputs_batch', { paths }); },
  update(id, patch)                { return api.invoke('update_input', { id, patch }); },
  delete(id)                       { return api.invoke('delete_input', { id }); },
  scan()                           { return api.invoke('scan_inputs'); },
};
```

- [ ] **Step 2: Verify**

`node --check frontend/js/api/inputs.js` → ok.

---

## Task 5: Inputs view (frontend)

**Files:**
- Create: `frontend/js/modules/inputs/view.js`
- Create: `frontend/css/views/inputs.css`
- Modify: `frontend/css/style.css` (import the new css)

- [ ] **Step 1: Create `frontend/js/modules/inputs/view.js`**

Structure matches `modules/tasks/view.js`:

- Module-level `viewState { inputs: [], filterKind: 'all', filterMissing: false, search: '', selected: Set }`.
- `loadAll()` calls `inputsApi.list()` and re-renders.
- `renderInputsView(container)`: emits header + toolbar (register files / register folder / scan / filter by kind / search / show-missing toggle / delete selected) + table (check, kind badge, name, path, size, registered_at, missing indicator, row delete).
- Event binding scoped to the container. Handled `data-act` names: `inputs-register-files`, `inputs-register-folder`, `inputs-scan`, `inputs-delete-selected`, `inputs-delete-row`. Plus change events for filter selects and search input.
- `inputs-register-files` → call existing `filesApi.selectFiles({ multiple: true })`, pass the returned list to `registerBatch`, then `loadAll()`.
- `inputs-register-folder` → call `filesApi.selectDirectory()`, then walk the directory via a new Tauri command **or (simpler for P1)** register every file one-by-one using `registerBatch` after listing with `filesApi`. Since there's no existing `list_directory_files` command, for P1 register the **single chosen folder path is not enough** — call `filesApi.selectFiles({ multiple: true })` from the folder-picker button (browser mode fallback handles multi). If we need true recursion, defer that to P2.
  - **Decision for P1:** drop the folder-register button; keep only multi-file registration. Document in the view's toolbar; revisit in P2.
- Reuse `formatBytes` exported from `modules/run-result.js`.
- Reuse the `<InputKindBadge>` pattern via a `kindPill(kind)` inline helper (no need for a separate component file).

- [ ] **Step 2: Create `frontend/css/views/inputs.css`**

Mostly mirror `views/tasks.css` (tables, pills, toolbar). Add kind-specific pill colors:
- `.kind-fastq` teal, `.kind-fasta` purple, `.kind-gtf` coral, `.kind-gff` gold, `.kind-countsmatrix` blue, `.kind-samplesheet` green, `.kind-other` slate.
- `.input-missing` row highlight: left red border + muted text.

- [ ] **Step 3: Import CSS**

Add `@import url('views/inputs.css');` to `frontend/css/style.css`.

- [ ] **Step 4: Verify**

`node --check frontend/js/modules/inputs/view.js` → ok.

---

## Task 6: Routing + sidebar + i18n

**Files:**
- Modify: `frontend/js/core/constants.js`
- Modify: `frontend/js/core/router.js`
- Modify: `frontend/index.html`
- Modify: `frontend/js/i18n.js`

- [ ] **Step 1: Register the view id**

In `frontend/js/core/constants.js`, add `'inputs'` to the `KNOWN_VIEWS` seed list (next to `'tasks'`).

- [ ] **Step 2: Route `#inputs`**

In `frontend/js/core/router.js`, add an `else if (view === 'inputs')` branch that dynamic-imports `modules/inputs/view.js` and calls `renderInputsView(content)`.

- [ ] **Step 3: Sidebar — add **Project** section**

In `frontend/index.html`, add a new `<div class="nav-section">` BETWEEN the AI Copilot section and the Analysis Pipeline section:

```html
<div class="nav-section">
  <div class="nav-section-title" data-i18n="nav.project">Project</div>
  <a class="nav-item" data-view="inputs" href="#inputs">
    <i data-lucide="database"></i>
    <span data-i18n="nav.inputs">Inputs</span>
  </a>
  <a class="nav-item" data-view="tasks" href="#tasks">
    <i data-lucide="list-checks"></i>
    <span data-i18n="nav.tasks">Tasks</span>
  </a>
</div>
```

**Remove** the `data-view="tasks"` link from the old System section (leaving only Settings there).

- [ ] **Step 4: i18n strings**

In `frontend/js/i18n.js`:

**EN block:**
```
nav.project = 'Project'
nav.inputs  = 'Inputs'
inputs.title = 'Project Inputs'
inputs.subtitle = 'Registered files that modules can pick from.'
inputs.register_files = 'Register files'
inputs.scan = 'Refresh / Scan'
inputs.filter_kind = 'Kind'
inputs.filter_missing = 'Missing only'
inputs.all_kinds = 'All kinds'
inputs.search_placeholder = 'Search by name…'
inputs.col_kind = 'Kind'
inputs.col_name = 'Name'
inputs.col_path = 'Path'
inputs.col_size = 'Size'
inputs.col_registered = 'Registered'
inputs.col_status = 'Status'
inputs.col_actions = 'Actions'
inputs.delete_selected = 'Delete selected'
inputs.delete_confirm_title = 'Remove input registration?'
inputs.delete_confirm_message = 'Remove {n} registration(s)? The file(s) on disk are not deleted.'
inputs.empty = 'No inputs registered yet.'
inputs.missing_badge = 'Missing'
inputs.kind.fastq = 'FASTQ'
inputs.kind.fasta = 'FASTA'
inputs.kind.gtf = 'GTF'
inputs.kind.gff = 'GFF'
inputs.kind.countsmatrix = 'Counts'
inputs.kind.samplesheet = 'Sample sheet'
inputs.kind.other = 'Other'
inputs.scan_toast_title = 'Scan complete'
inputs.scan_toast_message = '{refreshed} refreshed, {now_missing} missing, {recovered} recovered'
inputs.count_label = '{n} shown'
inputs.total_size_label = 'Total: {size}'
```

**ZH block** — mirror with translations:
```
nav.project = '项目'
nav.inputs  = '输入数据'
inputs.title = '项目输入'
inputs.subtitle = '已登记的文件，可被各模块选用。'
... (translate each key)
```

- [ ] **Step 5: Verify**

`node --check` on every modified JS file, plus:

```bash
cargo check -p rb-core -p rb-app
cargo test  -p rb-core
```

- [ ] **Step 6: Commit**

`feat(frontend): Inputs view + sidebar Project section (P1)`

---

## Task 7: Mock-mode shim for browser preview

**Files:** Modify: `frontend/index.html`

- [ ] **Step 1: Add stub handlers**

Inside the `window.__TAURI__` mock `invoke` block, add:

- `_mockInputs` Map
- `list_inputs` → `[..._mockInputs.values()]`
- `register_input` → add to map with uuid, return record
- `register_inputs_batch` → loop + return `{registered, errors:[]}`
- `update_input` → patch in place
- `delete_input` → delete from map
- `scan_inputs` → `{refreshed: N, now_missing: 0, recovered: 0}` (can't stat real files in browser)

Seed one fake entry so the view is non-empty on first browser preview.

- [ ] **Step 2: Verify by curl-loading `index.html`** (browser verification is optional; at minimum ensure the file parses).

- [ ] **Step 3: Commit**

`chore(frontend): browser-mode mock for inputs registry`

---

## Task 8: Wire up events.js dispatcher

**Files:** Modify: `frontend/js/core/events.js`

- [ ] **Step 1: No changes required if inputs view handles its own clicks via scoped listener.**

The tasks view pattern uses a scoped listener inside the view module, not a global `data-act` case. Follow the same pattern — events.js stays untouched. (If during implementation we find otherwise, add cases here and update this step.)

---

## Task 9: End-to-end smoke test

- [ ] **Step 1: Run the full test suite**

```bash
cargo test  -p rb-core -p rb-app
cargo clippy -p rb-core -p rb-app
```

Both clean.

- [ ] **Step 2: Browser smoke test (manual)**

Not automatable in this environment; document the test plan in the final commit message for reviewers:

```
Manual smoke:
- Open a project, visit #inputs → empty state renders
- Register a fastq → appears in table with Size column populated
- Rename via double-click → display_name persists after reload
- Delete a row → confirm → gone
- Close + reopen project → registrations survive
- Rename a file on disk + click Scan → missing flag appears, pill turns red
```

- [ ] **Step 3: Final commit squash (if anything trailing)**

Not required if each task committed separately.

---

## Non-goals explicitly out of P1 (do not implement, even if easy)

- Any changes to module forms (stays P4).
- `samples[]` / `assets[]` fields on `Project` (stay P2/P3).
- Global references registry (stays P5).
- Drag-drop onto the Inputs page directly — for P1, "Register files" button is enough.
- Folder-walk recursive register — deferred, see Task 5 Step 1 decision note.

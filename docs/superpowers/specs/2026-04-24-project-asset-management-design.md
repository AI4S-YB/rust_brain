# Project Asset Management — Design

**Date:** 2026-04-24
**Status:** approved (brainstorm phase)
**Scope:** Lift the project from a "directory with runs[]" to a full workspace that manages inputs, samples, derived assets, and their lineage — so users don't re-drag the same FASTQ into every module, and so big artifacts (STAR indexes, BAMs) become first-class reusable items.

## Problem

Today `Project` only tracks `runs[]`. Every module form starts empty: users drop the same FASTQ files repeatedly, remember by hand which STAR index they built last week, and when disk fills up they have no way to see *what* is taking space or *which* artifacts are still referenced by downstream runs. The app is shaped like a task runner, not a project.

Concretely:

- A 20-sample RNA-seq study forces the user to drop `sample_01_R1.fastq.gz`, `sample_01_R2.fastq.gz`, ... into QC → Trimming → STAR-align one file at a time.
- The STAR index output (10–30 GB) lives in `runs/star_index_abc/` with no discoverable handle. Users who want to re-use it for a second alignment have to navigate into runs and copy the path.
- Delete runs carelessly from the new Tasks page and you may delete the STAR index a completed-last-week alignment still logically depends on — there's no reference tracking.
- DESeq2 needs group/condition per sample. Today that's re-typed per run.

## Goals

- **Registry, not copy:** track user files by absolute path; don't move or duplicate big files.
- **Single sample sheet:** one place to declare sample_id / group / paired-end layout, re-used by every downstream module.
- **Asset reuse:** STAR index built once, selectable from any downstream align run's form.
- **Lineage:** each Run records `inputs_used[]` and `assets_produced[]` so we can warn "deleting this run will orphan 1 asset still referenced by 3 later runs".
- **Backward compatible:** existing `project.json` opens without migration; all new fields `#[serde(default)]`.
- **Incremental rollout:** six phases, each shippable on its own. P1 delivers value without touching any module form.

## Non-goals

- **No file copying / content-addressed storage.** `input/` stays as-is; we reference by absolute path. An explicit "Import a copy" action may land in P5 but is not required.
- **No automatic discovery scanning on project open (v1).** We offer a user-triggered "scan" action that walks `input/` and offers to register found files. Auto-index walks are out of scope until we have reason to believe scanning is fast on large projects.
- **No workflow / pipeline DAG execution.** Lineage is informational; running "the next step" is still a separate user action in P1–P4. A workflow engine may come later.
- **No cross-project asset sharing in v1.** Global reference registry is P5.
- **No SQLite.** Stay on JSON until profile shows it's the bottleneck.

## Architecture

### Entity model (persisted in `project.json`)

Five top-level arrays, all with `#[serde(default)]`:

```
Project {
  name, created_at, default_view,        // existing
  runs:    Vec<RunRecord>,               // existing (gains inputs_used, assets_produced)
  inputs:  Vec<InputRecord>,             // NEW — P1
  samples: Vec<SampleRecord>,            // NEW — P2
  assets:  Vec<AssetRecord>,             // NEW — P3
}
```

```rust
pub struct InputRecord {
    pub id: String,                // e.g. "in_a1b2c3d4"
    pub path: PathBuf,             // absolute
    pub display_name: String,      // defaults to file name, user-editable
    pub kind: InputKind,           // Fastq, Fasta, Gtf, Gff, CountsMatrix, SampleSheet, Other
    pub size_bytes: u64,           // captured at register; re-checked on refresh
    pub registered_at: DateTime<Utc>,
    pub sample_id: Option<String>, // P2 link
    pub paired_with: Option<String>, // P2 — the other input id in an R1/R2 pair
    pub missing: bool,             // set by scan when file is gone
    pub notes: Option<String>,
}

pub enum InputKind { Fastq, Fasta, Gtf, Gff, CountsMatrix, SampleSheet, Other }

pub struct SampleRecord {          // P2
    pub id: String,
    pub name: String,
    pub group: Option<String>,
    pub condition: Option<String>,
    pub inputs: Vec<String>,       // InputRecord ids
    pub paired: bool,
    pub notes: Option<String>,
}

pub struct AssetRecord {           // P3
    pub id: String,
    pub kind: AssetKind,           // StarIndex, Bam, TrimmedFastq, Gtf, CountsMatrix, Report, Other
    pub path: PathBuf,
    pub size_bytes: u64,
    pub produced_by_run_id: String,
    pub display_name: String,
    pub schema: Option<String>,    // free-form tag: "STAR 2.7 index", "counts (gene × sample)"
    pub created_at: DateTime<Utc>,
}
```

`RunRecord` gains (P3+):

```rust
pub struct RunRecord {
    // ... existing fields ...
    #[serde(default)] pub inputs_used:      Vec<String>,  // InputRecord ids
    #[serde(default)] pub assets_used:      Vec<String>,  // AssetRecord ids
    #[serde(default)] pub assets_produced:  Vec<String>,  // AssetRecord ids
}
```

### UI structure (sidebar reorganization)

Today the sidebar has **Overview / AI Copilot / Analysis Pipeline / Utilities / System**. We promote the Tasks page into a new **Project** section that collects the registry views:

```
Project
  ├── Overview    (P6)  # disk usage, recent activity, workflow completion
  ├── Inputs      (P1)  # Input registry
  ├── Samples     (P2)
  ├── Assets      (P3)
  ├── Tasks       (shipped) — Runs table, gains "Inputs / Assets" columns in P4
  └── References  (P5)
Analysis Pipeline … (unchanged)
Utilities …        (unchanged)
System …           (Settings only; Tasks moves up)
```

The **Project** section only appears when a project is open — matching the existing "no project open" behavior of Tasks.

### Delete semantics & integrity

| Deleting… | Behavior |
|---|---|
| an **Input** referenced by a Sample | refuse with clear message listing referencing sample ids |
| an **Input** referenced by a Run's `inputs_used[]` | warn "will break reproducibility of runs [..]"; require confirmation, then unlink |
| an **Input** not referenced | delete record only; file on disk is untouched (we never registered it by copying) |
| a **Sample** referenced by a Run | warn + unlink-on-confirm |
| an **Asset** referenced by a Run's `assets_used[]` | warn + unlink-on-confirm |
| a **Run** | default leaves its `assets_produced[]` in place (other runs may use them); offer "also delete orphan assets produced by this run" checkbox |
| a **Run** that is Running/Pending | same as today: refuse, must cancel first |

Missing-file handling: a scan action marks `missing = true`; registering/validating/running with a missing input fails fast with a Fix-or-Remove prompt.

### Module adapter hooks (P3–P4)

Two optional trait methods on `Module`, both with sensible defaults:

```rust
trait Module: Send + Sync {
    // ... existing ...

    /// Describe what kind of inputs this module expects, so the UI can offer
    /// "pick from project inputs" dropdowns instead of only file pickers.
    fn input_kinds(&self) -> Vec<InputKindSpec> { Vec::new() }

    /// Describe the assets a successful run produces. The orchestrator uses
    /// this to auto-register them in the project's assets[] list.
    fn produced_assets(&self, result: &ModuleResult) -> Vec<DeclaredAsset> { Vec::new() }
}
```

`DeclaredAsset { kind, relative_path, display_name, schema }` — the Runner resolves the relative path against the run's output dir and adds an `AssetRecord` with `produced_by_run_id` set.

### Storage layout on disk

No change: `project/input/`, `project/runs/{id}/`. `project.json` grows by 3 arrays. If `project.json` ever exceeds, say, 2 MB, we'll split into `inventory.json` + `runs.json`; today even 5k inputs + runs sits under 500 KB.

## Phased rollout

Each phase is independently shippable. Stop after any phase and the app still works.

### P1 — Inputs registry (this spec's detailed target)

**Deliverable:** users can register FASTQ / FASTA / GTF files into the project, see them in a table with size and kind, edit display name, remove registrations, and scan for missing files. No module form changes yet.

Detailed plan: `docs/superpowers/plans/2026-04-24-project-inputs-p1.md`.

### P2 — Samples & sample sheet

- Add `samples[]`, commands to CRUD, and UI.
- TSV import (sample_sheet.tsv columns: `sample_id`, `group`, `r1`, `r2`, optional `condition`).
- Auto-pair R1/R2 by filename convention with user override.

### P3 — Assets & lineage

- `assets[]` + `Module::produced_assets()` hook.
- Runner auto-registers declared assets on successful completion.
- Assets view with filter by kind + lineage tooltip ("produced by run X at T").
- Delete-run dialog checks `assets_produced[]`.

### P4 — Module form integration

- Each file-drop zone gains a tabbed switcher: "From project" (dropdown of matching inputs/assets) vs "Drop new" (existing behavior).
- Running a module records `inputs_used[]` / `assets_used[]` on the RunRecord.
- Tasks page shows a "Uses" column; deleting a run that's still referenced warns.

### P5 — Global reference registry

- `~/.rustbrain/references/` as a user-level asset store.
- RefBundle = `{ fasta, gtf, star_index }` group, versioned, shareable across projects.

### P6 — Project overview page

- Disk usage sunburst (inputs vs assets vs runs).
- Workflow completion: which samples have been through QC, trimmed, aligned, quantified.
- Recent activity feed.

## Commands (Tauri) — P1 set

| Command | Signature | Notes |
|---|---|---|
| `list_inputs` | `() -> Vec<InputRecord>` | ordered by `registered_at` desc |
| `register_input` | `(path: String, kind: Option<InputKind>, display_name: Option<String>) -> InputRecord` | auto-detects kind from extension if not supplied |
| `register_inputs_batch` | `(paths: Vec<String>) -> Vec<InputRecord>` | one-shot multi-file registration |
| `update_input` | `(id: String, patch: InputPatch) -> InputRecord` | only `display_name`, `notes`, `kind` mutable in P1 |
| `delete_input` | `(id: String) -> ()` | P1: unconditional (P2+ enforces integrity) |
| `scan_inputs` | `() -> InputScanReport` | re-checks size + existence; sets `missing` flag; returns `(refreshed, now_missing, recovered)` counts |

All commands follow the existing pattern: lock `runner.project()`, mutate, save.

## Kind detection

```rust
fn detect_kind(path: &Path) -> InputKind {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
    // strip .gz / .bz2 before checking
    let stripped = name.trim_end_matches(".gz").trim_end_matches(".bz2");
    if stripped.ends_with(".fastq") || stripped.ends_with(".fq")  { InputKind::Fastq }
    else if stripped.ends_with(".fasta") || stripped.ends_with(".fa") || stripped.ends_with(".fna") { InputKind::Fasta }
    else if stripped.ends_with(".gtf") { InputKind::Gtf }
    else if stripped.ends_with(".gff") || stripped.ends_with(".gff3") { InputKind::Gff }
    else if stripped.ends_with(".tsv") || stripped.ends_with(".csv") { InputKind::CountsMatrix } // user can change to SampleSheet in UI
    else { InputKind::Other }
}
```

User can always override via the Update command.

## Frontend (P1)

- New view at `#inputs`, registered in `router.js` alongside `tasks`.
- New sidebar section **Project** containing `Inputs` and `Tasks` — implemented by moving Tasks up from System.
- Minimum UX:
  - Header: title + "Register files" button (invokes `select_files` multi) + "Register folder" (recursively pulls fastq/fa/gtf/gff from chosen dir).
  - Table: checkbox, kind badge, name (editable on click), absolute path (truncated, full on tooltip), size, registered time, missing indicator, row delete.
  - Toolbar filters: kind (all / fastq / fasta / …), search by name, "show missing only" toggle.
  - Bulk: delete selected, scan-now.
- i18n: `inputs.*` + `nav.inputs` (EN + ZH).

## Error handling

- Duplicate registration (same absolute path already registered): `register_input` is idempotent — returns the existing record instead of creating a duplicate.
- Permission denied / path not exists: rejected immediately with a user-readable error before any record is created.
- Batch register with some failures: returns a partial-success struct `{ registered: Vec<InputRecord>, errors: Vec<(path, message)> }`.

## Testing strategy

Unit (rb-core):
- `register_input` creates record, assigns id, sets `size_bytes` from fs metadata.
- `register_input` is idempotent on duplicate paths.
- `delete_input` removes record, does not touch the file on disk.
- `scan_inputs` flips `missing` on vanished files and flips back on recovered.

Integration (rb-app): skip — Tauri handlers are thin; the rb-core tests cover the logic.

Frontend: no automated tests today; smoke-check manually with the browser mock shim.

## Open questions (defer until someone asks)

- Do we want a thumbnail / first-line preview for each input? Nice-to-have, ignore for v1.
- Should `scan_inputs` also re-check size (for "file changed since registration")? Nice-to-have; noted flag `content_changed` in the record for future use.
- Can users bulk-import from a `.list` file (one path per line)? Useful for pipeline workflows; ignore for v1.

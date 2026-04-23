# Utilities Framework + Genome Viewer (L1) + FASTQ Viewer — Design

**Date:** 2026-04-23
**Status:** approved (brainstorm phase)
**Upstream libraries:** [igv.js](https://github.com/igvteam/igv.js) (Apache-2.0), [noodles](https://github.com/zaeleus/noodles) crate family (MIT/Apache-2.0).

## Problem

Users routinely need to **look at** files in formats the app already produces or consumes — reference FASTA, annotation GFF/GTF/BED, raw FASTQ. Today they have no in-app way to do this. They drop out to terminal (`less`, `head`), to external tools (IGV desktop), or just skip the sanity check entirely. For large files (typical sizes: 3 GB reference FASTA, 500 MB GTF, 30 GB FASTQ), `less` is slow to navigate and IGV desktop requires a separate install + manual index management.

Rust Brain needs first-party **viewers**. They are different from existing modules in one key way: a viewer has no `run() → result` semantics. It's a long-lived interactive session. Shoehorning it into the `Module` trait would distort the abstraction (what does "Progress" or "RunRecord" mean for a viewer?) and confuse the `runs/` directory.

## Goals

- Introduce a new first-class concept **Utility** alongside Module: interactive tools that don't produce analysis results.
- Ship two utilities:
  - **Genome Viewer** — lightweight IGV-like browser. L1 scope: reference FASTA + GFF/GTF/BED annotation tracks, coordinate navigation, gene search, auto-index, session persistence.
  - **FASTQ Viewer** — large-file FASTQ pager with sparse index, virtualized rendering, quality-score coloring, read-ID search.
- Both utilities **work without a project open** — they are global tools, not per-project analysis steps.
- Design the Utility framework so future utilities (BAM viewer / VCF summary / BED intersect / etc.) can slot in by dropping a new crate + a new frontend directory, with no changes to the framework itself.
- Keep the L1 → L2 path open: Genome Viewer's Rust API and frontend reader should accept BAM alignment tracks as a future extension without breaking changes.

## Non-goals (L1)

- BAM/CRAM alignment tracks (L2 — structural hooks only).
- VCF variant tracks (L3).
- Multiple named, switchable sessions (only one implicit "last state" persisted).
- Editing / writing files. Viewers are read-only.
- Network file URIs (HTTP/S3/GCS). Local paths only.
- Session sharing / export.
- Splice junction display, read pileup, coverage tracks — all require BAM (L2).
- Parsing / validation beyond what `noodles` provides. The viewer trusts the crates; malformed files surface upstream errors as user-facing toasts.

## Architecture

### 1. The Utility concept

A **Utility** is a first-party tool that exposes a view + backend commands but does not implement `Module` trait. Utilities are orthogonal to projects — they can be opened with no project loaded.

**Contract (loose, not a Rust trait):**
- Utility crate exposes a public function `fn register_commands(builder: &mut tauri::Builder)` so `rb-app/src/main.rs` wires them in alongside module registration.
- Frontend lists utilities in a new `UTILITIES` registry, parallel to `MODULES`.
- Each utility's Tauri commands are namespaced with a prefix (`genome_viewer_*`, `fastq_viewer_*`) to avoid collision.
- Utilities must tolerate the absence of a project. If they want to persist state, they use `app_handle.path().app_data_dir()` (app-global), not the project directory.

### 2. Crate layout

```
crates/
  rb-genome-viewer/
    Cargo.toml
    src/
      lib.rs              # Public surface: register_commands, error types
      session.rs          # GenomeSession, persistence to disk
      reference.rs        # FASTA loader + .fai management
      tracks.rs           # TrackRegistry, Track enum (Gff/Gtf/Bed/Bam-reserved)
      index.rs            # IndexManager (memory IntervalTree / tabix selection)
      search.rs           # gene_id/transcript_id/Name indexing
      commands.rs         # Tauri command handlers
      error.rs            # ViewerError with serializable codes
  rb-fastq-viewer/
    Cargo.toml
    src/
      lib.rs
      index.rs            # SparseOffsetIndex, cache path
      session.rs          # FastqSession
      commands.rs
      error.rs
```

Neither crate depends on `rb-core` — they have no `Module`/`Project` semantics. Both depend on `tauri` (for command attr macros), `serde`, `serde_json`, `tokio`, `thiserror`, and the relevant `noodles-*` subcrates.

### 3. Frontend layout

```
frontend/js/
  core/
    constants.js          # adds UTILITIES registry and rebuildKnownViews() update
    events.js             # dispatchAction extended with utility-specific acts
  utilities/
    genome-viewer/
      view.js             # render() + UI wiring
      igv-adapter.js      # bridges igv.js ReaderFactory ↔ Tauri commands
      session.js          # fetch/save session state
      search.js           # coordinate/gene name parser
    fastq-viewer/
      view.js
      virtual-list.js     # viewport-based renderer
      coloring.js         # Phred→HSL, ACGT palette
  vendor/
    igv/
      igv.esm.min.js      # bundled Apache-2 release, pinned version
      LICENSE
```

**Sidebar integration:** sidebar structure is hand-written in `frontend/index.html` as `.sidebar-nav > .nav-section` blocks, with plugin entries injected at runtime by `main.js::injectPluginSidebarEntries`. A new static `<div class="nav-section">` block titled "Utilities" is added in `index.html` between the "Analysis Pipeline" section and the "System" section, containing one `<a class="nav-item">` per entry in `UTILITIES`. `main.js` is extended with an analogous `injectUtilitySidebarEntries` helper in case future utilities arrive via the plugin mechanism (forward-compat; not used in L1). `KNOWN_VIEWS` in `constants.js` includes every utility view id.

### 4. rb-genome-viewer — internals

#### GenomeSession

```rust
pub struct GenomeSession {
    reference: Option<ReferenceHandle>,
    tracks: HashMap<TrackId, TrackHandle>,
    position: Option<GenomicRegion>,      // last viewed
    dirty: bool,                          // debounced flush to disk
}

pub struct GenomicRegion {
    chrom: String,
    start: u64,
    end: u64,
}
```

A single `GenomeSession` is held in `AppState` as `Arc<Mutex<GenomeSession>>`. On app launch, it reads `<app_data_dir>/genome_viewer_session.json` if present; missing/unreadable files in the session are silently dropped with a warning log.

#### IndexManager — strategy C

```
On add_track(path, kind):
  1. If a tabix index (.tbi or .csi) exists next to the file → use tabix reader.
  2. Else if file size < 200 MB → stream-parse, build in-memory IntervalTree<chrom, FeatureRef>.
     Build feature search index (gene_id/transcript_id/Name → GenomicRegion).
  3. Else → return AddTrackResult { track_id, source: "memory", large_file: true,
            suggest_bgzip: true }.
     Frontend shows a dialog; on confirm, calls bgzip_and_tabix(path).
```

`bgzip_and_tabix(path)` is a separate command so the "offer bgzip" decision stays in the UI. It writes `<path>.gz` + `<path>.gz.tbi` alongside the original, streams progress events (`genome_viewer_index_progress`), and on success returns the new `.gz` path. The original file is not deleted or modified.

For FASTA, `.fai` is built unconditionally on `load_reference` if missing (`noodles-fasta::fai::index(&mut reader)`). Build is fast (~1s per GB) and the file is a harmless sidecar.

#### Track enum (L2-ready)

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TrackKind {
    Gff,
    Gtf,
    Bed,
    // L2: uncomment when rb-alignment-viewer or equivalent lands.
    // Bam,
}
```

Until L2, the `add_track` command inspects the file extension **before** mapping to `TrackKind`. Unknown or `.bam`/`.cram` extensions return `ViewerError { code: "unsupported_kind", ... }` with a message distinguishing "not a supported format" from "BAM support arrives in L2". The command signature does not change when L2 ships — only the enum gains a variant and the extension matcher maps `.bam` to it.

#### Tauri commands

All commands live in `rb-genome-viewer::commands` and are registered via `register_commands(builder)`.

| Command | Args | Returns |
|---|---|---|
| `genome_viewer_load_reference` | `path: PathBuf` | `ReferenceMeta { chroms: Vec<ChromMeta>, fai_built: bool, path: PathBuf }` |
| `genome_viewer_add_track` | `path: PathBuf, kind_hint: Option<String>` | `TrackMeta { track_id, kind, source: "memory"\|"tabix", feature_count, suggest_bgzip: bool }` |
| `genome_viewer_remove_track` | `track_id: TrackId` | `()` |
| `genome_viewer_list_tracks` | — | `Vec<TrackMeta>` |
| `genome_viewer_fetch_reference_region` | `chrom, start, end` | `bytes: String` (plain ASCII, no wrapping) |
| `genome_viewer_fetch_track_features` | `track_id, chrom, start, end` | `Vec<Feature>` (JSON-friendly `{start, end, name, strand, attrs}`) |
| `genome_viewer_search_feature` | `query: String, limit: usize` | `Vec<SearchHit { name, chrom, start, end, track_id }>` |
| `genome_viewer_bgzip_and_tabix` | `path: PathBuf` | `{ new_path: PathBuf }`, emits `genome_viewer_index_progress` |
| `genome_viewer_get_session_state` | — | `SerializedSession` |
| `genome_viewer_save_session_state` | `state: SerializedSession` | `()` |

Error type: `ViewerError { code: String, message: String, path: Option<PathBuf> }` — front-end routes to toast via `utils/toast.js`.

### 5. rb-fastq-viewer — internals

#### Sparse offset index

For a FASTQ file, the index is a `Vec<u64>` where entry `i` is the byte offset of record number `i * ANCHOR_SPACING`. `ANCHOR_SPACING = 10_000`.

```
Index build:
  - Stream file from start. Every time record_count % ANCHOR_SPACING == 0, push current offset.
  - Also accumulate total record count.
  - Emit progress events every 1_000_000 records (or 500 ms, whichever first).
  - Persist as bincode-encoded struct to `<app_cache_dir>/fastq_idx/<sha1(abs_path)>.idx` where `<app_cache_dir>` is Tauri's `app_cache_dir()` (platform-appropriate: `~/.cache/rust_brain/` on Linux, `~/Library/Caches/rust_brain/` on macOS, `%LOCALAPPDATA%\rust_brain\cache\` on Windows).
```

**Cache invalidation:** alongside the offset vector, persist `{file_size, mtime}`. On open, if either differs, rebuild.

#### Random access

```
seek(record_n):
  anchor_idx = record_n / ANCHOR_SPACING
  seek to offsets[anchor_idx]
  scan forward (record_n % ANCHOR_SPACING) records
  return the reader positioned there
```

Max linear scan is `ANCHOR_SPACING` records — fast enough for interactive use (sub-100ms on SSD).

#### Tauri commands

| Command | Args | Returns |
|---|---|---|
| `fastq_viewer_open` | `path: PathBuf` | `{ total_records, index_cached: bool }`, emits `fastq_viewer_index_progress` during build |
| `fastq_viewer_read_records` | `start_record, count` | `Vec<FastqRecord { id, seq, plus, qual }>` |
| `fastq_viewer_seek_percent` | `pct: f32` | `start_record: usize` |
| `fastq_viewer_search_id` | `query: String, from_record: usize, limit: usize` | `Vec<{ record_n, id }>` |

### 6. Frontend — Genome Viewer view

**Mount flow:**
1. Call `get_session_state`. If empty → show empty state with "Load Reference" CTA. If populated → reconstruct igv.js browser config and auto-load.
2. igv.js instantiated into a host `<div>`. Custom `ReaderFactory` handlers intercept:
   - `ReferenceSequence.read(chrom, start, end)` → `invoke('genome_viewer_fetch_reference_region', ...)`
   - `FeatureSource.read(chrom, start, end)` per track → `invoke('genome_viewer_fetch_track_features', ...)`
3. UI panel (sidebar inside the view, not the app sidebar):
   - Reference section: current reference path + chrom count, Change button
   - Tracks section: list with per-track toggle / remove
   - Add Track button — multi-file picker; each file dispatched to `add_track`
   - Coordinate/search bar at top: parses `chr7:55,000,000-55,500,000`, `chr7`, or free-text gene query

**Session save:** debounced 1s after any state change (track add/remove, reference change, position change via igv.js `locuschange` event). Writes through `save_session_state`.

### 7. Frontend — FASTQ Viewer view

- Header: file path + total records, input for record number, percent slider, read-ID search box.
- Main area: virtualized list. Visible window size = ~50 records. On scroll, compute new `start_record` and call `fastq_viewer_read_records`.
- Each record: 4 rows (id, seq, `+`, qual). `seq` chars colored by base (A/C/G/T/N palette). `qual` chars colored by Phred — background uses HSL from red (low Q) → green (high Q), text stays dark.
- Search: user types ID substring, hits Enter → call `search_id` with current cursor as `from_record`, jump to the returned record; next hit on repeat Enter.

### 8. igv.js integration

**Vendoring:** the Apache-2 licensed `igv.esm.min.js` is checked into `frontend/vendor/igv/` along with its LICENSE file. Version pinned in the spec and in a comment inside the vendored file. Bumping requires a spec amendment.

**Adapter strategy:** igv.js accepts custom `Reader` classes per track. Our `igv-adapter.js` exports:
- `TauriReferenceReader` — implements the igv.js fasta reader interface by proxying to `genome_viewer_fetch_reference_region`.
- `TauriFeatureReader` — implements the feature reader interface by proxying to `genome_viewer_fetch_track_features`.

Reference and tracks get passed into `igv.createBrowser(config)` with `url`/`indexURL` fields pointing to sentinel values, and our custom reader classes registered for those sentinels. This keeps us off the filesystem protocol entirely (no `file://` concerns in Tauri's isolation mode).

## Data flow

```
User drops genome.fa → Add Reference button
  → frontend invoke('genome_viewer_load_reference', {path})
  → rb-genome-viewer: open with noodles-fasta, build .fai if missing,
    extract chrom list, store in GenomeSession, return ReferenceMeta
  → frontend builds igv.js config, creates browser in <div>
  → user navigates to chr7:55M-55.5M
  → igv.js triggers TauriReferenceReader.read(chr7, 55_000_000, 55_500_000)
  → invoke('genome_viewer_fetch_reference_region', ...)
  → rb-genome-viewer: noodles-fasta indexed query → bytes → return
  → igv.js renders
```

Progress for long operations (bgzip+tabix, FASTQ index build) is emitted via Tauri events rather than returned from the command, so UI can show a progress bar without the command hanging.

## Error handling

| Situation | Behavior |
|---|---|
| `.fai` build fails (e.g., malformed FASTA) | Command returns `ViewerError { code: "fai_build_failed", ... }`; toast; no reference loaded |
| `add_track` on unsupported extension | `ViewerError { code: "unsupported_kind" }` with supported list |
| `add_track` on BAM (L1) | `ViewerError { code: "unsupported_kind", message: "BAM tracks arrive in L2. ..." }` |
| File > 200 MB annotation | Command succeeds with `suggest_bgzip: true`. Frontend dialog offers bgzip+tabix. User can decline → memory index used. |
| Session restore: file path missing on disk | Silently drop that entry. Emit a single warning toast listing dropped paths. Session remains valid. |
| FASTQ index cache stale (mtime/size changed) | Rebuild transparently, show progress |
| Noodles parse error mid-stream | Command returns error; toast. Partial state discarded. |

All errors carry a structured `code`, not just a message, so the frontend can localize or specialize handling later.

## Session persistence (option B)

**Location:** `<app_data_dir>/genome_viewer_session.json`, where `<app_data_dir>` is Tauri's `app_data_dir()` — Linux `~/.local/share/rust_brain/`, macOS `~/Library/Application Support/rust_brain/`, Windows `%APPDATA%\rust_brain\`.

**Schema:**
```json
{
  "version": 1,
  "reference": { "path": "/abs/path/genome.fa" },
  "tracks": [
    { "path": "/abs/path/annot.gff3", "kind": "gff", "visible": true }
  ],
  "position": { "chrom": "chr7", "start": 55000000, "end": 55500000 }
}
```

Writes are debounced server-side (coalesce multiple `save_session_state` calls within 1s into one disk write) to avoid thrashing.

**Forward-compat:** `version` field enables future schema migrations without a user-visible break. Unknown fields are preserved on write.

## L2 extension points

What lands in L1 that makes L2 (BAM tracks) a small change rather than a redesign:

1. **`TrackKind` enum** — flip `Bam` variant from commented-out to real.
2. **`TauriFeatureReader`** — unchanged. A new `TauriAlignmentReader` added for BAM, using a new command `genome_viewer_fetch_track_alignments` (signature drafted in this spec; not implemented).
3. **igv.js** — already supports BAM natively via our vendored build. No frontend changes beyond recognizing `.bam` extension in the add-track handler.
4. **Index strategy** — BAM already requires `.bai` by convention; we reuse strategy C (auto-build on open if missing, via noodles-bam).
5. **`SerializedSession.tracks[].kind`** — already a free-string tag; adding `"bam"` is a no-op.

## Testing

### rb-genome-viewer unit tests
- `reference.rs`: open fixture `.fa`, build `.fai`, confirm chrom count and sequence bytes for a known region.
- `index.rs`: build in-memory IntervalTree over fixture `.gff3`, verify overlap query results match hand-counted features.
- `tracks.rs`: round-trip track add/remove across session serialize/deserialize.
- `search.rs`: gene/transcript name substring search returns expected hits on fixture annotation.

### rb-fastq-viewer unit tests
- `index.rs`: build sparse index on fixture (a few thousand records), verify `seek(n)` lands on the Nth record.
- Cache invalidation: touch fixture file, confirm rebuild triggered.

### rb-app integration test
- End-to-end: `load_reference` + `add_track` + `fetch_reference_region` + `fetch_track_features` on bundled fixtures, assert shapes.
- Session persistence: save → reload → verify state round-trips.

### Frontend manual checklist (ship with spec, executed pre-release)
- Load a reference and navigate to a gene by name.
- Add GFF, GTF, BED tracks; confirm features render at expected positions.
- Open same file twice → session restores.
- Force-close while editing → session still intact next launch.
- Open 1 GB GTF → memory index builds without blocking UI.
- Open 5 GB GTF → prompt appears, accepting produces `.gz` + `.tbi`.
- Drop a BAM file → clear "not yet supported" error.
- Open 10 GB FASTQ → index builds with progress, navigate to 50% / 90% / last record works.
- FASTQ ID search finds a known read.

### Fixtures
`testdata/` in each crate:
- `tiny.fa` (2 chroms × 10 kb), `tiny.gff3` (~50 features), `tiny.gtf`, `tiny.bed`, `tiny.fastq` (1000 records).
- No large fixtures in-repo. Large-file behaviors tested manually.

## Bundled binaries / dependencies

- **No new external binaries.** Everything runs inside the Rust process. No `samtools`, no `tabix` — `noodles-bgzf` + `noodles-tabix` handle both.
- **igv.js** — vendored (Apache-2), ~1.5MB minified, bundled into the frontend output.

## Open risks

- **igv.js lock-in.** If igv.js development stalls or its reader API breaks compatibility, migration to an alternative (e.g., building a Canvas-based renderer from scratch) would be multi-week work. Mitigation: pin the version, isolate usage to `igv-adapter.js` so the blast radius of a swap is one file.
- **Memory-index blow-up on pathological annotations.** A 500 MB GFF with many small features could push memory usage into GB range. Mitigation: the 200 MB threshold triggers the tabix prompt; beyond that users choose.
- **FASTQ index cache directory growth.** Users who view many different large FASTQs accumulate cached indices. Each index is ~`total_records / 10_000 × 8 bytes` — 3 MB for a billion-record file. Acceptable; add a "Clear cache" button in settings if this becomes a real concern (not in L1).

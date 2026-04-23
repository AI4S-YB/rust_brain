# Utilities Framework + Genome Viewer + FASTQ Viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a new first-class "Utility" concept alongside Module, plus two utility implementations: a lightweight IGV-like genome browser (L1: reference FASTA + GFF/GTF/BED tracks) and a large-file FASTQ pager with sparse indexing.

**Architecture:** Two new Rust crates (`rb-genome-viewer`, `rb-fastq-viewer`) each owning their Tauri commands and using the `noodles` crate family for all bioinformatics I/O. Frontend gets a new `UTILITIES` registry parallel to `MODULES`, a new sidebar section, and two new views under `frontend/js/utilities/`. The genome viewer embeds vendored `igv.js` as its rendering engine, bridged to the Rust backend via custom `Reader` classes (no filesystem protocol). Framework stays orthogonal to projects — utilities open with or without a project loaded.

**Tech Stack:** Rust + Tauri v2 (backend), `noodles-fasta`/`noodles-gff`/`noodles-bed`/`noodles-fastq`/`noodles-tabix`/`noodles-bgzf` for file parsing, `bincode` for FASTQ index caching, vanilla JS + `igv.js` (Apache-2, vendored) for frontend.

**Spec:** [`docs/superpowers/specs/2026-04-23-utilities-genome-viewer-design.md`](../specs/2026-04-23-utilities-genome-viewer-design.md)

---

## File Structure

**New Rust crates**
```
crates/rb-genome-viewer/
  Cargo.toml
  src/
    lib.rs              # public: register_commands(), re-export error
    error.rs            # ViewerError + Serialize impl for Tauri
    reference.rs        # FASTA loader, .fai management
    tracks.rs           # TrackId, TrackKind, TrackMeta, TrackSource, new_track_id
    index.rs            # MemoryIndex (per-chrom feature list), file_is_large
    search.rs           # SearchIndex (feature name → positions)
    session.rs          # GenomeSession, TrackRuntime, SerializedSession, persistence
    commands.rs         # all genome_viewer_* Tauri commands
    bgzip.rs            # bgzip + tabix convert workflow with progress
  testdata/
    tiny.fa             # 2 chroms × 1 kb
    tiny.gff3           # ~20 features
    tiny.gtf            # same features as gtf
    tiny.bed            # ~10 BED entries

crates/rb-fastq-viewer/
  Cargo.toml
  src/
    lib.rs              # public: register_commands(), re-export error
    error.rs            # ViewerError + Serialize
    index.rs            # SparseOffsetIndex, build, persist, cache key
    session.rs          # FastqSession: open/read/seek/search
    commands.rs         # all fastq_viewer_* commands
  testdata/
    tiny.fastq          # 100 records (for unit tests)
    larger.fastq        # 5000 records (for anchor/seek tests)
```

**Modified Rust files**
- `Cargo.toml` — add 2 new crates to workspace members, add `bincode`, `sha1` to workspace deps
- `crates/rb-app/Cargo.toml` — add `rb-genome-viewer`, `rb-fastq-viewer` dependencies
- `crates/rb-app/src/main.rs` — call `rb_genome_viewer::register_commands(&mut builder)` and `rb_fastq_viewer::register_commands(&mut builder)` before `.run()`

**New frontend files**
```
frontend/js/utilities/
  genome-viewer/
    view.js             # renderGenomeViewerView()
    igv-adapter.js      # TauriReferenceReader, TauriFeatureReader
    session.js          # save/restore via commands
    search.js           # parse "chr:start-end", dispatch gene search
    controls.js         # reference picker, track list, search bar
  fastq-viewer/
    view.js             # renderFastqViewerView()
    virtual-list.js     # viewport-based record rendering
    coloring.js         # ACGT palette + Phred→HSL
frontend/vendor/igv/
  igv.esm.min.js        # pinned igv.js release
  LICENSE               # Apache-2
  VERSION               # pinned version string (one line)
```

**Modified frontend files**
- `frontend/js/core/constants.js` — add `UTILITIES` export, `setBootstrapUtilities()`, update `rebuildKnownViews`
- `frontend/js/core/router.js` — route `genome-viewer` and `fastq-viewer` views
- `frontend/index.html` — add `<div class="nav-section">` for Utilities after Analysis Pipeline
- `frontend/js/main.js` — optionally, `injectUtilitySidebarEntries` forward-compat stub

---

## Phase 1 — Utility Framework Foundation

Goal: establish the Utility registry, sidebar location, and router wiring so both utilities can mount into a shared skeleton.

### Task 1: Add `UTILITIES` registry to `constants.js`

**Files:**
- Modify: `frontend/js/core/constants.js`

- [ ] **Step 1: Add UTILITIES export and helper**

Open `frontend/js/core/constants.js`. Immediately after the `MODULES` array (around line 16), add:

```js
export const UTILITIES = [
  { id: 'genome-viewer', view_id: 'genome-viewer', name: 'Genome Viewer', icon: 'map',       color: 'purple', category: 'viewer', source: 'builtin' },
  { id: 'fastq-viewer',  view_id: 'fastq-viewer',  name: 'FASTQ Viewer',  icon: 'file-text', color: 'teal',   category: 'viewer', source: 'builtin' },
];

export function setBootstrapUtilities(descriptors) {
  UTILITIES.length = 0;
  for (const d of descriptors) UTILITIES.push({ ...d, source: d.source || 'builtin' });
  rebuildKnownViews();
}
```

- [ ] **Step 2: Extend `rebuildKnownViews`**

Find the existing `rebuildKnownViews` function and add utility ids alongside:

```js
function rebuildKnownViews() {
  KNOWN_VIEWS.clear();
  ['dashboard', 'settings', 'gff-convert', 'star-index', 'star-align', 'chat', 'plots']
    .forEach(v => KNOWN_VIEWS.add(v));
  MODULES.forEach(m => KNOWN_VIEWS.add(m.view_id || m.id));
  UTILITIES.forEach(u => KNOWN_VIEWS.add(u.view_id || u.id));
}
```

- [ ] **Step 3: Verify module parses**

Run: `cd frontend && python3 -m http.server 8090 & sleep 1 && curl -s http://localhost:8090/js/core/constants.js | head -40 && kill %1`
Expected: file contents returned cleanly (no syntax errors). Stop the server.

Alternative verification (no server): `node --input-type=module -e "import('file:///$(pwd)/frontend/js/core/constants.js').then(m => console.log(Object.keys(m)))"`
Expected: output includes `UTILITIES` and `setBootstrapUtilities`.

- [ ] **Step 4: Commit**

```bash
git add frontend/js/core/constants.js
git commit -m "feat(frontend): add UTILITIES registry for first-party tools"
```

---

### Task 2: Add Utilities section to sidebar HTML

**Files:**
- Modify: `frontend/index.html` (around the existing `.sidebar-nav` block at line 549)

- [ ] **Step 1: Locate insertion point**

Run: `grep -n "Analysis Pipeline\|nav.system\|data-view=\"settings\"" frontend/index.html`

The Utilities section must be inserted **after** the "Analysis Pipeline" `.nav-section` closes and **before** the section containing `data-view="settings"` (the System section).

- [ ] **Step 2: Add Utilities section**

Locate the closing `</div>` of the section containing `data-i18n="nav.pipeline"`. Immediately after it, insert:

```html
<div class="nav-section">
  <div class="nav-section-title" data-i18n="nav.utilities">Utilities</div>
  <a class="nav-item" data-view="genome-viewer" data-color="purple" href="#genome-viewer">
    <i data-lucide="map"></i>
    <span data-i18n="utility.genome_viewer.name">Genome Viewer</span>
  </a>
  <a class="nav-item" data-view="fastq-viewer" data-color="teal" href="#fastq-viewer">
    <i data-lucide="file-text"></i>
    <span data-i18n="utility.fastq_viewer.name">FASTQ Viewer</span>
  </a>
</div>
```

- [ ] **Step 3: Verify in browser**

Run: `cd frontend && python3 -m http.server 8090`
Open `http://localhost:8090` in a browser. Confirm a new "Utilities" section appears in the sidebar between Analysis Pipeline and System with two entries. Clicking either produces a blank content area (router not wired yet — expected).
Stop server.

- [ ] **Step 4: Commit**

```bash
git add frontend/index.html
git commit -m "feat(frontend): add Utilities sidebar section"
```

---

### Task 3: Route utility views in `router.js`

**Files:**
- Modify: `frontend/js/core/router.js`

- [ ] **Step 1: Add utility view routes**

In the `navigate()` function in `router.js`, locate the `else` branch that calls `renderModuleView`. Immediately **before** that `else`, add two new `else if` branches:

```js
} else if (view === 'genome-viewer') {
  content.innerHTML = `<div class="module-view"><p>${t('common.loading')}</p></div>`;
  const m = await import('../utilities/genome-viewer/view.js');
  if (state.currentView === view) m.renderGenomeViewerView(content);
} else if (view === 'fastq-viewer') {
  content.innerHTML = `<div class="module-view"><p>${t('common.loading')}</p></div>`;
  const m = await import('../utilities/fastq-viewer/view.js');
  if (state.currentView === view) m.renderFastqViewerView(content);
```

(These imports will 404 until Tasks 11 and 26 create the view files — that's fine for now.)

- [ ] **Step 2: Create placeholder view files so router doesn't 404**

Create `frontend/js/utilities/genome-viewer/view.js`:

```js
export function renderGenomeViewerView(content) {
  content.innerHTML = `
    <div class="module-view">
      <h1>Genome Viewer</h1>
      <p>Placeholder — implementation lands in later tasks.</p>
    </div>
  `;
}
```

Create `frontend/js/utilities/fastq-viewer/view.js`:

```js
export function renderFastqViewerView(content) {
  content.innerHTML = `
    <div class="module-view">
      <h1>FASTQ Viewer</h1>
      <p>Placeholder — implementation lands in later tasks.</p>
    </div>
  `;
}
```

- [ ] **Step 3: Verify routing**

Run: `cd frontend && python3 -m http.server 8090`
Browse `http://localhost:8090#genome-viewer` → confirm "Genome Viewer" header renders.
Browse `http://localhost:8090#fastq-viewer` → confirm "FASTQ Viewer" header renders.
Confirm sidebar entries highlight active state.
Stop server.

- [ ] **Step 4: Commit**

```bash
git add frontend/js/core/router.js frontend/js/utilities/
git commit -m "feat(frontend): route utility views (genome-viewer, fastq-viewer)"
```

---

## Phase 2 — `rb-fastq-viewer` Crate

Goal: ship the FASTQ pager first as a vertical slice that exercises the whole stack (Rust crate + Tauri commands + frontend view) with simpler semantics than the genome viewer.

### Task 4: Create crate skeleton

**Files:**
- Create: `crates/rb-fastq-viewer/Cargo.toml`
- Create: `crates/rb-fastq-viewer/src/lib.rs`
- Create: `crates/rb-fastq-viewer/src/error.rs`
- Modify: `Cargo.toml` (workspace)
- Modify: `crates/rb-app/Cargo.toml`

- [ ] **Step 1: Add workspace dependency**

Edit root `Cargo.toml`. Under `[workspace.dependencies]` add:

```toml
bincode = "1.3"
sha1 = "0.10"
noodles-fastq = "0.13"
```

Under `[workspace]` → `members`, add `"crates/rb-fastq-viewer"` (keep alphabetical):

```toml
members = [
    "crates/rb-ai",
    "crates/rb-app",
    "crates/rb-core",
    "crates/rb-deseq2",
    "crates/rb-fastq-viewer",
    "crates/rb-gff-convert",
    "crates/rb-plugin",
    "crates/rb-qc",
    "crates/rb-rustqc",
    "crates/rb-star-align",
    "crates/rb-star-index",
    "crates/rb-trimming",
]
```

- [ ] **Step 2: Create `crates/rb-fastq-viewer/Cargo.toml`**

```toml
[package]
name = "rb-fastq-viewer"
version = "0.1.0"
edition = "2021"

[dependencies]
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
thiserror.workspace = true
bincode.workspace = true
sha1.workspace = true
noodles-fastq.workspace = true
dirs = "5"
tauri = "2"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Create `crates/rb-fastq-viewer/src/error.rs`**

```rust
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ViewerError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("file not found: {0}")]
    NotFound(PathBuf),
    #[error("out of range: requested record {requested}, total {total}")]
    OutOfRange { requested: usize, total: usize },
    #[error("index corrupt: {0}")]
    IndexCorrupt(String),
    #[error("bincode error: {0}")]
    Bincode(#[from] bincode::Error),
}

#[derive(Debug, Serialize)]
pub struct SerializedError {
    pub code: String,
    pub message: String,
    pub path: Option<PathBuf>,
}

impl ViewerError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Parse(_) => "parse",
            Self::NotFound(_) => "not_found",
            Self::OutOfRange { .. } => "out_of_range",
            Self::IndexCorrupt(_) => "index_corrupt",
            Self::Bincode(_) => "index_corrupt",
        }
    }
}

impl Serialize for ViewerError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let path = match self {
            Self::NotFound(p) => Some(p.clone()),
            _ => None,
        };
        SerializedError {
            code: self.code().to_string(),
            message: self.to_string(),
            path,
        }
        .serialize(s)
    }
}

pub type Result<T> = std::result::Result<T, ViewerError>;
```

- [ ] **Step 4: Create `crates/rb-fastq-viewer/src/lib.rs` (stub)**

```rust
pub mod error;
pub mod index;
pub mod session;
pub mod commands;

pub use error::{Result, ViewerError};

use tauri::Runtime;

pub fn register_commands<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        commands::fastq_viewer_open,
        commands::fastq_viewer_read_records,
        commands::fastq_viewer_seek_percent,
        commands::fastq_viewer_search_id,
    ])
}
```

- [ ] **Step 5: Create empty module stubs so `lib.rs` compiles**

Create `crates/rb-fastq-viewer/src/index.rs` with `// stub`, `crates/rb-fastq-viewer/src/session.rs` with `// stub`, `crates/rb-fastq-viewer/src/commands.rs` with:

```rust
// stub — filled in Task 8
#[tauri::command]
pub async fn fastq_viewer_open() -> Result<(), String> { Err("not impl".into()) }
#[tauri::command]
pub async fn fastq_viewer_read_records() -> Result<(), String> { Err("not impl".into()) }
#[tauri::command]
pub async fn fastq_viewer_seek_percent() -> Result<(), String> { Err("not impl".into()) }
#[tauri::command]
pub async fn fastq_viewer_search_id() -> Result<(), String> { Err("not impl".into()) }
```

- [ ] **Step 6: Wire into `rb-app`**

Edit `crates/rb-app/Cargo.toml`. Under `[dependencies]` add:

```toml
rb-fastq-viewer = { path = "../rb-fastq-viewer" }
```

Edit `crates/rb-app/src/main.rs`. Find the line `tauri::Builder::default()` and keep the existing chain, but replace `.manage(app_state)` with the following restructuring:

```rust
let mut builder = tauri::Builder::default().manage(app_state);
builder = rb_fastq_viewer::register_commands(builder);
builder
    .setup(|app| { /* existing setup body */ })
    // ... rest of existing chain
```

Actually `invoke_handler` can only be called once in Tauri. Instead, add `rb_fastq_viewer` command names to the existing `tauri::generate_handler![...]` list. Update main.rs to:

- Open `crates/rb-app/src/main.rs`, locate the `tauri::generate_handler![...]` macro invocation.
- Append these lines inside the list (alphabetical doesn't matter here):

```rust
rb_fastq_viewer::commands::fastq_viewer_open,
rb_fastq_viewer::commands::fastq_viewer_read_records,
rb_fastq_viewer::commands::fastq_viewer_seek_percent,
rb_fastq_viewer::commands::fastq_viewer_search_id,
```

- Since the commands are registered directly from the crate's module path, the `register_commands` wrapper in `lib.rs` is unused by `rb-app`. **Simplify** `lib.rs`:

```rust
pub mod error;
pub mod index;
pub mod session;
pub mod commands;

pub use error::{Result, ViewerError};
```

- [ ] **Step 7: Build**

Run: `cargo check -p rb-fastq-viewer`
Expected: success (only stubs, no real code yet).

Run: `cargo check -p rb-app`
Expected: success — the new command symbols resolve to the stubs.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/rb-fastq-viewer crates/rb-app/Cargo.toml crates/rb-app/src/main.rs
git commit -m "feat(fastq-viewer): scaffold rb-fastq-viewer crate and wire into rb-app"
```

---

### Task 5: Sparse offset index — build + seek

**Files:**
- Modify: `crates/rb-fastq-viewer/src/index.rs`
- Create: `crates/rb-fastq-viewer/testdata/tiny.fastq`

- [ ] **Step 1: Create test fixture**

Create `crates/rb-fastq-viewer/testdata/tiny.fastq` with 100 records, each 4 lines. Use this generator:

```bash
python3 - <<'EOF' > crates/rb-fastq-viewer/testdata/tiny.fastq
for i in range(100):
    print(f"@read_{i:04d} metadata")
    print("ACGTACGTACGTACGT")
    print("+")
    print("IIIIIIIIIIIIIIII")
EOF
```

- [ ] **Step 2: Write failing test for index build**

Replace `crates/rb-fastq-viewer/src/index.rs` with:

```rust
use crate::error::{Result, ViewerError};
use serde::{Deserialize, Serialize};
use std::fs::{File, Metadata};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

pub const ANCHOR_SPACING: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SparseOffsetIndex {
    pub anchors: Vec<u64>,     // byte offset of record N where N = i * ANCHOR_SPACING
    pub total_records: usize,
    pub file_size: u64,
    pub mtime_unix: i64,
}

impl SparseOffsetIndex {
    pub fn build(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(ViewerError::NotFound(path.to_path_buf()));
        }
        let meta = std::fs::metadata(path)?;
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        let mut anchors = Vec::new();
        let mut offset: u64 = 0;
        let mut record_count: usize = 0;
        let mut line_buf = String::new();

        loop {
            if record_count % ANCHOR_SPACING == 0 {
                anchors.push(offset);
            }
            // A FASTQ record is exactly 4 lines.
            let mut bytes_in_record: u64 = 0;
            for line_idx in 0..4 {
                line_buf.clear();
                let n = reader.read_line(&mut line_buf)?;
                if n == 0 {
                    // EOF mid-record: if line_idx==0, clean end; else corrupt.
                    if line_idx == 0 {
                        return Ok(Self {
                            anchors,
                            total_records: record_count,
                            file_size: meta.len(),
                            mtime_unix: unix_mtime(&meta),
                        });
                    }
                    return Err(ViewerError::Parse(format!(
                        "unexpected EOF inside record {}, line {}",
                        record_count, line_idx
                    )));
                }
                bytes_in_record += n as u64;
            }
            offset += bytes_in_record;
            record_count += 1;
        }
    }

    /// Byte offset to seek to when jumping to `record_n`. Returns the offset of the nearest
    /// preceding anchor; caller is responsible for scanning forward `(record_n - anchor_idx * ANCHOR_SPACING)`
    /// records after seeking.
    pub fn anchor_for(&self, record_n: usize) -> (usize, u64) {
        let anchor_idx = record_n / ANCHOR_SPACING;
        let offset = self.anchors.get(anchor_idx).copied().unwrap_or(0);
        (anchor_idx, offset)
    }
}

fn unix_mtime(meta: &Metadata) -> i64 {
    use std::time::UNIX_EPOCH;
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.fastq")
    }

    #[test]
    fn counts_records_correctly() {
        let idx = SparseOffsetIndex::build(&fixture()).unwrap();
        assert_eq!(idx.total_records, 100);
    }

    #[test]
    fn anchor_zero_is_file_start() {
        let idx = SparseOffsetIndex::build(&fixture()).unwrap();
        assert_eq!(idx.anchors[0], 0);
    }

    #[test]
    fn anchor_for_small_file_returns_zero() {
        let idx = SparseOffsetIndex::build(&fixture()).unwrap();
        let (anchor_idx, offset) = idx.anchor_for(50);
        assert_eq!(anchor_idx, 0);
        assert_eq!(offset, 0);
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p rb-fastq-viewer --lib`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-fastq-viewer/src/index.rs crates/rb-fastq-viewer/testdata/tiny.fastq
git commit -m "feat(fastq-viewer): sparse offset index build + seek"
```

---

### Task 6: Anchored seek test with a larger fixture

**Files:**
- Create: `crates/rb-fastq-viewer/testdata/larger.fastq`
- Modify: `crates/rb-fastq-viewer/src/index.rs`

Goal: verify `anchor_for` returns nontrivial offsets when records cross anchor boundaries. `ANCHOR_SPACING` is 10,000 for production; for the test we'll use a conditionally smaller constant.

- [ ] **Step 1: Refactor ANCHOR_SPACING to be per-index**

Edit `index.rs`. Add a second associated method and make build spacing configurable:

```rust
impl SparseOffsetIndex {
    pub fn build(path: &Path) -> Result<Self> {
        Self::build_with_spacing(path, ANCHOR_SPACING)
    }

    pub fn build_with_spacing(path: &Path, spacing: usize) -> Result<Self> {
        // ... existing body, but replace ANCHOR_SPACING with `spacing`
        // also store `spacing` on the struct
    }
}
```

Add `pub spacing: usize` to the struct; persist it so `anchor_for` uses the same spacing:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SparseOffsetIndex {
    pub anchors: Vec<u64>,
    pub total_records: usize,
    pub file_size: u64,
    pub mtime_unix: i64,
    pub spacing: usize,
}

// Update anchor_for to use self.spacing:
pub fn anchor_for(&self, record_n: usize) -> (usize, u64) {
    let anchor_idx = record_n / self.spacing;
    let offset = self.anchors.get(anchor_idx).copied().unwrap_or(0);
    (anchor_idx, offset)
}
```

Update `build_with_spacing` to set `spacing` on the returned struct in both EOF paths.

- [ ] **Step 2: Generate larger fixture**

```bash
python3 - <<'EOF' > crates/rb-fastq-viewer/testdata/larger.fastq
for i in range(5000):
    print(f"@read_{i:05d}")
    print("ACGTACGT" * 8)   # 64 bp
    print("+")
    print("I" * 64)
EOF
```

- [ ] **Step 3: Add test for nontrivial anchor**

Append to the `tests` module in `index.rs`:

```rust
#[test]
fn larger_file_has_multiple_anchors() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/larger.fastq");
    let idx = SparseOffsetIndex::build_with_spacing(&path, 1000).unwrap();
    assert_eq!(idx.total_records, 5000);
    assert_eq!(idx.anchors.len(), 5);
    // Record 2500 sits in anchor bucket 2.
    let (anchor_idx, offset) = idx.anchor_for(2500);
    assert_eq!(anchor_idx, 2);
    assert!(offset > 0);
}

#[test]
fn anchor_offsets_are_monotonic() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/larger.fastq");
    let idx = SparseOffsetIndex::build_with_spacing(&path, 1000).unwrap();
    for w in idx.anchors.windows(2) {
        assert!(w[0] < w[1], "anchors must be strictly increasing");
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p rb-fastq-viewer --lib`
Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-fastq-viewer/src/index.rs crates/rb-fastq-viewer/testdata/larger.fastq
git commit -m "test(fastq-viewer): multi-anchor index with larger fixture"
```

---

### Task 7: Index cache (persist + invalidate)

**Files:**
- Modify: `crates/rb-fastq-viewer/src/index.rs`

- [ ] **Step 1: Add cache load/save with invalidation**

Append to `index.rs`:

```rust
use sha1::{Digest, Sha1};
use std::path::PathBuf;

impl SparseOffsetIndex {
    pub fn cache_key(file_path: &Path) -> String {
        let abs = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
        let mut hasher = Sha1::new();
        hasher.update(abs.to_string_lossy().as_bytes());
        let digest = hasher.finalize();
        digest.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn cache_path(cache_dir: &Path, file_path: &Path) -> PathBuf {
        cache_dir.join(format!("{}.idx", Self::cache_key(file_path)))
    }

    pub fn load_cached(cache_dir: &Path, file_path: &Path) -> Result<Option<Self>> {
        let cp = Self::cache_path(cache_dir, file_path);
        if !cp.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&cp)?;
        let idx: SparseOffsetIndex = bincode::deserialize(&bytes)
            .map_err(|e| ViewerError::IndexCorrupt(e.to_string()))?;

        let meta = std::fs::metadata(file_path)?;
        let current_mtime = unix_mtime(&meta);
        if idx.file_size != meta.len() || idx.mtime_unix != current_mtime {
            return Ok(None); // stale
        }
        Ok(Some(idx))
    }

    pub fn save(&self, cache_dir: &Path, file_path: &Path) -> Result<()> {
        std::fs::create_dir_all(cache_dir)?;
        let cp = Self::cache_path(cache_dir, file_path);
        let bytes = bincode::serialize(self)?;
        std::fs::write(cp, bytes)?;
        Ok(())
    }

    pub fn build_or_load(cache_dir: &Path, file_path: &Path) -> Result<(Self, bool)> {
        if let Some(idx) = Self::load_cached(cache_dir, file_path)? {
            return Ok((idx, true));
        }
        let idx = Self::build(file_path)?;
        idx.save(cache_dir, file_path)?;
        Ok((idx, false))
    }
}
```

- [ ] **Step 2: Test cache round-trip + invalidation**

Append to `tests`:

```rust
#[test]
fn cache_round_trip() {
    let cache = tempfile::tempdir().unwrap();
    let fp = fixture();
    let (idx1, hit1) = SparseOffsetIndex::build_or_load(cache.path(), &fp).unwrap();
    assert!(!hit1, "first call is a miss");
    let (idx2, hit2) = SparseOffsetIndex::build_or_load(cache.path(), &fp).unwrap();
    assert!(hit2, "second call hits cache");
    assert_eq!(idx1.total_records, idx2.total_records);
    assert_eq!(idx1.anchors, idx2.anchors);
}

#[test]
fn cache_invalidates_on_mtime_change() {
    use std::fs::OpenOptions;
    use std::io::Write;
    let cache = tempfile::tempdir().unwrap();
    let tmp_fq = tempfile::NamedTempFile::new().unwrap();
    std::fs::copy(&fixture(), tmp_fq.path()).unwrap();
    let (_, _hit1) = SparseOffsetIndex::build_or_load(cache.path(), tmp_fq.path()).unwrap();

    // Append a record to change size+mtime.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let mut f = OpenOptions::new().append(true).open(tmp_fq.path()).unwrap();
    writeln!(f, "@new_read\nACGT\n+\nIIII").unwrap();
    drop(f);

    let (_, hit2) = SparseOffsetIndex::build_or_load(cache.path(), tmp_fq.path()).unwrap();
    assert!(!hit2, "cache must invalidate after file change");
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rb-fastq-viewer --lib`
Expected: 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-fastq-viewer/src/index.rs
git commit -m "feat(fastq-viewer): cache index to disk with mtime invalidation"
```

---

### Task 8: `FastqSession` — open, read, seek, search

**Files:**
- Modify: `crates/rb-fastq-viewer/src/session.rs`

- [ ] **Step 1: Write session type and tests**

Replace `crates/rb-fastq-viewer/src/session.rs` with:

```rust
use crate::error::{Result, ViewerError};
use crate::index::SparseOffsetIndex;
use serde::Serialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct FastqRecord {
    pub id: String,
    pub seq: String,
    pub plus: String,
    pub qual: String,
}

#[derive(Debug, Serialize)]
pub struct OpenResult {
    pub total_records: usize,
    pub index_cached: bool,
    pub path: PathBuf,
}

pub struct FastqSession {
    pub path: PathBuf,
    pub index: SparseOffsetIndex,
}

impl FastqSession {
    pub fn open(path: &Path, cache_dir: &Path) -> Result<(Self, bool)> {
        let (index, cached) = SparseOffsetIndex::build_or_load(cache_dir, path)?;
        Ok((Self { path: path.to_path_buf(), index }, cached))
    }

    pub fn read_records(&self, start: usize, count: usize) -> Result<Vec<FastqRecord>> {
        if start >= self.index.total_records {
            return Ok(Vec::new());
        }
        let (_, offset) = self.index.anchor_for(start);
        let f = File::open(&self.path)?;
        let mut reader = BufReader::new(f);
        reader.seek(SeekFrom::Start(offset))?;

        let mut cursor = (start / self.index.spacing) * self.index.spacing;
        let mut skip_remaining = start - cursor;
        let mut out = Vec::with_capacity(count);
        let mut line = String::new();

        while out.len() < count && cursor < self.index.total_records {
            let mut rec = [String::new(), String::new(), String::new(), String::new()];
            for i in 0..4 {
                line.clear();
                let n = reader.read_line(&mut line)?;
                if n == 0 {
                    return Err(ViewerError::Parse(format!("unexpected EOF at record {}", cursor)));
                }
                rec[i] = line.trim_end_matches(&['\n', '\r'][..]).to_string();
            }
            if skip_remaining > 0 {
                skip_remaining -= 1;
            } else {
                out.push(FastqRecord {
                    id: rec[0].clone(),
                    seq: rec[1].clone(),
                    plus: rec[2].clone(),
                    qual: rec[3].clone(),
                });
            }
            cursor += 1;
        }
        Ok(out)
    }

    pub fn seek_percent(&self, pct: f32) -> usize {
        let pct = pct.clamp(0.0, 1.0);
        ((self.index.total_records as f32) * pct) as usize
    }

    pub fn search_id(&self, query: &str, from: usize, limit: usize) -> Result<Vec<(usize, String)>> {
        let mut hits = Vec::new();
        let mut cursor = from;
        let chunk = 1000;
        while cursor < self.index.total_records && hits.len() < limit {
            let batch = self.read_records(cursor, chunk)?;
            for (i, rec) in batch.iter().enumerate() {
                if rec.id.contains(query) {
                    hits.push((cursor + i, rec.id.clone()));
                    if hits.len() == limit {
                        break;
                    }
                }
            }
            if batch.is_empty() {
                break;
            }
            cursor += batch.len();
        }
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.fastq")
    }

    #[test]
    fn reads_first_records() {
        let cache = tempfile::tempdir().unwrap();
        let (session, _) = FastqSession::open(&fixture(), cache.path()).unwrap();
        let recs = session.read_records(0, 3).unwrap();
        assert_eq!(recs.len(), 3);
        assert!(recs[0].id.starts_with("@read_0000"));
        assert_eq!(recs[1].id, "@read_0001 metadata");
        assert_eq!(recs[0].seq, "ACGTACGTACGTACGT");
    }

    #[test]
    fn reads_from_middle() {
        let cache = tempfile::tempdir().unwrap();
        let (session, _) = FastqSession::open(&fixture(), cache.path()).unwrap();
        let recs = session.read_records(42, 2).unwrap();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].id, "@read_0042 metadata");
        assert_eq!(recs[1].id, "@read_0043 metadata");
    }

    #[test]
    fn seek_percent_returns_record_number() {
        let cache = tempfile::tempdir().unwrap();
        let (session, _) = FastqSession::open(&fixture(), cache.path()).unwrap();
        assert_eq!(session.seek_percent(0.0), 0);
        assert_eq!(session.seek_percent(0.5), 50);
        assert_eq!(session.seek_percent(1.0), 100);
    }

    #[test]
    fn search_finds_matching_id() {
        let cache = tempfile::tempdir().unwrap();
        let (session, _) = FastqSession::open(&fixture(), cache.path()).unwrap();
        let hits = session.search_id("0042", 0, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 42);
    }

    #[test]
    fn read_past_end_returns_empty() {
        let cache = tempfile::tempdir().unwrap();
        let (session, _) = FastqSession::open(&fixture(), cache.path()).unwrap();
        let recs = session.read_records(5000, 10).unwrap();
        assert!(recs.is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p rb-fastq-viewer --lib`
Expected: 12 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-fastq-viewer/src/session.rs
git commit -m "feat(fastq-viewer): FastqSession with read/seek/search"
```

---

### Task 9: Tauri commands

**Files:**
- Modify: `crates/rb-fastq-viewer/src/commands.rs`

- [ ] **Step 1: Write command handlers**

Replace `crates/rb-fastq-viewer/src/commands.rs` with:

```rust
use crate::error::{Result, ViewerError};
use crate::session::{FastqRecord, FastqSession, OpenResult};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Default)]
pub struct FastqState {
    pub session: Mutex<Option<Arc<FastqSession>>>,
}

fn cache_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf> {
    let base = app
        .path()
        .app_cache_dir()
        .map_err(|e| ViewerError::Parse(format!("app_cache_dir: {e}")))?;
    Ok(base.join("fastq_idx"))
}

fn ensure_state<R: Runtime>(app: &AppHandle<R>) -> Arc<FastqState> {
    if app.try_state::<Arc<FastqState>>().is_none() {
        app.manage(Arc::new(FastqState::default()));
    }
    app.state::<Arc<FastqState>>().inner().clone()
}

#[tauri::command]
pub async fn fastq_viewer_open<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
) -> std::result::Result<OpenResult, ViewerError> {
    let cd = cache_dir(&app)?;
    let state = ensure_state(&app);
    tokio::task::spawn_blocking(move || {
        let (session, cached) = FastqSession::open(&path, &cd)?;
        let result = OpenResult {
            total_records: session.index.total_records,
            index_cached: cached,
            path: session.path.clone(),
        };
        *state.session.lock().unwrap() = Some(Arc::new(session));
        Ok::<_, ViewerError>(result)
    })
    .await
    .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

#[tauri::command]
pub async fn fastq_viewer_read_records<R: Runtime>(
    app: AppHandle<R>,
    start_record: usize,
    count: usize,
) -> std::result::Result<Vec<FastqRecord>, ViewerError> {
    let state = ensure_state(&app);
    let session = state
        .session
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| ViewerError::Parse("no file open".into()))?;
    tokio::task::spawn_blocking(move || session.read_records(start_record, count))
        .await
        .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

#[tauri::command]
pub async fn fastq_viewer_seek_percent<R: Runtime>(
    app: AppHandle<R>,
    pct: f32,
) -> std::result::Result<usize, ViewerError> {
    let state = ensure_state(&app);
    let session = state
        .session
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| ViewerError::Parse("no file open".into()))?;
    Ok(session.seek_percent(pct))
}

#[derive(Serialize)]
pub struct SearchHit {
    pub record_n: usize,
    pub id: String,
}

#[tauri::command]
pub async fn fastq_viewer_search_id<R: Runtime>(
    app: AppHandle<R>,
    query: String,
    from_record: usize,
    limit: usize,
) -> std::result::Result<Vec<SearchHit>, ViewerError> {
    let state = ensure_state(&app);
    let session = state
        .session
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| ViewerError::Parse("no file open".into()))?;
    tokio::task::spawn_blocking(move || {
        let hits = session.search_id(&query, from_record, limit)?;
        Ok::<_, ViewerError>(
            hits.into_iter()
                .map(|(record_n, id)| SearchHit { record_n, id })
                .collect(),
        )
    })
    .await
    .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}
```

- [ ] **Step 2: Build the full workspace**

Run: `cargo check -p rb-fastq-viewer && cargo check -p rb-app`
Expected: both succeed.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-fastq-viewer/src/commands.rs
git commit -m "feat(fastq-viewer): tauri commands (open/read/seek/search)"
```

---

### Task 10: Frontend FASTQ view — virtualized list skeleton

**Files:**
- Modify: `frontend/js/utilities/fastq-viewer/view.js`
- Create: `frontend/js/utilities/fastq-viewer/virtual-list.js`

- [ ] **Step 1: Implement virtual-list module**

Create `frontend/js/utilities/fastq-viewer/virtual-list.js`:

```js
// A record-oriented virtualized list. Assumes every record has the same pixel height.
// Renders only records in the viewport ± overscan.
export class VirtualList {
  constructor({ host, recordHeight, overscan, renderRecord, fetchBatch }) {
    this.host = host;
    this.recordHeight = recordHeight;
    this.overscan = overscan;
    this.renderRecord = renderRecord;    // (record, index) => HTMLElement
    this.fetchBatch = fetchBatch;        // (start, count) => Promise<record[]>
    this.total = 0;
    this.cache = new Map();               // index → record
    this.pending = new Map();             // index → Promise

    this.host.classList.add('virtual-list');
    this.host.style.overflowY = 'auto';
    this.host.style.position = 'relative';
    this.spacer = document.createElement('div');
    this.viewport = document.createElement('div');
    this.viewport.style.position = 'absolute';
    this.viewport.style.top = '0';
    this.viewport.style.left = '0';
    this.viewport.style.right = '0';
    this.host.appendChild(this.spacer);
    this.host.appendChild(this.viewport);

    this.host.addEventListener('scroll', () => this._schedule());
    this._scheduled = false;
  }

  setTotal(total) {
    this.total = total;
    this.spacer.style.height = `${total * this.recordHeight}px`;
    this.cache.clear();
    this._schedule();
  }

  scrollToIndex(index) {
    this.host.scrollTop = index * this.recordHeight;
  }

  _schedule() {
    if (this._scheduled) return;
    this._scheduled = true;
    requestAnimationFrame(() => {
      this._scheduled = false;
      this._render();
    });
  }

  async _render() {
    const scrollTop = this.host.scrollTop;
    const hostH = this.host.clientHeight;
    const firstVisible = Math.max(0, Math.floor(scrollTop / this.recordHeight) - this.overscan);
    const lastVisible = Math.min(this.total - 1, Math.ceil((scrollTop + hostH) / this.recordHeight) + this.overscan);
    if (lastVisible < firstVisible) {
      this.viewport.innerHTML = '';
      return;
    }
    await this._ensureRange(firstVisible, lastVisible);
    this._paint(firstVisible, lastVisible);
  }

  async _ensureRange(first, last) {
    const missing = [];
    for (let i = first; i <= last; i++) {
      if (!this.cache.has(i) && !this.pending.has(i)) missing.push(i);
    }
    if (missing.length === 0) return;
    // Coalesce contiguous gaps.
    missing.sort((a, b) => a - b);
    const runs = [];
    let runStart = missing[0];
    let runEnd = missing[0];
    for (let i = 1; i < missing.length; i++) {
      if (missing[i] === runEnd + 1) runEnd = missing[i];
      else { runs.push([runStart, runEnd]); runStart = missing[i]; runEnd = missing[i]; }
    }
    runs.push([runStart, runEnd]);

    const promises = runs.map(([s, e]) => {
      const count = e - s + 1;
      const p = this.fetchBatch(s, count).then(recs => {
        recs.forEach((r, i) => this.cache.set(s + i, r));
      }).finally(() => {
        for (let i = s; i <= e; i++) this.pending.delete(i);
      });
      for (let i = s; i <= e; i++) this.pending.set(i, p);
      return p;
    });
    await Promise.all(promises);
  }

  _paint(first, last) {
    this.viewport.style.transform = `translateY(${first * this.recordHeight}px)`;
    this.viewport.innerHTML = '';
    for (let i = first; i <= last; i++) {
      const rec = this.cache.get(i);
      if (!rec) continue;
      const el = this.renderRecord(rec, i);
      el.style.height = `${this.recordHeight}px`;
      el.style.boxSizing = 'border-box';
      this.viewport.appendChild(el);
    }
  }
}
```

- [ ] **Step 2: Replace fastq-viewer/view.js with the wired version**

Replace `frontend/js/utilities/fastq-viewer/view.js` with:

```js
import { VirtualList } from './virtual-list.js';

const api = window.__TAURI__?.core?.invoke
  ? (cmd, args) => window.__TAURI__.core.invoke(cmd, args)
  : async () => { throw new Error('tauri not available'); };

export function renderFastqViewerView(content) {
  content.innerHTML = `
    <div class="module-view fastq-viewer">
      <header class="utility-header">
        <h1>FASTQ Viewer</h1>
        <div class="utility-controls">
          <button class="btn" data-act="fastq-open">Open File</button>
          <label>Record <input type="number" class="fastq-jump" min="0" value="0" style="width:100px"></label>
          <label>% <input type="range" class="fastq-pct" min="0" max="100" value="0" style="width:120px"></label>
          <input type="text" class="fastq-search" placeholder="Search read ID..." style="width:200px">
          <button class="btn" data-act="fastq-search-next">Find Next</button>
        </div>
        <div class="utility-meta"><span class="fastq-path">No file open</span> · <span class="fastq-count">—</span> records</div>
      </header>
      <div class="fastq-list" style="height:70vh;border:1px solid #e7e5e4;border-radius:6px;background:#faf8f4"></div>
    </div>
  `;

  const host = content.querySelector('.fastq-list');
  const pathEl = content.querySelector('.fastq-path');
  const countEl = content.querySelector('.fastq-count');
  const jumpEl = content.querySelector('.fastq-jump');
  const pctEl = content.querySelector('.fastq-pct');
  const searchEl = content.querySelector('.fastq-search');

  const state = { total: 0, searchCursor: 0 };

  const list = new VirtualList({
    host,
    recordHeight: 88,  // 4 text lines × ~22 px
    overscan: 10,
    fetchBatch: (start, count) => api('fastq_viewer_read_records', { startRecord: start, count }),
    renderRecord: (rec, i) => renderRecordEl(rec, i),
  });

  async function openFile() {
    const path = await api('select_files', { multiple: false });
    if (!path || !path[0]) return;
    pathEl.textContent = path[0];
    const res = await api('fastq_viewer_open', { path: path[0] });
    state.total = res.total_records;
    countEl.textContent = res.total_records.toLocaleString();
    list.setTotal(res.total_records);
    jumpEl.max = res.total_records - 1;
  }

  content.addEventListener('click', async (e) => {
    const act = e.target.closest('[data-act]')?.dataset.act;
    if (act === 'fastq-open') openFile();
    if (act === 'fastq-search-next') {
      const q = searchEl.value.trim();
      if (!q) return;
      const hits = await api('fastq_viewer_search_id', {
        query: q, fromRecord: state.searchCursor, limit: 1,
      });
      if (hits.length) {
        list.scrollToIndex(hits[0].record_n);
        state.searchCursor = hits[0].record_n + 1;
      } else {
        state.searchCursor = 0; // wrap
      }
    }
  });

  jumpEl.addEventListener('change', () => list.scrollToIndex(Number(jumpEl.value)));
  pctEl.addEventListener('change', async () => {
    const n = await api('fastq_viewer_seek_percent', { pct: Number(pctEl.value) / 100 });
    list.scrollToIndex(n);
  });
}

function renderRecordEl(rec, i) {
  const el = document.createElement('div');
  el.className = 'fastq-record';
  el.style.padding = '4px 8px';
  el.style.fontFamily = 'monospace';
  el.style.fontSize = '13px';
  el.style.borderBottom = '1px solid #f1ede7';
  el.innerHTML = `
    <div style="color:#5c7080">#${i} ${escapeHtml(rec.id)}</div>
    <div class="seq">${escapeHtml(rec.seq)}</div>
    <div style="color:#a8a29e">${escapeHtml(rec.plus)}</div>
    <div class="qual">${escapeHtml(rec.qual)}</div>
  `;
  return el;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
```

- [ ] **Step 3: Manual test**

Run: `cd crates/rb-app && cargo tauri dev`
Steps:
1. Wait for app launch.
2. Click sidebar "FASTQ Viewer".
3. Click "Open File", pick `crates/rb-fastq-viewer/testdata/tiny.fastq`.
4. Confirm header shows "100 records".
5. Scroll — records render in viewport.
6. Type `0042` in search, click "Find Next" — viewport jumps to record 42.
7. Change % slider to 50 — viewport jumps to record 50.

- [ ] **Step 4: Commit**

```bash
git add frontend/js/utilities/fastq-viewer/
git commit -m "feat(fastq-viewer): virtualized list view wired to tauri backend"
```

---

### Task 11: FASTQ coloring (ACGT + Phred)

**Files:**
- Create: `frontend/js/utilities/fastq-viewer/coloring.js`
- Modify: `frontend/js/utilities/fastq-viewer/view.js`

- [ ] **Step 1: Implement coloring**

Create `frontend/js/utilities/fastq-viewer/coloring.js`:

```js
const BASE_COLOR = { A: '#2d8659', C: '#3b6ea5', G: '#b8860b', T: '#c9503c', N: '#a8a29e' };

export function colorSeq(seq) {
  let out = '';
  for (const ch of seq) {
    const color = BASE_COLOR[ch.toUpperCase()] || '#57534e';
    out += `<span style="color:${color}">${ch}</span>`;
  }
  return out;
}

// Phred 33-encoded ASCII → Q score → HSL red→green
export function colorQual(qual) {
  let out = '';
  for (const ch of qual) {
    const q = Math.max(0, Math.min(40, ch.charCodeAt(0) - 33));
    // 0 → red (0deg), 40 → green (120deg)
    const hue = Math.round((q / 40) * 120);
    out += `<span style="background:hsl(${hue},60%,85%);padding:0 1px">${escape(ch)}</span>`;
  }
  return out;
}

function escape(s) {
  return s.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
```

- [ ] **Step 2: Use in view.js**

In `frontend/js/utilities/fastq-viewer/view.js`, add import at top:

```js
import { colorSeq, colorQual } from './coloring.js';
```

Update `renderRecordEl` so the seq and qual lines use colored HTML:

```js
function renderRecordEl(rec, i) {
  const el = document.createElement('div');
  el.className = 'fastq-record';
  el.style.padding = '4px 8px';
  el.style.fontFamily = 'monospace';
  el.style.fontSize = '13px';
  el.style.borderBottom = '1px solid #f1ede7';
  el.innerHTML = `
    <div style="color:#5c7080">#${i} ${escapeHtml(rec.id)}</div>
    <div class="seq">${colorSeq(rec.seq)}</div>
    <div style="color:#a8a29e">${escapeHtml(rec.plus)}</div>
    <div class="qual">${colorQual(rec.qual)}</div>
  `;
  return el;
}
```

- [ ] **Step 3: Manual verification**

Run: `cd crates/rb-app && cargo tauri dev`
Open fastq-viewer → Open tiny.fastq. Confirm:
- A/C/G/T in sequence row are colored (green/blue/gold/coral respectively).
- Quality row has a reddish/greenish background per character. All-`I` quality should be green (Q=40 → hue=120).

- [ ] **Step 4: Commit**

```bash
git add frontend/js/utilities/fastq-viewer/coloring.js frontend/js/utilities/fastq-viewer/view.js
git commit -m "feat(fastq-viewer): ACGT palette + Phred→HSL quality coloring"
```

---

## Phase 3 — `rb-genome-viewer` Backend

Goal: build the Rust half of the genome viewer: reference FASTA, annotation indexing (memory + tabix), gene search, session persistence, and Tauri commands. No frontend yet.

### Task 12: Crate skeleton + fixtures

**Files:**
- Create: `crates/rb-genome-viewer/Cargo.toml`
- Create: `crates/rb-genome-viewer/src/lib.rs`
- Create: `crates/rb-genome-viewer/src/error.rs`
- Create: `crates/rb-genome-viewer/testdata/tiny.fa`
- Create: `crates/rb-genome-viewer/testdata/tiny.gff3`
- Create: `crates/rb-genome-viewer/testdata/tiny.gtf`
- Create: `crates/rb-genome-viewer/testdata/tiny.bed`
- Modify: `Cargo.toml` (workspace)
- Modify: `crates/rb-app/Cargo.toml`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: Add workspace dependencies**

Edit root `Cargo.toml`, under `[workspace.dependencies]` append:

```toml
noodles-fasta = "0.40"
noodles-gff = "0.35"
noodles-bed = "0.17"
noodles-bgzf = "0.30"
noodles-tabix = "0.45"
noodles-core = "0.15"
```

Add `"crates/rb-genome-viewer"` to `[workspace]` `members` (alphabetical).

- [ ] **Step 2: Create crate Cargo.toml**

```toml
[package]
name = "rb-genome-viewer"
version = "0.1.0"
edition = "2021"

[dependencies]
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
thiserror.workspace = true
noodles-fasta.workspace = true
noodles-gff.workspace = true
noodles-bed.workspace = true
noodles-bgzf.workspace = true
noodles-tabix.workspace = true
noodles-core.workspace = true
tauri = "2"
uuid = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Create `src/error.rs` (clone of fastq-viewer pattern)**

```rust
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ViewerError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("file not found: {0}")]
    NotFound(PathBuf),
    #[error("unsupported format: {0}")]
    UnsupportedKind(String),
    #[error("track not found: {0}")]
    TrackNotFound(String),
    #[error("no reference loaded")]
    NoReference,
    #[error("index build failed: {0}")]
    IndexBuildFailed(String),
}

#[derive(Serialize)]
struct SerializedError {
    code: String,
    message: String,
    path: Option<PathBuf>,
}

impl ViewerError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Parse(_) => "parse",
            Self::NotFound(_) => "not_found",
            Self::UnsupportedKind(_) => "unsupported_kind",
            Self::TrackNotFound(_) => "track_not_found",
            Self::NoReference => "no_reference",
            Self::IndexBuildFailed(_) => "index_build_failed",
        }
    }
}

impl Serialize for ViewerError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let path = match self {
            Self::NotFound(p) => Some(p.clone()),
            _ => None,
        };
        SerializedError {
            code: self.code().to_string(),
            message: self.to_string(),
            path,
        }
        .serialize(s)
    }
}

pub type Result<T> = std::result::Result<T, ViewerError>;
```

- [ ] **Step 4: Create `src/lib.rs` stub**

```rust
pub mod error;
pub mod reference;
pub mod tracks;
pub mod index;
pub mod search;
pub mod session;
pub mod commands;
pub mod bgzip;

pub use error::{Result, ViewerError};
```

Create empty stubs for each module (`// stub` file bodies) so the lib compiles.

- [ ] **Step 5: Create fixtures**

`crates/rb-genome-viewer/testdata/tiny.fa`:

```
>chr1 synthetic
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT
>chr2 synthetic
GGGGAAAATTTTCCCCGGGGAAAATTTTCCCCGGGGAAAATTTTCCCCGGGGAAAATTTTCCCC
GGGGAAAATTTTCCCCGGGGAAAATTTTCCCCGGGGAAAATTTTCCCCGGGGAAAATTTTCCCC
```

Both sequences are 2 × 64 = 128 bp.

`crates/rb-genome-viewer/testdata/tiny.gff3`:

```
##gff-version 3
chr1	test	gene	10	50	.	+	.	ID=gene1;Name=BRCA1-like
chr1	test	mRNA	10	50	.	+	.	ID=mrna1;Parent=gene1
chr1	test	exon	10	25	.	+	.	ID=exon1;Parent=mrna1
chr1	test	exon	35	50	.	+	.	ID=exon2;Parent=mrna1
chr2	test	gene	20	80	.	-	.	ID=gene2;Name=TP53-like
chr2	test	mRNA	20	80	.	-	.	ID=mrna2;Parent=gene2
chr2	test	exon	20	40	.	-	.	ID=exon3;Parent=mrna2
chr2	test	exon	60	80	.	-	.	ID=exon4;Parent=mrna2
```

`crates/rb-genome-viewer/testdata/tiny.gtf`:

```
chr1	test	gene	10	50	.	+	.	gene_id "gene1"; gene_name "BRCA1-like";
chr1	test	transcript	10	50	.	+	.	gene_id "gene1"; transcript_id "mrna1";
chr1	test	exon	10	25	.	+	.	gene_id "gene1"; transcript_id "mrna1"; exon_number "1";
chr1	test	exon	35	50	.	+	.	gene_id "gene1"; transcript_id "mrna1"; exon_number "2";
chr2	test	gene	20	80	.	-	.	gene_id "gene2"; gene_name "TP53-like";
```

`crates/rb-genome-viewer/testdata/tiny.bed`:

```
chr1	10	50	peak1	100	+
chr1	60	90	peak2	200	+
chr2	5	40	peak3	50	-
chr2	70	110	peak4	150	-
```

Note: BED uses 0-based start, half-open. GFF/GTF use 1-based closed.

- [ ] **Step 6: Wire into rb-app**

Edit `crates/rb-app/Cargo.toml`, add under `[dependencies]`:

```toml
rb-genome-viewer = { path = "../rb-genome-viewer" }
```

In `crates/rb-app/src/main.rs`, **don't** add command names to `generate_handler!` yet (they'll be added in Task 22). Just ensure the crate compiles.

- [ ] **Step 7: Build**

Run: `cargo check -p rb-genome-viewer && cargo check -p rb-app`
Expected: success.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/rb-genome-viewer crates/rb-app/Cargo.toml
git commit -m "feat(genome-viewer): scaffold rb-genome-viewer crate with fixtures"
```

---

### Task 13: Reference loader with .fai build

**Files:**
- Modify: `crates/rb-genome-viewer/src/reference.rs`

- [ ] **Step 1: Implement and test**

Replace `crates/rb-genome-viewer/src/reference.rs` with:

```rust
use crate::error::{Result, ViewerError};
use noodles_fasta::{self as fasta, fai};
use serde::Serialize;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct ChromMeta {
    pub name: String,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReferenceMeta {
    pub path: PathBuf,
    pub chroms: Vec<ChromMeta>,
    pub fai_built: bool,
}

pub struct ReferenceHandle {
    pub path: PathBuf,
    pub fai: fai::Index,
}

impl ReferenceHandle {
    pub fn load(path: &Path) -> Result<(Self, ReferenceMeta)> {
        if !path.exists() {
            return Err(ViewerError::NotFound(path.to_path_buf()));
        }
        let fai_path = {
            let mut p = path.to_path_buf();
            p.as_mut_os_string().push(".fai");
            p
        };
        let fai_built = if !fai_path.exists() {
            let indexed = fai::Index::index(path)
                .map_err(|e| ViewerError::IndexBuildFailed(format!(".fai index: {e}")))?;
            let out = File::create(&fai_path)?;
            fai::write(out, &indexed)
                .map_err(|e| ViewerError::IndexBuildFailed(format!("write .fai: {e}")))?;
            true
        } else {
            false
        };
        let fai = fai::read(&fai_path)
            .map_err(|e| ViewerError::IndexBuildFailed(format!("read .fai: {e}")))?;

        let chroms: Vec<ChromMeta> = fai
            .as_ref()
            .iter()
            .map(|rec| ChromMeta {
                name: String::from_utf8_lossy(rec.name()).into_owned(),
                length: rec.length(),
            })
            .collect();

        let handle = Self { path: path.to_path_buf(), fai };
        let meta = ReferenceMeta { path: path.to_path_buf(), chroms, fai_built };
        Ok((handle, meta))
    }

    pub fn fetch_region(&self, chrom: &str, start: u64, end: u64) -> Result<String> {
        // Build a noodles indexed reader on demand.
        let file = File::open(&self.path)?;
        let mut reader = fasta::io::IndexedReader::new(BufReader::new(file), self.fai.clone());
        let region: noodles_core::Region = format!("{chrom}:{}-{}", start.max(1), end)
            .parse()
            .map_err(|e| ViewerError::Parse(format!("region parse: {e}")))?;
        let seq = reader
            .query(&region)
            .map_err(|e| ViewerError::Parse(format!("fasta query: {e}")))?;
        Ok(String::from_utf8_lossy(seq.sequence().as_ref()).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fa() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.fa")
    }

    #[test]
    fn loads_two_chroms_and_builds_fai() {
        // Ensure any previous .fai is removed for a deterministic test.
        let fai_path = {
            let mut p = fa();
            p.as_mut_os_string().push(".fai");
            p
        };
        let _ = std::fs::remove_file(&fai_path);

        let (_handle, meta) = ReferenceHandle::load(&fa()).unwrap();
        assert!(meta.fai_built);
        assert_eq!(meta.chroms.len(), 2);
        assert_eq!(meta.chroms[0].name, "chr1");
        assert_eq!(meta.chroms[0].length, 128);
        assert_eq!(meta.chroms[1].name, "chr2");
        assert_eq!(meta.chroms[1].length, 128);
    }

    #[test]
    fn fetches_region_bytes() {
        let (handle, _) = ReferenceHandle::load(&fa()).unwrap();
        let seq = handle.fetch_region("chr1", 1, 16).unwrap();
        assert_eq!(seq, "ACGTACGTACGTACGT");
    }

    #[test]
    fn fetches_cross_chrom_distinctly() {
        let (handle, _) = ReferenceHandle::load(&fa()).unwrap();
        let a = handle.fetch_region("chr1", 1, 4).unwrap();
        let b = handle.fetch_region("chr2", 1, 4).unwrap();
        assert_eq!(a, "ACGT");
        assert_eq!(b, "GGGG");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p rb-genome-viewer --lib reference`
Expected: 3 tests pass.

If `fai::Index::index` or `IndexedReader` APIs differ in the pinned noodles version, consult `cargo doc -p noodles-fasta --open` for the actual signatures and adapt. The tests above define the observable contract — keep them passing.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-genome-viewer/src/reference.rs
git commit -m "feat(genome-viewer): reference FASTA loader with .fai auto-build"
```

---

### Task 14: Track types + GFF memory index

**Files:**
- Modify: `crates/rb-genome-viewer/src/tracks.rs`
- Modify: `crates/rb-genome-viewer/src/index.rs`

- [ ] **Step 1: Define track types in `tracks.rs`**

Replace `crates/rb-genome-viewer/src/tracks.rs` with:

```rust
use crate::error::{Result, ViewerError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub type TrackId = String;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Gff,
    Gtf,
    Bed,
    // L2 — reserved:
    // Bam,
}

impl TrackKind {
    pub fn detect(path: &Path, hint: Option<&str>) -> Result<Self> {
        if let Some(h) = hint {
            return match h.to_lowercase().as_str() {
                "gff" | "gff3" => Ok(Self::Gff),
                "gtf" => Ok(Self::Gtf),
                "bed" => Ok(Self::Bed),
                "bam" | "cram" => Err(ViewerError::UnsupportedKind(
                    "BAM/CRAM alignment tracks arrive in L2".into(),
                )),
                other => Err(ViewerError::UnsupportedKind(other.into())),
            };
        }
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        // Also handle `.gff3.gz`, `.gtf.gz`, `.bed.gz` by stripping .gz first.
        let effective = if ext == "gz" {
            path.file_stem()
                .and_then(|s| Path::new(s).extension())
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_default()
        } else {
            ext
        };
        match effective.as_str() {
            "gff" | "gff3" => Ok(Self::Gff),
            "gtf" => Ok(Self::Gtf),
            "bed" => Ok(Self::Bed),
            "bam" | "cram" => Err(ViewerError::UnsupportedKind(
                "BAM/CRAM alignment tracks arrive in L2".into(),
            )),
            other => Err(ViewerError::UnsupportedKind(other.into())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMeta {
    pub track_id: TrackId,
    pub kind: TrackKind,
    pub path: PathBuf,
    pub source: TrackSource,
    pub feature_count: usize,
    pub suggest_bgzip: bool,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackSource {
    Memory,
    Tabix,
}

pub fn new_track_id() -> TrackId {
    Uuid::new_v4().simple().to_string()[..12].to_string()
}
```

- [ ] **Step 2: Define feature + GFF in-memory indexing in `index.rs`**

Replace `crates/rb-genome-viewer/src/index.rs` with:

```rust
use crate::error::{Result, ViewerError};
use crate::tracks::TrackKind;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufRead;
use std::path::Path;

pub const MEMORY_INDEX_MAX_BYTES: u64 = 200 * 1024 * 1024; // 200 MB

#[derive(Debug, Clone, Serialize)]
pub struct Feature {
    pub chrom: String,
    pub start: u64,          // 1-based inclusive (GFF/GTF convention); BED converted on load
    pub end: u64,            // inclusive
    pub name: Option<String>,
    pub strand: Option<char>,
    pub kind: String,        // e.g., "gene", "exon", free-text
    pub attrs: HashMap<String, String>,
}

/// Simple per-chromosome feature list. For L1 we linear-scan per query — fast enough
/// for typical annotation file sizes (<100k features). If we ever need true interval
/// trees we can swap in `rust-lapper` behind this API.
#[derive(Default)]
pub struct MemoryIndex {
    by_chrom: HashMap<String, Vec<Feature>>,
}

impl MemoryIndex {
    pub fn load(path: &Path, kind: TrackKind) -> Result<Self> {
        match kind {
            TrackKind::Gff => Self::load_gff(path, false),
            TrackKind::Gtf => Self::load_gff(path, true), // same format; different attr syntax
            TrackKind::Bed => Self::load_bed(path),
        }
    }

    pub fn query(&self, chrom: &str, start: u64, end: u64) -> Vec<&Feature> {
        match self.by_chrom.get(chrom) {
            None => Vec::new(),
            Some(v) => v
                .iter()
                .filter(|f| f.end >= start && f.start <= end)
                .collect(),
        }
    }

    pub fn feature_count(&self) -> usize {
        self.by_chrom.values().map(|v| v.len()).sum()
    }

    pub fn all_features(&self) -> impl Iterator<Item = &Feature> {
        self.by_chrom.values().flat_map(|v| v.iter())
    }

    fn load_gff(path: &Path, is_gtf: bool) -> Result<Self> {
        let mut idx = Self::default();
        let file = File::open(path)?;
        let reader: Box<dyn BufRead> = if has_gz_ext(path) {
            Box::new(std::io::BufReader::new(
                noodles_bgzf::Reader::new(file),
            ))
        } else {
            Box::new(std::io::BufReader::new(file))
        };
        for line in reader.lines() {
            let line = line?;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 8 {
                continue;
            }
            let chrom = fields[0].to_string();
            let kind_str = fields[2].to_string();
            let start: u64 = fields[3].parse().map_err(|e| ViewerError::Parse(format!("start: {e}")))?;
            let end: u64 = fields[4].parse().map_err(|e| ViewerError::Parse(format!("end: {e}")))?;
            let strand = fields[6].chars().next();
            let attrs = if is_gtf {
                parse_gtf_attrs(fields.get(8).copied().unwrap_or(""))
            } else {
                parse_gff_attrs(fields.get(8).copied().unwrap_or(""))
            };
            let name = attrs
                .get("Name").cloned()
                .or_else(|| attrs.get("gene_name").cloned())
                .or_else(|| attrs.get("ID").cloned())
                .or_else(|| attrs.get("gene_id").cloned())
                .or_else(|| attrs.get("transcript_id").cloned());
            idx.by_chrom.entry(chrom.clone()).or_default().push(Feature {
                chrom,
                start,
                end,
                name,
                strand,
                kind: kind_str,
                attrs,
            });
        }
        Ok(idx)
    }

    fn load_bed(path: &Path) -> Result<Self> {
        let mut idx = Self::default();
        let file = File::open(path)?;
        let reader: Box<dyn BufRead> = if has_gz_ext(path) {
            Box::new(std::io::BufReader::new(
                noodles_bgzf::Reader::new(file),
            ))
        } else {
            Box::new(std::io::BufReader::new(file))
        };
        for line in reader.lines() {
            let line = line?;
            if line.is_empty() || line.starts_with('#') || line.starts_with("track") || line.starts_with("browser") {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 3 {
                continue;
            }
            let chrom = fields[0].to_string();
            let start: u64 = fields[1].parse::<u64>()
                .map_err(|e| ViewerError::Parse(format!("bed start: {e}")))?
                + 1; // BED 0-based → internal 1-based inclusive
            let end: u64 = fields[2].parse::<u64>()
                .map_err(|e| ViewerError::Parse(format!("bed end: {e}")))?;
            let name = fields.get(3).map(|s| s.to_string());
            let strand = fields.get(5).and_then(|s| s.chars().next());
            idx.by_chrom.entry(chrom.clone()).or_default().push(Feature {
                chrom,
                start,
                end,
                name,
                strand,
                kind: "region".into(),
                attrs: HashMap::new(),
            });
        }
        Ok(idx)
    }
}

fn has_gz_ext(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).map(|s| s == "gz").unwrap_or(false)
}

fn parse_gff_attrs(s: &str) -> HashMap<String, String> {
    s.split(';')
        .filter_map(|kv| {
            let mut it = kv.trim().splitn(2, '=');
            let k = it.next()?.trim().to_string();
            let v = it.next()?.trim().to_string();
            if k.is_empty() { None } else { Some((k, v)) }
        })
        .collect()
}

fn parse_gtf_attrs(s: &str) -> HashMap<String, String> {
    s.split(';')
        .filter_map(|kv| {
            let kv = kv.trim();
            if kv.is_empty() { return None; }
            let mut it = kv.splitn(2, ' ');
            let k = it.next()?.trim().to_string();
            let v = it.next()?.trim().trim_matches('"').to_string();
            Some((k, v))
        })
        .collect()
}

pub fn file_is_large(path: &Path) -> Result<bool> {
    let meta = std::fs::metadata(path)?;
    Ok(meta.len() > MEMORY_INDEX_MAX_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn gff() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gff3") }
    fn gtf() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gtf") }
    fn bed() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.bed") }

    #[test]
    fn loads_gff3_and_counts_features() {
        let idx = MemoryIndex::load(&gff(), TrackKind::Gff).unwrap();
        assert_eq!(idx.feature_count(), 8);
    }

    #[test]
    fn gff_query_returns_overlap() {
        let idx = MemoryIndex::load(&gff(), TrackKind::Gff).unwrap();
        let hits = idx.query("chr1", 20, 40);
        // gene (10-50), mRNA (10-50), exon1 (10-25), exon2 (35-50)
        assert_eq!(hits.len(), 4);
    }

    #[test]
    fn gff_query_different_chrom_empty() {
        let idx = MemoryIndex::load(&gff(), TrackKind::Gff).unwrap();
        assert!(idx.query("chr1", 200, 300).is_empty());
        assert!(idx.query("chrX", 1, 1000).is_empty());
    }

    #[test]
    fn gtf_attrs_parsed() {
        let idx = MemoryIndex::load(&gtf(), TrackKind::Gtf).unwrap();
        let feats: Vec<&Feature> = idx.all_features().collect();
        let gene = feats.iter().find(|f| f.kind == "gene" && f.chrom == "chr1").unwrap();
        assert_eq!(gene.attrs.get("gene_id").map(|s| s.as_str()), Some("gene1"));
        assert_eq!(gene.name.as_deref(), Some("BRCA1-like"));
    }

    #[test]
    fn bed_converted_to_one_based() {
        let idx = MemoryIndex::load(&bed(), TrackKind::Bed).unwrap();
        let peak1 = idx.all_features().find(|f| f.name.as_deref() == Some("peak1")).unwrap();
        assert_eq!(peak1.start, 11); // BED 10 → internal 11 (1-based)
        assert_eq!(peak1.end, 50);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rb-genome-viewer --lib`
Expected: 5 new tests + 3 reference tests = 8 pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-genome-viewer/src/tracks.rs crates/rb-genome-viewer/src/index.rs
git commit -m "feat(genome-viewer): track kinds + memory index for GFF/GTF/BED"
```

---

### Task 15: Feature search by name

**Files:**
- Modify: `crates/rb-genome-viewer/src/search.rs`

- [ ] **Step 1: Implement search index**

Replace `crates/rb-genome-viewer/src/search.rs` with:

```rust
use crate::index::MemoryIndex;
use crate::tracks::TrackId;
use serde::Serialize;
use std::collections::HashMap;

/// Maps normalized feature name → (track_id, chrom, start, end).
#[derive(Default)]
pub struct SearchIndex {
    entries: HashMap<String, Vec<SearchEntry>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchEntry {
    pub track_id: TrackId,
    pub name: String,
    pub chrom: String,
    pub start: u64,
    pub end: u64,
}

impl SearchIndex {
    pub fn add_track(&mut self, track_id: &TrackId, memory: &MemoryIndex) {
        for f in memory.all_features() {
            let keys: Vec<String> = std::iter::once(f.name.clone())
                .chain(f.attrs.get("gene_id").cloned().map(Some))
                .chain(f.attrs.get("gene_name").cloned().map(Some))
                .chain(f.attrs.get("transcript_id").cloned().map(Some))
                .chain(f.attrs.get("Name").cloned().map(Some))
                .chain(f.attrs.get("ID").cloned().map(Some))
                .flatten()
                .collect();
            for k in keys {
                let lc = k.to_lowercase();
                self.entries.entry(lc).or_default().push(SearchEntry {
                    track_id: track_id.clone(),
                    name: k,
                    chrom: f.chrom.clone(),
                    start: f.start,
                    end: f.end,
                });
            }
        }
    }

    pub fn remove_track(&mut self, track_id: &TrackId) {
        for v in self.entries.values_mut() {
            v.retain(|e| e.track_id != *track_id);
        }
        self.entries.retain(|_, v| !v.is_empty());
    }

    /// Case-insensitive substring match. Returns at most `limit` entries.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&SearchEntry> {
        let q = query.to_lowercase();
        let mut out = Vec::new();
        for (key, entries) in &self.entries {
            if key.contains(&q) {
                for e in entries {
                    out.push(e);
                    if out.len() == limit {
                        return out;
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::MemoryIndex;
    use crate::tracks::TrackKind;
    use std::path::PathBuf;

    #[test]
    fn search_finds_gene_by_name() {
        let gtf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gtf");
        let mem = MemoryIndex::load(&gtf, TrackKind::Gtf).unwrap();
        let mut idx = SearchIndex::default();
        idx.add_track(&"t1".to_string(), &mem);
        let hits = idx.search("brca", 10);
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|e| e.chrom == "chr1" && e.start == 10));
    }

    #[test]
    fn search_case_insensitive() {
        let gtf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gtf");
        let mem = MemoryIndex::load(&gtf, TrackKind::Gtf).unwrap();
        let mut idx = SearchIndex::default();
        idx.add_track(&"t1".to_string(), &mem);
        let lower = idx.search("tp53", 10);
        let upper = idx.search("TP53", 10);
        assert_eq!(lower.len(), upper.len());
    }

    #[test]
    fn remove_track_clears_entries() {
        let gtf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gtf");
        let mem = MemoryIndex::load(&gtf, TrackKind::Gtf).unwrap();
        let mut idx = SearchIndex::default();
        idx.add_track(&"t1".to_string(), &mem);
        idx.remove_track(&"t1".to_string());
        assert!(idx.search("brca", 10).is_empty());
    }
}
```

- [ ] **Step 2: Run**

Run: `cargo test -p rb-genome-viewer --lib search`
Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-genome-viewer/src/search.rs
git commit -m "feat(genome-viewer): feature name search index"
```

---

### Task 16: bgzip + tabix workflow

**Files:**
- Modify: `crates/rb-genome-viewer/src/bgzip.rs`

- [ ] **Step 1: Implement bgzip+tabix conversion**

Replace `crates/rb-genome-viewer/src/bgzip.rs` with:

```rust
use crate::error::{Result, ViewerError};
use crate::tracks::TrackKind;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

/// Runs synchronously (caller wraps in spawn_blocking). Writes `<input>.gz` and
/// `<input>.gz.tbi` next to the input. Returns the new `.gz` path.
pub fn bgzip_and_tabix<F: FnMut(u64, u64)>(
    input: &Path,
    kind: TrackKind,
    mut progress: F,
) -> Result<PathBuf> {
    let total = std::fs::metadata(input)?.len();
    let out_gz = {
        let mut p = input.to_path_buf();
        p.as_mut_os_string().push(".gz");
        p
    };

    // Phase 1: bgzip the input.
    {
        let src = File::open(input)?;
        let mut reader = BufReader::new(src);
        let dst = File::create(&out_gz)?;
        let mut writer = noodles_bgzf::Writer::new(dst);
        let mut buf = [0u8; 64 * 1024];
        let mut bytes_written: u64 = 0;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 { break; }
            writer.write_all(&buf[..n])?;
            bytes_written += n as u64;
            progress(bytes_written, total);
        }
        writer.finish()
            .map_err(|e| ViewerError::IndexBuildFailed(format!("bgzf finish: {e}")))?;
    }

    // Phase 2: build .tbi next to .gz.
    let preset = match kind {
        TrackKind::Bed => noodles_tabix::index::header::format::Preset::Bed,
        _ => noodles_tabix::index::header::format::Preset::Gff,
    };
    let index = noodles_tabix::index(&out_gz, preset)
        .map_err(|e| ViewerError::IndexBuildFailed(format!("tabix index: {e}")))?;
    let tbi_path = {
        let mut p = out_gz.clone();
        p.as_mut_os_string().push(".tbi");
        p
    };
    let tbi = File::create(&tbi_path)?;
    let mut writer = noodles_tabix::io::Writer::new(tbi);
    writer.write_index(&index)
        .map_err(|e| ViewerError::IndexBuildFailed(format!("write tabix: {e}")))?;
    Ok(out_gz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracks::TrackKind;

    #[test]
    fn bgzip_gff_produces_gz_and_tbi() {
        let tmp = tempfile::tempdir().unwrap();
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gff3");
        let copy = tmp.path().join("tiny.gff3");
        std::fs::copy(&src, &copy).unwrap();

        let mut last_pct = 0.0;
        let gz = bgzip_and_tabix(&copy, TrackKind::Gff, |done, total| {
            last_pct = done as f64 / total as f64;
        }).unwrap();
        assert!(gz.exists());
        let tbi = {
            let mut p = gz.clone();
            p.as_mut_os_string().push(".tbi");
            p
        };
        assert!(tbi.exists(), "tabix index {} not found", tbi.display());
        assert!(last_pct > 0.99);
    }
}
```

- [ ] **Step 2: Run test**

Run: `cargo test -p rb-genome-viewer --lib bgzip`
Expected: 1 test passes.

Note: if pinned `noodles-tabix` exposes a different API, check `cargo doc` for actual `index::header::format::Preset` path or equivalent (the presets are stable concepts but occasionally move between sub-modules). The test defines the contract.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-genome-viewer/src/bgzip.rs
git commit -m "feat(genome-viewer): bgzip+tabix converter for large annotation files"
```

---

### Task 17: Session + persistence

**Files:**
- Modify: `crates/rb-genome-viewer/src/session.rs`

- [ ] **Step 1: Implement session**

Replace `crates/rb-genome-viewer/src/session.rs` with:

```rust
use crate::error::Result;
use crate::index::MemoryIndex;
use crate::reference::{ReferenceHandle, ReferenceMeta};
use crate::search::SearchIndex;
use crate::tracks::{TrackId, TrackKind, TrackMeta, TrackSource};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenomicRegion {
    pub chrom: String,
    pub start: u64,
    pub end: u64,
}

#[derive(Default)]
pub struct GenomeSession {
    pub reference: Option<ReferenceHandle>,
    pub reference_meta: Option<ReferenceMeta>,
    pub tracks: HashMap<TrackId, TrackRuntime>,
    pub search: SearchIndex,
    pub position: Option<GenomicRegion>,
}

pub struct TrackRuntime {
    pub meta: TrackMeta,
    pub memory: Option<Arc<MemoryIndex>>,
    // tabix reader reconstructed on demand; see commands.rs fetch_track_features
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedSession {
    pub version: u32,
    pub reference: Option<SerializedReference>,
    pub tracks: Vec<SerializedTrack>,
    pub position: Option<GenomicRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedReference {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedTrack {
    pub path: PathBuf,
    pub kind: TrackKind,
    pub visible: bool,
}

impl GenomeSession {
    pub fn serialize(&self) -> SerializedSession {
        SerializedSession {
            version: 1,
            reference: self.reference_meta.as_ref().map(|m| SerializedReference { path: m.path.clone() }),
            tracks: self
                .tracks
                .values()
                .map(|t| SerializedTrack {
                    path: t.meta.path.clone(),
                    kind: t.meta.kind,
                    visible: t.meta.visible,
                })
                .collect(),
            position: self.position.clone(),
        }
    }
}

pub fn load_session_from_disk(path: &Path) -> Result<Option<SerializedSession>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    let s: SerializedSession = serde_json::from_slice(&bytes)
        .map_err(|e| crate::error::ViewerError::Parse(format!("session parse: {e}")))?;
    Ok(Some(s))
}

pub fn save_session_to_disk(path: &Path, s: &SerializedSession) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(s).map_err(|e| crate::error::ViewerError::Parse(format!("serde: {e}")))?;
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_empty_session() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("session.json");
        let s = SerializedSession {
            version: 1,
            reference: None,
            tracks: vec![],
            position: None,
        };
        save_session_to_disk(&p, &s).unwrap();
        let loaded = load_session_from_disk(&p).unwrap().unwrap();
        assert_eq!(loaded.version, 1);
        assert!(loaded.reference.is_none());
    }

    #[test]
    fn round_trip_with_position() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("session.json");
        let s = SerializedSession {
            version: 1,
            reference: Some(SerializedReference { path: PathBuf::from("/x/y.fa") }),
            tracks: vec![
                SerializedTrack { path: PathBuf::from("/x/y.gff"), kind: TrackKind::Gff, visible: true },
            ],
            position: Some(GenomicRegion { chrom: "chr1".into(), start: 100, end: 200 }),
        };
        save_session_to_disk(&p, &s).unwrap();
        let loaded = load_session_from_disk(&p).unwrap().unwrap();
        assert_eq!(loaded.tracks.len(), 1);
        assert!(matches!(loaded.tracks[0].kind, TrackKind::Gff));
        let pos = loaded.position.unwrap();
        assert_eq!(pos.start, 100);
    }

    #[test]
    fn missing_file_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("nope.json");
        assert!(load_session_from_disk(&p).unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p rb-genome-viewer --lib session`
Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-genome-viewer/src/session.rs
git commit -m "feat(genome-viewer): session serialization + disk persistence"
```

---

### Task 18: Tauri commands (reference, tracks, fetch)

**Files:**
- Modify: `crates/rb-genome-viewer/src/commands.rs`

- [ ] **Step 1: Implement command layer**

Replace `crates/rb-genome-viewer/src/commands.rs` with:

```rust
use crate::bgzip;
use crate::error::{Result, ViewerError};
use crate::index::{file_is_large, Feature, MemoryIndex};
use crate::reference::ReferenceMeta;
use crate::search::SearchEntry;
use crate::session::{
    load_session_from_disk, save_session_to_disk, GenomeSession, GenomicRegion,
    SerializedSession, TrackRuntime,
};
use crate::tracks::{new_track_id, TrackId, TrackKind, TrackMeta, TrackSource};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

#[derive(Default)]
pub struct GenomeState {
    pub session: Mutex<GenomeSession>,
}

fn ensure_state<R: Runtime>(app: &AppHandle<R>) -> Arc<GenomeState> {
    if app.try_state::<Arc<GenomeState>>().is_none() {
        app.manage(Arc::new(GenomeState::default()));
    }
    app.state::<Arc<GenomeState>>().inner().clone()
}

fn session_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| ViewerError::Parse(format!("app_data_dir: {e}")))?;
    Ok(base.join("genome_viewer_session.json"))
}

#[tauri::command]
pub async fn genome_viewer_load_reference<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
) -> std::result::Result<ReferenceMeta, ViewerError> {
    let state = ensure_state(&app);
    tokio::task::spawn_blocking(move || {
        let (handle, meta) = crate::reference::ReferenceHandle::load(&path)?;
        let mut s = state.session.lock().unwrap();
        s.reference = Some(handle);
        s.reference_meta = Some(meta.clone());
        Ok::<_, ViewerError>(meta)
    })
    .await
    .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

#[tauri::command]
pub async fn genome_viewer_add_track<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
    kind_hint: Option<String>,
) -> std::result::Result<TrackMeta, ViewerError> {
    let state = ensure_state(&app);
    tokio::task::spawn_blocking(move || {
        let kind = TrackKind::detect(&path, kind_hint.as_deref())?;
        if !path.exists() {
            return Err(ViewerError::NotFound(path));
        }
        let large = file_is_large(&path)?;
        // L1: always memory index unless file is already bgzipped (tabix path wired in Task 19).
        let mem = MemoryIndex::load(&path, kind)?;
        let feature_count = mem.feature_count();
        let track_id = new_track_id();
        let meta = TrackMeta {
            track_id: track_id.clone(),
            kind,
            path: path.clone(),
            source: TrackSource::Memory,
            feature_count,
            suggest_bgzip: large,
            visible: true,
        };
        let mem = Arc::new(mem);
        {
            let mut s = state.session.lock().unwrap();
            s.search.add_track(&track_id, &mem);
            s.tracks.insert(track_id.clone(), TrackRuntime { meta: meta.clone(), memory: Some(mem) });
        }
        Ok::<_, ViewerError>(meta)
    })
    .await
    .map_err(|e| ViewerError::Parse(format!("join: {e}")))?
}

#[tauri::command]
pub async fn genome_viewer_remove_track<R: Runtime>(
    app: AppHandle<R>,
    track_id: TrackId,
) -> std::result::Result<(), ViewerError> {
    let state = ensure_state(&app);
    let mut s = state.session.lock().unwrap();
    s.tracks.remove(&track_id);
    s.search.remove_track(&track_id);
    Ok(())
}

#[tauri::command]
pub async fn genome_viewer_list_tracks<R: Runtime>(
    app: AppHandle<R>,
) -> std::result::Result<Vec<TrackMeta>, ViewerError> {
    let state = ensure_state(&app);
    let s = state.session.lock().unwrap();
    Ok(s.tracks.values().map(|t| t.meta.clone()).collect())
}

#[tauri::command]
pub async fn genome_viewer_fetch_reference_region<R: Runtime>(
    app: AppHandle<R>,
    chrom: String,
    start: u64,
    end: u64,
) -> std::result::Result<String, ViewerError> {
    let state = ensure_state(&app);
    let s = state.session.lock().unwrap();
    let handle = s.reference.as_ref().ok_or(ViewerError::NoReference)?;
    handle.fetch_region(&chrom, start, end)
}

#[tauri::command]
pub async fn genome_viewer_fetch_track_features<R: Runtime>(
    app: AppHandle<R>,
    track_id: TrackId,
    chrom: String,
    start: u64,
    end: u64,
) -> std::result::Result<Vec<Feature>, ViewerError> {
    let state = ensure_state(&app);
    let s = state.session.lock().unwrap();
    let track = s
        .tracks
        .get(&track_id)
        .ok_or_else(|| ViewerError::TrackNotFound(track_id.clone()))?;
    let mem = track.memory.as_ref().ok_or_else(|| {
        ViewerError::Parse("track has no memory index (tabix not yet wired)".into())
    })?;
    Ok(mem.query(&chrom, start, end).into_iter().cloned().collect())
}

#[tauri::command]
pub async fn genome_viewer_search_feature<R: Runtime>(
    app: AppHandle<R>,
    query: String,
    limit: usize,
) -> std::result::Result<Vec<SearchEntry>, ViewerError> {
    let state = ensure_state(&app);
    let s = state.session.lock().unwrap();
    Ok(s.search.search(&query, limit).into_iter().cloned().collect())
}

#[derive(Serialize)]
pub struct BgzipResult {
    pub new_path: PathBuf,
}

#[tauri::command]
pub async fn genome_viewer_bgzip_and_tabix<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
) -> std::result::Result<BgzipResult, ViewerError> {
    let kind = TrackKind::detect(&path, None)?;
    let app_for_emit = app.clone();
    let p = path.clone();
    let new_path = tokio::task::spawn_blocking(move || {
        bgzip::bgzip_and_tabix(&p, kind, |done, total| {
            let _ = app_for_emit.emit(
                "genome_viewer_index_progress",
                serde_json::json!({ "path": p.clone(), "done": done, "total": total }),
            );
        })
    })
    .await
    .map_err(|e| ViewerError::Parse(format!("join: {e}")))??;
    Ok(BgzipResult { new_path })
}

#[tauri::command]
pub async fn genome_viewer_get_session_state<R: Runtime>(
    app: AppHandle<R>,
) -> std::result::Result<Option<SerializedSession>, ViewerError> {
    let p = session_path(&app)?;
    load_session_from_disk(&p)
}

#[tauri::command]
pub async fn genome_viewer_save_session_state<R: Runtime>(
    app: AppHandle<R>,
    state: SerializedSession,
) -> std::result::Result<(), ViewerError> {
    let p = session_path(&app)?;
    save_session_to_disk(&p, &state)?;
    Ok(())
}
```

- [ ] **Step 2: Build**

Run: `cargo check -p rb-genome-viewer`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-genome-viewer/src/commands.rs
git commit -m "feat(genome-viewer): tauri command layer (reference/tracks/fetch/search/session)"
```

---

### Task 19: Register genome-viewer commands in rb-app

**Files:**
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: Add command names to generate_handler**

In `crates/rb-app/src/main.rs`, locate the `tauri::generate_handler![...]` macro invocation. Append these entries (alongside the fastq ones):

```rust
rb_genome_viewer::commands::genome_viewer_load_reference,
rb_genome_viewer::commands::genome_viewer_add_track,
rb_genome_viewer::commands::genome_viewer_remove_track,
rb_genome_viewer::commands::genome_viewer_list_tracks,
rb_genome_viewer::commands::genome_viewer_fetch_reference_region,
rb_genome_viewer::commands::genome_viewer_fetch_track_features,
rb_genome_viewer::commands::genome_viewer_search_feature,
rb_genome_viewer::commands::genome_viewer_bgzip_and_tabix,
rb_genome_viewer::commands::genome_viewer_get_session_state,
rb_genome_viewer::commands::genome_viewer_save_session_state,
```

- [ ] **Step 2: Build**

Run: `cargo check -p rb-app`
Expected: success.

- [ ] **Step 3: Full workspace check**

Run: `cargo check --workspace`
Expected: success.

- [ ] **Step 4: Full test run**

Run: `cargo test --workspace`
Expected: all pre-existing tests still pass; new fastq-viewer + genome-viewer tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-app/src/main.rs
git commit -m "feat(rb-app): register genome-viewer tauri commands"
```

---

## Phase 4 — Genome Viewer Frontend

### Task 20: Vendor igv.js

**Files:**
- Create: `frontend/vendor/igv/igv.esm.min.js`
- Create: `frontend/vendor/igv/LICENSE`
- Create: `frontend/vendor/igv/VERSION`

- [ ] **Step 1: Download pinned release**

Pick a specific igv.js release tag for reproducibility. Use v3.5.0 (current stable as of this plan; update if a newer one is preferred at implementation time).

```bash
mkdir -p frontend/vendor/igv
curl -fsSL -o frontend/vendor/igv/igv.esm.min.js \
  https://cdn.jsdelivr.net/npm/igv@3.5.0/dist/igv.esm.min.js
curl -fsSL -o frontend/vendor/igv/LICENSE \
  https://raw.githubusercontent.com/igvteam/igv.js/v3.5.0/LICENSE
echo "3.5.0" > frontend/vendor/igv/VERSION
```

- [ ] **Step 2: Sanity-check**

Run: `head -c 200 frontend/vendor/igv/igv.esm.min.js | grep -c '^' && wc -c frontend/vendor/igv/igv.esm.min.js`
Expected: a file ~1-2 MB in size.

Run: `grep -q "Apache License" frontend/vendor/igv/LICENSE && echo "license ok"`
Expected: "license ok"

- [ ] **Step 3: Commit**

```bash
git add frontend/vendor/igv
git commit -m "chore: vendor igv.js 3.5.0 for genome viewer utility"
```

---

### Task 21: Genome viewer adapter (Tauri readers for igv.js)

**Files:**
- Create: `frontend/js/utilities/genome-viewer/igv-adapter.js`

- [ ] **Step 1: Write adapter**

Create `frontend/js/utilities/genome-viewer/igv-adapter.js`:

```js
const api = window.__TAURI__?.core?.invoke
  ? (cmd, args) => window.__TAURI__.core.invoke(cmd, args)
  : async () => { throw new Error('tauri not available'); };

/**
 * igv.js expects Reader instances with a read(chr, start, end) method returning
 * a string of bases or an array of feature objects. We route those to our Rust backend.
 */
export class TauriReferenceReader {
  constructor({ path }) { this.path = path; }
  async readSequence(chr, start, end) {
    // igv.js uses 0-based start, exclusive end. Our Rust API uses 1-based inclusive.
    const rustStart = start + 1;
    const rustEnd = end;
    const seq = await api('genome_viewer_fetch_reference_region', {
      chrom: chr, start: rustStart, end: rustEnd,
    });
    return seq;
  }
}

export class TauriFeatureReader {
  constructor({ trackId, kind }) { this.trackId = trackId; this.kind = kind; }
  async readFeatures(chr, start, end) {
    const rustStart = start + 1;
    const rustEnd = end;
    const features = await api('genome_viewer_fetch_track_features', {
      trackId: this.trackId, chrom: chr, start: rustStart, end: rustEnd,
    });
    return features.map(f => ({
      chr: f.chrom,
      start: Number(f.start) - 1, // back to 0-based for igv.js
      end: Number(f.end),
      name: f.name || '',
      strand: f.strand || '.',
      type: f.kind,
      attributes: f.attrs || {},
    }));
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/js/utilities/genome-viewer/igv-adapter.js
git commit -m "feat(genome-viewer): tauri-backed readers for igv.js"
```

---

### Task 22: Genome viewer view — controls + mount

**Files:**
- Modify: `frontend/js/utilities/genome-viewer/view.js`
- Create: `frontend/js/utilities/genome-viewer/controls.js`

- [ ] **Step 1: Create controls module**

Create `frontend/js/utilities/genome-viewer/controls.js`:

```js
export function renderControls(container) {
  container.innerHTML = `
    <div class="gv-controls" style="display:flex;gap:8px;align-items:center;padding:8px;background:#f5f1eb;border-bottom:1px solid #e7e5e4">
      <button class="btn" data-act="gv-load-reference">Load Reference FASTA</button>
      <button class="btn" data-act="gv-add-track">Add Track</button>
      <input type="text" class="gv-search" placeholder="chr1:10-50  or  gene name" style="flex:1;padding:4px 8px;font-family:monospace">
      <button class="btn" data-act="gv-search-go">Go</button>
    </div>
    <div class="gv-track-list" style="padding:6px 8px;background:#faf8f4;border-bottom:1px solid #f1ede7;font-size:13px"></div>
    <div class="gv-browser" style="height:calc(100vh - 240px);background:#fff"></div>
  `;
}

export function renderTrackList(host, tracks, onRemove, onToggle) {
  if (!tracks.length) {
    host.innerHTML = '<span style="color:#a8a29e">No tracks loaded</span>';
    return;
  }
  host.innerHTML = tracks.map(t => `
    <span class="gv-track-chip" style="display:inline-flex;align-items:center;margin-right:8px;padding:2px 6px;border:1px solid #d6d3d1;border-radius:4px;background:#fff">
      <input type="checkbox" class="gv-track-toggle" data-track="${t.track_id}" ${t.visible ? 'checked' : ''} style="margin-right:4px">
      <span style="margin-right:4px">${escapeHtml(filename(t.path))} <span style="color:#a8a29e">(${t.kind}, ${t.feature_count})</span></span>
      <button class="gv-track-remove" data-track="${t.track_id}" style="background:none;border:none;color:#a8a29e;cursor:pointer">×</button>
    </span>
  `).join('');
  host.querySelectorAll('.gv-track-remove').forEach(b => {
    b.addEventListener('click', () => onRemove(b.dataset.track));
  });
  host.querySelectorAll('.gv-track-toggle').forEach(cb => {
    cb.addEventListener('change', () => onToggle(cb.dataset.track, cb.checked));
  });
}

function filename(p) {
  return String(p).split(/[\\/]/).pop();
}
function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
```

- [ ] **Step 2: Replace view.js with full wiring**

Replace `frontend/js/utilities/genome-viewer/view.js`:

```js
import igv from '/vendor/igv/igv.esm.min.js';
import { TauriReferenceReader, TauriFeatureReader } from './igv-adapter.js';
import { renderControls, renderTrackList } from './controls.js';

const api = window.__TAURI__?.core?.invoke
  ? (cmd, args) => window.__TAURI__.core.invoke(cmd, args)
  : async () => { throw new Error('tauri not available'); };

const state = {
  browser: null,
  reference: null,        // { path, chroms: [...] }
  tracks: [],             // TrackMeta[]
  position: null,
  saveTimer: null,
};

export async function renderGenomeViewerView(content) {
  content.innerHTML = `<div class="module-view genome-viewer" style="padding:0"></div>`;
  const root = content.querySelector('.genome-viewer');
  renderControls(root);
  const browserHost = root.querySelector('.gv-browser');
  const trackListHost = root.querySelector('.gv-track-list');

  root.addEventListener('click', (e) => {
    const act = e.target.closest('[data-act]')?.dataset.act;
    if (act === 'gv-load-reference') loadReference();
    if (act === 'gv-add-track') addTrack();
    if (act === 'gv-search-go') doSearch(root.querySelector('.gv-search').value.trim());
  });
  root.querySelector('.gv-search').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') doSearch(e.currentTarget.value.trim());
  });

  // Auto-restore.
  try {
    const saved = await api('genome_viewer_get_session_state');
    if (saved) await restoreSession(saved, browserHost, trackListHost);
  } catch (err) { console.warn('session restore failed', err); }

  async function loadReference() {
    const chosen = await api('select_files', { multiple: false });
    if (!chosen || !chosen[0]) return;
    const meta = await api('genome_viewer_load_reference', { path: chosen[0] });
    state.reference = meta;
    await createBrowser(browserHost, meta);
    scheduleSave();
  }

  async function addTrack() {
    const chosen = await api('select_files', { multiple: true });
    if (!chosen) return;
    for (const p of chosen) {
      try {
        const tm = await api('genome_viewer_add_track', { path: p, kindHint: null });
        state.tracks.push(tm);
        if (state.browser) {
          await state.browser.loadTrack({
            name: p.split(/[\\/]/).pop(),
            type: tm.kind === 'bed' ? 'annotation' : 'annotation',
            format: tm.kind === 'bed' ? 'bed' : tm.kind,
            reader: new TauriFeatureReader({ trackId: tm.track_id, kind: tm.kind }),
          });
        }
      } catch (err) {
        alert(`Failed to load ${p}: ${err?.message || err}`);
      }
    }
    refreshTrackList(trackListHost);
    scheduleSave();
  }

  async function doSearch(q) {
    if (!q || !state.browser) return;
    if (/^[\w.]+:\d+(?:[-,]\s*[\d,]+)?$/i.test(q)) {
      state.browser.search(q.replace(/,/g, ''));
      return;
    }
    const hits = await api('genome_viewer_search_feature', { query: q, limit: 1 });
    if (hits.length) {
      const h = hits[0];
      state.browser.search(`${h.chrom}:${h.start}-${h.end}`);
    }
  }

  function refreshTrackList(host) {
    renderTrackList(host, state.tracks, removeTrack, toggleTrack);
  }

  async function removeTrack(id) {
    await api('genome_viewer_remove_track', { trackId: id });
    state.tracks = state.tracks.filter(t => t.track_id !== id);
    if (state.browser) {
      const t = state.browser.findTracks(tr => tr.reader?.trackId === id);
      t.forEach(x => state.browser.removeTrack(x));
    }
    refreshTrackList(trackListHost);
    scheduleSave();
  }

  function toggleTrack(id, visible) {
    const t = state.tracks.find(t => t.track_id === id);
    if (t) t.visible = visible;
    if (state.browser) {
      const tr = state.browser.findTracks(tr => tr.reader?.trackId === id)[0];
      if (tr) tr.visible = visible;
      state.browser.update();
    }
    scheduleSave();
  }

  function scheduleSave() {
    clearTimeout(state.saveTimer);
    state.saveTimer = setTimeout(async () => {
      try {
        await api('genome_viewer_save_session_state', {
          state: {
            version: 1,
            reference: state.reference ? { path: state.reference.path } : null,
            tracks: state.tracks.map(t => ({ path: t.path, kind: t.kind, visible: t.visible })),
            position: state.position,
          },
        });
      } catch (err) { console.warn('session save failed', err); }
    }, 1000);
  }
}

async function createBrowser(host, meta) {
  host.innerHTML = '';
  const chrom0 = meta.chroms[0];
  const referenceConfig = {
    id: 'rustbrain-ref',
    name: meta.path.split(/[\\/]/).pop(),
    reader: new TauriReferenceReader({ path: meta.path }),
    chromosomes: meta.chroms.map(c => ({ name: c.name, bpLength: Number(c.length) })),
  };
  state.browser = await igv.createBrowser(host, {
    reference: referenceConfig,
    locus: `${chrom0.name}:1-${Math.min(10000, Number(chrom0.length))}`,
    showNavigation: true,
    showIdeogram: false,
    tracks: [],
  });
  state.browser.on('locuschange', (ref) => {
    if (ref && ref.chr) {
      state.position = { chrom: ref.chr, start: ref.start + 1, end: ref.end };
    }
  });
}

async function restoreSession(saved, browserHost, trackListHost) {
  if (!saved.reference) return;
  try {
    const meta = await api('genome_viewer_load_reference', { path: saved.reference.path });
    state.reference = meta;
    await createBrowser(browserHost, meta);
  } catch (e) { console.warn('ref restore failed', e); return; }

  for (const t of saved.tracks || []) {
    try {
      const tm = await api('genome_viewer_add_track', { path: t.path, kindHint: null });
      tm.visible = t.visible;
      state.tracks.push(tm);
      await state.browser.loadTrack({
        name: t.path.split(/[\\/]/).pop(),
        type: 'annotation',
        format: tm.kind,
        reader: new TauriFeatureReader({ trackId: tm.track_id, kind: tm.kind }),
      });
    } catch (e) { console.warn(`track restore skipped: ${t.path}`, e); }
  }
  state.tracks = state.tracks.filter(t => t.visible !== false);
  renderTrackList(trackListHost, state.tracks,
    async (id) => {
      await api('genome_viewer_remove_track', { trackId: id });
      state.tracks = state.tracks.filter(x => x.track_id !== id);
    },
    () => {}
  );
  if (saved.position) {
    state.position = saved.position;
    try { state.browser.search(`${saved.position.chrom}:${saved.position.start}-${saved.position.end}`); } catch {}
  }
}
```

Note: the import `import igv from '/vendor/igv/igv.esm.min.js'` works when served from the app's frontend root. If the Tauri asset resolution needs a relative path, change to `'../../vendor/igv/igv.esm.min.js'`. Verify in Step 3.

- [ ] **Step 3: Run dev app**

Run: `cd crates/rb-app && cargo tauri dev`

Manual test:
1. Sidebar → "Genome Viewer" → should show the controls bar and empty browser area.
2. Click "Load Reference FASTA" → pick `crates/rb-genome-viewer/testdata/tiny.fa`.
3. Confirm igv.js browser renders with chr1 visible.
4. Click "Add Track" → pick `tiny.gff3` → a track appears in the list and as a track row in the browser.
5. Search "BRCA" → navigates to chr1:10-50.
6. Close app, reopen → session restores automatically.

If `import igv from '/vendor/igv/...'` 404s, change to relative path `'../../vendor/igv/igv.esm.min.js'` and rebuild.

- [ ] **Step 4: Commit**

```bash
git add frontend/js/utilities/genome-viewer/
git commit -m "feat(genome-viewer): igv.js-backed view with tracks + search + session restore"
```

---

### Task 23: Large-file bgzip prompt

**Files:**
- Modify: `frontend/js/utilities/genome-viewer/view.js`

- [ ] **Step 1: Wire suggest_bgzip dialog into addTrack**

In `view.js`'s `addTrack`, after the successful `genome_viewer_add_track` call, check `tm.suggest_bgzip`:

```js
if (tm.suggest_bgzip) {
  const ok = confirm(
    `${p.split(/[\\/]/).pop()} is large (>200 MB). Build bgzip + tabix index for faster future opens?\n` +
    `This will create a .gz file next to the original (the original is preserved).`
  );
  if (ok) {
    try {
      const res = await api('genome_viewer_bgzip_and_tabix', { path: p });
      console.log(`bgzipped: ${res.new_path}`);
    } catch (err) {
      alert(`bgzip failed: ${err?.message || err}`);
    }
  }
}
```

(Listener for `genome_viewer_index_progress` can be added later if users want a progress bar; L1 ships with the confirm dialog only.)

- [ ] **Step 2: Commit**

```bash
git add frontend/js/utilities/genome-viewer/view.js
git commit -m "feat(genome-viewer): prompt to bgzip+tabix large annotation files"
```

---

## Phase 5 — Integration & Polish

### Task 24: Integration test in rb-app

**Files:**
- Create: `crates/rb-app/tests/utilities_integration.rs`

- [ ] **Step 1: Write integration test**

Create `crates/rb-app/tests/utilities_integration.rs`:

```rust
// Lightweight integration test: exercise rb-genome-viewer and rb-fastq-viewer
// library-level APIs end-to-end without spinning up Tauri.

use std::path::PathBuf;

fn gv_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../rb-genome-viewer/testdata")
        .join(name)
}

fn fq_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../rb-fastq-viewer/testdata")
        .join(name)
}

#[test]
fn genome_viewer_end_to_end() {
    use rb_genome_viewer::reference::ReferenceHandle;
    use rb_genome_viewer::index::MemoryIndex;
    use rb_genome_viewer::tracks::TrackKind;
    use rb_genome_viewer::search::SearchIndex;

    let (handle, meta) = ReferenceHandle::load(&gv_fixture("tiny.fa")).unwrap();
    assert_eq!(meta.chroms.len(), 2);

    let mem = MemoryIndex::load(&gv_fixture("tiny.gtf"), TrackKind::Gtf).unwrap();
    let mut search = SearchIndex::default();
    search.add_track(&"t1".to_string(), &mem);

    let hits = search.search("brca", 1);
    assert_eq!(hits.len(), 1);

    let seq = handle.fetch_region(&hits[0].chrom, hits[0].start, hits[0].end).unwrap();
    assert!(!seq.is_empty(), "fetched sequence should not be empty");
}

#[test]
fn fastq_viewer_end_to_end() {
    use rb_fastq_viewer::session::FastqSession;

    let cache = tempfile::tempdir().unwrap();
    let (session, _) = FastqSession::open(&fq_fixture("tiny.fastq"), cache.path()).unwrap();
    assert_eq!(session.index.total_records, 100);
    let recs = session.read_records(0, 5).unwrap();
    assert_eq!(recs.len(), 5);
    let hits = session.search_id("0042", 0, 1).unwrap();
    assert_eq!(hits.len(), 1);
}
```

- [ ] **Step 2: Add tempfile dev-dep**

Edit `crates/rb-app/Cargo.toml`, under `[dev-dependencies]` add (if not present):

```toml
tempfile = "3"
```

- [ ] **Step 3: Run**

Run: `cargo test -p rb-app --test utilities_integration`
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-app/tests/utilities_integration.rs crates/rb-app/Cargo.toml
git commit -m "test(rb-app): end-to-end integration for genome viewer + fastq viewer"
```

---

### Task 25: Workspace lint + format pass

**Files:** (no new files)

- [ ] **Step 1: Run clippy on new crates**

```bash
RUSTFLAGS="--cap-lints=warn" cargo clippy -p rb-genome-viewer -p rb-fastq-viewer -p rb-app -- -D warnings
```
Expected: no warnings/errors. If clippy flags anything, fix it — these are new crates so there's no grandfathered code.

- [ ] **Step 2: Run format**

```bash
cargo fmt -p rb-genome-viewer -p rb-fastq-viewer -p rb-app
```

- [ ] **Step 3: Confirm no changes left**

Run: `git diff --stat`
If there are diffs, review them and commit.

- [ ] **Step 4: Commit**

```bash
git add -u
git diff --cached --quiet || git commit -m "style: clippy + fmt pass for new utility crates"
```

---

### Task 26: Manual regression checklist

**Files:** none (manual verification)

- [ ] **Step 1: Launch dev app**

Run: `cd crates/rb-app && cargo tauri dev`

- [ ] **Step 2: Walk through every existing module**

Confirm none of these existing views regressed:
- Dashboard renders
- QC Analysis view opens
- Trimming view opens
- STAR Align, STAR Index, GFF Convert, Differential Expression open
- Chat opens
- Settings opens
- Plots opens

- [ ] **Step 3: Walk through new utilities**

- Sidebar shows "Utilities" section between Analysis Pipeline and System.
- Genome Viewer: load tiny.fa → load tiny.gff3 → navigate to BRCA → close → reopen app → session restored.
- FASTQ Viewer: open tiny.fastq → scroll 100 records → search `0042` → jump works → close → reopen with no state (expected; FASTQ viewer doesn't persist file path in L1).

- [ ] **Step 4: Commit (no code changes; note in a tag or skip)**

No commit needed. Note completion in the PR description when opening the review.

---

## Out of scope for this plan (future follow-ups)

- **L2 — BAM alignment tracks.** Covered in spec section "L2 extension points"; requires a new `TauriAlignmentReader` and `noodles-sam`/`noodles-bam` deps.
- **Tabix reader path for existing `.gz.tbi` files.** `TrackSource::Tabix` is defined but not wired — new tracks always use memory index in L1. If a user's workflow produces bgzipped annotations and they want to skip the 200 MB threshold, they can still open the `.gz.tbi` — but `add_track` currently falls through to memory load. A follow-up task will add the tabix read path in `index.rs` and wire it when `<path>.tbi` exists.
- **FASTQ viewer session persistence.** Current FASTQ viewer doesn't restore last-opened file. Trivial to add (store last path in app data); deferred to keep this plan bounded.
- **Named sessions (L3).** Spec explicitly non-goal.
- **VCF tracks.** Spec non-goal.
- **i18n strings.** Sidebar labels use English fallback; `data-i18n` keys are set in the HTML but the translation bundles aren't updated yet. Add Chinese strings alongside in a follow-up matching the project's i18n pattern.

---

## Summary

**26 tasks** delivering:
- A new `UTILITIES` framework parallel to `MODULES` — reusable for future tools (BED intersect viewer, VCF summary, etc.)
- `rb-fastq-viewer` — sparse-indexed FASTQ pager with virtualized rendering and Phred coloring
- `rb-genome-viewer` — L1 lightweight IGV (FASTA + GFF/GTF/BED), igv.js-backed, session-persisted, with L2 extension hooks

Dependency graph: Phase 1 must complete first. Phases 2 and 3 are independent and can be interleaved. Phase 4 depends on Phase 3. Phase 5 depends on Phases 2, 3, 4.

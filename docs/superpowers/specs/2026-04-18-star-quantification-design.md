# STAR Quantification Modules — Design

**Date:** 2026-04-18
**Status:** Approved for planning
**Tooling:** [STAR_rs](https://github.com/AI4S-YB/STAR_rs) (Rust port of STAR RNA-seq aligner)

## Goal

Add STAR-based RNA-seq quantification to rust_brain. Given a reference genome FASTA, GTF annotation, and FASTQ reads (SE or PE), produce per-sample alignments, per-gene counts, and a merged counts matrix ready for DESeq2.

## Scope decisions (answered during brainstorming)

| Decision | Choice | Rationale |
|---|---|---|
| Module boundary | Two modules: `rb-star-index` + `rb-star-align` | Matches single-responsibility style of existing modules; index reusable across runs |
| Integration method | CLI subprocess (like rb-trimming) | STAR_rs library API is not documented; process isolation safer for large memory allocations |
| Binary discovery | Settings file + PATH fallback + explicit error hint | User-friendly without CI packaging complexity; rb-trimming upgraded to same mechanism |
| User-facing params | Minimal set + `extra_args` escape hatch per module | Supports future AI parameter suggestion; power users can tune any flag |
| Progress granularity | Sample-level (`i/N`) | STAR stderr format is unstable across versions; streaming logs compensate |
| Streaming logs | Yes, as a generic `RunEvent::Log` channel | Benefits all modules; long-running STAR otherwise feels frozen |
| Counts matrix handoff | `rb-star-align` auto-emits `counts_matrix.tsv` | STAR-specific merge logic; one-module closure; avoids cross-run coordination |
| Strand default | `unstranded` | Safest default; works for all library types |

---

## 1. Crate layout

New workspace members:

```
crates/
  rb-star-index/    # genomeGenerate
  rb-star-align/    # alignReads + counts merge
```

Both depend on `rb-core` only. STAR_rs is **not** added as a git submodule — users install the `star` binary separately.

Each crate has an internal helper module (initially duplicated; can promote to rb-core later if a fourth consumer appears) for:

- Resolving the `star` binary via `BinaryResolver` (see §3).
- Running the subprocess with streaming stderr/stdout forwarding and cooperative cancellation.

## 2. rb-core changes

### 2.1 `RunEvent` replaces `Progress` in the module channel

```rust
pub enum RunEvent {
    Progress { fraction: f64, message: String },
    Log { line: String, stream: LogStream },
}

pub enum LogStream { Stdout, Stderr }
```

`Module::run` signature becomes:

```rust
async fn run(
    &self,
    params: &serde_json::Value,
    project_dir: &Path,
    events_tx: mpsc::Sender<RunEvent>,
    cancel: CancellationToken,
) -> Result<ModuleResult, ModuleError>;
```

The Runner forwards:

- `RunEvent::Progress` → Tauri event `"run-progress"` (unchanged payload shape).
- `RunEvent::Log` → Tauri event `"run-log"` (new; `{run_id, line, stream}`).

Existing adapters (rb-qc, rb-trimming, rb-deseq2) migrate mechanically: every `Progress { ... }` becomes `RunEvent::Progress { ... }`; they ignore the `cancel` arg for now (rb-trimming gets real cancellation in §3).

### 2.2 Cooperative cancellation

Add `tokio-util` dep for `CancellationToken`. Runner's `cancel_run` command:

1. Calls `token.cancel()` on the run's token.
2. After a short grace period, still calls `JoinHandle::abort()` as a safety net.

Subprocess-based adapters use `tokio::select!` or polling to detect cancellation and call `child.kill().await` before returning `ModuleError::Cancelled`. Library-based adapters (qc, deseq2) can't interrupt mid-computation in `spawn_blocking`; they check the token between files/stages, which is acceptable given their granularity.

### 2.3 `BinaryResolver` and settings

New module `rb_core::binary`:

```rust
pub struct BinaryResolver { settings_path: PathBuf, overrides: HashMap<String, PathBuf> }

impl BinaryResolver {
    pub fn load() -> Self;                        // reads settings.json
    pub fn resolve(&self, name: &str) -> Result<PathBuf, BinaryError>;
    pub fn set(&mut self, name: &str, path: PathBuf) -> Result<(), BinaryError>;
    pub fn clear(&mut self, name: &str) -> Result<(), BinaryError>;
    pub fn list_known(&self) -> Vec<BinaryStatus>; // {name, configured, detected_on_path}
}

pub enum BinaryError {
    NotFound { name: String, searched: Vec<String>, hint: String },
    NotExecutable(PathBuf),
    SettingsIo(std::io::Error),
}
```

Resolution order: configured override → `which(name)` → `NotFound` with install hint.

Settings file location (using the `directories` crate for cross-platform):

- Linux: `~/.config/rust_brain/settings.json`
- macOS: `~/Library/Application Support/rust_brain/settings.json`
- Windows: `%APPDATA%\rust_brain\settings.json`

Format:

```json
{
  "binary_paths": {
    "star": "/opt/star_rs/target/release/star",
    "cutadapt-rs": null
  }
}
```

Known binaries are registered via a static table in rb-core (name → install hint). Adding a binary means adding one entry there.

## 3. `rb-star-index` module

**id:** `"star_index"` · **name:** `"STAR Genome Index"`

### Parameters

| Field | Type | Required | Default | Notes |
|---|---|---|---|---|
| `genome_fasta` | string | ✅ | — | FASTA path |
| `gtf_file` | string | ✅ | — | GTF annotation |
| `threads` | u32 | — | 4 | `--runThreadN` |
| `sjdb_overhang` | u32 | — | 100 | `--sjdbOverhang` (= readLen - 1) |
| `genome_sa_index_nbases` | u32 | — | 14 | `--genomeSAindexNbases`; small genomes require lower |
| `extra_args` | string[] | — | `[]` | Raw token list appended to command line |

### `validate()`

- Both required files exist and are readable.
- `extra_args` is an array of strings.
- Resolver can locate `star` (surfaced as field `"binary"` error if not).

### `run()` flow

1. Output dir: `{project}/runs/star_index_{uuid8}/`.
2. Build command:
   ```
   star --runMode genomeGenerate
        --genomeDir <outDir>
        --genomeFastaFiles <genome_fasta>
        --sjdbGTFfile <gtf_file>
        --runThreadN <threads>
        --sjdbOverhang <sjdb_overhang>
        --genomeSAindexNbases <genome_sa_index_nbases>
        [extra_args...]
   ```
3. Spawn child, forward stderr lines as `RunEvent::Log { stream: Stderr }`; emit `Progress { 0.0, "Starting genome generation" }` once at start, `Progress { 1.0, "Done" }` on completion.
4. Poll cancellation token; on cancel, call `child.kill()` and return `Cancelled`.
5. On success, verify key artifacts exist: `SA`, `SAindex`, `Genome`, `chrNameLength.txt`, `geneInfo.tab`. Missing artifacts → `ToolError`.

### Result

```json
{
  "output_files": [
    "<outDir>/SA",
    "<outDir>/SAindex",
    "<outDir>/Genome",
    "<outDir>/chrNameLength.txt",
    "<outDir>/geneInfo.tab",
    "<outDir>/Log.out"
  ],
  "summary": {
    "genome_dir": "<absolute path>",
    "genome_fasta": "...",
    "gtf_file": "...",
    "threads": 4,
    "sjdb_overhang": 100,
    "genome_sa_index_nbases": 14,
    "index_size_bytes": 12345678901,
    "generation_seconds": 1234
  },
  "log": "<full stderr + Log.out contents>"
}
```

## 4. `rb-star-align` module

**id:** `"star_align"` · **name:** `"STAR Alignment & Quantification"`

### Parameters

| Field | Type | Required | Default | Notes |
|---|---|---|---|---|
| `genome_dir` | string | ✅ | — | Path to a STAR index (rb-star-index output or external) |
| `reads_1` | string[] | ✅ | — | Per-sample R1 FASTQ paths |
| `reads_2` | string[] | — | `null` | Per-sample R2 paths; when present, length must equal `reads_1`; triggers PE mode |
| `sample_names` | string[] | — | inferred | When omitted, derived from R1 filename (strip `.fastq`, `.fq`, `.gz`, trailing `_R1` / `_1`) |
| `threads` | u32 | — | 4 | `--runThreadN` |
| `strand` | string | — | `"unstranded"` | One of `unstranded` / `forward` / `reverse`; selects counts column for matrix |
| `extra_args` | string[] | — | `[]` | Raw tokens appended to every per-sample command |

### `validate()`

- `genome_dir` exists and contains an `SA` file (sniff check).
- `reads_1` non-empty; every path exists.
- If `reads_2` present, length matches `reads_1` and every path exists.
- If `sample_names` present, length matches and names are unique, non-empty, `[A-Za-z0-9_.-]+`.
- `strand` is in the allowed enum.
- `extra_args` is a string array.
- Resolver can locate `star`.

### `run()` flow

Sample-level progress (`fraction = i / N`):

```
for i, sample in enumerate(samples):
    emit Progress { i / N, f"Aligning {sample.name} ({i+1}/{N})" }
    outDir = {run_dir}/{sample.name}/
    mkdir outDir
    readFilesIn = [sample.r1] + ([sample.r2] if PE)
    readFilesCommand = "zcat" if any path ends in .gz else None
    cmd = star --runMode alignReads
              --genomeDir {genome_dir}
              --readFilesIn {readFilesIn}
              [--readFilesCommand {readFilesCommand}]
              --outFileNamePrefix {outDir}/
              --runThreadN {threads}
              --quantMode GeneCounts
              --outSAMtype BAM Unsorted
              [extra_args...]
    run subprocess; stream stderr → RunEvent::Log; poll cancel → kill
    parse outDir/Log.final.out → stats
    parse outDir/ReadsPerGene.out.tab → summary rows + counts column

emit Progress { 1.0, "Merging counts matrix" }
merge counts matrix → {run_dir}/counts_matrix.tsv
emit Progress { 1.0, "Done" }
```

### Counts matrix merge

- Read each sample's `ReadsPerGene.out.tab`; the first 4 lines are summary rows (`N_unmapped`, `N_multimapping`, `N_noFeature`, `N_ambiguous`); remaining rows are gene-level counts (4 columns: geneId, unstranded, forward, reverse).
- Extract the column indexed by `strand`.
- Build `BTreeMap<geneId, Vec<u64>>` spanning the union of geneIds across samples; missing samples for a given gene default to `0` (STAR should produce identical gene sets for a given index, but we tolerate mismatches).
- Write TSV: first row `gene_id\t<sample1>\t<sample2>\t...`; one row per gene.

### `Log.final.out` parsing

Extract and surface (tolerant to missing keys):

- Number of input reads
- Uniquely mapped reads number + %
- Number of reads mapped to multiple loci (+ %)
- Unmapped reads (too short / too many mismatches / other) — summed
- Average mapped length, splice counts (optional)

### Result

```json
{
  "output_files": [
    "<run_dir>/<sample>/Aligned.out.bam",
    "<run_dir>/<sample>/ReadsPerGene.out.tab",
    "<run_dir>/<sample>/Log.final.out",
    "<run_dir>/counts_matrix.tsv"
  ],
  "summary": {
    "run_dir": "<absolute path>",
    "counts_matrix": "<run_dir>/counts_matrix.tsv",
    "strand": "unstranded",
    "genome_dir": "...",
    "samples": [
      {
        "name": "S1",
        "r1": "...",
        "r2": null,
        "status": "ok",
        "bam": "...",
        "reads_per_gene": "...",
        "log_final": "...",
        "stats": {
          "input_reads": 10000000,
          "uniquely_mapped": 9000000,
          "uniquely_mapped_pct": 90.0,
          "multi_mapped": 500000,
          "multi_mapped_pct": 5.0,
          "unmapped": 500000,
          "unmapped_pct": 5.0,
          "n_unmapped": 12,
          "n_multimapping": 34,
          "n_nofeature": 56,
          "n_ambiguous": 78
        }
      }
    ]
  },
  "log": "<concatenated per-sample stderr>"
}
```

## 5. rb-app wiring

`main.rs`:

```rust
registry.register(Arc::new(rb_star_index::StarIndexModule));
registry.register(Arc::new(rb_star_align::StarAlignModule));
```

New Tauri commands in `commands/settings.rs`:

- `get_binary_paths()` → `Vec<BinaryStatus>`
- `set_binary_path(name, path)` → `Result<(), String>`
- `clear_binary_path(name)` → `Result<(), String>`

Adapters call `BinaryResolver::load()` at `run()` start — settings are global and load is cheap (single JSON file). No change to `Module::run` signature is required to thread a resolver through. `AppState` still holds an `Arc<Mutex<BinaryResolver>>` for the Tauri settings commands to read/write, but adapters do not consume that instance.

**rb-trimming upgrade**: replace `Command::new("cutadapt-rs")` with `resolver.resolve("cutadapt-rs")?` in the same PR as the rb-core changes; keeps the three subprocess-based adapters symmetric.

## 6. Frontend

### Navigation

Sidebar adds a group **"Alignment & Quantification"** with two items:
- `#star-index` — STAR Index
- `#star-align` — STAR Alignment

Plus a new top-level `#settings` entry (gear icon) for the binary paths view.

### Forms

Both STAR forms follow the "minimal params + Advanced" pattern. The **Advanced** section is a collapsed details block with one textarea for `extra_args` (one token per line; split and trim on submit).

**`#star-index`**: file picker for genome FASTA, file picker for GTF, number inputs for threads / sjdbOverhang / genomeSAindexNbases, Advanced block.

**`#star-align`**: directory picker for genomeDir (with a "pick from recent star_index runs" shortcut), multi-file picker for reads_1, multi-file picker for reads_2 (optional), optional sample_names textarea, threads input, strand radio group, Advanced block.

### Runtime UI

Every module's run view gets a new **"Log"** collapsible panel:

- `<pre>` with auto-scroll-to-bottom (disabled if user has scrolled up manually).
- Listens to Tauri `run-log` events filtered by `run_id`.
- Caps at 500 lines client-side (drops oldest) to prevent DOM bloat.
- Shared component used by all module views.

Progress bar continues to be driven by `run-progress`.

### Results (star-align)

- **Mapping rate chart** (ECharts stacked bar): one bar per sample, segments for `uniquely_mapped`, `multi_mapped`, `unmapped`. Uses `ECHART_THEME` constant.
- **Counts matrix preview**: calls existing `read_table_preview` command with `{run_dir}/counts_matrix.tsv`, renders first 50 rows × first 10 columns as a simple table.
- **"Use this matrix in DESeq2"** button: navigates to `#differential`, passes `counts_matrix` path via frontend state so the DESeq2 form pre-fills. Pure frontend concern — no backend change.

### Results (star-index)

Single-run artifact; render summary JSON as a key/value table. No chart.

### Settings view

Table with columns: **Tool** / **Configured path** / **Detected on PATH** / **Status** / **Actions (browse / clear)**. Row per known binary (`star`, `cutadapt-rs`, …). Uses `get_binary_paths` and `set_binary_path`.

### Browser mock

`index.html` shim adds fixtures for `run_module("star_index")`, `run_module("star_align")`, `get_binary_paths`, `set_binary_path`, `clear_binary_path` so frontend-only development (no Rust backend) still works.

## 7. Testing

### Unit tests (no `star` binary required)

- `rb_core::binary::BinaryResolver`: settings load/save; resolve order (override → PATH → NotFound); hint includes known install command.
- `rb_core::RunEvent`: serde round-trip; Runner routes Progress vs Log to correct Tauri event.
- `rb_star_align::counts::merge_counts_matrix`: feed synthetic `ReadsPerGene.out.tab` files (with 4 summary lines + gene rows); verify strand column selection, gene union, missing-sample zero-fill, header ordering.
- `rb_star_align::log_final::parse`: golden `Log.final.out` fixture at `crates/rb-star-align/tests/fixtures/Log.final.out`; assert extracted stats.
- Both modules' `validate()`: required fields, path existence, length matching, strand enum, extra_args type.

### Integration tests (opt-in)

Gated on `STAR_BIN` env var so CI default skips:

```rust
#[test]
#[ignore = "requires STAR_BIN env var and star binary"]
fn integration_index_then_align() { ... }
```

Minimal test fixture under `crates/rb-star-align/tests/data/`: a ~10 kb chromosome FASTA, a hand-written GTF with a few exons, a small FASTQ (100 reads). Run index → align → assert `counts_matrix.tsv` generated and has the expected gene columns.

### Cancellation test

Unit test using a tiny helper binary (`/bin/sleep 30` or a fixture shell script) registered via `BinaryResolver` override; spawn the adapter, trigger cancel after 100 ms, assert `ModuleError::Cancelled` and process is reaped.

### Frontend

Manual testing of golden paths:
1. Run STAR index → verify progress + streaming logs.
2. Run STAR align with 2 PE samples → verify per-sample progress, mapping rate chart, counts matrix preview.
3. Click "Use this matrix in DESeq2" → verify navigation and form pre-fill.
4. Settings view: browse to a binary path, save, verify persistence after app restart.
5. Cancel a running index — verify STAR process is gone (check via `pgrep star`).

## 8. Implementation phases

Each phase is independently reviewable and testable. Plan granularity will refine further in the writing-plans step.

1. **rb-core infrastructure**
   - `RunEvent` enum + cancellation token plumbing (breaking change to `Module::run`).
   - `BinaryResolver` + settings.json.
   - Runner updates: `run-log` Tauri event; `cancel_run` uses token + abort.
   - Migrate rb-qc, rb-trimming, rb-deseq2 signatures (mechanical).
   - Upgrade rb-trimming to use `BinaryResolver`.
   - New Tauri commands: `get_binary_paths`, `set_binary_path`, `clear_binary_path`.

2. **rb-star-index**: crate scaffolding, Module impl, unit tests, registration.

3. **rb-star-align**: crate scaffolding, Module impl, counts merge, Log.final.out parsing, unit tests, registration.

4. **Frontend**
   - Log panel component (shared across modules).
   - Settings view.
   - STAR index / align views with Advanced extra_args.
   - Mapping rate chart, counts matrix preview.
   - DESeq2 handoff button.
   - Browser mock fixtures.

5. **Integration tests + docs**
   - Small FASTA/GTF/FASTQ fixture.
   - CI workflow: optional STAR_BIN job.
   - Update CLAUDE.md and README with STAR install notes.

## Open questions / risks

- **Memory pressure**: human-scale indexes need ~30 GB RAM during generation. No pre-flight check in MVP; STAR will OOM-kill and we surface the exit code in the error. Documented as a known limitation.
- **STAR_rs stability**: the binary is under active development; stderr format changes could break log parsing (not progress). We only parse `Log.final.out`, which follows the upstream STAR format and is stable.
- **Index reuse across projects**: out of scope for MVP. Users can point `genome_dir` at any pre-existing index path, which implicitly handles the use case without cross-project bookkeeping.
- **Windows**: subprocess behavior is fine, but STAR_rs Windows builds may not exist. Documentation will note the dependency; frontend Settings view works regardless.

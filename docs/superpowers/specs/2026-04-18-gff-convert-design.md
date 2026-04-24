# GFF Convert Module — Design

**Date:** 2026-04-18
**Status:** approved (brainstorm phase)
**Upstream tool:** [AI4S-YB/gffread_rs](https://github.com/AI4S-YB/gffread_rs) v0.1.0 — Rust rewrite of gpertea/gffread, aiming for byte-identical output.

## Problem

STAR's genome indexing step requires a **GTF** annotation, but users often have **GFF3** (Ensembl, NCBI, many plant / non-model-organism annotations ship GFF3 first). Today a user with only GFF3 is stuck — they have to step outside RustBrain, install `gffread` themselves, convert, and come back. The gap breaks the "everything in one app" promise for a common real-world starting point.

## Goals

- Add a first-class **GFF Converter** module that runs gffread-rs as a subprocess and produces a STAR-ready annotation file.
- Bidirectional — GFF3↔GTF, so the module's identity is "GFF-family format conversion", not "GFF3→GTF".
- Auto-wire the output to **STAR Index** via the existing prefill handoff pattern, so the common pipeline (GFF3 → convert → STAR index) is one click.
- Ship the `gffread-rs` binary bundled in released artifacts, consistent with how STAR_rs is bundled since v0.3.0.

## Non-goals

- FASTA sequence extraction (exon / CDS / protein). gffread-rs supports this; we intentionally defer it. Users who need it today can set `extra_args` with the appropriate `-w` / `-x` / `-y` flags — the tool will produce the files, they just won't be surfaced in the UI.
- Any GFF parsing or validation in Rust. The module is a thin adapter; gffread-rs itself is the parser / validator / oracle.
- Multi-input batch conversion. Single file in, single file out. Users with many annotations run the module multiple times.

## Architecture

### Crate layout

New workspace member: `crates/rb-gff-convert/`, patterned after `rb-star-index`:

```
crates/rb-gff-convert/
├── Cargo.toml
└── src/
    ├── lib.rs         # GffConvertModule + Module trait impl, validate(), run()
    └── subprocess.rs  # tokio::process::Command wrapper, stdout/stderr → RunEvent::Log,
                       # cooperative cancel via tokio::select! on child.wait() vs cancel.cancelled()
```

### Binary discovery

Reuse `rb_core::binary::BinaryResolver`. Add a new entry to `KNOWN_BINARIES` in `rb-core/src/binary.rs`:

```rust
KnownBinary {
    id: "gffread-rs",
    display_name: "gffread-rs",
    install_hint: "Prebuilt binaries at https://github.com/AI4S-YB/gffread_rs/releases — drop on PATH or set the path in Settings.",
},
```

Resolution order stays override → bundled → PATH (per v0.3.0).

### Module registration

In `rb-app/src/main.rs`:

```rust
registry.register(Arc::new(rb_gff_convert::GffConvertModule));
```

In the Tauri setup hook, extend the sidecar-registration code to also register `binaries/gffread-rs[.exe]` when it exists. The existing `register_bundled_star` function in `crates/rb-app/src/main.rs` should be generalized into a helper that takes the binary id (`"star"`, `"gffread-rs"`) and the filename stem, then called once per bundled binary. Keeping the logic in one helper avoids copy-paste drift as more sidecars are added.

## Module interface

### `Module` impl

| Method | Value |
|---|---|
| `id()` | `"gff_convert"` |
| `name()` | `"GFF Converter"` |

### Params (JSON)

```json
{
  "input_file": "/abs/path/anno.gff3",
  "target_format": "gtf",
  "extra_args": ["--keep-comments"]
}
```

- `input_file` — required, must exist, any file extension (gffread-rs auto-detects GFF3 vs GTF by content).
- `target_format` — required, one of `"gtf"` | `"gff3"`.
- `extra_args` — optional, array of strings. Not validated beyond "is an array of strings"; gffread-rs itself rejects unknown flags.

### `validate()`

Returns `Vec<ValidationError>` with entries for:
- `input_file` missing / empty / not a file on disk
- `target_format` missing / not in `{gtf, gff3}`
- `extra_args` present but not an array, or contains non-string elements
- `binary` — if `BinaryResolver::load()?.resolve("gffread-rs")` fails, surface that error at validate time (mirrors rb-trimming behaviour so the UI can show "Missing binary" immediately instead of at run time)

### `run()`

1. Re-run `validate()`; bail with `ModuleError::InvalidParams` if dirty.
2. Resolve binary via `BinaryResolver`.
3. Compute output path: `{run_dir}/{input_stem}.{gtf|gff3}` where `input_stem` = `Path::file_stem(input_file)`.
4. Assemble argv:
   - Always: `[<input_file>]`
   - If `target_format == "gtf"`: append `["-T", "-o", <output>]`
   - Else: append `["-o", <output>]`
   - Append all `extra_args`
5. Spawn via `tokio::process::Command` with piped stdout+stderr; stream each line as `RunEvent::Log { line, stream }`.
6. `tokio::select!` on `child.wait()` vs `cancel.cancelled()`; on cancel → `child.kill().await` → return `ModuleError::Cancelled`.
7. On success, verify output file exists and is non-empty (gffread-rs can exit 0 while writing zero bytes if the input has no records). If empty, return `ModuleError::ToolError("gffread-rs produced no output records — check input file validity")`.
8. Emit `RunEvent::Progress { fraction: 1.0, message: "Done" }` and return `ModuleResult`:

```rust
ModuleResult {
    output_files: vec![output_path],
    summary: json!({
        "input": input_file,
        "output": output_path,
        "target_format": target_format,
        "input_bytes": input_size,
        "output_bytes": output_size,
        "elapsed_ms": elapsed,
        "stderr_tail": last_16_stderr_lines.join("\n"),
    }),
    log: "".into(),  // live streaming handles the log; nothing extra here
}
```

### Progress reporting

gffread-rs doesn't emit structured progress. We emit only `fraction: 0.0` on start and `fraction: 1.0` on done — the log pane carries the visible feedback. (Same approach as rb-trimming.)

## Frontend

### New view

Route: `#gff_convert`. Sidebar nav order becomes **QC → Trimming → GFF Convert → STAR Index → STAR Align → DESeq2**.

Form (`renderGffConvert`):

- **Input file** — file picker (single file, filters `*.gff`, `*.gff3`, `*.gtf`, and `*.*`)
- **Target format** — `<select>` with two options, `GTF` and `GFF3`
- **Extra args** — optional `<textarea>`, parsed as either a JSON array or whitespace-separated tokens
- **Run** button → `invoke('run_module', { moduleId: 'gff_convert', params })`

### Result card (`renderGffConvertResult`)

Shown after the run finishes. Contents:

- Input / output paths (escaped for XSS safety — use the existing `escapeHtml` helper)
- `target_format`, byte sizes, elapsed time
- Last 16 stderr lines (collapsible)
- Button: **"Use in STAR Index"**

### Handoff button behaviour

```js
state.prefill = state.prefill || {};
state.prefill.star_index = { gtf_file: result.output_files[0] };
location.hash = '#star_index';
```

`renderStarIndex` gets a matching clause mirroring the existing DESeq2 handoff exactly (see `renderDifferential` at `frontend/js/app.js:714`): on entry, read `(state.prefill && state.prefill.star_index) || {}`, then reset `state.prefill = {}`, then use the prefill to seed the form. Using the same idiom keeps prefill semantics consistent across modules.

### Log panel

Use the same `log-{moduleId}-{runId}` panel scheme currently in use. No changes to the log plumbing.

## Packaging — CI bundling

`.github/workflows/ci.yml`, inside the `build-and-release` job, alongside the existing STAR download step, add:

```yaml
- name: Download bundled gffread_rs binary
  shell: bash
  env:
    GFFREAD_VERSION: v0.1.0
  run: |
    case "${{ runner.os }}" in
      Linux)
        URL="https://github.com/AI4S-YB/gffread_rs/releases/download/${GFFREAD_VERSION}/gffread-rs-${GFFREAD_VERSION}-x86_64-unknown-linux-gnu.tar.gz"
        curl -fsSL "$URL" | tar xz -C crates/rb-app/binaries
        chmod +x crates/rb-app/binaries/gffread-rs
        ;;
      macOS)
        URL="https://github.com/AI4S-YB/gffread_rs/releases/download/${GFFREAD_VERSION}/gffread-rs-${GFFREAD_VERSION}-aarch64-apple-darwin.tar.gz"
        curl -fsSL "$URL" | tar xz -C crates/rb-app/binaries
        chmod +x crates/rb-app/binaries/gffread-rs
        ;;
      Windows)
        URL="https://github.com/AI4S-YB/gffread_rs/releases/download/${GFFREAD_VERSION}/gffread-rs-${GFFREAD_VERSION}-x86_64-pc-windows-msvc.zip"
        curl -fsSL -o /tmp/gffread.zip "$URL"
        unzip -o -d crates/rb-app/binaries /tmp/gffread.zip
        ;;
    esac
```

Assumption: gffread_rs tarballs are flat (same layout as STAR_rs tarballs, which we verified to be a single `star`/`star.exe` at root). The plan step should verify this before scripting the copy; if the layout is nested, use a `find` / glob. Bundle size overhead: ~400 KB per platform — negligible.

The local `.gitignore` rule `crates/rb-app/binaries/*` (with `!.gitkeep` exception) already covers the new binary.

## Testing

Following the STAR pattern:

- **`rb-gff-convert/src/lib.rs` unit tests** — `validate()` coverage:
  - missing `input_file`
  - non-existent `input_file` on disk
  - missing `target_format`
  - `target_format` not in `{gtf, gff3}`
  - `extra_args` is an object / number / array-with-non-strings
- **CLI-assembly unit test** — pure function `build_argv(params, binary, output) -> Vec<OsString>`; assert exact argv for GFF3→GTF, GTF→GFF3, and with `extra_args`
- **`tests/integration_smoke.rs`** — `end_to_end_gff3_to_gtf`, opt-in via `GFFREAD_BIN` env var (mirrors STAR's `STAR_BIN`). Uses a tiny ~10-line GFF3 fixture under `tests/data/anno.gff3`, asserts converted file exists, has non-zero bytes, and contains at least one GTF attribute token like `transcript_id`.
- **`rb-core/src/binary.rs` unit test** — extend the existing `list_known_contains_all_registered` test to cover the new `gffread-rs` id.

Happy path tests run unconditionally; the integration smoke test is silent-skip when `GFFREAD_BIN` is unset. CI doesn't currently export either `STAR_BIN` or `GFFREAD_BIN`; this is intentional — the download step for bundling lives in the `build-and-release` job, not `check`, to keep the fast-feedback check job under a minute.

## Error surfaces

| Scenario | Module behaviour | UI behaviour |
|---|---|---|
| Input file doesn't exist | `ValidationError { field: "input_file", ... }` at validate | Form error banner; Run button disabled |
| `gffread-rs` binary missing (no override, no bundle, not on PATH) | `ValidationError { field: "binary", message: install_hint }` | Run button enabled but shows "Missing binary — see Settings" inline |
| gffread-rs exits non-zero | `ModuleError::ToolError("gffread-rs exited with status N")` with `stderr_tail` in summary | Result card shows ❌ with stderr tail |
| gffread-rs exits 0 but output is empty | `ModuleError::ToolError("gffread-rs produced no output records — check input file validity")` | Result card shows ❌ with the message |
| User cancels mid-run | `ModuleError::Cancelled`; subprocess killed via `child.kill().await` | Run status → `Cancelled`, no handoff button |

## Version & release

First release with this module: **v0.4.0** (new user-visible module → minor bump).
Both bundled binaries pinned: STAR_rs v0.3.1, gffread_rs v0.1.0. Future bumps of either tracked by a single-line CI diff.

## Open questions — none

All decisions from brainstorming:
- Scope: GFF↔GTF conversion only; FASTA extraction deferred
- Direction: bidirectional via `target_format` param
- Handoff: auto-prefill STAR Index via `state.prefill`
- Bundling: yes, same pattern as STAR (v0.3.0 mechanism)

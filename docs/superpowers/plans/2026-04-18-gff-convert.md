# GFF Convert Module — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `rb-gff-convert` module that wraps `gffread-rs` to convert GFF3↔GTF, auto-prefills the STAR Index form with the converted file, and ships the `gffread-rs` binary bundled in released artifacts (same mechanism STAR uses since v0.3.0).

**Architecture:** New workspace crate `rb-gff-convert`, subprocess adapter built directly on top of the patterns already used by `rb-star-index` (tokio spawn, mpsc `RunEvent` streaming, cooperative `CancellationToken`). `BinaryResolver` gains a new registered id (`gffread-rs`). A small refactor turns the existing hardcoded `register_bundled_star` into a reusable `register_bundled(app, binary_id, filename_stem)` helper, called once per bundled sidecar. Frontend adds a new view + reuses the existing DESeq2 prefill handoff pattern for pushing the converted GTF into STAR Index.

**Tech Stack:** Rust 1.75+, tokio, `tokio::process::Command`, serde_json, Tauri v2, vanilla JS frontend.

**Upstream binary:** [AI4S-YB/gffread_rs v0.1.0](https://github.com/AI4S-YB/gffread_rs/releases/tag/v0.1.0) — Rust rewrite of gpertea/gffread. Binary name `gffread-rs` (with the dash). CLI is gffread-compatible: `gffread-rs <input> [-T] -o <output>` — `-T` emits GTF, absence emits GFF3, input format auto-detected.

---

## File Structure

**Created:**

- `crates/rb-gff-convert/Cargo.toml` — package definition, deps on rb-core + tokio + serde_json + async-trait + thiserror
- `crates/rb-gff-convert/src/lib.rs` — `GffConvertModule`, `Module` impl, `TargetFormat` enum, `build_argv`, `validate()`
- `crates/rb-gff-convert/src/subprocess.rs` — `run_with_streaming()` helper: spawn, stream stdout/stderr as `RunEvent::Log`, cooperative cancel via `tokio::select!`, mirrors `crates/rb-star-index/src/subprocess.rs`
- `crates/rb-gff-convert/tests/data/anno.gff3` — 5-line GFF3 fixture used by integration test
- `crates/rb-gff-convert/tests/integration_smoke.rs` — opt-in end-to-end test, skips unless `GFFREAD_BIN` env var is set

**Modified:**

- `Cargo.toml` (workspace root) — add `"crates/rb-gff-convert"` to `members`
- `crates/rb-core/src/binary.rs` — add `"gffread-rs"` entry to `KNOWN_BINARIES`, extend the `list_known_contains_all_registered` test
- `crates/rb-app/Cargo.toml` — add `rb-gff-convert = { path = "../rb-gff-convert" }` and register in main
- `crates/rb-app/src/main.rs` — generalize `register_bundled_star` into `register_bundled(app, binary_id, filename_stem)`; call for `"star"` and `"gffread-rs"`; register `GffConvertModule`
- `.github/workflows/ci.yml` — add a "Download bundled gffread_rs binary" step in the `build-and-release` job parallel to the existing STAR step
- `frontend/index.html` — add a sidebar nav entry for GFF Convert, update the mock-mode shim to route `#gff_convert`
- `frontend/js/app.js` — `navigate()` branch for `"gff-convert"`, `renderGffConvert()`, `submitGffConvert()`, `renderGffConvertResult()` dispatcher entry, `loadRunsForView` entry, `renderStarIndex()` prefill consumer
- `README.md` — Features list + crate tree + pipeline diagram

---

## Task 1: Scaffold `rb-gff-convert` crate

**Files:**
- Create: `crates/rb-gff-convert/Cargo.toml`
- Create: `crates/rb-gff-convert/src/lib.rs`
- Modify: `Cargo.toml` (workspace)

- [ ] **Step 1: Create `crates/rb-gff-convert/Cargo.toml`**

```toml
[package]
name = "rb-gff-convert"
version = "0.1.0"
edition = "2021"

[dependencies]
rb-core = { path = "../rb-core" }
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
async-trait.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Create `crates/rb-gff-convert/src/lib.rs` with a stub Module impl**

```rust
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::Path;
use tokio::sync::mpsc;

pub struct GffConvertModule;

#[async_trait::async_trait]
impl Module for GffConvertModule {
    fn id(&self) -> &str {
        "gff_convert"
    }
    fn name(&self) -> &str {
        "GFF Converter"
    }

    fn validate(&self, _params: &serde_json::Value) -> Vec<ValidationError> {
        Vec::new()
    }

    async fn run(
        &self,
        _params: &serde_json::Value,
        _project_dir: &Path,
        _events_tx: mpsc::Sender<RunEvent>,
        _cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        Err(ModuleError::ToolError("not yet implemented".into()))
    }
}
```

- [ ] **Step 3: Add the crate to the workspace**

Open the top-level `Cargo.toml`; find the `[workspace]` `members = [...]` list and add `"crates/rb-gff-convert"` so it sits alphabetically next to the other rb-* crates.

- [ ] **Step 4: Verify the workspace still builds**

Run: `cargo check -p rb-gff-convert`
Expected: clean build, only the "profiles for the non root package" warning (pre-existing, unrelated).

- [ ] **Step 5: Commit**

```bash
git add crates/rb-gff-convert/Cargo.toml crates/rb-gff-convert/src/lib.rs Cargo.toml
git commit -m "feat(rb-gff-convert): scaffold crate with Module stub"
```

---

## Task 2: Register `gffread-rs` in `KNOWN_BINARIES`

**Files:**
- Modify: `crates/rb-core/src/binary.rs`

- [ ] **Step 1: Extend the existing `list_known_contains_all_registered` test to cover the new id**

Open `crates/rb-core/src/binary.rs`, find the test, and extend it so it asserts the `"gffread-rs"` entry is present. The existing test currently checks for `"star"` and `"cutadapt-rs"`; add `"gffread-rs"` in the same style. If the test is a simple set-equality check, update the expected set to `["star", "cutadapt-rs", "gffread-rs"]`.

- [ ] **Step 2: Run the test to confirm it fails**

Run: `cargo test -p rb-core binary::tests::list_known_contains_all_registered`
Expected: FAIL — `gffread-rs` not in list.

- [ ] **Step 3: Add the new entry to `KNOWN_BINARIES`**

In `crates/rb-core/src/binary.rs`, append to the `KNOWN_BINARIES` slice literal:

```rust
KnownBinary {
    id: "gffread-rs",
    display_name: "gffread-rs",
    install_hint: "Prebuilt binaries at https://github.com/AI4S-YB/gffread_rs/releases — drop on PATH or set the path in Settings.",
},
```

- [ ] **Step 4: Re-run the test**

Run: `cargo test -p rb-core binary::tests::list_known_contains_all_registered`
Expected: PASS.

- [ ] **Step 5: Run the full rb-core test suite to check nothing regressed**

Run: `cargo test -p rb-core --lib`
Expected: all passing (11 tests previously, still 11).

- [ ] **Step 6: Commit**

```bash
git add crates/rb-core/src/binary.rs
git commit -m "feat(rb-core): register gffread-rs in KNOWN_BINARIES"
```

---

## Task 3: `TargetFormat` enum with tests

**Files:**
- Modify: `crates/rb-gff-convert/src/lib.rs`

The enum is the single source of truth for "what output format does the user want?" and keeps the string-vs-enum conversion in one place.

- [ ] **Step 1: Write failing unit tests for `TargetFormat`**

Append to `crates/rb-gff-convert/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_format_from_str_accepts_gtf_and_gff3() {
        assert_eq!(TargetFormat::from_str("gtf"), Some(TargetFormat::Gtf));
        assert_eq!(TargetFormat::from_str("gff3"), Some(TargetFormat::Gff3));
    }

    #[test]
    fn target_format_from_str_rejects_unknown() {
        assert_eq!(TargetFormat::from_str("bed"), None);
        assert_eq!(TargetFormat::from_str(""), None);
        assert_eq!(TargetFormat::from_str("GTF"), None); // case-sensitive
    }

    #[test]
    fn target_format_ext() {
        assert_eq!(TargetFormat::Gtf.ext(), "gtf");
        assert_eq!(TargetFormat::Gff3.ext(), "gff3");
    }

    #[test]
    fn target_format_needs_t_flag() {
        assert!(TargetFormat::Gtf.needs_t_flag());
        assert!(!TargetFormat::Gff3.needs_t_flag());
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test -p rb-gff-convert`
Expected: compile errors — `TargetFormat` not in scope.

- [ ] **Step 3: Add the enum to `crates/rb-gff-convert/src/lib.rs`**

Insert before the `GffConvertModule` struct:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetFormat {
    Gtf,
    Gff3,
}

impl TargetFormat {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "gtf" => Some(Self::Gtf),
            "gff3" => Some(Self::Gff3),
            _ => None,
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Self::Gtf => "gtf",
            Self::Gff3 => "gff3",
        }
    }

    pub fn needs_t_flag(self) -> bool {
        matches!(self, Self::Gtf)
    }
}
```

The `#[allow(clippy::should_implement_trait)]` attribute mirrors `rb-star-align::Strand::from_str`. Returning `Option<Self>` instead of implementing `FromStr` keeps call sites simple.

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cargo test -p rb-gff-convert`
Expected: all 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-gff-convert/src/lib.rs
git commit -m "feat(rb-gff-convert): TargetFormat enum for gtf/gff3 output choice"
```

---

## Task 4: `validate()` — input_file, target_format, extra_args, binary

**Files:**
- Modify: `crates/rb-gff-convert/src/lib.rs`

- [ ] **Step 1: Write failing tests covering each validation case**

Append inside the existing `mod tests` block:

```rust
    use serde_json::json;

    #[test]
    fn validate_requires_input_file() {
        let m = GffConvertModule;
        let errs = m.validate(&json!({ "target_format": "gtf" }));
        assert!(errs.iter().any(|e| e.field == "input_file"),
            "expected input_file error, got {:?}", errs);
    }

    #[test]
    fn validate_requires_existing_input_file() {
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": "/definitely/does/not/exist.gff3",
            "target_format": "gtf",
        }));
        assert!(errs.iter().any(|e| e.field == "input_file"),
            "expected input_file error for missing file, got {:?}", errs);
    }

    #[test]
    fn validate_requires_target_format() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": tmp.path().to_string_lossy(),
        }));
        assert!(errs.iter().any(|e| e.field == "target_format"),
            "expected target_format error, got {:?}", errs);
    }

    #[test]
    fn validate_rejects_unknown_target_format() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": tmp.path().to_string_lossy(),
            "target_format": "bed",
        }));
        assert!(errs.iter().any(|e| e.field == "target_format"));
    }

    #[test]
    fn validate_rejects_non_array_extra_args() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": tmp.path().to_string_lossy(),
            "target_format": "gtf",
            "extra_args": "not-an-array",
        }));
        assert!(errs.iter().any(|e| e.field == "extra_args"));
    }

    #[test]
    fn validate_rejects_non_string_extra_args_elements() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": tmp.path().to_string_lossy(),
            "target_format": "gtf",
            "extra_args": ["-T", 42, "--keep-comments"],
        }));
        assert!(errs.iter().any(|e| e.field == "extra_args"));
    }

    #[test]
    fn validate_accepts_valid_params() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let m = GffConvertModule;
        let errs = m.validate(&json!({
            "input_file": tmp.path().to_string_lossy(),
            "target_format": "gtf",
            "extra_args": ["--keep-comments"],
        }));
        // binary error may or may not be present depending on PATH; filter it out.
        let other: Vec<_> = errs.iter().filter(|e| e.field != "binary").collect();
        assert!(other.is_empty(), "unexpected errors: {:?}", other);
    }
```

- [ ] **Step 2: Run the tests to confirm they fail**

Run: `cargo test -p rb-gff-convert validate`
Expected: all 7 new tests FAIL (validate returns empty Vec).

- [ ] **Step 3: Implement `validate()`**

Replace the stub `validate()` in `crates/rb-gff-convert/src/lib.rs` with:

```rust
    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        match params.get("input_file").and_then(|v| v.as_str()) {
            None => errors.push(ValidationError {
                field: "input_file".into(),
                message: "input_file is required".into(),
            }),
            Some(p) if p.is_empty() => errors.push(ValidationError {
                field: "input_file".into(),
                message: "input_file must not be empty".into(),
            }),
            Some(p) if !std::path::Path::new(p).is_file() => errors.push(ValidationError {
                field: "input_file".into(),
                message: format!("input_file does not exist: {p}"),
            }),
            Some(_) => {}
        }

        match params.get("target_format").and_then(|v| v.as_str()) {
            None => errors.push(ValidationError {
                field: "target_format".into(),
                message: "target_format is required (gtf or gff3)".into(),
            }),
            Some(s) if TargetFormat::from_str(s).is_none() => errors.push(ValidationError {
                field: "target_format".into(),
                message: format!("target_format must be 'gtf' or 'gff3', got: {s}"),
            }),
            Some(_) => {}
        }

        if let Some(v) = params.get("extra_args") {
            match v.as_array() {
                None => errors.push(ValidationError {
                    field: "extra_args".into(),
                    message: "extra_args must be an array of strings".into(),
                }),
                Some(arr) if arr.iter().any(|e| !e.is_string()) => errors.push(ValidationError {
                    field: "extra_args".into(),
                    message: "all extra_args elements must be strings".into(),
                }),
                Some(_) => {}
            }
        }

        // Surface binary resolution failures at validate time so the UI can
        // show "Missing binary" immediately instead of at run time.
        if let Ok(resolver) = rb_core::binary::BinaryResolver::load() {
            if let Err(e) = resolver.resolve("gffread-rs") {
                errors.push(ValidationError {
                    field: "binary".into(),
                    message: e.to_string(),
                });
            }
        }

        errors
    }
```

- [ ] **Step 4: Run the tests to confirm they pass**

Run: `cargo test -p rb-gff-convert validate`
Expected: all 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-gff-convert/src/lib.rs
git commit -m "feat(rb-gff-convert): validate() covering input/format/extra_args/binary"
```

---

## Task 5: `build_argv()` pure function with tests

**Files:**
- Modify: `crates/rb-gff-convert/src/lib.rs`

- [ ] **Step 1: Write failing tests for argv assembly**

Append inside `mod tests`:

```rust
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn os(s: &str) -> OsString { OsString::from(s) }

    #[test]
    fn argv_gtf_target() {
        let input = PathBuf::from("/data/anno.gff3");
        let output = PathBuf::from("/runs/anno.gtf");
        let argv = build_argv(&input, &output, TargetFormat::Gtf, &[]);
        assert_eq!(argv, vec![
            os("/data/anno.gff3"),
            os("-T"),
            os("-o"),
            os("/runs/anno.gtf"),
        ]);
    }

    #[test]
    fn argv_gff3_target_omits_dash_t() {
        let input = PathBuf::from("/data/anno.gtf");
        let output = PathBuf::from("/runs/anno.gff3");
        let argv = build_argv(&input, &output, TargetFormat::Gff3, &[]);
        assert_eq!(argv, vec![
            os("/data/anno.gtf"),
            os("-o"),
            os("/runs/anno.gff3"),
        ]);
    }

    #[test]
    fn argv_appends_extra_args_after_output() {
        let input = PathBuf::from("/data/anno.gff3");
        let output = PathBuf::from("/runs/anno.gtf");
        let extras = vec!["--keep-comments".to_string(), "--force-exons".to_string()];
        let argv = build_argv(&input, &output, TargetFormat::Gtf, &extras);
        assert_eq!(argv, vec![
            os("/data/anno.gff3"),
            os("-T"),
            os("-o"),
            os("/runs/anno.gtf"),
            os("--keep-comments"),
            os("--force-exons"),
        ]);
    }
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test -p rb-gff-convert argv`
Expected: compile error — `build_argv` undefined.

- [ ] **Step 3: Implement `build_argv`**

Add to `crates/rb-gff-convert/src/lib.rs` (not inside the `impl Module` block — a free function):

```rust
use std::ffi::OsString;
use std::path::Path;

pub fn build_argv(
    input: &Path,
    output: &Path,
    target: TargetFormat,
    extra_args: &[String],
) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    args.push(input.as_os_str().to_os_string());
    if target.needs_t_flag() {
        args.push("-T".into());
    }
    args.push("-o".into());
    args.push(output.as_os_str().to_os_string());
    for a in extra_args {
        args.push(OsString::from(a));
    }
    args
}
```

If you already have a `use std::path::Path;` near the top for the `Module::run` signature, consolidate the imports.

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cargo test -p rb-gff-convert argv`
Expected: all 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-gff-convert/src/lib.rs
git commit -m "feat(rb-gff-convert): build_argv pure function + tests"
```

---

## Task 6: `subprocess.rs` — spawn, stream, cancel

**Files:**
- Create: `crates/rb-gff-convert/src/subprocess.rs`
- Modify: `crates/rb-gff-convert/src/lib.rs` (add `mod subprocess;`)

This is a thin adapter around `tokio::process::Command` that mirrors the exact structure of `crates/rb-star-index/src/subprocess.rs`. Read that file first to match the conventions.

- [ ] **Step 1: Create `crates/rb-gff-convert/src/subprocess.rs`**

```rust
use rb_core::cancel::CancellationToken;
use rb_core::module::ModuleError;
use rb_core::run_event::{LogStream, RunEvent};
use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Spawn the binary with the given argv, stream stdout/stderr lines as
/// `RunEvent::Log`, and honour cooperative cancellation. Returns Ok(()) when
/// the child exited zero; returns `ModuleError::Cancelled` if cancel fired;
/// returns `ModuleError::ToolError` with the exit status and tail of stderr
/// for non-zero exits or spawn failures.
pub async fn run_streamed(
    binary: &Path,
    argv: &[OsString],
    events_tx: mpsc::Sender<RunEvent>,
    cancel: CancellationToken,
) -> Result<(), ModuleError> {
    let mut cmd = Command::new(binary);
    cmd.args(argv);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| ModuleError::ToolError(format!("failed to spawn gffread-rs: {e}")))?;

    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let tx_out = events_tx.clone();
    let tx_err = events_tx.clone();

    tokio::spawn(async move {
        let mut r = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = r.next_line().await {
            let _ = tx_out
                .send(RunEvent::Log {
                    line,
                    stream: LogStream::Stdout,
                })
                .await;
        }
    });

    tokio::spawn(async move {
        let mut r = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = r.next_line().await {
            let _ = tx_err
                .send(RunEvent::Log {
                    line,
                    stream: LogStream::Stderr,
                })
                .await;
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
        Err(e) => Err(e),
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(ModuleError::ToolError(format!(
            "gffread-rs exited with status {}",
            status.code().map(|c| c.to_string()).unwrap_or_else(|| "killed".into())
        ))),
        Ok(Err(e)) => Err(ModuleError::ToolError(format!(
            "failed waiting for gffread-rs: {e}"
        ))),
    }
}
```

- [ ] **Step 2: Declare the module in `crates/rb-gff-convert/src/lib.rs`**

Add near the top of `lib.rs`:

```rust
mod subprocess;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p rb-gff-convert`
Expected: clean build.

- [ ] **Step 4: Write a cancel test**

Create `crates/rb-gff-convert/tests/cancel.rs` — on Unix we use `/bin/sleep` as a stand-in for gffread-rs so we can verify the cancel path without needing the real binary:

```rust
// Unix-only: uses /bin/sleep to simulate a long-running child. The point is
// to verify run_streamed returns ModuleError::Cancelled and that the child
// is actually killed, not hung around.
#[cfg(unix)]
#[tokio::test]
async fn run_streamed_honours_cancel() {
    use rb_core::cancel::CancellationToken;
    use rb_core::module::ModuleError;
    use rb_core::run_event::RunEvent;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use tokio::sync::mpsc;

    let (tx, _rx) = mpsc::channel::<RunEvent>(64);
    let cancel = CancellationToken::new();
    let binary = PathBuf::from("/bin/sleep");
    let argv: Vec<OsString> = vec!["30".into()];

    let cancel_for_task = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel_for_task.cancel();
    });

    let start = std::time::Instant::now();
    let res = rb_gff_convert::subprocess::run_streamed(&binary, &argv, tx, cancel).await;
    assert!(matches!(res, Err(ModuleError::Cancelled)));
    // Should return well before the 30-second sleep would have finished.
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
}
```

- [ ] **Step 5: Make the subprocess module public from the crate**

In `crates/rb-gff-convert/src/lib.rs`, change `mod subprocess;` to `pub mod subprocess;` so the integration test can call `rb_gff_convert::subprocess::run_streamed`.

- [ ] **Step 6: Run the cancel test**

Run: `cargo test -p rb-gff-convert --test cancel`
Expected: PASS on Linux/macOS, test is cfg-gated so it doesn't run on Windows.

- [ ] **Step 7: Commit**

```bash
git add crates/rb-gff-convert/src/subprocess.rs crates/rb-gff-convert/src/lib.rs crates/rb-gff-convert/tests/cancel.rs
git commit -m "feat(rb-gff-convert): subprocess runner with streaming logs and cancel"
```

---

## Task 7: `run()` happy path + integration smoke test

**Files:**
- Modify: `crates/rb-gff-convert/src/lib.rs`
- Create: `crates/rb-gff-convert/tests/data/anno.gff3`
- Create: `crates/rb-gff-convert/tests/integration_smoke.rs`

- [ ] **Step 1: Create the GFF3 fixture `crates/rb-gff-convert/tests/data/anno.gff3`**

```
##gff-version 3
chr1	test	gene	1	1000	.	+	.	ID=gene1;Name=geneA
chr1	test	mRNA	1	1000	.	+	.	ID=transcript1;Parent=gene1
chr1	test	exon	1	500	.	+	.	ID=exon1;Parent=transcript1
chr1	test	exon	600	1000	.	+	.	ID=exon2;Parent=transcript1
```

(Each line is tab-separated. The `##gff-version 3` pragma is the trigger gffread uses to identify the input as GFF3.)

- [ ] **Step 2: Create `crates/rb-gff-convert/tests/integration_smoke.rs` — opt-in end-to-end test**

```rust
//! Optional end-to-end test: requires `GFFREAD_BIN` env var pointing to a
//! gffread-rs binary. Skipped silently when unset.

#[tokio::test]
async fn end_to_end_gff3_to_gtf() {
    let gffread_bin = match std::env::var("GFFREAD_BIN") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("GFFREAD_BIN not set; skipping");
            return;
        }
    };

    let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let input = data.join("anno.gff3");
    let tmp = tempfile::tempdir().unwrap();

    // Point the resolver at the user-supplied binary.
    let settings = tmp.path().join("settings.json");
    let mut r = rb_core::binary::BinaryResolver::load_from(settings).unwrap();
    r.set("gffread-rs", std::path::PathBuf::from(&gffread_bin))
        .unwrap();

    let run_dir = tmp.path().join("run");
    std::fs::create_dir_all(&run_dir).unwrap();
    let (tx, mut _rx) = tokio::sync::mpsc::channel::<rb_core::run_event::RunEvent>(64);
    let token = rb_core::cancel::CancellationToken::new();

    use rb_core::module::Module;
    let m = rb_gff_convert::GffConvertModule;
    let params = serde_json::json!({
        "input_file": input.to_string_lossy(),
        "target_format": "gtf",
    });

    let result = m.run(&params, &run_dir, tx, token).await.unwrap();
    assert_eq!(result.output_files.len(), 1);
    let out = &result.output_files[0];
    assert!(out.exists(), "output file missing: {:?}", out);
    let contents = std::fs::read_to_string(out).unwrap();
    assert!(!contents.is_empty(), "output was empty");
    assert!(
        contents.contains("transcript_id"),
        "GTF output should contain transcript_id attribute: {}",
        &contents[..contents.len().min(300)]
    );
}
```

- [ ] **Step 3: Implement `run()` in `crates/rb-gff-convert/src/lib.rs`**

Replace the stub `async fn run` with:

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

        let resolver = rb_core::binary::BinaryResolver::load()
            .map_err(|e| ModuleError::ToolError(e.to_string()))?;
        let bin = resolver
            .resolve("gffread-rs")
            .map_err(|e| ModuleError::ToolError(e.to_string()))?;

        let input_str = params["input_file"].as_str().unwrap();
        let input_path = Path::new(input_str);
        let target_str = params["target_format"].as_str().unwrap();
        let target = TargetFormat::from_str(target_str).expect("validated above");

        let extra_args: Vec<String> = params
            .get("extra_args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let input_stem = input_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".into());
        let output_path = project_dir.join(format!("{input_stem}.{}", target.ext()));

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 0.0,
                message: format!("Converting {} → {}", input_str, target.ext().to_uppercase()),
            })
            .await;

        let argv = build_argv(input_path, &output_path, target, &extra_args);
        let start = std::time::Instant::now();
        subprocess::run_streamed(&bin, &argv, events_tx.clone(), cancel).await?;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let input_bytes = std::fs::metadata(input_path).map(|m| m.len()).unwrap_or(0);
        let output_bytes = match std::fs::metadata(&output_path) {
            Ok(m) => m.len(),
            Err(_) => {
                return Err(ModuleError::ToolError(format!(
                    "expected output file {:?} was not created",
                    output_path
                )));
            }
        };
        if output_bytes == 0 {
            return Err(ModuleError::ToolError(
                "gffread-rs produced no output records — check input file validity".into(),
            ));
        }

        let _ = events_tx
            .send(RunEvent::Progress {
                fraction: 1.0,
                message: "Done".into(),
            })
            .await;

        let summary = serde_json::json!({
            "input": input_str,
            "output": output_path.to_string_lossy(),
            "target_format": target_str,
            "input_bytes": input_bytes,
            "output_bytes": output_bytes,
            "elapsed_ms": elapsed_ms,
        });

        Ok(ModuleResult {
            output_files: vec![output_path],
            summary,
            log: String::new(),
        })
    }
```

- [ ] **Step 4: Verify compile**

Run: `cargo check -p rb-gff-convert`
Expected: clean.

- [ ] **Step 5: Run the full test suite for the crate**

Run: `cargo test -p rb-gff-convert`
Expected: all unit tests pass; `end_to_end_gff3_to_gtf` is silent-skip because `GFFREAD_BIN` is not set.

- [ ] **Step 6: (Manual) Verify the integration test if you have a gffread-rs binary available locally**

```bash
GFFREAD_BIN=/absolute/path/to/gffread-rs cargo test -p rb-gff-convert --test integration_smoke -- --nocapture
```
Expected: PASS with the produced GTF containing `transcript_id`.

Skip this step in CI — CI will pick up the binary through the bundle mechanism (Task 10); adding another download step in `check` is unnecessary overhead for a fast-feedback signal.

- [ ] **Step 7: Commit**

```bash
git add crates/rb-gff-convert/src/lib.rs crates/rb-gff-convert/tests/
git commit -m "feat(rb-gff-convert): run() happy path and opt-in integration smoke test"
```

---

## Task 8: Generalize `register_bundled_star` into a reusable helper

**Files:**
- Modify: `crates/rb-app/src/main.rs`

The current helper is hardcoded for `"star"`. We turn it into a small generic helper called once per bundled binary so the next sidecar doesn't need a copy-paste.

- [ ] **Step 1: Read the current implementation of `register_bundled_star` so you know what you're refactoring**

Run: `sed -n '1,50p' crates/rb-app/src/main.rs` (or open the file). The function currently looks for `binaries/star[.exe]` under `BaseDirectory::Resource` and, if found, calls `BinaryResolver::register_bundled("star", path)` via `block_on`.

- [ ] **Step 2: Rewrite as a generic helper and wire both ids**

Replace the `register_bundled_star` function and its call site in the Tauri `setup` hook:

```rust
// Before:
// .setup(|app| {
//     register_bundled_star(app);
//     Ok(())
// })
// fn register_bundled_star(app: &tauri::App) { ... }

// After:
.setup(|app| {
    register_bundled(app, "star", "star");
    register_bundled(app, "gffread-rs", "gffread-rs");
    Ok(())
})
```

```rust
fn register_bundled(app: &tauri::App, binary_id: &str, filename_stem: &str) {
    let exe = if cfg!(windows) {
        format!("{filename_stem}.exe")
    } else {
        filename_stem.to_string()
    };
    let path = match app
        .path()
        .resolve(format!("binaries/{exe}"), BaseDirectory::Resource)
    {
        Ok(p) if p.exists() => p,
        _ => return,
    };
    let state = app.state::<AppState>();
    let resolver = state.binary_resolver.clone();
    let id = binary_id.to_string();
    tauri::async_runtime::block_on(async move {
        resolver.lock().await.register_bundled(&id, path);
    });
}
```

- [ ] **Step 3: Build to verify**

Run: `cargo check -p rb-app`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-app/src/main.rs
git commit -m "refactor(rb-app): generalize bundled-sidecar registration into register_bundled"
```

---

## Task 9: Register `GffConvertModule` in the Tauri app

**Files:**
- Modify: `crates/rb-app/Cargo.toml`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: Add the crate dependency**

In `crates/rb-app/Cargo.toml`, under `[dependencies]`, add (alphabetical position matches existing style):

```toml
rb-gff-convert = { path = "../rb-gff-convert" }
```

- [ ] **Step 2: Register the module in `crates/rb-app/src/main.rs`**

Next to the existing `registry.register(Arc::new(rb_star_index::StarIndexModule));`, add:

```rust
registry.register(Arc::new(rb_gff_convert::GffConvertModule));
```

- [ ] **Step 3: Verify workspace still builds and tests pass**

```bash
cargo check --workspace
cargo test --workspace
```
Expected: clean + all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-app/Cargo.toml crates/rb-app/src/main.rs
git commit -m "feat(rb-app): register GffConvertModule and gffread-rs sidecar"
```

---

## Task 10: CI — download bundled `gffread-rs` in `build-and-release`

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Add the download step**

In `.github/workflows/ci.yml`, locate the existing `Download bundled STAR_rs binary` step in the `build-and-release` job. Immediately below it, add a sibling step. Both steps write into the same `crates/rb-app/binaries/` directory that the Tauri bundle resources field already ships:

```yaml
      - name: Download bundled gffread_rs binary
        shell: bash
        env:
          GFFREAD_VERSION: v0.1.0
        run: |
          mkdir -p crates/rb-app/binaries
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
          ls -la crates/rb-app/binaries/
```

The tarball/zip layout assumption (flat, single binary at root) matches STAR_rs v0.3.1 and the structure used by cargo-dist. Before merging, verify by running locally:

```bash
curl -sL https://github.com/AI4S-YB/gffread_rs/releases/download/v0.1.0/gffread-rs-v0.1.0-x86_64-unknown-linux-gnu.tar.gz | tar tz
```

Expected output: a single line `gffread-rs`. If the tarball wraps the binary in a directory, change the extraction to handle it (e.g., `tar xz --strip-components=1` or a `find` + copy).

- [ ] **Step 2: Also extend the `fmt` and `clippy` jobs to cover rb-gff-convert**

Both jobs currently list the workspace crates explicitly. Append `-p rb-gff-convert` to both command lines.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: bundle gffread_rs v0.1.0; include rb-gff-convert in fmt/clippy"
```

---

## Task 11: Frontend — nav entry + routing

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/js/app.js`

- [ ] **Step 1: Add the sidebar nav entry**

In `frontend/index.html`, inside the `<div class="nav-section">` that holds the `Alignment & Quantification` block (currently lines ~137-149 containing STAR Index and STAR Alignment), insert a new `<a class="nav-item">` **above** the STAR Index item so the pipeline order is GFF Convert → STAR Index → STAR Alignment:

```html
        <a class="nav-item" data-view="gff-convert" href="#gff-convert">
          <span class="pipeline-dot" style="--dot-color: var(--mod-purple)"></span>
          <i data-lucide="file-cog"></i>
          <span>GFF Convert</span>
        </a>
```

- [ ] **Step 2: Add the `navigate()` branch in `frontend/js/app.js`**

In the `navigate(view)` function (around line 88), add a branch that mirrors the existing `star-index` / `star-align` entries. Insert **above** `else if (view === 'star-index')`:

```javascript
    else if (view === 'gff-convert') content.innerHTML = renderGffConvert();
```

Also extend the breadcrumb-label ladder:

```javascript
      : view === 'star-index' ? 'STAR Index'
      : view === 'star-align' ? 'STAR Alignment'
      : view === 'gff-convert' ? 'GFF Convert'
      : MODULES.find(m => m.id === view)?.name || view;
```

(Keep the chain order consistent — the one you add must appear somewhere in the `?:` ladder before the `MODULES.find` fallback.)

- [ ] **Step 3: Add a placeholder `renderGffConvert` so the route compiles**

Near where `renderStarIndex` is defined (around line 541), add:

```javascript
  function renderGffConvert() {
    return `<h2>GFF Converter</h2><p>Loading…</p>`;
  }
```

This gets replaced with the real form in Task 12.

- [ ] **Step 4: Sanity-check the frontend loads**

```bash
cd frontend && python3 -m http.server 8090 &
```

Open `http://localhost:8090/#gff-convert` in a browser; the page should show "GFF Converter — Loading…". Kill the server afterwards.

- [ ] **Step 5: Commit**

```bash
git add frontend/index.html frontend/js/app.js
git commit -m "feat(frontend): add GFF Convert sidebar entry and routing stub"
```

---

## Task 12: Frontend — `renderGffConvert` form + `submitGffConvert`

**Files:**
- Modify: `frontend/js/app.js`

- [ ] **Step 1: Replace the `renderGffConvert` placeholder with the real form**

```javascript
  function renderGffConvert() {
    return `
    <h2>GFF Converter</h2>
    <p>Convert between GFF3 and GTF using gffread-rs. STAR Index requires GTF — if your annotation is GFF3, run this first.</p>
    <form id="form-gff-convert">
      <label>Input annotation file
        <input type="text" name="input_file" data-pick="file" placeholder="/path/to/anno.gff3" required />
        <button type="button" data-pick-for="input_file">Browse…</button>
      </label>
      <label>Target format
        <select name="target_format" required>
          <option value="gtf">GTF (for STAR Index, HISAT2, featureCounts, …)</option>
          <option value="gff3">GFF3</option>
        </select>
      </label>
      <details><summary>Advanced</summary>
        <label>Extra args (one per line, passed to gffread-rs)
          <textarea name="extra_args" placeholder="--keep-comments&#10;--force-exons"></textarea>
        </label>
      </details>
      <button type="submit">Convert</button>
    </form>
    <div id="gff-convert-runs"></div>
    ${renderLogPanel('gff_convert')}
  `;
  }

  async function submitGffConvert(form) {
    const fd = new FormData(form);
    const extra_args = (fd.get('extra_args') || '').toString()
      .split('\n').map(s => s.trim()).filter(Boolean);
    const params = {
      input_file: fd.get('input_file'),
      target_format: fd.get('target_format'),
      extra_args,
    };
    try {
      const runId = await window.__TAURI__.core.invoke('run_module', {
        moduleId: 'gff_convert', params,
      });
      state.runIdToModule = state.runIdToModule || {};
      state.runIdToModule[runId] = 'gff_convert';
      navigate('gff-convert');
    } catch (err) {
      alert('Failed to start run: ' + err);
    }
  }
```

- [ ] **Step 2: Hook the form submit into the existing event router**

Find the existing delegating submit handler (around line 1426 where the code checks `if (e.target.id === 'form-star-index')`). Add a parallel branch:

```javascript
      if (e.target.id === 'form-gff-convert') { e.preventDefault(); submitGffConvert(e.target); }
```

- [ ] **Step 3: Add a `loadRunsForView` entry so completed runs appear on the page**

In the `initChartsForView` switch (around line 1108), add a branch next to the existing `star-index` / `star-align` ones:

```javascript
      case 'gff-convert':  loadRunsForView('gff_convert', 'gff-convert-runs'); break;
```

- [ ] **Step 4: Extend the `renderRunResultHtml` dispatcher**

In the switch around line 1078, add a case:

```javascript
      case 'gff_convert': html = renderGffConvertResult(result, runId); break;
```

(Will reference `renderGffConvertResult` which we define in Task 13. Leave the reference; compile-time JS has no unresolved-symbol problem here — the reference is inside a function called after navigation. Tests in Task 14 will catch breakage if we skip Task 13.)

- [ ] **Step 5: Commit**

```bash
git add frontend/js/app.js
git commit -m "feat(frontend): GFF Convert form + submit wiring"
```

---

## Task 13: Frontend — `renderGffConvertResult` + prefill handoff

**Files:**
- Modify: `frontend/js/app.js`

- [ ] **Step 1: Add the result renderer near the other `render*Result` functions**

```javascript
  function renderGffConvertResult(result, runId) {
    const s = result.summary || {};
    const out = (result.output_files && result.output_files[0]) || s.output || '';
    return `
      <div class="run-result-card">
        <h3>Converted ${escapeHtml(String(s.target_format || '').toUpperCase())}</h3>
        <dl class="result-kv">
          <dt>Input</dt><dd class="path">${escapeHtml(s.input || '')}</dd>
          <dt>Output</dt><dd class="path">${escapeHtml(out)}</dd>
          <dt>Input size</dt><dd>${s.input_bytes ?? '?'} bytes</dd>
          <dt>Output size</dt><dd>${s.output_bytes ?? '?'} bytes</dd>
          <dt>Elapsed</dt><dd>${s.elapsed_ms ?? '?'} ms</dd>
        </dl>
        <button type="button" data-gff-use-in-star="${escapeHtml(out)}">Use in STAR Index</button>
      </div>
    `;
  }
```

The `data-gff-use-in-star` attribute carries the output path; the click handler below reads it. Escaping the path into an HTML attribute (via `escapeHtml`) matches how other result views handle user-supplied path strings in this file.

- [ ] **Step 2: Add the click handler for "Use in STAR Index"**

Find the top-level delegated click handler used by the existing result views (where DESeq2's "use this counts matrix" button is wired — around line 1438). Add a parallel branch:

```javascript
      const gffBtn = e.target.closest('[data-gff-use-in-star]');
      if (gffBtn) {
        state.prefill = state.prefill || {};
        state.prefill.star_index = { gtf_file: gffBtn.dataset.gffUseInStar };
        location.hash = '#star-index';
      }
```

- [ ] **Step 3: Teach `renderStarIndex` to consume `state.prefill.star_index`**

Replace the first line of the existing `renderStarIndex()` function with:

```javascript
  function renderStarIndex() {
    const prefill = (state.prefill && state.prefill.star_index) || {};
    state.prefill = {};
    const gtfValue = prefill.gtf_file || '';
    return `
    <h2>STAR Genome Index</h2>
    <p>Build a STAR index from a reference genome FASTA and GTF annotation. Required before any alignment run.</p>
    <form id="form-star-index">
      <label>Genome FASTA
        <input type="text" name="genome_fasta" data-pick="file" placeholder="/path/to/genome.fa" required />
        <button type="button" data-pick-for="genome_fasta">Browse…</button>
      </label>
      <label>GTF annotation
        <input type="text" name="gtf_file" data-pick="file" value="${escapeHtml(gtfValue)}" placeholder="/path/to/annotation.gtf" required />
        <button type="button" data-pick-for="gtf_file">Browse…</button>
      </label>
```

The rest of `renderStarIndex()` is unchanged. Key details:
- `(state.prefill && state.prefill.star_index) || {}` — identical shape to the DESeq2 handoff at line 714.
- `state.prefill = {}` — same reset, same reason.
- `value="${escapeHtml(gtfValue)}"` — ensures paths with quotes or `<` etc. can't break the HTML.

- [ ] **Step 4: Manual verification (browser mock mode)**

```bash
cd frontend && python3 -m http.server 8090
```

Open `http://localhost:8090/#gff-convert`, submit the form (mock backend fakes the run). After the fake result card appears, click **Use in STAR Index** and confirm the URL changes to `#star-index` with the GTF path populated.

Kill the server afterwards.

- [ ] **Step 5: Commit**

```bash
git add frontend/js/app.js
git commit -m "feat(frontend): GFF Convert result card + prefill handoff to STAR Index"
```

---

## Task 14: Mock-mode shim update (frontend/index.html)

**Files:**
- Modify: `frontend/index.html`

The browser-mode shim at the top of `index.html` returns mocked run IDs so the frontend is developable without a Rust backend. Add a branch for `gff_convert` so the form is exercisable in browser mock mode.

- [ ] **Step 1: Extend the mock `invoke` function**

Find the existing `if (cmd === 'run_module' && args.moduleId === 'star_index')` block (around line 31). Right next to it, add:

```javascript
            if (cmd === 'run_module' && args.moduleId === 'gff_convert') {
              return Promise.resolve('mock-gff-run-' + Date.now());
            }
```

If the shim has a `get_run_result` mock, also teach it to return a plausible shape for `gff_convert` — look for the existing per-module branches and add:

```javascript
            if (cmd === 'get_run_result' && args.runId && args.runId.startsWith('mock-gff-run-')) {
              return Promise.resolve({
                output_files: ['/tmp/mock/anno.gtf'],
                summary: {
                  input: '/tmp/mock/anno.gff3',
                  output: '/tmp/mock/anno.gtf',
                  target_format: 'gtf',
                  input_bytes: 1234,
                  output_bytes: 987,
                  elapsed_ms: 42,
                },
                log: '',
              });
            }
```

If there isn't an existing `get_run_result` mock, skip the second addition — the first is what unblocks the demo.

- [ ] **Step 2: Commit**

```bash
git add frontend/index.html
git commit -m "feat(frontend): mock-mode shim entries for gff_convert"
```

---

## Task 15: README update

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add a Features bullet**

In the `## Features` list, insert between the `Adapter Trimming` and `Alignment & Quantification` bullets:

```markdown
- **GFF Conversion** — Powered by [gffread_rs](https://github.com/AI4S-YB/gffread_rs), GFF3↔GTF conversion so annotations from any source feed straight into STAR
```

- [ ] **Step 2: Add the new crate to the architecture tree**

In the `## Architecture` code block, add a line under `rb-trimming/`:

```
│   ├── rb-gff-convert/   # gffread-rs adapter (GFF3↔GTF)
```

- [ ] **Step 3: Update the pipeline diagram**

Replace:

```
Raw Reads → QC → Trimming → Alignment → Quantification → DESeq2 → Enrichment
             ✅      ✅         ✅              ✅           ✅
```

with:

```
Raw Reads → QC → Trimming → [GFF Convert] → Alignment → Quantification → DESeq2 → Enrichment
             ✅      ✅           ✅             ✅              ✅           ✅
```

(Brackets indicate the step is optional — only needed when annotations arrive as GFF3.)

- [ ] **Step 4: Extend the dependency section**

Below the existing `## STAR_rs dependency` and `## cutadapt-rs dependency` sections, add:

```markdown
## gffread_rs dependency

`rb-gff-convert` invokes the `gffread-rs` binary from
https://github.com/AI4S-YB/gffread_rs.

**Released builds:** bundled automatically — no separate install needed.

**Local development:** grab a prebuilt binary:

    curl -sL https://github.com/AI4S-YB/gffread_rs/releases/download/v0.1.0/gffread-rs-v0.1.0-x86_64-unknown-linux-gnu.tar.gz \
      | tar xz -C ~/.local/bin

or build from source:

    git clone https://github.com/AI4S-YB/gffread_rs.git
    cd gffread_rs && cargo build -p gffread-rs --release
```

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: document GFF Convert module and gffread_rs bundling"
```

---

## Self-review checklist (ran before saving)

- **Spec coverage:**
  - Module scope (GFF↔GTF, bidirectional): Tasks 3, 4, 5, 7 ✓
  - Binary discovery (override → bundled → PATH): covered by existing BinaryResolver + Task 2 registration + Task 8 refactor ✓
  - Param surface (minimal UI + extra_args): Tasks 4, 12 ✓
  - STAR prefill handoff: Task 13 ✓
  - Bundling via CI: Task 10 ✓
  - Cancellation: Task 6 ✓
  - Testing (validate coverage, argv assembly, integration smoke, binary registry): Tasks 2, 4, 5, 6, 7 ✓
  - README update: Task 15 ✓
- **Placeholder scan:** No "TBD" / "TODO" / "similar to Task N" remaining; every step has the actual code the engineer needs.
- **Type consistency:** `GffConvertModule`, `TargetFormat`, `build_argv`, `run_streamed` used with identical signatures across tasks. Module id `"gff_convert"` consistent. View name `"gff-convert"` (hyphenated) consistent in navigate / URL / nav-item / routing branches; module-id (underscored) consistent in runModule payloads / state.runIdToModule / log-panel keys.
- **Plan size:** 15 tasks, each 3-7 steps of 2-5 minutes — within the bite-sized target.

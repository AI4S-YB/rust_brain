# Third-Party Tool Plugins — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a declarative TOML plugin system that turns external CLI tools (RustQC first) into runnable modules without writing a Rust adapter crate.

**Architecture:** New `rb-plugin` workspace crate parses TOML manifests into `PluginManifest` structs and exposes an `ExternalToolModule` adapter implementing the existing `rb_core::module::Module` trait. Bundled manifests live in `crates/rb-app/plugins/*.toml` and are embedded via `include_dir!`; user manifests live in `<config_dir>/rust_brain/plugins/*.toml` (user wins on id collision). `BinaryResolver` is refactored to merge compile-time `KNOWN_BINARIES` with runtime entries declared by plugin manifests. The frontend swaps its hardcoded `MODULES` constant for a dynamic `list_modules` Tauri command and gains one generic `frontend/js/modules/plugin/view.js` that renders any manifest as a form + result panel.

**Tech Stack:** Rust 1.75+, tokio, `tokio::process::Command`, serde/serde_json, `toml = "0.8"`, `include_dir = "0.7"`, `glob = "0.3"`, `shlex = "1"`, Tauri v2, vanilla JS frontend.

**Spec:** [`docs/superpowers/specs/2026-04-21-third-party-tool-plugins-design.md`](../specs/2026-04-21-third-party-tool-plugins-design.md)

**Motivating example:** [seqeralabs/RustQC](https://seqeralabs.github.io/RustQC/) — a FastQC-equivalent that ships Linux/macOS binaries but no Windows build, so we cannot bundle it as a first-party crate.

---

## File Structure

**Created (Rust):**

- `crates/rb-plugin/Cargo.toml` — package definition
- `crates/rb-plugin/src/lib.rs` — re-exports + `load_plugins()` entry
- `crates/rb-plugin/src/manifest.rs` — TOML deserialization structs (`PluginManifest`, `ParamSpec`, `CliRule`, `OutputSpec`, `Strings`, `BinarySpec`)
- `crates/rb-plugin/src/validate.rs` — `validate_manifest()` returning `Vec<ManifestIssue>`; `validate_against_manifest()` for runtime params
- `crates/rb-plugin/src/argv.rs` — `build_argv()` consuming a manifest + params Value, returning `Vec<String>`
- `crates/rb-plugin/src/schema.rs` — `derive_json_schema()` from `PluginManifest.params`
- `crates/rb-plugin/src/loader.rs` — scans bundled + user dirs, parses, validates, deduplicates by id
- `crates/rb-plugin/src/subprocess.rs` — `run_streamed()` mirror of `rb-gff-convert`'s subprocess helper, with `RunEvent::Log` streaming and cooperative cancel
- `crates/rb-plugin/src/module.rs` — `ExternalToolModule` implementing `rb_core::module::Module`
- `crates/rb-plugin/tests/manifest_parse.rs` — integration test: parse the bundled RustQC fixture
- `crates/rb-plugin/tests/run_smoke.rs` — opt-in test that runs a tiny shell stub via `ExternalToolModule`
- `crates/rb-plugin/tests/data/rustqc.toml` — fixture used by parse tests
- `crates/rb-plugin/tests/data/echo_plugin.toml` — fixture used by run_smoke
- `crates/rb-app/plugins/rustqc.toml` — first bundled plugin (real, ships with the app)

**Created (Frontend):**

- `frontend/js/modules/plugin/view.js` — generic plugin view: renders form from manifest, wires data-param contract, renders log + runs panel
- `frontend/js/modules/plugin/result.js` — generic plugin result view: status card + output files + open/show actions
- `frontend/js/modules/plugin/missing-binary.js` — guidance card when plugin's binary is unconfigured
- `frontend/js/modules/settings/plugins.js` — Plugins section for Settings page

**Modified (Rust):**

- `Cargo.toml` (workspace) — add `"crates/rb-plugin"` to members; add `toml = "0.8"`, `include_dir = "0.7"`, `glob = "0.3"`, `shlex = "1"` to `[workspace.dependencies]`
- `crates/rb-core/src/binary.rs` — add `register_known_dynamic()` method, change `list_known()` to merge static slice with runtime additions, expose `KnownBinary` constructor for plugin use
- `crates/rb-app/Cargo.toml` — add `rb-plugin = { path = "../rb-plugin" }` and `include_dir = "0.7"`
- `crates/rb-app/src/main.rs` — load plugins after `BinaryResolver::load()`, register both as modules and as known binaries; add plugin modules to the AI `modules_for_ai` list
- `crates/rb-app/src/state.rs` — `ModuleRegistry::list_all()`, `AppState::plugin_load_errors`, `AppState::user_plugin_dir()`
- `crates/rb-app/src/commands/mod.rs` — re-export new `plugins` module
- `crates/rb-app/src/commands/modules.rs` — new `list_modules` command
- `crates/rb-app/src/commands/plugins.rs` (new) — `list_plugin_status`, `reload_plugins`, `get_plugin_manifest`
- `crates/rb-app/src/commands/settings.rs` — `get_binary_paths` already returns dynamic merged list (no change once `BinaryResolver` is refactored)

**Modified (Frontend):**

- `frontend/js/core/constants.js` — replace `MODULES` and `KNOWN_VIEWS` with mutable arrays; export `setBootstrapModules()`
- `frontend/js/api/modules.js` — add `listModules`, `getPluginManifest`
- `frontend/js/api/plugins.js` (new) — `listPluginStatus`, `reloadPlugins`
- `frontend/js/core/router.js` — generic plugin dispatch path (when `mod.source !== 'builtin'`)
- `frontend/js/core/actions.js` — `runModule` reads dynamic MODULES (no change needed if the export is mutated in place)
- `frontend/js/main.js` (or boot entrypoint) — `await listModules()` before initial `navigate()`
- `frontend/js/modules/run-result.js` — generic-plugin branch in `renderRunResultHtml` switch
- `frontend/js/modules/settings/view.js` — wire new Plugins section
- `frontend/js/i18n.js` — strings for plugin badge, missing-binary card, settings plugins panel, errors

---

## Task 1: Scaffold `rb-plugin` crate

**Files:**
- Create: `crates/rb-plugin/Cargo.toml`
- Create: `crates/rb-plugin/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add workspace dependencies in root `Cargo.toml`**

In the `[workspace.dependencies]` block, append:

```toml
toml = "0.8"
include_dir = "0.7"
glob = "0.3"
shlex = "1"
```

- [ ] **Step 2: Add the new crate to workspace members in root `Cargo.toml`**

In `members`, insert `"crates/rb-plugin",` between `"crates/rb-gff-convert",` and `"crates/rb-qc",` (alphabetical order is fine).

- [ ] **Step 3: Create `crates/rb-plugin/Cargo.toml`**

```toml
[package]
name = "rb-plugin"
version = "0.1.0"
edition = "2021"

[dependencies]
rb-core = { path = "../rb-core" }
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
async-trait.workspace = true
thiserror.workspace = true
toml.workspace = true
glob.workspace = true
shlex.workspace = true
include_dir.workspace = true
tracing = "0.1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Create `crates/rb-plugin/src/lib.rs` with stub re-exports**

```rust
//! TOML-manifest-driven plugin system for external CLI tools.
//!
//! See `docs/superpowers/specs/2026-04-21-third-party-tool-plugins-design.md`.

pub mod argv;
pub mod loader;
pub mod manifest;
pub mod module;
pub mod schema;
pub mod subprocess;
pub mod validate;

pub use loader::{load_plugins, LoadedPlugin, PluginRegistry, PluginSource};
pub use manifest::{BinarySpec, CliRule, OutputSpec, ParamSpec, ParamType, PluginManifest, Strings};
pub use module::ExternalToolModule;
pub use validate::{ManifestIssue, ManifestIssueLevel};
```

- [ ] **Step 5: Stub each module file so the crate compiles**

Create empty files with placeholder content; later tasks will fill them.

`crates/rb-plugin/src/manifest.rs`:
```rust
//! Manifest data types — filled in Task 2.
```

Repeat the same single-line file with appropriate doc comment for: `validate.rs`, `argv.rs`, `schema.rs`, `loader.rs`, `subprocess.rs`, `module.rs`.

- [ ] **Step 6: Verify the workspace compiles**

Run: `cargo check -p rb-plugin`
Expected: compiles with warnings about unused empty modules — no errors.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/rb-plugin/
git commit -m "feat(plugin): scaffold rb-plugin crate"
```

---

## Task 2: Manifest data types + TOML parsing

**Files:**
- Modify: `crates/rb-plugin/src/manifest.rs`
- Create: `crates/rb-plugin/tests/data/rustqc.toml`
- Create: `crates/rb-plugin/tests/manifest_parse.rs`

- [ ] **Step 1: Write the failing test fixture `crates/rb-plugin/tests/data/rustqc.toml`**

```toml
id          = "rustqc"
name        = "RustQC"
description = "Rust re-implementation of FastQC."
category    = "qc"
icon        = "shield-check"
version     = "0.1.0"

[strings]
name_en = "RustQC"
name_zh = "RustQC 质量控制"
description_en = "Rust re-implementation of FastQC for read quality assessment."
description_zh = "FastQC 的 Rust 重写版本，用于读段质量评估。"
ai_hint_en = "Run RustQC for FASTQ quality assessment."
ai_hint_zh = "用 RustQC 做 FASTQ 质量评估。"

[binary]
id           = "rustqc"
display_name = "RustQC"
install_hint = "Download from https://seqeralabs.github.io/RustQC/."

[[params]]
name = "input_files"
type = "file_list"
required = true
ui = "drop_zone"
label_en = "Input FASTQ files"
label_zh = "输入 FASTQ 文件"
cli = { flag = "-i", repeat_per_value = true }

[[params]]
name = "threads"
type = "integer"
default = 4
minimum = 1
cli = { flag = "--threads" }

[[params]]
name = "nogroup"
type = "boolean"
default = false
cli = { flag = "--nogroup" }

[[params]]
name = "format"
type = "enum"
values = ["fastq", "bam", "sam"]
default = "fastq"
cli = { flag = "--format" }

[[params]]
name = "output_dir"
type = "output_dir"
cli = { flag = "-o" }

[[params]]
name = "extra_args"
type = "string"
default = ""
ui = "text"
cli = { raw = true }

[outputs]
patterns = ["*.html", "*.json", "*.zip"]
```

- [ ] **Step 2: Write the failing parse test `crates/rb-plugin/tests/manifest_parse.rs`**

```rust
use rb_plugin::{CliRule, ParamType, PluginManifest};

#[test]
fn parses_rustqc_fixture() {
    let toml_str = include_str!("data/rustqc.toml");
    let m: PluginManifest = toml::from_str(toml_str).expect("parse rustqc manifest");

    assert_eq!(m.id, "rustqc");
    assert_eq!(m.name, "RustQC");
    assert_eq!(m.category.as_deref(), Some("qc"));
    assert_eq!(m.icon.as_deref(), Some("shield-check"));
    assert_eq!(m.version.as_deref(), Some("0.1.0"));
    assert_eq!(m.binary.id, "rustqc");
    assert_eq!(m.params.len(), 6);

    let input = &m.params[0];
    assert_eq!(input.name, "input_files");
    assert!(matches!(input.r#type, ParamType::FileList));
    assert!(input.required);
    match &input.cli {
        CliRule::Flag { flag, repeat_per_value, join_with } => {
            assert_eq!(flag, "-i");
            assert!(*repeat_per_value);
            assert!(join_with.is_none());
        }
        other => panic!("expected Flag rule, got {:?}", other),
    }

    let threads = &m.params[1];
    assert!(matches!(threads.r#type, ParamType::Integer));
    assert_eq!(threads.default, Some(serde_json::json!(4)));
    assert_eq!(threads.minimum, Some(1.0));

    let extra = m.params.iter().find(|p| p.name == "extra_args").unwrap();
    assert!(matches!(extra.cli, CliRule::Raw));

    assert_eq!(
        m.outputs.as_ref().unwrap().patterns,
        vec!["*.html", "*.json", "*.zip"]
    );

    let s = m.strings.as_ref().unwrap();
    assert_eq!(s.name_en.as_deref(), Some("RustQC"));
    assert_eq!(s.ai_hint_zh.as_deref(), Some("用 RustQC 做 FASTQ 质量评估。"));
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p rb-plugin --test manifest_parse`
Expected: FAIL — types `PluginManifest`, `CliRule`, `ParamType` don't exist yet.

- [ ] **Step 4: Implement `crates/rb-plugin/src/manifest.rs`**

```rust
//! TOML manifest data types.
//!
//! Hand-written serde structs (no schema crates) so the surface stays small
//! and validation messages cite the exact toml field names. Validation
//! rules live in `validate.rs`; this file only describes shape.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub strings: Option<Strings>,
    pub binary: BinarySpec,
    #[serde(default)]
    pub params: Vec<ParamSpec>,
    #[serde(default)]
    pub outputs: Option<OutputSpec>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Strings {
    #[serde(default)]
    pub name_en: Option<String>,
    #[serde(default)]
    pub name_zh: Option<String>,
    #[serde(default)]
    pub description_en: Option<String>,
    #[serde(default)]
    pub description_zh: Option<String>,
    #[serde(default)]
    pub ai_hint_en: Option<String>,
    #[serde(default)]
    pub ai_hint_zh: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinarySpec {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub install_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParamSpec {
    pub name: String,
    pub r#type: ParamType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub label_en: Option<String>,
    #[serde(default)]
    pub label_zh: Option<String>,
    #[serde(default)]
    pub help_en: Option<String>,
    #[serde(default)]
    pub help_zh: Option<String>,
    #[serde(default)]
    pub ui: Option<String>,
    #[serde(default)]
    pub minimum: Option<f64>,
    #[serde(default)]
    pub maximum: Option<f64>,
    #[serde(default)]
    pub values: Vec<String>,
    pub cli: CliRule,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    String,
    Integer,
    Boolean,
    File,
    FileList,
    Directory,
    Enum,
    OutputDir,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CliRule {
    Positional {
        positional: bool,
    },
    Raw {
        raw: bool,
    },
    Flag {
        flag: String,
        #[serde(default)]
        repeat_per_value: bool,
        #[serde(default)]
        join_with: Option<String>,
    },
}

impl CliRule {
    /// Exposed for tests + the loader to assert exactly one of the variants.
    pub fn is_positional(&self) -> bool {
        matches!(self, CliRule::Positional { positional: true })
    }
    pub fn is_raw(&self) -> bool {
        matches!(self, CliRule::Raw { raw: true })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OutputSpec {
    #[serde(default)]
    pub patterns: Vec<String>,
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p rb-plugin --test manifest_parse`
Expected: PASS — `parses_rustqc_fixture ... ok`.

- [ ] **Step 6: Commit**

```bash
git add crates/rb-plugin/src/manifest.rs crates/rb-plugin/tests/
git commit -m "feat(plugin): TOML manifest schema + parse test"
```

---

## Task 3: Manifest validation

**Files:**
- Modify: `crates/rb-plugin/src/validate.rs`
- Modify: `crates/rb-plugin/tests/manifest_parse.rs` (add validation cases)

- [ ] **Step 1: Write failing tests for validation rules**

Append to `crates/rb-plugin/tests/manifest_parse.rs`:

```rust
use rb_plugin::{validate::validate_manifest, ManifestIssueLevel};

fn parse(s: &str) -> rb_plugin::PluginManifest {
    toml::from_str(s).expect("parse")
}

#[test]
fn rustqc_fixture_is_valid() {
    let m = parse(include_str!("data/rustqc.toml"));
    let issues = validate_manifest(&m);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.level == ManifestIssueLevel::Error)
        .collect();
    assert!(errors.is_empty(), "fixture should validate, got {:?}", errors);
}

#[test]
fn rejects_duplicate_param_names() {
    let m = parse(
        r#"
        id = "x"
        name = "X"
        [binary]
        id = "x"
        [[params]]
        name = "a"
        type = "string"
        cli = { flag = "--a" }
        [[params]]
        name = "a"
        type = "integer"
        cli = { flag = "--a" }
        "#,
    );
    let issues = validate_manifest(&m);
    assert!(issues.iter().any(|i| i.field == "params[1].name" && i.level == ManifestIssueLevel::Error));
}

#[test]
fn rejects_unsupported_version() {
    let m = parse(
        r#"
        id = "x"
        name = "X"
        version = "9.9.9"
        [binary]
        id = "x"
        "#,
    );
    let issues = validate_manifest(&m);
    assert!(issues.iter().any(|i| i.field == "version" && i.level == ManifestIssueLevel::Error));
}

#[test]
fn rejects_enum_param_without_values() {
    let m = parse(
        r#"
        id = "x"
        name = "X"
        [binary]
        id = "x"
        [[params]]
        name = "fmt"
        type = "enum"
        cli = { flag = "--fmt" }
        "#,
    );
    let issues = validate_manifest(&m);
    assert!(issues.iter().any(|i| i.field.starts_with("params[0]") && i.level == ManifestIssueLevel::Error));
}

#[test]
fn rejects_required_with_default() {
    // Required + default is contradictory; flag as error so authors fix it.
    let m = parse(
        r#"
        id = "x"
        name = "X"
        [binary]
        id = "x"
        [[params]]
        name = "n"
        type = "integer"
        required = true
        default = 4
        cli = { flag = "--n" }
        "#,
    );
    let issues = validate_manifest(&m);
    assert!(issues.iter().any(|i| i.field.starts_with("params[0]") && i.level == ManifestIssueLevel::Error));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rb-plugin --test manifest_parse`
Expected: 5 tests in this file: `parses_rustqc_fixture` PASS; the 4 new ones FAIL with "module `validate` is private" or similar.

- [ ] **Step 3: Implement `crates/rb-plugin/src/validate.rs`**

```rust
//! Manifest + runtime parameter validation.
//!
//! Validation is a pure function over the manifest data — no I/O, no
//! filesystem access. Returns a list of issues so callers can display all
//! problems at once.

use crate::manifest::{CliRule, ParamType, PluginManifest, ParamSpec};

pub const SUPPORTED_MANIFEST_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestIssueLevel {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct ManifestIssue {
    pub field: String,
    pub message: String,
    pub level: ManifestIssueLevel,
}

impl ManifestIssue {
    fn err(field: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { field: field.into(), message: msg.into(), level: ManifestIssueLevel::Error }
    }
}

pub fn validate_manifest(m: &PluginManifest) -> Vec<ManifestIssue> {
    let mut out = Vec::new();

    if m.id.trim().is_empty() {
        out.push(ManifestIssue::err("id", "id must be non-empty"));
    }
    if m.name.trim().is_empty() {
        out.push(ManifestIssue::err("name", "name must be non-empty"));
    }
    if m.binary.id.trim().is_empty() {
        out.push(ManifestIssue::err("binary.id", "binary.id must be non-empty"));
    }
    if let Some(v) = m.version.as_deref() {
        if v != SUPPORTED_MANIFEST_VERSION {
            out.push(ManifestIssue::err(
                "version",
                format!("unsupported manifest version '{}', expected '{}'", v, SUPPORTED_MANIFEST_VERSION),
            ));
        }
    }

    let mut seen = std::collections::HashSet::new();
    for (i, p) in m.params.iter().enumerate() {
        let prefix = format!("params[{}]", i);
        if !seen.insert(p.name.clone()) {
            out.push(ManifestIssue::err(format!("{prefix}.name"), format!("duplicate param name '{}'", p.name)));
        }
        if p.required && p.default.is_some() {
            out.push(ManifestIssue::err(
                format!("{prefix}.required"),
                "required and default are mutually exclusive — pick one",
            ));
        }
        if matches!(p.r#type, ParamType::Enum) && p.values.is_empty() {
            out.push(ManifestIssue::err(
                format!("{prefix}.values"),
                "enum params must declare a non-empty `values` list",
            ));
        }
        validate_cli_rule(&p.cli, &prefix, &mut out, p);
    }

    out
}

fn validate_cli_rule(
    rule: &CliRule,
    prefix: &str,
    out: &mut Vec<ManifestIssue>,
    p: &ParamSpec,
) {
    match rule {
        CliRule::Flag { flag, repeat_per_value, join_with } => {
            if flag.trim().is_empty() {
                out.push(ManifestIssue::err(format!("{prefix}.cli.flag"), "flag must be non-empty"));
            }
            if *repeat_per_value && join_with.is_some() {
                out.push(ManifestIssue::err(
                    format!("{prefix}.cli"),
                    "repeat_per_value and join_with are mutually exclusive",
                ));
            }
            if (*repeat_per_value || join_with.is_some()) && !matches!(p.r#type, ParamType::FileList) {
                out.push(ManifestIssue::err(
                    format!("{prefix}.cli"),
                    "repeat_per_value / join_with apply only to file_list params",
                ));
            }
        }
        CliRule::Positional { .. } | CliRule::Raw { .. } => {}
    }
}

/// Validate a runtime params Value against a manifest. Returns
/// `rb_core::module::ValidationError` so it slots straight into
/// `Module::validate()`.
pub fn validate_against_manifest(
    m: &PluginManifest,
    params: &serde_json::Value,
) -> Vec<rb_core::module::ValidationError> {
    use rb_core::module::ValidationError;
    let mut errs = Vec::new();
    let obj = match params.as_object() {
        Some(o) => o,
        None => {
            errs.push(ValidationError {
                field: "_".into(),
                message: "params must be a JSON object".into(),
            });
            return errs;
        }
    };

    for p in &m.params {
        let v = obj.get(&p.name);
        if v.is_none() && p.required && p.default.is_none() {
            errs.push(ValidationError {
                field: p.name.clone(),
                message: format!("'{}' is required", p.name),
            });
            continue;
        }
        let Some(v) = v else { continue };
        type_check(p, v, &mut errs);
    }
    errs
}

fn type_check(p: &ParamSpec, v: &serde_json::Value, errs: &mut Vec<rb_core::module::ValidationError>) {
    use rb_core::module::ValidationError;
    let mismatch = |msg: String| ValidationError { field: p.name.clone(), message: msg };
    match p.r#type {
        ParamType::String | ParamType::OutputDir => {
            if !v.is_string() {
                errs.push(mismatch(format!("'{}' must be a string", p.name)));
            }
        }
        ParamType::Integer => {
            if !v.is_i64() && !v.is_u64() {
                errs.push(mismatch(format!("'{}' must be an integer", p.name)));
            } else if let Some(n) = v.as_f64() {
                if let Some(min) = p.minimum {
                    if n < min {
                        errs.push(mismatch(format!("'{}' must be >= {}", p.name, min)));
                    }
                }
                if let Some(max) = p.maximum {
                    if n > max {
                        errs.push(mismatch(format!("'{}' must be <= {}", p.name, max)));
                    }
                }
            }
        }
        ParamType::Boolean => {
            if !v.is_boolean() {
                errs.push(mismatch(format!("'{}' must be a boolean", p.name)));
            }
        }
        ParamType::File | ParamType::Directory => {
            match v.as_str() {
                None => errs.push(mismatch(format!("'{}' must be a path string", p.name))),
                Some(s) => {
                    let path = std::path::Path::new(s);
                    let ok = match p.r#type {
                        ParamType::File => path.is_file(),
                        ParamType::Directory => path.is_dir(),
                        _ => true,
                    };
                    if !ok {
                        errs.push(mismatch(format!("'{}': path does not exist or wrong kind: {}", p.name, s)));
                    }
                }
            }
        }
        ParamType::FileList => match v.as_array() {
            None => errs.push(mismatch(format!("'{}' must be an array of paths", p.name))),
            Some(arr) => {
                if p.required && arr.is_empty() {
                    errs.push(mismatch(format!("'{}' must be non-empty", p.name)));
                }
                for (i, item) in arr.iter().enumerate() {
                    if !item.is_string() {
                        errs.push(mismatch(format!("'{}'[{}] must be a string", p.name, i)));
                    }
                }
            }
        },
        ParamType::Enum => match v.as_str() {
            None => errs.push(mismatch(format!("'{}' must be a string", p.name))),
            Some(s) => {
                if !p.values.iter().any(|allowed| allowed == s) {
                    errs.push(mismatch(format!(
                        "'{}' must be one of: {}",
                        p.name,
                        p.values.join(", ")
                    )));
                }
            }
        },
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rb-plugin --test manifest_parse`
Expected: all 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-plugin/src/validate.rs crates/rb-plugin/tests/manifest_parse.rs
git commit -m "feat(plugin): manifest + runtime parameter validation"
```

---

## Task 4: Argv builder

**Files:**
- Modify: `crates/rb-plugin/src/argv.rs`

- [ ] **Step 1: Write failing unit tests inside `crates/rb-plugin/src/argv.rs`**

```rust
//! Build a `Vec<String>` argv from a manifest + runtime params Value.
//!
//! Pure function. No shell. The output is fed to `tokio::process::Command::args`,
//! so tokens are passed verbatim (no splitting, no quoting required).

use crate::manifest::{CliRule, ParamType, PluginManifest, ParamSpec};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum ArgvError {
    #[error("required param '{0}' missing and no default")]
    MissingRequired(String),
    #[error("param '{0}' has wrong type for cli rule: {1}")]
    TypeMismatch(String, String),
    #[error("raw arg '{0}' could not be shlex-split")]
    BadRaw(String),
}

pub fn build_argv(
    binary_path: &std::path::Path,
    manifest: &PluginManifest,
    params: &Value,
) -> Result<Vec<String>, ArgvError> {
    let mut out = vec![binary_path.to_string_lossy().to_string()];
    let obj = params.as_object().cloned().unwrap_or_default();

    for p in &manifest.params {
        let v = obj.get(&p.name).cloned().or_else(|| p.default.clone());
        let v = match v {
            Some(v) => v,
            None => {
                if p.required {
                    return Err(ArgvError::MissingRequired(p.name.clone()));
                }
                continue;
            }
        };
        render_param(p, &v, &mut out)?;
    }
    Ok(out)
}

fn render_param(p: &ParamSpec, v: &Value, out: &mut Vec<String>) -> Result<(), ArgvError> {
    match &p.cli {
        CliRule::Raw { .. } => {
            let s = v
                .as_str()
                .ok_or_else(|| ArgvError::TypeMismatch(p.name.clone(), "raw needs a string".into()))?;
            if s.is_empty() {
                return Ok(());
            }
            let parts =
                shlex::split(s).ok_or_else(|| ArgvError::BadRaw(s.to_string()))?;
            out.extend(parts);
        }
        CliRule::Positional { .. } => {
            extend_values(p, v, out)?;
        }
        CliRule::Flag { flag, repeat_per_value, join_with } => {
            if matches!(p.r#type, ParamType::Boolean) {
                if v.as_bool().unwrap_or(false) {
                    out.push(flag.clone());
                }
                return Ok(());
            }
            if matches!(p.r#type, ParamType::FileList) {
                let arr = v
                    .as_array()
                    .ok_or_else(|| ArgvError::TypeMismatch(p.name.clone(), "file_list needs array".into()))?;
                if arr.is_empty() {
                    return Ok(());
                }
                if *repeat_per_value {
                    for item in arr {
                        let s = item.as_str().ok_or_else(|| {
                            ArgvError::TypeMismatch(p.name.clone(), "file_list items must be strings".into())
                        })?;
                        out.push(flag.clone());
                        out.push(s.to_string());
                    }
                } else if let Some(sep) = join_with {
                    let joined: Vec<&str> = arr.iter().filter_map(|i| i.as_str()).collect();
                    out.push(flag.clone());
                    out.push(joined.join(sep));
                } else {
                    out.push(flag.clone());
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            out.push(s.to_string());
                        }
                    }
                }
                return Ok(());
            }
            // scalar value
            out.push(flag.clone());
            out.push(value_to_string(v));
        }
    }
    Ok(())
}

fn extend_values(p: &ParamSpec, v: &Value, out: &mut Vec<String>) -> Result<(), ArgvError> {
    if matches!(p.r#type, ParamType::FileList) {
        let arr = v
            .as_array()
            .ok_or_else(|| ArgvError::TypeMismatch(p.name.clone(), "file_list needs array".into()))?;
        for item in arr {
            if let Some(s) = item.as_str() {
                out.push(s.to_string());
            }
        }
    } else {
        out.push(value_to_string(v));
    }
    Ok(())
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;
    use serde_json::json;
    use std::path::Path;

    fn manifest(toml_str: &str) -> PluginManifest {
        toml::from_str(toml_str).expect("parse")
    }

    #[test]
    fn flag_with_scalar_value() {
        let m = manifest(
            r#"
            id="x" name="X"
            [binary] id="x"
            [[params]] name="threads" type="integer" cli={ flag="--threads" }
            "#,
        );
        let argv = build_argv(Path::new("/usr/bin/x"), &m, &json!({"threads": 4})).unwrap();
        assert_eq!(argv, vec!["/usr/bin/x", "--threads", "4"]);
    }

    #[test]
    fn boolean_flag_present_only_when_true() {
        let m = manifest(
            r#"
            id="x" name="X"
            [binary] id="x"
            [[params]] name="quiet" type="boolean" cli={ flag="--quiet" }
            "#,
        );
        let on = build_argv(Path::new("/x"), &m, &json!({"quiet": true})).unwrap();
        let off = build_argv(Path::new("/x"), &m, &json!({"quiet": false})).unwrap();
        assert_eq!(on, vec!["/x", "--quiet"]);
        assert_eq!(off, vec!["/x"]);
    }

    #[test]
    fn file_list_repeat_per_value() {
        let m = manifest(
            r#"
            id="x" name="X"
            [binary] id="x"
            [[params]] name="inputs" type="file_list" cli={ flag="-i", repeat_per_value=true }
            "#,
        );
        let argv = build_argv(
            Path::new("/x"),
            &m,
            &json!({"inputs": ["a.fq", "b.fq"]}),
        )
        .unwrap();
        assert_eq!(argv, vec!["/x", "-i", "a.fq", "-i", "b.fq"]);
    }

    #[test]
    fn file_list_joined_with_comma() {
        let m = manifest(
            r#"
            id="x" name="X"
            [binary] id="x"
            [[params]] name="inputs" type="file_list" cli={ flag="-I", join_with="," }
            "#,
        );
        let argv = build_argv(
            Path::new("/x"),
            &m,
            &json!({"inputs": ["a", "b", "c"]}),
        )
        .unwrap();
        assert_eq!(argv, vec!["/x", "-I", "a,b,c"]);
    }

    #[test]
    fn positional_file_list() {
        let m = manifest(
            r#"
            id="x" name="X"
            [binary] id="x"
            [[params]] name="inputs" type="file_list" cli={ positional=true }
            "#,
        );
        let argv = build_argv(
            Path::new("/x"),
            &m,
            &json!({"inputs": ["a", "b"]}),
        )
        .unwrap();
        assert_eq!(argv, vec!["/x", "a", "b"]);
    }

    #[test]
    fn raw_extra_args_split_with_shlex() {
        let m = manifest(
            r#"
            id="x" name="X"
            [binary] id="x"
            [[params]] name="extra" type="string" cli={ raw=true }
            "#,
        );
        let argv = build_argv(
            Path::new("/x"),
            &m,
            &json!({"extra": "--foo bar --baz \"two words\""}),
        )
        .unwrap();
        assert_eq!(argv, vec!["/x", "--foo", "bar", "--baz", "two words"]);
    }

    #[test]
    fn raw_empty_string_is_ignored() {
        let m = manifest(
            r#"
            id="x" name="X"
            [binary] id="x"
            [[params]] name="extra" type="string" cli={ raw=true }
            "#,
        );
        let argv = build_argv(Path::new("/x"), &m, &json!({"extra": ""})).unwrap();
        assert_eq!(argv, vec!["/x"]);
    }

    #[test]
    fn missing_required_errors() {
        let m = manifest(
            r#"
            id="x" name="X"
            [binary] id="x"
            [[params]] name="inputs" type="file_list" required=true cli={ flag="-i", repeat_per_value=true }
            "#,
        );
        let err = build_argv(Path::new("/x"), &m, &json!({})).unwrap_err();
        assert!(matches!(err, ArgvError::MissingRequired(ref n) if n == "inputs"));
    }

    #[test]
    fn default_used_when_param_missing() {
        let m = manifest(
            r#"
            id="x" name="X"
            [binary] id="x"
            [[params]] name="threads" type="integer" default=8 cli={ flag="--threads" }
            "#,
        );
        let argv = build_argv(Path::new("/x"), &m, &json!({})).unwrap();
        assert_eq!(argv, vec!["/x", "--threads", "8"]);
    }

    #[test]
    fn order_follows_manifest_declaration() {
        let m = manifest(
            r#"
            id="x" name="X"
            [binary] id="x"
            [[params]] name="t" type="integer" default=1 cli={ flag="-t" }
            [[params]] name="o" type="output_dir" default="out" cli={ flag="-o" }
            [[params]] name="i" type="file_list" default=["a"] cli={ flag="-i", repeat_per_value=true }
            "#,
        );
        let argv = build_argv(Path::new("/x"), &m, &json!({})).unwrap();
        assert_eq!(argv, vec!["/x", "-t", "1", "-o", "out", "-i", "a"]);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass (no separate impl required — embedded above)**

Run: `cargo test -p rb-plugin --lib argv`
Expected: 10 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-plugin/src/argv.rs
git commit -m "feat(plugin): argv builder with per-param CLI rules"
```

---

## Task 5: JSON Schema derivation

**Files:**
- Modify: `crates/rb-plugin/src/schema.rs`

- [ ] **Step 1: Write failing unit tests inside `crates/rb-plugin/src/schema.rs`**

```rust
//! Derive a JSON Schema (draft-07-ish, matching what rb-ai expects) from a
//! manifest's params. Used by:
//!   - `Module::params_schema()` so plugins surface in the AI tool registry
//!   - `validate_params` Tauri command so the frontend gets the same shape
//!     of errors as for first-party modules

use crate::manifest::{ParamType, PluginManifest};
use serde_json::{json, Map, Value};

pub fn derive_json_schema(m: &PluginManifest) -> Value {
    let mut props = Map::new();
    let mut required = Vec::new();
    for p in &m.params {
        let mut entry = Map::new();
        match p.r#type {
            ParamType::String | ParamType::OutputDir | ParamType::File | ParamType::Directory => {
                entry.insert("type".into(), json!("string"));
            }
            ParamType::Integer => {
                entry.insert("type".into(), json!("integer"));
                if let Some(min) = p.minimum {
                    entry.insert("minimum".into(), json!(min));
                }
                if let Some(max) = p.maximum {
                    entry.insert("maximum".into(), json!(max));
                }
            }
            ParamType::Boolean => {
                entry.insert("type".into(), json!("boolean"));
            }
            ParamType::FileList => {
                entry.insert("type".into(), json!("array"));
                entry.insert("items".into(), json!({"type": "string"}));
            }
            ParamType::Enum => {
                entry.insert("type".into(), json!("string"));
                entry.insert("enum".into(), json!(p.values));
            }
        }
        if let Some(desc) = p.help_en.clone().or_else(|| p.label_en.clone()) {
            entry.insert("description".into(), json!(desc));
        }
        if let Some(d) = &p.default {
            entry.insert("default".into(), d.clone());
        }
        props.insert(p.name.clone(), Value::Object(entry));
        if p.required {
            required.push(p.name.clone());
        }
    }
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> PluginManifest {
        toml::from_str(s).expect("parse")
    }

    #[test]
    fn derives_object_schema_with_required_list() {
        let m = parse(
            r#"
            id="x" name="X"
            [binary] id="x"
            [[params]] name="inputs" type="file_list" required=true cli={ flag="-i", repeat_per_value=true }
            [[params]] name="threads" type="integer" default=4 minimum=1 maximum=32 cli={ flag="-t" }
            [[params]] name="fmt" type="enum" values=["a","b"] default="a" cli={ flag="--fmt" }
            "#,
        );
        let s = derive_json_schema(&m);
        assert_eq!(s["type"], "object");
        assert_eq!(s["additionalProperties"], false);
        assert_eq!(s["required"], json!(["inputs"]));
        assert_eq!(s["properties"]["inputs"]["type"], "array");
        assert_eq!(s["properties"]["inputs"]["items"]["type"], "string");
        assert_eq!(s["properties"]["threads"]["type"], "integer");
        assert_eq!(s["properties"]["threads"]["minimum"], 1.0);
        assert_eq!(s["properties"]["threads"]["maximum"], 32.0);
        assert_eq!(s["properties"]["threads"]["default"], 4);
        assert_eq!(s["properties"]["fmt"]["type"], "string");
        assert_eq!(s["properties"]["fmt"]["enum"], json!(["a", "b"]));
    }

    #[test]
    fn rustqc_fixture_derives_valid_schema() {
        let m: PluginManifest = toml::from_str(include_str!("../tests/data/rustqc.toml")).unwrap();
        let s = derive_json_schema(&m);
        assert_eq!(s["required"], json!(["input_files"]));
        assert!(s["properties"].as_object().unwrap().contains_key("nogroup"));
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p rb-plugin --lib schema`
Expected: 2 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-plugin/src/schema.rs
git commit -m "feat(plugin): derive JSON Schema from manifest params"
```

---

## Task 6: Plugin loader (bundled + user dirs, dedupe)

**Files:**
- Modify: `crates/rb-plugin/src/loader.rs`

- [ ] **Step 1: Write failing unit tests inside `crates/rb-plugin/src/loader.rs`**

```rust
//! Scan plugin directories, parse + validate, dedupe by id (user wins).

use crate::manifest::PluginManifest;
use crate::validate::{validate_manifest, ManifestIssueLevel};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSource {
    Bundled,
    User,
}

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub source: PluginSource,
    pub origin_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct PluginLoadError {
    pub source_label: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct PluginRegistry {
    pub by_id: HashMap<String, LoadedPlugin>,
    pub errors: Vec<PluginLoadError>,
}

/// Load plugins from an embedded bundled dir + an optional user dir on disk.
pub fn load_plugins(
    bundled: &include_dir::Dir<'_>,
    user_dir: Option<&Path>,
) -> PluginRegistry {
    let mut reg = PluginRegistry::default();

    for f in bundled.files() {
        if f.path().extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let label = format!("bundled:{}", f.path().display());
        let text = match std::str::from_utf8(f.contents()) {
            Ok(t) => t,
            Err(_) => {
                reg.errors.push(PluginLoadError { source_label: label, message: "non-UTF8 manifest".into() });
                continue;
            }
        };
        match parse_one(text) {
            Ok(m) => {
                reg.by_id.insert(
                    m.id.clone(),
                    LoadedPlugin { manifest: m, source: PluginSource::Bundled, origin_path: None },
                );
            }
            Err(e) => reg.errors.push(PluginLoadError { source_label: label, message: e }),
        }
    }

    if let Some(dir) = user_dir {
        if dir.exists() {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(e) => {
                    reg.errors.push(PluginLoadError {
                        source_label: format!("user:{}", dir.display()),
                        message: format!("read_dir failed: {e}"),
                    });
                    return reg;
                }
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                    continue;
                }
                let label = format!("user:{}", path.display());
                let text = match std::fs::read_to_string(&path) {
                    Ok(t) => t,
                    Err(e) => {
                        reg.errors.push(PluginLoadError { source_label: label, message: e.to_string() });
                        continue;
                    }
                };
                match parse_one(&text) {
                    Ok(m) => {
                        reg.by_id.insert(
                            m.id.clone(),
                            LoadedPlugin {
                                manifest: m,
                                source: PluginSource::User,
                                origin_path: Some(path),
                            },
                        );
                    }
                    Err(e) => reg.errors.push(PluginLoadError { source_label: label, message: e }),
                }
            }
        }
    }
    reg
}

fn parse_one(text: &str) -> Result<PluginManifest, String> {
    let m: PluginManifest = toml::from_str(text).map_err(|e| format!("toml parse error: {e}"))?;
    let issues = validate_manifest(&m);
    let errors: Vec<_> = issues.into_iter().filter(|i| i.level == ManifestIssueLevel::Error).collect();
    if !errors.is_empty() {
        let joined = errors.iter().map(|i| format!("{}: {}", i.field, i.message)).collect::<Vec<_>>().join("; ");
        return Err(joined);
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, body: &str) {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    static EMPTY_BUNDLED: include_dir::Dir<'_> =
        include_dir::Dir::new("empty", &[], &[]);

    #[test]
    fn user_dir_loads_valid_toml() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "rustqc.toml",
            include_str!("../tests/data/rustqc.toml"),
        );
        let reg = load_plugins(&EMPTY_BUNDLED, Some(tmp.path()));
        assert_eq!(reg.by_id.len(), 1);
        assert!(reg.by_id.contains_key("rustqc"));
        assert_eq!(reg.by_id["rustqc"].source, PluginSource::User);
        assert!(reg.errors.is_empty());
    }

    #[test]
    fn user_dir_with_invalid_toml_records_error() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "broken.toml", "this is not = valid toml [[[[");
        let reg = load_plugins(&EMPTY_BUNDLED, Some(tmp.path()));
        assert!(reg.by_id.is_empty());
        assert_eq!(reg.errors.len(), 1);
        assert!(reg.errors[0].source_label.ends_with("broken.toml"));
    }

    #[test]
    fn missing_user_dir_is_ok() {
        let reg = load_plugins(&EMPTY_BUNDLED, Some(Path::new("/nope/does/not/exist")));
        assert!(reg.by_id.is_empty());
        assert!(reg.errors.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p rb-plugin --lib loader`
Expected: 3 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-plugin/src/loader.rs
git commit -m "feat(plugin): loader with bundled + user dirs and dedupe"
```

---

## Task 7: Subprocess runner

**Files:**
- Modify: `crates/rb-plugin/src/subprocess.rs`

- [ ] **Step 1: Implement `crates/rb-plugin/src/subprocess.rs`** (mirrors `rb-gff-convert`'s subprocess.rs)

```rust
//! Spawn an external command, stream stdout/stderr lines as RunEvent::Log,
//! honour cancellation. Returns Ok if exit zero; ToolError otherwise.
//!
//! Same shape as `rb-gff-convert::subprocess::run_streamed` — kept duplicated
//! intentionally because the two crates have no shared subprocess crate.
//! When a third copy appears, extract to a shared helper.

use rb_core::cancel::CancellationToken;
use rb_core::module::ModuleError;
use rb_core::run_event::{LogStream, RunEvent};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub async fn run_streamed(
    binary: &std::path::Path,
    argv_after_binary: &[String],
    events_tx: mpsc::Sender<RunEvent>,
    cancel: CancellationToken,
) -> Result<i32, ModuleError> {
    let mut cmd = Command::new(binary);
    cmd.args(argv_after_binary);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| ModuleError::ToolError(format!("failed to spawn {}: {e}", binary.display())))?;

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
        Err(e) => Err(e),
        Ok(Ok(status)) if status.success() => Ok(status.code().unwrap_or(0)),
        Ok(Ok(status)) => Err(ModuleError::ToolError(format!(
            "process exited with status {}",
            status.code().map(|c| c.to_string()).unwrap_or_else(|| "killed".into())
        ))),
        Ok(Err(e)) => Err(ModuleError::ToolError(format!("failed waiting on child: {e}"))),
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p rb-plugin`
Expected: clean compile.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-plugin/src/subprocess.rs
git commit -m "feat(plugin): subprocess runner with streaming + cancel"
```

---

## Task 8: `ExternalToolModule` — Module trait impl

**Files:**
- Modify: `crates/rb-plugin/src/module.rs`
- Create: `crates/rb-plugin/tests/data/echo_plugin.toml`
- Create: `crates/rb-plugin/tests/run_smoke.rs`

- [ ] **Step 1: Write the failing fixture `crates/rb-plugin/tests/data/echo_plugin.toml`**

```toml
id = "echo_plugin"
name = "Echo"
description = "Test fixture that runs /bin/echo"
category = "test"
version = "0.1.0"

[binary]
id = "echo_plugin_bin"
display_name = "Echo (test)"
install_hint = "Test fixture; uses /bin/echo."

[[params]]
name = "msg"
type = "string"
required = true
cli = { flag = "--msg" }

[outputs]
patterns = []
```

- [ ] **Step 2: Write the failing integration test `crates/rb-plugin/tests/run_smoke.rs`**

```rust
//! Smoke test for ExternalToolModule using /bin/echo on Unix.
//! Skipped on non-Unix where /bin/echo isn't guaranteed.

#![cfg(unix)]

use rb_core::cancel::CancellationToken;
use rb_core::module::Module;
use rb_core::run_event::RunEvent;
use rb_plugin::{ExternalToolModule, PluginManifest};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::test]
async fn echo_plugin_runs_to_completion() {
    let manifest: PluginManifest =
        toml::from_str(include_str!("data/echo_plugin.toml")).unwrap();
    let module = ExternalToolModule::new(Arc::new(manifest), PathBuf::from("/bin/echo"));

    let tmp = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel::<RunEvent>(32);
    let cancel = CancellationToken::new();

    let result = module
        .run(&json!({"msg": "hello world"}), tmp.path(), tx, cancel)
        .await
        .expect("echo plugin should succeed");

    // Drain log events; assert we saw the message echoed back on stdout.
    let mut saw = false;
    while let Ok(ev) = rx.try_recv() {
        if let RunEvent::Log { line, .. } = ev {
            if line.contains("hello world") {
                saw = true;
            }
        }
    }
    assert!(saw, "expected stdout log line containing the echoed message");
    assert_eq!(result.summary["exit_code"], 0);
}

#[tokio::test]
async fn missing_required_param_validates_out() {
    let manifest: PluginManifest =
        toml::from_str(include_str!("data/echo_plugin.toml")).unwrap();
    let module = ExternalToolModule::new(Arc::new(manifest), PathBuf::from("/bin/echo"));
    let errs = module.validate(&json!({}));
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].field, "msg");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rb-plugin --test run_smoke`
Expected: FAIL — `ExternalToolModule` not yet defined.

- [ ] **Step 4: Implement `crates/rb-plugin/src/module.rs`**

```rust
//! `ExternalToolModule` — implements rb_core::module::Module from a manifest.
//!
//! The module owns:
//!   * an `Arc<PluginManifest>` (cheap clone, shared across runs)
//!   * a resolved `binary_path` snapshot taken at registration time
//!     (re-resolved on every run via the AppState BinaryResolver — see
//!     rb-app integration; the embedded copy is a fallback)
//!
//! Output discovery: after a successful exit, glob `manifest.outputs.patterns`
//! relative to the resolved `output_dir` and add matches to `output_files`.

use crate::argv::build_argv;
use crate::manifest::{ParamType, PluginManifest};
use crate::schema::derive_json_schema;
use crate::subprocess::run_streamed;
use crate::validate::validate_against_manifest;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct ExternalToolModule {
    manifest: Arc<PluginManifest>,
    binary_path: PathBuf,
    schema_cache: serde_json::Value,
}

impl ExternalToolModule {
    pub fn new(manifest: Arc<PluginManifest>, binary_path: PathBuf) -> Self {
        let schema_cache = derive_json_schema(&manifest);
        Self { manifest, binary_path, schema_cache }
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
}

#[async_trait::async_trait]
impl Module for ExternalToolModule {
    fn id(&self) -> &str {
        &self.manifest.id
    }

    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(self.schema_cache.clone())
    }

    fn ai_hint(&self, lang: &str) -> String {
        let s = self.manifest.strings.as_ref();
        let from_strings = match lang {
            "zh" => s.and_then(|s| s.ai_hint_zh.clone()),
            _ => s.and_then(|s| s.ai_hint_en.clone()),
        };
        from_strings
            .or_else(|| s.and_then(|s| s.ai_hint_en.clone()))
            .or_else(|| self.manifest.description.clone())
            .unwrap_or_default()
    }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        validate_against_manifest(&self.manifest, params)
    }

    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        events_tx: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        let errs = self.validate(params);
        if !errs.is_empty() {
            return Err(ModuleError::InvalidParams(errs));
        }

        let mut effective_params = params.clone();
        let output_dir = resolve_output_dir(&self.manifest, &mut effective_params, project_dir)?;
        std::fs::create_dir_all(&output_dir)?;

        let argv = build_argv(&self.binary_path, &self.manifest, &effective_params)
            .map_err(|e| ModuleError::ToolError(e.to_string()))?;
        // Index 0 is the binary path itself; subprocess::run_streamed takes args after it.
        let after = argv[1..].to_vec();

        let _ = events_tx
            .send(RunEvent::Progress { fraction: 0.0, message: format!("Running {}", self.manifest.name) })
            .await;

        let exit_code = run_streamed(&self.binary_path, &after, events_tx.clone(), cancel).await?;

        let output_files = discover_outputs(&self.manifest, &output_dir);
        let _ = events_tx
            .send(RunEvent::Progress { fraction: 1.0, message: "Done".into() })
            .await;

        Ok(ModuleResult {
            output_files: output_files.clone(),
            summary: serde_json::json!({
                "plugin_id": self.manifest.id,
                "exit_code": exit_code,
                "argv": argv,
                "output_dir": output_dir.display().to_string(),
                "output_files": output_files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            }),
            log: String::new(),
        })
    }
}

fn resolve_output_dir(
    m: &PluginManifest,
    params: &mut serde_json::Value,
    project_dir: &Path,
) -> Result<PathBuf, ModuleError> {
    let output_param = m
        .params
        .iter()
        .find(|p| matches!(p.r#type, ParamType::OutputDir));
    let Some(p) = output_param else {
        return Ok(project_dir.join("output"));
    };
    let obj = params.as_object_mut().ok_or_else(|| {
        ModuleError::InvalidParams(vec![ValidationError {
            field: "_".into(),
            message: "params must be an object".into(),
        }])
    })?;
    let provided = obj
        .get(&p.name)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let resolved = match provided {
        Some(s) => PathBuf::from(s),
        None => {
            let d = project_dir.join("output");
            obj.insert(p.name.clone(), serde_json::Value::String(d.display().to_string()));
            d
        }
    };
    Ok(resolved)
}

fn discover_outputs(m: &PluginManifest, output_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Some(spec) = &m.outputs else { return files };
    for pat in &spec.patterns {
        let glob_pattern = output_dir.join(pat);
        let pattern_str = match glob_pattern.to_str() {
            Some(s) => s,
            None => continue,
        };
        let Ok(it) = glob::glob(pattern_str) else { continue };
        for entry in it.flatten() {
            files.push(entry);
        }
    }
    files
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rb-plugin --test run_smoke`
Expected: 2 tests PASS on Linux/macOS; (skipped on Windows due to `#![cfg(unix)]`).

- [ ] **Step 6: Run all `rb-plugin` tests**

Run: `cargo test -p rb-plugin`
Expected: every test green.

- [ ] **Step 7: Commit**

```bash
git add crates/rb-plugin/src/module.rs crates/rb-plugin/tests/data/echo_plugin.toml crates/rb-plugin/tests/run_smoke.rs
git commit -m "feat(plugin): ExternalToolModule implementing Module trait"
```

---

## Task 9: `BinaryResolver` runtime registration

**Files:**
- Modify: `crates/rb-core/src/binary.rs`

- [ ] **Step 1: Write the failing test in `crates/rb-core/src/binary.rs`** (append to existing tests module)

```rust
#[test]
fn runtime_known_binary_appears_in_list_known() {
    let tmp = tempfile::tempdir().unwrap();
    let settings = tmp.path().join("settings.json");
    let mut r = BinaryResolver::load_from(settings).unwrap();
    r.register_known_dynamic(KnownBinaryEntry {
        id: "rustqc".into(),
        display_name: "RustQC".into(),
        install_hint: "Download from seqera site.".into(),
    });
    let ids: Vec<_> = r.list_known().into_iter().map(|b| b.id).collect();
    assert!(ids.contains(&"rustqc".to_string()));
    assert!(ids.contains(&"star".to_string())); // built-in entries still present
}

#[test]
fn runtime_known_binary_collision_with_builtin_keeps_builtin() {
    let tmp = tempfile::tempdir().unwrap();
    let settings = tmp.path().join("settings.json");
    let mut r = BinaryResolver::load_from(settings).unwrap();
    r.register_known_dynamic(KnownBinaryEntry {
        id: "star".into(),
        display_name: "BogusName".into(),
        install_hint: "Bogus".into(),
    });
    let star = r
        .list_known()
        .into_iter()
        .find(|b| b.id == "star")
        .expect("star still listed");
    assert_eq!(star.display_name, "STAR (STAR_rs)"); // built-in display_name preserved
}

#[test]
fn resolve_consults_runtime_install_hint_when_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let settings = tmp.path().join("settings.json");
    let mut r = BinaryResolver::load_from(settings).unwrap();
    r.register_known_dynamic(KnownBinaryEntry {
        id: "definitely_not_on_path_xyz".into(),
        display_name: "Plugin".into(),
        install_hint: "Get it from upstream.".into(),
    });
    let err = r.resolve("definitely_not_on_path_xyz").unwrap_err();
    match err {
        BinaryError::NotFound { hint, .. } => assert!(hint.contains("Get it from upstream")),
        _ => panic!("expected NotFound"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rb-core binary`
Expected: 3 new tests FAIL — `KnownBinaryEntry`, `register_known_dynamic` not defined.

- [ ] **Step 3: Modify `crates/rb-core/src/binary.rs` — add runtime registration**

Add this struct above `KnownBinary`:

```rust
/// Owned form of a binary entry, registerable at runtime by plugins.
/// Same fields as `KnownBinary` but `String`-based instead of `&'static str`.
#[derive(Debug, Clone)]
pub struct KnownBinaryEntry {
    pub id: String,
    pub display_name: String,
    pub install_hint: String,
}
```

Add a `dynamic_known: Vec<KnownBinaryEntry>` field on `BinaryResolver`:

```rust
pub struct BinaryResolver {
    settings_path: PathBuf,
    settings: SettingsFile,
    bundled: HashMap<String, PathBuf>,
    dynamic_known: Vec<KnownBinaryEntry>,
}
```

Initialize it in `with_defaults_at` and `load_from` (set to `Vec::new()`).

Add this method:

```rust
impl BinaryResolver {
    /// Register a binary at runtime (e.g. from a plugin manifest). Built-in
    /// entries from `KNOWN_BINARIES` always win on id collision.
    pub fn register_known_dynamic(&mut self, entry: KnownBinaryEntry) {
        let exists_builtin = KNOWN_BINARIES.iter().any(|k| k.id == entry.id);
        if exists_builtin {
            return;
        }
        if let Some(slot) = self.dynamic_known.iter_mut().find(|e| e.id == entry.id) {
            *slot = entry;
        } else {
            self.dynamic_known.push(entry);
        }
    }

    /// Iterator over the merged known set (built-in + dynamic).
    pub fn known_iter(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        let builtin = KNOWN_BINARIES.iter().map(|k| (k.id, k.display_name, k.install_hint));
        let dynamic = self
            .dynamic_known
            .iter()
            .map(|e| (e.id.as_str(), e.display_name.as_str(), e.install_hint.as_str()));
        builtin.chain(dynamic)
    }
}
```

Modify `list_known` to use `known_iter`:

```rust
pub fn list_known(&self) -> Vec<BinaryStatus> {
    self.known_iter()
        .map(|(id, display_name, install_hint)| {
            let configured = self.settings.binary_paths.get(id).and_then(|o| o.clone());
            let bundled = self.bundled.get(id).cloned();
            let detected = which::which(id).ok();
            BinaryStatus {
                id: id.to_string(),
                display_name: display_name.to_string(),
                configured_path: configured,
                bundled_path: bundled,
                detected_on_path: detected,
                install_hint: install_hint.to_string(),
            }
        })
        .collect()
}
```

Modify the `NotFound` branch in `resolve` to look up the install hint via `known_iter`:

```rust
let hint = self
    .known_iter()
    .find(|(id, _, _)| *id == name)
    .map(|(_, _, hint)| hint.to_string())
    .unwrap_or_else(|| format!("No install hint registered for '{}'.", name));
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rb-core binary`
Expected: all binary tests PASS, including the 3 new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-core/src/binary.rs
git commit -m "feat(core): runtime binary registration for plugins"
```

---

## Task 10: Wire `rb-plugin` into `rb-app` (no bundled plugins yet)

**Files:**
- Modify: `crates/rb-app/Cargo.toml`
- Modify: `crates/rb-app/src/state.rs`
- Modify: `crates/rb-app/src/main.rs`
- Create: `crates/rb-app/plugins/.keep` (empty file so the directory exists for `include_dir!`)

- [ ] **Step 1: Add deps to `crates/rb-app/Cargo.toml`**

In `[dependencies]`, append:

```toml
rb-plugin = { path = "../rb-plugin" }
include_dir = "0.7"
```

- [ ] **Step 2: Create the empty bundled-plugins dir**

```bash
mkdir -p crates/rb-app/plugins
touch crates/rb-app/plugins/.keep
```

- [ ] **Step 3: Modify `crates/rb-app/src/state.rs`** — add a method to enumerate all modules and a slot for plugin load errors

Replace `ModuleRegistry` impl with this expanded version (only `list_all` is new; the rest is unchanged):

```rust
impl ModuleRegistry {
    pub fn new() -> Self {
        Self { modules: HashMap::new() }
    }
    pub fn register(&mut self, module: Arc<dyn Module>) {
        self.modules.insert(module.id().to_string(), module);
    }
    pub fn get(&self, id: &str) -> Option<Arc<dyn Module>> {
        self.modules.get(id).cloned()
    }
    pub fn list_ids(&self) -> Vec<String> {
        self.modules.keys().cloned().collect()
    }
    /// Snapshot of every registered module — used by `list_modules` Tauri
    /// command so the frontend can render dynamic sidebar entries.
    pub fn list_all(&self) -> Vec<Arc<dyn Module>> {
        self.modules.values().cloned().collect()
    }
}
```

Append to the same file:

```rust
/// Plugin ids tagged by source so the frontend can render the badge.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginSourceTag {
    pub id: String,
    pub source: String, // "bundled" | "user"
    pub origin_path: Option<PathBuf>,
}

/// Plugin loader diagnostics surfaced in Settings.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PluginDiagnostics {
    pub loaded: Vec<PluginSourceTag>,
    pub errors: Vec<PluginErrorView>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginErrorView {
    pub source_label: String,
    pub message: String,
}
```

Modify `AppState` and add a `plugins` field:

```rust
pub struct AppState {
    pub registry: Arc<Mutex<ModuleRegistry>>,
    pub runner: Arc<Mutex<Option<Arc<Runner>>>>,
    pub recent_projects: Arc<Mutex<Vec<PathBuf>>>,
    pub binary_resolver: Arc<Mutex<rb_core::binary::BinaryResolver>>,
    pub plugins: Arc<Mutex<PluginDiagnostics>>,
    pub user_plugin_dir: PathBuf,
    pub ai: Arc<AiState>,
}
```

> **Why `Mutex<ModuleRegistry>`?** Reload-plugins (Task 13) needs to mutate the registry at runtime. In-flight runs hold `Arc<dyn Module>` directly so they're unaffected.

Update `AppState::new` to take ownership of the resolver / registry from `main.rs` and to initialize the new fields:

```rust
impl AppState {
    pub fn new(registry: ModuleRegistry, ai: Arc<AiState>) -> Self {
        let resolver = rb_core::binary::BinaryResolver::load().unwrap_or_else(|e| {
            eprintln!("warning: failed to load binary settings ({e}); using defaults");
            rb_core::binary::BinaryResolver::with_defaults_at(
                rb_core::binary::BinaryResolver::default_settings_path(),
            )
        });
        let user_plugin_dir = directories::ProjectDirs::from("", "", "rust_brain")
            .map(|pd| pd.config_dir().join("plugins"))
            .unwrap_or_else(|| std::path::PathBuf::from("plugins"));
        Self {
            registry: Arc::new(Mutex::new(registry)),
            runner: Arc::new(Mutex::new(None)),
            recent_projects: Arc::new(Mutex::new(Vec::new())),
            binary_resolver: Arc::new(Mutex::new(resolver)),
            plugins: Arc::new(Mutex::new(PluginDiagnostics::default())),
            user_plugin_dir,
            ai,
        }
    }
}
```

- [ ] **Step 4: Update every existing reference to `state.registry`** (commands/modules.rs, runner setup) to lock first. For each `state.registry.get(&id)` call, replace with:

```rust
let module = {
    let reg = state.registry.lock().await;
    reg.get(&id)
};
```

For `state.registry.list_ids()`:

```rust
let ids = state.registry.lock().await.list_ids();
```

Search for compile errors and fix them in commands/modules.rs and any other site touched.

- [ ] **Step 5: Modify `crates/rb-app/src/main.rs`** — load plugins, register their modules and binaries, and add them to the AI tools list

Add an `include_dir!` declaration near the top of `main.rs`:

```rust
static BUNDLED_PLUGINS: include_dir::Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/plugins");
```

In `fn main()`, between the registry creation and the `tools_by_lang` build, insert:

```rust
// Load user plugin dir lazily — main.rs constructs AppState which derives it.
let user_plugin_dir = directories::ProjectDirs::from("", "", "rust_brain")
    .map(|pd| pd.config_dir().join("plugins"));
let plugin_reg = rb_plugin::load_plugins(&BUNDLED_PLUGINS, user_plugin_dir.as_deref());

// Resolver is constructed inside AppState::new; we need it earlier to register
// dynamic binaries. Simpler: do the registration after AppState::new but
// before we build modules_for_ai — see below.
let plugin_modules: Vec<Arc<dyn rb_core::module::Module>> = plugin_reg
    .by_id
    .values()
    .map(|loaded| {
        let manifest = Arc::new(loaded.manifest.clone());
        // Binary path is resolved lazily on each run via BinaryResolver,
        // but ExternalToolModule::new takes a snapshot. We pass an empty
        // PathBuf as a sentinel and re-resolve in run() — but that requires
        // holding a handle to the resolver. To keep Module trait pure,
        // store the binary id and resolve via `which::which` at run time
        // inside the module. For v1 we take the lazy snapshot approach via
        // a wrapper.
        Arc::new(LazyResolvingPluginModule::new(manifest, loaded.manifest.binary.id.clone()))
            as Arc<dyn rb_core::module::Module>
    })
    .collect();

for m in &plugin_modules {
    registry.register(m.clone());
}
```

Then below that, update `modules_for_ai` to include `plugin_modules`:

```rust
let mut modules_for_ai: Vec<Arc<dyn rb_core::module::Module>> = vec![
    Arc::new(rb_deseq2::DeseqModule),
    Arc::new(rb_qc::QcModule),
    Arc::new(rb_trimming::TrimmingModule),
    Arc::new(rb_gff_convert::GffConvertModule),
    Arc::new(rb_star_index::StarIndexModule),
    Arc::new(rb_star_align::StarAlignModule),
];
modules_for_ai.extend(plugin_modules.iter().cloned());
```

And after `AppState::new` is constructed:

```rust
let app_state = AppState::new(registry, ai.clone());
{
    let mut resolver = tauri::async_runtime::block_on(app_state.binary_resolver.lock());
    for loaded in plugin_reg.by_id.values() {
        resolver.register_known_dynamic(rb_core::binary::KnownBinaryEntry {
            id: loaded.manifest.binary.id.clone(),
            display_name: loaded
                .manifest
                .binary
                .display_name
                .clone()
                .unwrap_or_else(|| loaded.manifest.name.clone()),
            install_hint: loaded
                .manifest
                .binary
                .install_hint
                .clone()
                .unwrap_or_else(|| format!("Install '{}' and configure its path.", loaded.manifest.binary.id)),
        });
    }
}
{
    let mut diag = tauri::async_runtime::block_on(app_state.plugins.lock());
    diag.loaded = plugin_reg
        .by_id
        .iter()
        .map(|(id, lp)| crate::state::PluginSourceTag {
            id: id.clone(),
            source: match lp.source {
                rb_plugin::PluginSource::Bundled => "bundled".into(),
                rb_plugin::PluginSource::User => "user".into(),
            },
            origin_path: lp.origin_path.clone(),
        })
        .collect();
    diag.errors = plugin_reg
        .errors
        .iter()
        .map(|e| crate::state::PluginErrorView {
            source_label: e.source_label.clone(),
            message: e.message.clone(),
        })
        .collect();
}

tauri::Builder::default()
    .manage(app_state)
    .setup(...)
```

- [ ] **Step 6: Add `LazyResolvingPluginModule` wrapper in `crates/rb-app/src/state.rs`**

```rust
use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use rb_plugin::PluginManifest;
use std::path::Path;
use tokio::sync::mpsc;

/// Wraps `ExternalToolModule` so the binary path is resolved fresh each run
/// against the live `BinaryResolver` — picking up Settings changes without
/// rebuilding the registry.
pub struct LazyResolvingPluginModule {
    manifest: Arc<PluginManifest>,
    binary_id: String,
    resolver: Arc<Mutex<BinaryResolver>>,
}

impl LazyResolvingPluginModule {
    pub fn new(
        manifest: Arc<PluginManifest>,
        binary_id: String,
        resolver: Arc<Mutex<BinaryResolver>>,
    ) -> Self {
        Self { manifest, binary_id, resolver }
    }
}

#[async_trait::async_trait]
impl Module for LazyResolvingPluginModule {
    fn id(&self) -> &str {
        &self.manifest.id
    }
    fn name(&self) -> &str {
        &self.manifest.name
    }
    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(rb_plugin::schema::derive_json_schema(&self.manifest))
    }
    fn ai_hint(&self, lang: &str) -> String {
        let s = self.manifest.strings.as_ref();
        match lang {
            "zh" => s.and_then(|s| s.ai_hint_zh.clone()),
            _ => s.and_then(|s| s.ai_hint_en.clone()),
        }
        .or_else(|| s.and_then(|s| s.ai_hint_en.clone()))
        .or_else(|| self.manifest.description.clone())
        .unwrap_or_default()
    }
    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        rb_plugin::validate::validate_against_manifest(&self.manifest, params)
    }
    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        events_tx: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        let path = {
            let r = self.resolver.lock().await;
            r.resolve(&self.binary_id).map_err(|e| ModuleError::ToolError(e.to_string()))?
        };
        let inner = rb_plugin::ExternalToolModule::new(self.manifest.clone(), path);
        inner.run(params, project_dir, events_tx, cancel).await
    }
}
```

> The `LazyResolvingPluginModule::new` signature now requires the resolver. Update Step 5 main.rs construction: build `plugin_modules` AFTER `AppState::new` so the resolver is available; move that block down accordingly.

Move construction order so it reads:

```rust
let app_state = AppState::new(registry_no_plugins, ai.clone());
let plugin_modules: Vec<Arc<dyn rb_core::module::Module>> = plugin_reg
    .by_id
    .values()
    .map(|loaded| {
        let manifest = Arc::new(loaded.manifest.clone());
        Arc::new(LazyResolvingPluginModule::new(
            manifest,
            loaded.manifest.binary.id.clone(),
            app_state.binary_resolver.clone(),
        )) as Arc<dyn rb_core::module::Module>
    })
    .collect();
{
    let mut reg = tauri::async_runtime::block_on(app_state.registry.lock());
    for m in &plugin_modules {
        reg.register(m.clone());
    }
}
modules_for_ai.extend(plugin_modules.iter().cloned());
```

(Replace the old direct `registry.register(...)` for plugin modules with the locked variant.)

- [ ] **Step 7: Verify the workspace compiles**

Run: `cargo check --workspace`
Expected: clean compile.

- [ ] **Step 8: Run all existing tests to confirm no regressions**

Run: `cargo test --workspace`
Expected: every test green.

- [ ] **Step 9: Commit**

```bash
git add crates/rb-app/Cargo.toml crates/rb-app/plugins/.keep crates/rb-app/src/state.rs crates/rb-app/src/main.rs crates/rb-app/src/commands/
git commit -m "feat(app): wire rb-plugin loader into AppState"
```

---

## Task 11: Tauri command — `list_modules`

**Files:**
- Modify: `crates/rb-app/src/commands/modules.rs`
- Modify: `crates/rb-app/src/main.rs` (register handler)

- [ ] **Step 1: Add the command in `crates/rb-app/src/commands/modules.rs`**

Append:

```rust
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ModuleDescriptor {
    pub id: String,            // backend id (e.g. "qc", "rustqc")
    pub view_id: String,       // frontend view id (built-ins use existing ids; plugins == backend id)
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub icon: String,
    pub source: String,        // "builtin" | "bundled-plugin" | "user-plugin"
    pub has_native_view: bool, // true → use frontend/js/modules/<view-id>/view.js
    pub binary_id: Option<String>,
}

#[tauri::command]
pub async fn list_modules(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<ModuleDescriptor>, String> {
    let modules = state.registry.lock().await.list_all();
    let plugins = state.plugins.lock().await;
    let plugin_sources: std::collections::HashMap<&str, &str> = plugins
        .loaded
        .iter()
        .map(|t| (t.id.as_str(), t.source.as_str()))
        .collect();

    let mut out = Vec::new();
    for m in modules {
        let id = m.id().to_string();
        let plugin_source = plugin_sources.get(id.as_str()).copied();
        let (source, has_native_view, view_id, category, icon, binary_id) = match plugin_source {
            None => (
                "builtin".to_string(),
                true,
                view_id_for_builtin(&id),
                category_for_builtin(&id).to_string(),
                icon_for_builtin(&id).to_string(),
                None,
            ),
            Some(src) => (
                if src == "bundled" { "bundled-plugin".into() } else { "user-plugin".into() },
                false,
                id.clone(),
                "other".into(),  // overridden below from manifest
                "plug".into(),
                None,
            ),
        };
        out.push(ModuleDescriptor {
            id,
            view_id,
            name: m.name().to_string(),
            description: None,
            category,
            icon,
            source,
            has_native_view,
            binary_id,
        });
    }
    Ok(out)
}

fn view_id_for_builtin(id: &str) -> String {
    match id {
        "deseq2" => "differential".into(),
        "gff_convert" => "gff-convert".into(),
        "star_index" => "star-index".into(),
        "star_align" => "star-align".into(),
        other => other.into(),
    }
}

fn category_for_builtin(id: &str) -> &'static str {
    match id {
        "qc" => "qc",
        "trimming" => "trimming",
        "star_index" | "star_align" => "alignment",
        "gff_convert" => "annotation",
        "deseq2" => "differential",
        _ => "other",
    }
}

fn icon_for_builtin(id: &str) -> &'static str {
    match id {
        "qc" => "microscope",
        "trimming" => "scissors",
        "star_align" => "git-merge",
        "star_index" => "database",
        "gff_convert" => "file-code-2",
        "deseq2" => "flame",
        _ => "puzzle",
    }
}
```

- [ ] **Step 2: Override category/icon/description from plugin manifest**

In `list_modules`, after `let modules = ...`, also obtain plugin manifests:

```rust
// Map plugin id -> &PluginManifest for category/icon/description.
let plugin_manifests = state.registry.lock().await.list_all();
// Note: ExternalToolModule exposes manifest() but we have LazyResolvingPluginModule.
// Add a helper trait method below to avoid downcasting.
```

Add a method on `LazyResolvingPluginModule` in `state.rs`:

```rust
impl LazyResolvingPluginModule {
    pub fn manifest_arc(&self) -> Arc<PluginManifest> {
        self.manifest.clone()
    }
}
```

And a tagging trait so we can detect it without downcast:

Actually simpler — store plugin metadata separately. Modify `PluginSourceTag` to carry `category`, `icon`, `description`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginSourceTag {
    pub id: String,
    pub source: String,
    pub origin_path: Option<PathBuf>,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub binary_id: String,
}
```

Update `main.rs` plugin diagnostics population to fill the new fields from `loaded.manifest`.

Then in `list_modules`, when `plugin_source` is Some, look up the full tag and use `category`, `icon`, `description`, `binary_id` from it.

```rust
let plugin_tags: std::collections::HashMap<String, crate::state::PluginSourceTag> = state
    .plugins
    .lock()
    .await
    .loaded
    .iter()
    .cloned()
    .map(|t| (t.id.clone(), t))
    .collect();

// in the match arm for plugin:
let tag = plugin_tags.get(&id).expect("tag present for plugin");
(
    if tag.source == "bundled" { "bundled-plugin".into() } else { "user-plugin".into() },
    false,
    id.clone(),
    tag.category.clone().unwrap_or_else(|| "other".into()),
    tag.icon.clone().unwrap_or_else(|| "plug".into()),
    Some(tag.binary_id.clone()),
)
```

And include `description: tag.description.clone()` in the `out.push`.

- [ ] **Step 3: Register the handler in `crates/rb-app/src/main.rs`**

In `invoke_handler!` macro, add `commands::modules::list_modules,` near the other module commands.

- [ ] **Step 4: Verify**

Run: `cargo check --workspace`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-app/src/commands/modules.rs crates/rb-app/src/main.rs crates/rb-app/src/state.rs
git commit -m "feat(app): list_modules Tauri command"
```

---

## Task 12: Tauri commands — `list_plugin_status`, `reload_plugins`, `get_plugin_manifest`

**Files:**
- Create: `crates/rb-app/src/commands/plugins.rs`
- Modify: `crates/rb-app/src/commands/mod.rs`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: Create `crates/rb-app/src/commands/plugins.rs`**

```rust
use serde::Serialize;
use tauri::State;

use crate::state::{AppState, PluginDiagnostics};

#[derive(Debug, Serialize)]
pub struct PluginManifestView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub binary_id: String,
    pub params_schema: serde_json::Value,
    pub strings: serde_json::Value,
    pub params: serde_json::Value, // raw param specs for form rendering
    pub outputs: serde_json::Value,
}

#[tauri::command]
pub async fn list_plugin_status(
    state: State<'_, AppState>,
) -> Result<PluginDiagnostics, String> {
    Ok(state.plugins.lock().await.clone())
}

#[tauri::command]
pub async fn get_plugin_manifest(
    id: String,
    state: State<'_, AppState>,
) -> Result<PluginManifestView, String> {
    use crate::state::LazyResolvingPluginModule;
    let registry = state.registry.lock().await;
    let m = registry.get(&id).ok_or_else(|| format!("module '{id}' not found"))?;
    let lp = (m.as_ref() as &dyn std::any::Any)
        .downcast_ref::<LazyResolvingPluginModule>()
        .ok_or_else(|| format!("module '{id}' is not a plugin"))?;
    let manifest = lp.manifest_arc();
    Ok(PluginManifestView {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        category: manifest.category.clone(),
        icon: manifest.icon.clone(),
        binary_id: manifest.binary.id.clone(),
        params_schema: rb_plugin::schema::derive_json_schema(&manifest),
        strings: serde_json::to_value(&manifest.strings).unwrap_or(serde_json::Value::Null),
        params: serde_json::to_value(&manifest.params).unwrap_or(serde_json::Value::Null),
        outputs: serde_json::to_value(&manifest.outputs).unwrap_or(serde_json::Value::Null),
    })
}
```

> **Note on downcast:** `Arc<dyn Module>` → `&dyn Any` doesn't work directly because `Module` doesn't extend `Any`. Use a helper trait instead:

In `crates/rb-app/src/state.rs`, add:

```rust
pub trait AsPluginModule {
    fn as_plugin(&self) -> Option<Arc<PluginManifest>>;
}

impl<T: Module + 'static> AsPluginModule for T {
    default fn as_plugin(&self) -> Option<Arc<PluginManifest>> {
        None
    }
}

impl AsPluginModule for LazyResolvingPluginModule {
    fn as_plugin(&self) -> Option<Arc<PluginManifest>> {
        Some(self.manifest.clone())
    }
}
```

> Specialization (`default fn`) requires the unstable feature; **avoid** that. Use a different approach:

Replace the trait with a method on `Module` itself? That would touch `rb-core`. Cleaner: store a separate `HashMap<String, Arc<PluginManifest>>` on `AppState`:

```rust
pub struct AppState {
    // ...existing...
    pub plugin_manifests: Arc<Mutex<HashMap<String, Arc<PluginManifest>>>>,
}
```

Populate in `main.rs` when registering plugins. Then `get_plugin_manifest`:

```rust
#[tauri::command]
pub async fn get_plugin_manifest(
    id: String,
    state: State<'_, AppState>,
) -> Result<PluginManifestView, String> {
    let manifests = state.plugin_manifests.lock().await;
    let manifest = manifests.get(&id).cloned().ok_or_else(|| format!("plugin '{id}' not found"))?;
    Ok(PluginManifestView {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        category: manifest.category.clone(),
        icon: manifest.icon.clone(),
        binary_id: manifest.binary.id.clone(),
        params_schema: rb_plugin::schema::derive_json_schema(&manifest),
        strings: serde_json::to_value(&manifest.strings).unwrap_or(serde_json::Value::Null),
        params: serde_json::to_value(&manifest.params).unwrap_or(serde_json::Value::Null),
        outputs: serde_json::to_value(&manifest.outputs).unwrap_or(serde_json::Value::Null),
    })
}
```

Add `reload_plugins` (deferred body — just shell now, full implementation in Task 13):

```rust
#[tauri::command]
pub async fn reload_plugins(_state: State<'_, AppState>) -> Result<(), String> {
    Err("not yet implemented — see Task 13".into())
}
```

- [ ] **Step 2: Re-export from `crates/rb-app/src/commands/mod.rs`**

```rust
pub mod ai_provider;
pub mod chat;
pub mod files;
pub mod modules;
pub mod plugins;        // <-- new
pub mod project;
pub mod settings;
```

- [ ] **Step 3: Register the new commands in `crates/rb-app/src/main.rs`**

In the `invoke_handler!` macro, add:

```rust
commands::plugins::list_plugin_status,
commands::plugins::reload_plugins,
commands::plugins::get_plugin_manifest,
```

- [ ] **Step 4: Populate `plugin_manifests` in `main.rs`** when registering plugins:

```rust
{
    let mut mans = tauri::async_runtime::block_on(app_state.plugin_manifests.lock());
    for loaded in plugin_reg.by_id.values() {
        mans.insert(loaded.manifest.id.clone(), Arc::new(loaded.manifest.clone()));
    }
}
```

Initialize `plugin_manifests: Arc::new(Mutex::new(HashMap::new())),` in `AppState::new`.

- [ ] **Step 5: Verify**

Run: `cargo check --workspace`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/rb-app/src/commands/ crates/rb-app/src/main.rs crates/rb-app/src/state.rs
git commit -m "feat(app): plugin status + manifest Tauri commands"
```

---

## Task 13: Implement `reload_plugins`

**Files:**
- Modify: `crates/rb-app/src/commands/plugins.rs`
- Modify: `crates/rb-app/src/main.rs` (extract reload helper to share with bootstrap)

- [ ] **Step 1: Extract a helper into `crates/rb-app/src/commands/plugins.rs`**

```rust
use std::sync::Arc;
use std::collections::HashMap;
use rb_plugin::PluginManifest;

/// Re-scan bundled + user plugin dirs and update AppState in place.
/// Returns the new diagnostics so the caller can decide whether to broadcast.
pub async fn reload_plugins_impl(
    state: &AppState,
    bundled: &include_dir::Dir<'_>,
) -> PluginDiagnostics {
    let user_dir = state.user_plugin_dir.clone();
    let plugin_reg = rb_plugin::load_plugins(bundled, Some(&user_dir));

    // Rebuild module registry: drop existing plugin entries, keep first-party.
    {
        let mut reg = state.registry.lock().await;
        let plugin_ids: Vec<String> = state.plugin_manifests.lock().await.keys().cloned().collect();
        for id in plugin_ids {
            reg.remove(&id); // see Task 13 Step 2 for new method
        }
    }

    // Rebuild plugin_manifests map and module registry from new plugin_reg.
    let mut new_manifests: HashMap<String, Arc<PluginManifest>> = HashMap::new();
    {
        let mut reg = state.registry.lock().await;
        let mut resolver = state.binary_resolver.lock().await;
        for loaded in plugin_reg.by_id.values() {
            let manifest = Arc::new(loaded.manifest.clone());
            resolver.register_known_dynamic(rb_core::binary::KnownBinaryEntry {
                id: manifest.binary.id.clone(),
                display_name: manifest.binary.display_name.clone().unwrap_or_else(|| manifest.name.clone()),
                install_hint: manifest.binary.install_hint.clone().unwrap_or_else(||
                    format!("Install '{}' and configure its path.", manifest.binary.id)),
            });
            let module: Arc<dyn rb_core::module::Module> = Arc::new(crate::state::LazyResolvingPluginModule::new(
                manifest.clone(),
                manifest.binary.id.clone(),
                state.binary_resolver.clone(),
            ));
            reg.register(module);
            new_manifests.insert(manifest.id.clone(), manifest);
        }
    }
    *state.plugin_manifests.lock().await = new_manifests;

    let diag = PluginDiagnostics {
        loaded: plugin_reg
            .by_id
            .iter()
            .map(|(id, lp)| crate::state::PluginSourceTag {
                id: id.clone(),
                source: match lp.source {
                    rb_plugin::PluginSource::Bundled => "bundled".into(),
                    rb_plugin::PluginSource::User => "user".into(),
                },
                origin_path: lp.origin_path.clone(),
                category: lp.manifest.category.clone(),
                icon: lp.manifest.icon.clone(),
                description: lp.manifest.description.clone(),
                binary_id: lp.manifest.binary.id.clone(),
            })
            .collect(),
        errors: plugin_reg.errors.iter().map(|e| crate::state::PluginErrorView {
            source_label: e.source_label.clone(),
            message: e.message.clone(),
        }).collect(),
    };
    *state.plugins.lock().await = diag.clone();
    diag
}
```

- [ ] **Step 2: Add `ModuleRegistry::remove`** in `crates/rb-app/src/state.rs`

```rust
impl ModuleRegistry {
    pub fn remove(&mut self, id: &str) {
        self.modules.remove(id);
    }
}
```

- [ ] **Step 3: Implement `reload_plugins`**

Replace the stub:

```rust
#[tauri::command]
pub async fn reload_plugins(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<PluginDiagnostics, String> {
    let diag = reload_plugins_impl(&state, &crate::BUNDLED_PLUGINS).await;
    let _ = app.emit("modules-changed", &serde_json::Value::Null);
    Ok(diag)
}
```

- [ ] **Step 4: Make `BUNDLED_PLUGINS` public** at the crate root (`crates/rb-app/src/main.rs`):

```rust
pub static BUNDLED_PLUGINS: include_dir::Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/plugins");
```

(Move to `lib.rs` if main.rs's pub items aren't accessible from `commands::plugins` — Tauri main.rs is a binary entry, but for module access from sibling modules `pub` works because `commands` is a child module.)

- [ ] **Step 5: Verify**

Run: `cargo check --workspace`
Expected: clean compile.

- [ ] **Step 6: Commit**

```bash
git add crates/rb-app/src/commands/plugins.rs crates/rb-app/src/state.rs crates/rb-app/src/main.rs
git commit -m "feat(app): reload_plugins command"
```

---

## Task 14: Frontend bootstrap — replace static `MODULES` with dynamic fetch

**Files:**
- Modify: `frontend/js/core/constants.js`
- Modify: `frontend/js/api/modules.js`
- Create: `frontend/js/api/plugins.js`
- Modify: `frontend/js/main.js` (or whatever boots the app — verify entrypoint)

- [ ] **Step 1: Find the boot entrypoint**

Run: `grep -rln 'navigate(' /home/xzg/project/rust_brain/frontend/js/ | head` — confirm where the initial `navigate()` is called.

- [ ] **Step 2: Refactor `frontend/js/core/constants.js`**

Replace the file:

```js
// Built-in module set — used as the initial fallback before the backend
// `list_modules` call resolves. After bootstrap, `setBootstrapModules()`
// replaces the contents with the merged first-party + plugin list.
export const MODULES = [
  { id: 'qc',           view_id: 'qc',           name: 'QC Analysis',           icon: 'microscope',  color: 'teal',   tool: 'fastqc-rs',  status: 'ready', backend: 'qc',          source: 'builtin', has_native_view: true,  category: 'qc' },
  { id: 'trimming',     view_id: 'trimming',     name: 'Adapter Trimming',      icon: 'scissors',    color: 'blue',   tool: 'cutadapt-rs', status: 'ready', backend: 'trimming',    source: 'builtin', has_native_view: true,  category: 'trimming' },
  { id: 'star-align',   view_id: 'star-align',   name: 'Alignment & Quantification', icon: 'git-merge', color: 'purple', tool: 'STAR_rs',  status: 'ready', backend: 'star_align',  source: 'builtin', has_native_view: true,  category: 'alignment' },
  { id: 'differential', view_id: 'differential', name: 'Differential Expr.',    icon: 'flame',       color: 'coral',  tool: 'DESeq2_rs',  status: 'ready', backend: 'deseq2',      source: 'builtin', has_native_view: true,  category: 'differential' },
  { id: 'network',      view_id: 'network',      name: 'Network Analysis',      icon: 'share-2',     color: 'green',  tool: 'WGCNA_rs',   status: 'soon',  utility: true,                              source: 'builtin', has_native_view: true,  category: 'other' },
  { id: 'enrichment',   view_id: 'enrichment',   name: 'Enrichment',            icon: 'target',      color: 'slate',  tool: 'TBD',        status: 'soon',                                              source: 'builtin', has_native_view: true,  category: 'other' },
];

/**
 * Replace the contents of MODULES with the dynamic list from `list_modules`,
 * preserving the array reference so other modules' imports stay live.
 */
export function setBootstrapModules(descriptors) {
  MODULES.length = 0;
  for (const d of descriptors) MODULES.push(d);
  rebuildKnownViews();
}

export const COLOR_MAP = {
  teal:   '#0d7377', blue: '#3b6ea5', purple: '#7c5cbf', gold: '#b8860b',
  coral:  '#c9503c', green: '#2d8659', slate: '#5c7080', plug: '#5c7080',
};

export const KNOWN_VIEWS = new Set();
function rebuildKnownViews() {
  KNOWN_VIEWS.clear();
  ['dashboard', 'settings', 'gff-convert', 'star-index', 'star-align', 'chat']
    .forEach(v => KNOWN_VIEWS.add(v));
  MODULES.forEach(m => KNOWN_VIEWS.add(m.view_id || m.id));
}
rebuildKnownViews();

export const ECHART_THEME = {
  backgroundColor: '#faf8f4',
  textStyle: { fontFamily: 'Karla, sans-serif', color: '#57534e' },
  title: { textStyle: { fontFamily: 'Zilla Slab, serif', fontSize: 15, color: '#1c1917' } },
  grid: { left: 60, right: 24, top: 44, bottom: 50 },
  toolbox: {
    feature: {
      saveAsImage: { title: 'Save PNG', pixelRatio: 2 },
      dataZoom: { title: { zoom: 'Zoom', back: 'Reset' } },
    },
    right: 20, top: 10,
  },
};

export const LOG_BUFFER_MAX = 500;
export const MAX_COMPUTE_LOAD = 8;

export const RUN_TASKS = {
  qc: { backend: 'qc', computeCost: 4 },
  trimming: { backend: 'trimming', computeCost: 4 },
  differential: { backend: 'deseq2', computeCost: 2 },
  'star-index': { backend: 'star_index', computeCost: 6 },
  'star-align': { backend: 'star_align', computeCost: 7 },
  'gff-convert': { backend: 'gff_convert', computeCost: 1 },
};
```

> **Compatibility:** descriptors from the backend already use `id` (backend id) and `view_id`. The old `backend` field is filled by `id`; downstream code reading `mod.backend` keeps working if we map `descriptor.id` → `descriptor.backend = descriptor.id` during ingestion. Add this in `setBootstrapModules`:

```js
export function setBootstrapModules(descriptors) {
  MODULES.length = 0;
  for (const d of descriptors) {
    MODULES.push({
      ...d,
      backend: d.id,
      status: 'ready',
      // color: pick by category for plugins; keep existing for built-ins
      color: d.source === 'builtin' ? colorForBuiltin(d.id) : 'plug',
    });
  }
  rebuildKnownViews();
}
function colorForBuiltin(id) {
  return ({ qc:'teal', trimming:'blue', star_align:'purple', deseq2:'coral',
            star_index:'purple', gff_convert:'gold' })[id] || 'slate';
}
```

- [ ] **Step 3: Add `listModules` and `getPluginManifest` to `frontend/js/api/modules.js`**

Append:

```js
export const modulesApi = {
  // ...existing methods...
  listModules: () => window.__TAURI__.core.invoke('list_modules'),
  getPluginManifest: (id) => window.__TAURI__.core.invoke('get_plugin_manifest', { id }),
};
```

(If the file already uses an `export const modulesApi = { ... }` object, just add the two methods inside it.)

- [ ] **Step 4: Create `frontend/js/api/plugins.js`**

```js
export const pluginsApi = {
  listStatus: () => window.__TAURI__.core.invoke('list_plugin_status'),
  reload: () => window.__TAURI__.core.invoke('reload_plugins'),
};
```

- [ ] **Step 5: Modify boot to fetch modules before first navigation**

In the boot file (likely `frontend/js/main.js` — confirm via grep), wrap the initial navigation:

```js
import { modulesApi } from './api/modules.js';
import { setBootstrapModules } from './core/constants.js';

async function boot() {
  try {
    const descriptors = await modulesApi.listModules();
    setBootstrapModules(descriptors);
  } catch (e) {
    console.warn('list_modules failed; falling back to static module list', e);
  }
  // ...existing navigate(currentRoute) call...
}
boot();
```

If a static MODULES export was being read at module-init time elsewhere, those references will still see the mutated array because we replace contents in place.

- [ ] **Step 6: Manually verify in dev mode**

Run: `cd crates/rb-app && cargo tauri dev`
Expected: app launches; sidebar shows the same 6 modules as before; no console errors.

- [ ] **Step 7: Commit**

```bash
git add frontend/js/core/constants.js frontend/js/api/modules.js frontend/js/api/plugins.js frontend/js/main.js
git commit -m "feat(frontend): dynamic MODULES from list_modules backend"
```

---

## Task 15: Generic plugin view — form generator

**Files:**
- Create: `frontend/js/modules/plugin/view.js`
- Modify: `frontend/js/core/router.js`

- [ ] **Step 1: Create `frontend/js/modules/plugin/view.js`**

```js
import { state } from '../../core/state.js';
import { getLang } from '../../core/i18n-helpers.js';
import { renderLogPanel } from '../../ui/log-panel.js';
import { renderModuleHeader } from '../module-header.js';
import { modulesApi } from '../../api/modules.js';
import { escapeHtml } from '../../ui/escape.js';

export async function renderPluginView(container, viewId) {
  container.innerHTML = `<div class="module-view"><p>Loading…</p></div>`;
  let manifest;
  try {
    manifest = await modulesApi.getPluginManifest(viewId);
  } catch (e) {
    container.innerHTML = `<div class="module-view"><div class="error">Failed to load plugin manifest: ${escapeHtml(String(e))}</div></div>`;
    return;
  }
  const lang = getLang();
  const mod = {
    id: viewId,
    icon: manifest.icon || 'plug',
    color: 'plug',
    tool: manifest.binary_id,
    status: 'ready',
  };
  const header = renderModuleHeader(mod);
  const body = renderPluginBody(manifest, lang, viewId);
  container.innerHTML = `<div class="module-view">${header}${body}</div>`;
}

function localized(strings, key, lang, fallback) {
  if (!strings) return fallback;
  return strings[`${key}_${lang}`] || strings[`${key}_en`] || fallback;
}

function renderPluginBody(m, lang, viewId) {
  const desc = localized(m.strings, 'description', lang, m.description || '');
  const params = m.params || [];
  return `
    <div class="module-layout">
      <div>
        ${desc ? `<p class="module-intro">${escapeHtml(desc)}</p>` : ''}
        <div class="module-panel animate-slide-up">
          <div class="panel-header"><span class="panel-title">Parameters</span></div>
          <div class="panel-body">
            ${params.map(p => renderParam(p, lang, viewId)).join('')}
          </div>
          <div class="panel-footer">
            <button type="button" class="btn btn-secondary btn-sm" data-act="reset-form" data-mod="${viewId}"><i data-lucide="rotate-ccw"></i> Reset</button>
            <button type="button" class="btn btn-primary btn-sm" data-act="run-module" data-mod="${viewId}" data-run-button data-run-button-act="run-module" data-run-button-type="button"><i data-lucide="play"></i> Run</button>
          </div>
          ${renderLogPanel(viewId)}
        </div>
      </div>
      <div>
        <div class="module-panel animate-slide-up" style="animation-delay:160ms">
          <div class="panel-header"><span class="panel-title">Runs</span></div>
          <div class="panel-body"><div id="${viewId}-runs"></div></div>
        </div>
      </div>
    </div>
  `;
}

function renderParam(p, lang, viewId) {
  const label = localized(p, 'label', lang, p.name);
  const help = localized(p, 'help', lang, '');
  const ui = p.ui || defaultUiForType(p.type);
  const dataParam = `data-param="${escapeHtml(p.name)}"`;

  if (p.type === 'file_list' || (p.type === 'file' && ui === 'drop_zone')) {
    const single = p.type === 'file' ? 'data-param-single' : '';
    return `
      <div class="form-group">
        <label class="form-label">${escapeHtml(label)}${p.required ? ' *' : ''}</label>
        <div class="file-drop-zone" data-module="${viewId}" ${dataParam} ${single}>
          <div class="file-drop-icon"><i data-lucide="upload-cloud"></i></div>
          <div class="file-drop-text">Drop files here or click to browse</div>
        </div>
        <div class="file-list" id="${viewId}-${p.name}-list"></div>
        ${help ? `<small class="form-help">${escapeHtml(help)}</small>` : ''}
      </div>`;
  }
  if (p.type === 'boolean') {
    return `
      <div class="form-group">
        <label class="form-checkbox">
          <input type="checkbox" ${dataParam} ${p.default ? 'checked' : ''}>
          ${escapeHtml(label)}
        </label>
        ${help ? `<small class="form-help">${escapeHtml(help)}</small>` : ''}
      </div>`;
  }
  if (p.type === 'enum') {
    const opts = (p.values || []).map(v => `<option value="${escapeHtml(v)}" ${p.default === v ? 'selected' : ''}>${escapeHtml(v)}</option>`).join('');
    return `
      <div class="form-group">
        <label class="form-label">${escapeHtml(label)}</label>
        <select class="form-select" ${dataParam}>${opts}</select>
        ${help ? `<small class="form-help">${escapeHtml(help)}</small>` : ''}
      </div>`;
  }
  if (p.type === 'integer') {
    return `
      <div class="form-group">
        <label class="form-label">${escapeHtml(label)}</label>
        <input type="number" class="form-input" ${dataParam} value="${p.default ?? ''}" ${p.minimum != null ? `min="${p.minimum}"` : ''} ${p.maximum != null ? `max="${p.maximum}"` : ''}>
        ${help ? `<small class="form-help">${escapeHtml(help)}</small>` : ''}
      </div>`;
  }
  // string / output_dir / directory / file (path input)
  return `
    <div class="form-group">
      <label class="form-label">${escapeHtml(label)}${p.required ? ' *' : ''}</label>
      <input type="text" class="form-input" ${dataParam} value="${escapeHtml(p.default ?? '')}" placeholder="${escapeHtml(label)}">
      ${help ? `<small class="form-help">${escapeHtml(help)}</small>` : ''}
    </div>`;
}

function defaultUiForType(type) {
  switch (type) {
    case 'file_list': return 'drop_zone';
    case 'boolean': return 'checkbox';
    case 'enum': return 'select';
    case 'integer': return 'number';
    default: return 'text';
  }
}
```

- [ ] **Step 2: Modify `frontend/js/core/router.js`** — add a generic plugin dispatch path

In `renderModuleView` (or wherever the module-id switch lives), after the existing `switch (moduleId) { ... }`, before the default, add a fallback that checks if the module is a plugin:

Find the relevant function (search: `renderModuleView` or `case 'qc'`). Modify the default branch:

```js
async function renderModuleView(content, moduleId) {
  const mod = MODULES.find(m => m.id === moduleId || m.view_id === moduleId);
  if (!mod) {
    content.innerHTML = `<div class="module-view">${renderEmptyState(t('common.module_not_found'))}</div>`;
    return;
  }
  if (mod.status === 'soon') {
    const { renderModuleHeader } = await import('../modules/module-header.js');
    content.innerHTML = `<div class="module-view">${renderModuleHeader(mod)}${renderComingSoon(mod)}</div>`;
    return;
  }
  if (!mod.has_native_view) {
    const { renderPluginView } = await import('../modules/plugin/view.js');
    if (state.currentView === moduleId) await renderPluginView(content, moduleId);
    return;
  }
  switch (moduleId) {
    // ...existing cases...
  }
}
```

- [ ] **Step 3: Manually verify**

Run: `cd crates/rb-app && cargo tauri dev`
With no plugins yet, navigate around — confirm built-in modules still work.

- [ ] **Step 4: Commit**

```bash
git add frontend/js/modules/plugin/view.js frontend/js/core/router.js
git commit -m "feat(frontend): generic plugin view from manifest"
```

---

## Task 16: Generic plugin result view + run-result integration

**Files:**
- Create: `frontend/js/modules/plugin/result.js`
- Modify: `frontend/js/modules/run-result.js`

- [ ] **Step 1: Create `frontend/js/modules/plugin/result.js`**

```js
import { escapeHtml } from '../../ui/escape.js';

export function renderPluginResult(result, runId) {
  const summary = result.summary || {};
  const outputDir = summary.output_dir || '';
  const outputs = summary.output_files || result.output_files || [];
  const argv = summary.argv || [];
  const exitCode = summary.exit_code;

  return `
    <div class="plugin-result">
      <div class="result-status-card">
        <div class="status-row"><strong>Status:</strong> Done${exitCode != null ? ` (exit ${exitCode})` : ''}</div>
        ${outputDir ? `<div class="status-row"><strong>Output dir:</strong> <code>${escapeHtml(outputDir)}</code></div>` : ''}
      </div>
      ${renderOutputsList(outputs)}
      ${argv.length ? `<details><summary>Command</summary><pre>${escapeHtml(argv.join(' '))}</pre></details>` : ''}
    </div>
  `;
}

function renderOutputsList(files) {
  if (!files || !files.length) {
    return `<p><em>No output files matched the manifest patterns.</em></p>`;
  }
  const rows = files.map(f => {
    const path = typeof f === 'string' ? f : (f.path || String(f));
    const ext = (path.split('.').pop() || '').toLowerCase();
    return `
      <li class="output-file">
        <code>${escapeHtml(path)}</code>
        <span class="output-actions">
          ${renderOpenButton(path, ext)}
          <button type="button" class="btn btn-ghost btn-sm" data-act="show-in-folder" data-path="${escapeHtml(path)}">
            <i data-lucide="folder-open"></i> Show
          </button>
        </span>
      </li>`;
  }).join('');
  return `<ul class="output-list">${rows}</ul>`;
}

function renderOpenButton(path, ext) {
  if (ext === 'html') {
    return `<button type="button" class="btn btn-secondary btn-sm" data-act="open-external" data-path="${escapeHtml(path)}"><i data-lucide="external-link"></i> Open</button>`;
  }
  if (['json', 'tsv', 'csv', 'txt'].includes(ext)) {
    return `<button type="button" class="btn btn-secondary btn-sm" data-act="open-external" data-path="${escapeHtml(path)}"><i data-lucide="file-text"></i> Open</button>`;
  }
  return '';
}
```

- [ ] **Step 2: Wire into `frontend/js/modules/run-result.js`**

Modify the switch:

```js
import { renderPluginResult } from './plugin/result.js';
import { MODULES } from '../core/constants.js';

export function renderRunResultHtml(moduleId, result, runId) {
  const mod = MODULES.find(m => m.id === moduleId || m.view_id === moduleId);
  if (mod && !mod.has_native_view) {
    return renderPluginResult(result, runId);
  }
  let html = '';
  switch (moduleId) {
    case 'qc': html = renderQcResult(result, runId); break;
    case 'gff_convert': html = renderGffConvertResult(result, runId); break;
    case 'star_align': html = renderStarAlignResult(result, runId); break;
    case 'star_index': html = `<pre>${escapeHtml(JSON.stringify(result.summary, null, 2))}</pre>`; break;
    default: html = `<pre>${escapeHtml(JSON.stringify(result, null, 2))}</pre>`; break;
  }
  return html;
}
```

- [ ] **Step 3: Add the `open-external` and `show-in-folder` action handlers**

In `frontend/js/core/events.js` (the `dispatchAction` function), add cases:

```js
case 'open-external': {
  const path = el.dataset.path;
  if (path) window.__TAURI__.opener?.openPath(path);
  return;
}
case 'show-in-folder': {
  const path = el.dataset.path;
  if (path) window.__TAURI__.opener?.revealItemInDir(path);
  return;
}
```

> **Note:** these require the Tauri `opener` plugin. If not already enabled, add `tauri-plugin-opener = "2"` to `crates/rb-app/Cargo.toml` and `.plugin(tauri_plugin_opener::init())` to the builder. Check `crates/rb-app/Cargo.toml` first to see if already present.

- [ ] **Step 4: If opener plugin is missing, add it**

In `crates/rb-app/Cargo.toml` `[dependencies]`:
```toml
tauri-plugin-opener = "2"
```

In `crates/rb-app/src/main.rs` `Builder::default()` chain:
```rust
.plugin(tauri_plugin_opener::init())
```

Add to capabilities (`crates/rb-app/capabilities/default.json`):
```json
{
  "permissions": [
    "core:default",
    "opener:default",
    "opener:allow-open-path",
    "opener:allow-reveal-item-in-dir"
  ]
}
```

(Adjust based on existing capabilities file.)

- [ ] **Step 5: Manually verify**

Run app, no plugins yet — confirm built-in modules still render their results correctly (no plugin path triggered).

- [ ] **Step 6: Commit**

```bash
git add frontend/js/modules/plugin/result.js frontend/js/modules/run-result.js frontend/js/core/events.js crates/rb-app/Cargo.toml crates/rb-app/src/main.rs crates/rb-app/capabilities/
git commit -m "feat(frontend): generic plugin result view + opener actions"
```

---

## Task 17: Sidebar plugin badge + missing-binary guidance

**Files:**
- Create: `frontend/js/modules/plugin/missing-binary.js`
- Modify: `frontend/js/modules/plugin/view.js`
- Modify: the sidebar render (search for where `MODULES` is rendered into nav items)

- [ ] **Step 1: Find the sidebar renderer**

Run: `grep -rn 'nav-item' /home/xzg/project/rust_brain/frontend/js/ | head` — locate the file that renders sidebar entries from `MODULES`.

- [ ] **Step 2: Add a plugin badge in the sidebar render**

Wherever each nav item is templated, when `mod.source !== 'builtin'`, append a small badge:

```js
// inside the nav-item template:
${mod.source !== 'builtin' ? '<span class="nav-plug-badge" title="Third-party plugin"><i data-lucide="plug"></i></span>' : ''}
```

Add a tiny CSS rule in `frontend/css/style.css` (or whichever stylesheet the sidebar uses):

```css
.nav-plug-badge { margin-left: 6px; opacity: 0.6; display: inline-flex; align-items: center; }
.nav-plug-badge i { width: 12px; height: 12px; }
```

- [ ] **Step 3: Create `frontend/js/modules/plugin/missing-binary.js`**

```js
import { escapeHtml } from '../../ui/escape.js';
import { navigate } from '../../core/router.js';

export function renderMissingBinaryCard(manifest) {
  const binaryId = manifest.binary_id;
  const name = manifest.name;
  return `
    <div class="missing-binary-card">
      <div class="missing-binary-icon"><i data-lucide="alert-triangle"></i></div>
      <h2>${escapeHtml(name)} needs a binary path</h2>
      <p>Plugin <code>${escapeHtml(manifest.id)}</code> depends on the binary <code>${escapeHtml(binaryId)}</code>, which is not configured and not on PATH.</p>
      <button type="button" class="btn btn-primary" data-act="goto-settings"><i data-lucide="settings"></i> Open Settings</button>
    </div>
  `;
}
```

- [ ] **Step 4: Show the card in `plugin/view.js` when binary is missing**

After fetching the manifest, also fetch binary status:

```js
import { binaryApi } from '../../api/binary.js';
import { renderMissingBinaryCard } from './missing-binary.js';

// inside renderPluginView, after manifest loaded:
const binaries = await binaryApi.getPaths();
const myBinary = binaries.find(b => b.id === manifest.binary_id);
const ok = myBinary && (myBinary.configured_path || myBinary.bundled_path || myBinary.detected_on_path);
if (!ok) {
  container.innerHTML = `<div class="module-view">${header}${renderMissingBinaryCard(manifest)}</div>`;
  return;
}
```

- [ ] **Step 5: Add `goto-settings` action**

In `frontend/js/core/events.js`:
```js
case 'goto-settings': {
  navigate('settings');
  return;
}
```
(Import `navigate` at the top if not already.)

- [ ] **Step 6: Manually verify**

(No plugins to test against yet — visual verification deferred to Task 19.)

- [ ] **Step 7: Commit**

```bash
git add frontend/js/modules/plugin/ frontend/js/core/events.js frontend/css/
git commit -m "feat(frontend): plugin sidebar badge + missing-binary guidance"
```

---

## Task 18: Settings panel — Plugins section

**Files:**
- Create: `frontend/js/modules/settings/plugins.js`
- Modify: `frontend/js/modules/settings/view.js`
- Modify: `frontend/js/i18n.js` (add strings)

- [ ] **Step 1: Create `frontend/js/modules/settings/plugins.js`**

```js
import { t } from '../../core/i18n-helpers.js';
import { escapeHtml } from '../../ui/escape.js';
import { pluginsApi } from '../../api/plugins.js';

export async function renderPluginsSection() {
  let diag;
  try {
    diag = await pluginsApi.listStatus();
  } catch (e) {
    return { html: `<div class="error">${t('settings.plugins_load_failed')}: ${escapeHtml(String(e))}</div>`, bind: () => {} };
  }
  const bundled = (diag.loaded || []).filter(p => p.source === 'bundled');
  const user = (diag.loaded || []).filter(p => p.source === 'user');
  const errors = diag.errors || [];

  const html = `
    <div class="module-panel animate-slide-up" style="animation-delay:200ms">
      <div class="panel-header"><span class="panel-title">${t('settings.plugins_section')}</span></div>
      <div class="panel-body">
        <div class="plugins-group">
          <h4>${t('settings.plugins_bundled')}</h4>
          ${renderList(bundled)}
        </div>
        <div class="plugins-group">
          <h4>${t('settings.plugins_user')}</h4>
          ${renderList(user)}
        </div>
        ${errors.length ? `
          <div class="plugins-group">
            <h4>${t('settings.plugins_errors')}</h4>
            <ul class="plugin-errors">
              ${errors.map(e => `<li><code>${escapeHtml(e.source_label)}</code>: ${escapeHtml(e.message)}</li>`).join('')}
            </ul>
          </div>` : ''}
        <div class="plugins-actions">
          <button type="button" class="btn btn-secondary btn-sm" data-act="reload-plugins">
            <i data-lucide="refresh-cw"></i> ${t('settings.plugins_reload')}
          </button>
        </div>
      </div>
    </div>
  `;
  return { html, bind: () => {} };
}

function renderList(items) {
  if (!items.length) return `<p><em>${escapeHtml('(none)')}</em></p>`;
  return `<ul class="plugins-list">
    ${items.map(p => `
      <li>
        <strong>${escapeHtml(p.id)}</strong>
        ${p.description ? ` — ${escapeHtml(p.description)}` : ''}
        <small> · binary: <code>${escapeHtml(p.binary_id)}</code></small>
      </li>`).join('')}
  </ul>`;
}
```

- [ ] **Step 2: Wire into `frontend/js/modules/settings/view.js`**

Import + render below the AI section:

```js
import { renderPluginsSection } from './plugins.js';

// inside renderSettingsView, after aiSection:
const pluginsSection = await renderPluginsSection();

container.innerHTML = `
  <div class="module-view">
    ${settingsHeader()}
    <!-- existing binary table panel -->
    <!-- existing language panel -->
    ${aiSection.html}
    ${pluginsSection.html}
  </div>
`;
await aiSection.bind(container);
pluginsSection.bind(container);
```

- [ ] **Step 3: Add the `reload-plugins` action handler**

In `frontend/js/core/events.js`:

```js
case 'reload-plugins': {
  el.disabled = true;
  pluginsApi.reload()
    .then(() => navigate('settings'))   // re-render the settings page
    .catch(err => alert(`${t('status.error_prefix')}: ${err}`))
    .finally(() => { el.disabled = false; });
  return;
}
```

(Import `pluginsApi` from `../api/plugins.js` and `navigate` from `./router.js`.)

- [ ] **Step 4: Add i18n strings to `frontend/js/i18n.js`**

In both `en` and `zh` `settings` blocks, add:

```js
plugins_section: 'Plugins',          // zh: '插件'
plugins_bundled: 'Bundled',          // zh: '内置'
plugins_user: 'User',                // zh: '用户'
plugins_errors: 'Load errors',       // zh: '加载错误'
plugins_reload: 'Reload plugins',    // zh: '重新加载插件'
plugins_load_failed: 'Failed to load plugins',  // zh: '加载插件失败'
```

- [ ] **Step 5: Manually verify**

Run app, open Settings — Plugins section appears (empty bundled list, empty user list, no errors).

- [ ] **Step 6: Commit**

```bash
git add frontend/js/modules/settings/plugins.js frontend/js/modules/settings/view.js frontend/js/core/events.js frontend/js/i18n.js
git commit -m "feat(frontend): Settings → Plugins panel"
```

---

## Task 19: Ship the bundled RustQC plugin

**Files:**
- Create: `crates/rb-app/plugins/rustqc.toml`

- [ ] **Step 1: Copy the validated fixture into the bundled directory**

```bash
cp crates/rb-plugin/tests/data/rustqc.toml crates/rb-app/plugins/rustqc.toml
```

- [ ] **Step 2: Verify the bundled manifest is now embedded**

Run: `cargo build -p rb-app`
Expected: clean build. `include_dir!` picks up the new file at compile time.

- [ ] **Step 3: Manual end-to-end smoke test**

1. Build & launch: `cd crates/rb-app && cargo tauri dev`
2. Sidebar should show **RustQC** under the QC group with a plug badge.
3. Click RustQC: missing-binary card appears (binary not configured yet).
4. Open Settings → binary table now contains a **RustQC** row → set it to a valid path (or to `/bin/echo` for a smoke test).
5. Re-open RustQC view — form renders: input_files drop-zone, threads input, nogroup checkbox, format select, output_dir text, extra_args text.
6. Drop a small `.fastq.gz`, click Run.
7. Logs stream into the panel; on exit, runs panel shows status + output file list (HTML/JSON/zip if real RustQC; nothing if /bin/echo).

If any of the above fails, fix the relevant earlier task before continuing.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-app/plugins/rustqc.toml
git commit -m "feat(plugin): ship bundled RustQC manifest"
```

---

## Task 20: Smoke checklist doc

**Files:**
- Create: `docs/superpowers/smoke/2026-04-21-rustqc-plugin-smoke.md`

- [ ] **Step 1: Write the smoke checklist**

```markdown
# RustQC Plugin — Smoke Checklist

Manual end-to-end test for the third-party tool plugin system shipped via the
RustQC bundled manifest.

## Prerequisites

- A built RustQC binary (Linux/macOS) downloaded from
  https://seqeralabs.github.io/RustQC/ and somewhere readable.
- A small `.fastq.gz` sample (any small file works for the smoke pass).

## Steps

1. **Boot:** `cd crates/rb-app && cargo tauri dev` — app launches without errors.
2. **Sidebar:** RustQC entry appears under the QC group with a 🔌 plug badge.
3. **Missing binary:** Clicking RustQC shows the "needs a binary path" guidance
   card with a button to open Settings.
4. **Settings → Binaries:** The table now lists RustQC alongside `star`,
   `cutadapt-rs`, etc. Click "Browse" and select your RustQC binary.
5. **Settings → Plugins:** Bundled section lists `rustqc`. No load errors.
6. **Re-open RustQC view:** Now renders the parameter form (drop zone, threads,
   nogroup checkbox, format select, output dir, extra args).
7. **Run:** Drop a FASTQ file, click Run. Toast confirms run started. Log panel
   streams stderr lines.
8. **Result:** When done, runs panel shows the run with a generic plugin
   result view: status, output dir, output file list with "Open" / "Show"
   buttons. HTML report opens in default browser.
9. **AI integration:** Open Chat. Ask "use RustQC on this file" — the assistant
   should call `run_rustqc` (the auto-derived tool from the manifest) with
   the right `input_files` argument.
10. **Reload:** Drop a custom `.toml` into `<config_dir>/rust_brain/plugins/`,
    open Settings → Plugins → click "Reload plugins". Sidebar refreshes.
```

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/smoke/
git commit -m "docs: RustQC plugin smoke checklist"
```

---

## Task 21: README updates

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add a Plugins section to README.md** (under the Features list, before the Development Setup or wherever architecture is described)

Insert a section like:

```markdown
## Plugins (third-party tools)

Beyond the first-party modules, RustBrain supports declarative TOML plugins
that wrap any external CLI tool — RustQC ships bundled. Drop a `.toml`
manifest into `<config_dir>/rust_brain/plugins/` and reload from Settings to
add your own. See
[`docs/superpowers/specs/2026-04-21-third-party-tool-plugins-design.md`](docs/superpowers/specs/2026-04-21-third-party-tool-plugins-design.md)
for the manifest format.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mention plugin system in README"
```

---

## Self-Review

Going through the spec section-by-section to confirm coverage:

| Spec section                              | Covered by tasks                |
| ----------------------------------------- | ------------------------------- |
| `rb-plugin` crate layout                  | Tasks 1, 2, 3, 4, 5, 6, 7, 8    |
| TOML manifest format                      | Task 2                          |
| Manifest validation rules                 | Task 3                          |
| Argv builder (5 CLI rule shapes)          | Task 4 (+ 8 unit tests)         |
| Loader with bundled + user dirs, dedupe   | Task 6                          |
| ExternalToolModule + subprocess           | Tasks 7, 8                      |
| BinaryResolver runtime registration       | Task 9                          |
| BinaryResolver collision rule (built-in wins) | Task 9 test               |
| AppState wiring + AI tool registry        | Task 10                         |
| `list_modules` Tauri command              | Task 11                         |
| `list_plugin_status` / `get_plugin_manifest` | Task 12                      |
| `reload_plugins`                          | Task 13                         |
| Frontend dynamic MODULES                  | Task 14                         |
| Generic plugin form view                  | Task 15                         |
| Generic plugin result view + opener       | Task 16                         |
| Sidebar plug badge + missing-binary card  | Task 17                         |
| Settings Plugins panel                    | Task 18                         |
| Bundled RustQC manifest                   | Task 19                         |
| Smoke checklist                           | Task 20                         |
| README mention                            | Task 21                         |

Spec items deferred deliberately:
- Per-project plugin dir → out of scope per spec non-goals.
- Plugin marketplace → out of scope per spec non-goals.
- iframe HTML embedding for results → out of scope per spec non-goals.
- Stdout redirection → out of scope per spec non-goals.

No `TBD`/`TODO`/"implement later" placeholders remain in the plan body.

Type-name consistency check:
- `KnownBinaryEntry` (Task 9) used in Task 10 step 5 ✓
- `LazyResolvingPluginModule` (Task 10) used in Task 11 / 12 / 13 ✓
- `PluginDiagnostics`, `PluginSourceTag`, `PluginErrorView` (Task 10) used in Tasks 12, 13 ✓
- `setBootstrapModules` (Task 14) used in Task 14 boot only ✓
- `renderPluginView` (Task 15), `renderPluginResult` (Task 16), `renderMissingBinaryCard` (Task 17) — all imported by their consumers in matching tasks ✓
- `pluginsApi.listStatus` / `.reload` (Task 14) used in Task 18 ✓
- `BUNDLED_PLUGINS` (Task 13 step 4) — declared as `pub static` so commands/plugins.rs can reference `crate::BUNDLED_PLUGINS` ✓

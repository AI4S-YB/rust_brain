# Third-Party Tool Plugins — Design

**Date:** 2026-04-21
**Status:** approved (brainstorm phase)
**Motivating example:** [seqeralabs/RustQC](https://seqeralabs.github.io/RustQC/) — a Rust FastQC re-implementation that does **not** ship a Windows build, so we cannot bundle it as a first-party module the way we do `STAR_rs` / `gffread-rs`. We need a way for Linux/macOS users to drop in the binary and use it without us shipping per-platform adapter crates.

## Problem

Today, "supporting" an external bioinformatics tool requires **two** things:
1. Register the binary in `KNOWN_BINARIES` (`crates/rb-core/src/binary.rs`)
2. Hand-write a Rust adapter crate implementing the `Module` trait (`rb-qc`, `rb-trimming`, `rb-star-align`, …) and register it in `rb-app/src/main.rs`

That's a heavy bar for tools we don't want to commit to maintaining ourselves — niche tools, platform-specific tools (RustQC), in-house lab scripts, or anything a user wants to wire up themselves. There is no path for "user downloads a binary and uses it" without shipping a release.

## Goals

- A **plugin** mechanism: a declarative TOML manifest turns an external command-line tool into a runnable module — no Rust code, no rebuild.
- Plugins are **first-class** in the UI: they appear in the sidebar alongside first-party modules, render a parameter form, stream logs, store run history, and surface results — all by reusing existing module machinery.
- Two delivery channels: **bundled** (manifests we curate and ship with the app) and **user** (manifests dropped into a config directory). User wins on id collision.
- A plugin's binary participates in the existing `BinaryResolver` flow — Settings shows the same install/configure UX as for `star`, `cutadapt-rs`, `gffread-rs`.
- The AI copilot (`rb-ai`) auto-discovers plugin tools the same way it discovers first-party `run_*` tools, via derived JSON Schema + `ai_hint`.

## Non-goals (explicit boundaries to prevent plugin-system bloat)

- **No specialized result rendering for plugins.** First-party modules (e.g. `rb-qc`'s structured FastQC report → ECharts) parse JSON and render charts because we wrote the adapter. Plugins get a generic result view: status card + output file list + log panel. If a plugin tool deserves rich result UI, it graduates to a first-party crate.
- **No iframe HTML embedding.** `.html` outputs are surfaced as "open in browser" buttons, not embedded — same reason as above (first-party privilege, complexity ceiling).
- **No stdout/stderr redirection in v1.** Plugins are single-process `argv` invocations. Tools that depend on shell redirection (`bwa mem ... > out.sam`) are out of scope for v1; users can wrap them in a shell script that becomes the binary.
- **No hot reload.** Plugins are scanned once at startup. A "Reload plugins" button in Settings re-scans without app restart, but the existing `ModuleRegistry` state is rebuilt — runs in flight are unaffected because they hold `Arc<dyn Module>` already.
- **No plugin marketplace / remote install.** v1 is local files only. Sharing happens by sending someone a `.toml`.
- **No per-project plugins.** Plugins are tool capabilities (like binary paths), not project state. If projects ever need private plugins, a third layer can be added later (`<project>/plugins/`) without breaking the v1 contract.
- **No conditional / templated CLI logic** (no `if`/`unless` blocks, no Jinja). Each param has a fixed CLI rendering rule. Tools needing conditional argv either get a wrapper script or graduate to first-party.

## Architecture

### Crate layout

New workspace member: `crates/rb-plugin/` (sibling of `rb-core`, depends on `rb-core`).

```
crates/rb-plugin/
├── Cargo.toml
└── src/
    ├── lib.rs              # public API: load_plugins(), PluginRegistry
    ├── manifest.rs         # TOML schema: PluginManifest, ParamSpec, CliRule, OutputSpec
    ├── loader.rs           # scan dirs, parse + validate manifests, dedupe by id (user > bundled)
    ├── module.rs           # ExternalToolModule — implements rb_core::Module from a manifest
    ├── argv.rs             # build Vec<String> argv from manifest + params Value
    ├── schema.rs           # derive JSON Schema from PluginManifest.params (for validate_params + AI tool registry)
    └── subprocess.rs       # tokio::process::Command runner, stderr→RunEvent::Log,
                            #   cooperative cancel via tokio::select! on child.wait() vs cancel.cancelled()
```

`rb-app` depends on `rb-plugin`. Bundled manifests live at `crates/rb-app/plugins/*.toml` and are embedded via the `include_dir` crate so they ship inside the binary — no install path concerns on Windows/macOS/Linux. User manifests live at `<config_dir>/rust_brain/plugins/*.toml` (same parent as `settings.json`).

### Manifest format

A complete RustQC manifest, illustrating every supported field:

```toml
# Identity
id          = "rustqc"                 # unique; collides → user overrides bundled, error otherwise
name        = "RustQC"
description = "Rust re-implementation of FastQC for read quality assessment."
category    = "qc"                     # used to group in sidebar; reuses first-party categories
icon        = "shield-check"           # lucide icon name; falls back to "plug" if missing
version     = "0.1.0"                  # manifest schema version, not tool version (currently always "0.1.0")

# Localized strings (en is required, zh optional → falls back to en)
[strings]
name_en = "RustQC"
name_zh = "RustQC 质量控制"
description_en = "Rust re-implementation of FastQC for read quality assessment."
description_zh = "FastQC 的 Rust 重写版本，用于读段质量评估。"
ai_hint_en = "Run RustQC for FASTQ quality assessment. Cross-platform alternative to fastqc-rs on Linux/macOS where Windows binaries are unavailable."
ai_hint_zh = "用 RustQC 做 FASTQ 质量评估。Linux/macOS 上可作为 fastqc 的跨平台替代。"

# Binary dependency — auto-registered into BinaryResolver at plugin load time
[binary]
id           = "rustqc"                # used as both PATH lookup name and settings.json key
display_name = "RustQC"
install_hint = "Download from https://seqeralabs.github.io/RustQC/ — drop on PATH or set the path in Settings."

# Parameters — order in this list = order in argv
[[params]]
name         = "input_files"
type         = "file_list"             # string | integer | boolean | file | file_list | directory | enum | output_dir
required     = true
ui           = "drop_zone"             # drop_zone | text | number | checkbox | select | path
label_en     = "Input FASTQ files"
label_zh     = "输入 FASTQ 文件"
help_en      = "One or more FASTQ files (.fastq, .fq, .fastq.gz)."
cli          = { flag = "-i", repeat_per_value = true }   # → -i a.fq -i b.fq

[[params]]
name         = "threads"
type         = "integer"
default      = 4
minimum      = 1
label_en     = "Threads"
label_zh     = "线程数"
cli          = { flag = "--threads" }                      # → --threads 4

[[params]]
name         = "nogroup"
type         = "boolean"
default      = false
label_en     = "Disable base grouping"
cli          = { flag = "--nogroup" }                      # appears only when true; no value token

[[params]]
name         = "format"
type         = "enum"
values       = ["fastq", "bam", "sam"]
default      = "fastq"
cli          = { flag = "--format" }

[[params]]
name         = "output_dir"
type         = "output_dir"            # special; if user leaves blank → <run_dir>/output, auto-created
cli          = { flag = "-o" }

[[params]]
name         = "extra_args"
type         = "string"
default      = ""
ui           = "text"
label_en     = "Extra arguments"
help_en      = "Appended verbatim (shlex-split). Escape hatch for flags not exposed above."
cli          = { raw = true }                              # shlex::split(value) appended as-is

# Output discovery — globs evaluated relative to the resolved output_dir
[outputs]
patterns = ["*.html", "*.json", "*.zip"]
```

**Type system.** Eight param types — chosen to match every concrete first-party param in the repo today:

| Type         | UI default              | Validation                 | CLI value rendering           |
| ------------ | ----------------------- | -------------------------- | ----------------------------- |
| `string`     | `text`                  | none                       | as-is                         |
| `integer`    | `number`                | min/max bounds             | string form                   |
| `boolean`    | `checkbox`              | none                       | flag-only when true           |
| `file`       | `path` (single)         | path must exist            | as-is                         |
| `file_list`  | `drop_zone` (multi)     | non-empty if required      | repeat or join — see CliRule  |
| `directory`  | `path` (dir picker)     | path must be dir           | as-is                         |
| `enum`       | `select`                | value in `values`          | as-is                         |
| `output_dir` | `path` (or auto-derive) | parent must exist          | as-is, auto-mkdir before run  |

**CLI rule grammar.** Five mutually-exclusive shapes:

| Shape                                              | Argv emitted                           |
| -------------------------------------------------- | -------------------------------------- |
| `cli = { flag = "--xyz" }`                         | `["--xyz", VALUE]` (or `["--xyz"]` for true bool, omitted for false bool) |
| `cli = { flag = "-i", repeat_per_value = true }`   | `["-i", v1, "-i", v2, …]`              |
| `cli = { flag = "--xyz", join_with = "," }`        | `["--xyz", "v1,v2,v3"]`                |
| `cli = { positional = true }`                      | `[v1, v2, …]` (no flag)                |
| `cli = { raw = true }`                             | `shlex::split(value)` appended as-is   |

This intentionally has **no conditionals**. If a tool needs them, it gets a first-party adapter.

### Argv construction (`argv.rs`)

```
build_argv(manifest, params) -> Vec<String>:
    out = [resolved_binary_path]
    for spec in manifest.params (in declared order):
        v = params.get(spec.name) or spec.default
        if missing and spec.required: error
        if missing and not required: skip
        out.extend(render_cli(spec.cli, v))
    return out
```

No shell, no string templating, no joining-then-splitting. Argv is built directly and handed to `tokio::process::Command::args(...)`. **Zero shell-injection surface.**

### Loader (`loader.rs`)

```
load_plugins(bundled_dir: &include_dir::Dir, user_dir: Option<&Path>) -> PluginRegistry:
    let mut by_id = HashMap::new();   // id -> (PluginManifest, source)
    let mut errors = Vec::new();      // PluginLoadError per failed file (path, reason)

    for file in bundled_dir.files() if .toml:
        match parse + validate:
            Ok(m)  => by_id.insert(m.id, (m, Source::Bundled))
            Err(e) => errors.push(...)

    if let Some(dir) = user_dir, dir.exists():
        for file in fs::read_dir(dir) if .toml:
            match parse + validate:
                Ok(m)  => by_id.insert(m.id, (m, Source::User))   // overwrites bundled by id
                Err(e) => errors.push(...)

    PluginRegistry { by_id, errors }
```

Validation rejects: duplicate param `name`, unknown param `type`, mutually-exclusive CLI rule fields set together, missing required `[binary]`, missing `id`/`name`, unsupported manifest `version` (v1 accepts only `"0.1.0"`). Unknown top-level keys are warned (`PluginLoadError::Warning`) but not rejected — forward compat for future schema additions.

**Binary id collisions with built-in `KNOWN_BINARIES`** (e.g. a plugin re-declares `star`): the existing built-in entry wins; the plugin's `display_name` / `install_hint` are silently ignored (the plugin reuses the existing binary configuration). This lets a plugin legitimately depend on `star` without confusing the Settings UI.

**Unknown `category`** values are accepted; the frontend groups them under a default "Other" sidebar section.

### Module integration (`module.rs`)

```rust
pub struct ExternalToolModule {
    manifest: Arc<PluginManifest>,
    binary_id: String,
    schema_cache: serde_json::Value,        // derived once from manifest.params
}

#[async_trait]
impl Module for ExternalToolModule {
    fn id(&self) -> &str          { &self.manifest.id }
    fn name(&self) -> &str        { &self.manifest.name }
    fn params_schema(&self) -> Option<Value>  { Some(self.schema_cache.clone()) }
    fn ai_hint(&self, lang: &str) -> String   { manifest.strings.ai_hint(lang) }

    fn validate(&self, params: &Value) -> Vec<ValidationError> {
        validate_against_manifest(&self.manifest, params)
    }

    async fn run(&self, params, project_dir, events_tx, cancel) -> Result<ModuleResult, ModuleError> {
        // 1. resolve binary via shared BinaryResolver (held by AppState)
        // 2. resolve output_dir param (default: run_dir/output, mkdir -p)
        // 3. build argv via argv::build_argv
        // 4. spawn child, stream stderr lines as RunEvent::Log
        // 5. tokio::select! on child.wait() vs cancel.cancelled() → child.kill on cancel
        // 6. on success: glob outputs.patterns in output_dir → ModuleResult.output_files
        // 7. summary = { command: shell_quoted_argv, exit_code, output_dir, output_files }
    }
}
```

`AppState::register_plugins()` is called in `main.rs` after `BinaryResolver` is loaded:

```
let plugins = rb_plugin::load_plugins(BUNDLED_DIR, Some(&user_plugin_dir));
for (id, (manifest, source)) in plugins.by_id {
    binary_resolver.register_known_dynamic(&manifest.binary);   // add to KNOWN_BINARIES at runtime
    let module = Arc::new(ExternalToolModule::from_manifest(manifest));
    registry.register(module);
}
state.plugin_load_errors = plugins.errors;   // surfaced in Settings UI
```

This requires a small refactor: `KNOWN_BINARIES` becomes `BinaryResolver::known_binaries()` returning a merged list of compile-time `KNOWN_BINARIES` + runtime-registered entries. The compile-time slice stays as the static seed; runtime additions live in a `Vec<KnownBinary>` on the resolver.

### Frontend integration

**Dynamic module list.** Today `frontend/js/core/constants.js`'s `MODULES` is a static array. New Tauri command `list_modules() -> Vec<ModuleDescriptor>`:

```rust
struct ModuleDescriptor {
    id: String,                 // backend id
    view_id: String,            // frontend view id (== id for plugins)
    name: String,               // localized
    description: String,        // localized
    category: String,           // qc | trimming | alignment | quantification | differential | other
    icon: String,               // lucide icon name
    source: ModuleSource,       // Builtin | BundledPlugin | UserPlugin
    has_native_view: bool,      // true for first-party (use frontend/js/modules/<view-id>/view.js)
                                // false for plugins (use generic frontend/js/modules/plugin/view.js)
    binary_id: Option<String>,  // for "missing binary" gating in sidebar
}
```

Frontend boot fetches `list_modules` once, populates `MODULES` dynamically. Existing `data-param` form contract, `runModule(viewId)` flow, run-result panel, log streaming — all reused as-is. The only new view is `frontend/js/modules/plugin/view.js` which:

1. Reads the descriptor + manifest (delivered alongside `list_modules` or via a `get_module_manifest(id)` companion)
2. Generates form HTML from `params[]`: `data-param="<name>"`, `data-param-single` for `file` not `file_list`, drop zones for `file_list`, `<input type=number>` for integer, `<input type=checkbox>` for bool, `<select>` for enum, `<input type=text>` for string and `output_dir`
3. Renders the run-controls + run-results panels using the same shared partials as first-party modules

**Sidebar.** Plugin entries get a small `🔌` badge (lucide `plug` icon) next to the name. Grouped by `category` with first-party modules — a plugin in `category = "qc"` sits next to FastQC. Disabled (greyed) when its `binary_id` is unconfigured *and* unresolvable on PATH; click shows a one-card guidance view: "**RustQC needs a binary path.** [Open Settings]".

**Result view.** Same `frontend/js/modules/run-result.js` partial, generic branch:

- Status card (Done / Failed / duration / exit code)
- Output files list (one row per matched file): name, size, action buttons
  - `*.html`        → "Open in browser" (Tauri `opener` plugin)
  - `*.json`/`*.tsv`/`*.csv`/`*.txt` → "Open" (system default) + "Show in folder"
  - other          → "Show in folder"
- Log panel (existing component, no changes)

**Settings.** New panel below the binary table:

```
┌─ Plugins ──────────────────────────────────────────────┐
│ Bundled                                                │
│  • RustQC (rustqc)         loaded — binary configured  │
│                                                        │
│ User                                                   │
│  • my-custom-tool          loaded — binary missing     │
│  • broken.toml             ERROR: missing required ...  │
│                                                        │
│ User plugin directory: /home/.../config/rust_brain/... │
│   [Open folder]  [Reload plugins]                      │
└────────────────────────────────────────────────────────┘
```

`Reload plugins` → re-runs `load_plugins`, rebuilds the registry, broadcasts a `modules-changed` event so the frontend re-fetches `list_modules`. In-flight runs unaffected (they hold `Arc<dyn Module>`).

### AI integration

`rb-ai`'s tool registry already iterates `ModuleRegistry` and derives a `run_{id}` tool from each module's `params_schema()` + `ai_hint()`. Plugins implement both, so they show up automatically — no changes to `rb-ai`. The only adjustment: AI hint resolution needs to receive the user's language so plugin manifests can serve `ai_hint_zh` vs `ai_hint_en` (the existing `Module::ai_hint(&self, lang: &str)` signature already supports this; first-party modules already use it).

## Data flow

```
App boot
 ├─ BinaryResolver::load()  →  reads settings.json
 ├─ rb_plugin::load_plugins(BUNDLED_DIR, user_dir)
 │     ├─ parse bundled *.toml  →  PluginManifest list
 │     ├─ parse user *.toml     →  override by id
 │     └─ collect PluginLoadError list
 ├─ for each manifest:
 │     ├─ binary_resolver.register_known_dynamic(manifest.binary)
 │     └─ registry.register(Arc::new(ExternalToolModule::from(manifest)))
 └─ AppState  =  { registry, binary_resolver, plugin_load_errors, ... }

Frontend boot
 ├─ invoke('list_modules')         →  MODULES (first-party + plugins, merged)
 ├─ invoke('get_binary_paths')     →  status table (now includes plugin-declared binaries)
 └─ invoke('list_plugin_status')   →  bundled + user + errors (Settings panel)

Run a plugin module
 ├─ frontend collects params via data-param scan (unchanged)
 ├─ invoke('validate_params', backend_id=plugin.id, params)
 │     └─ ExternalToolModule.validate  →  per-manifest type/required checks
 ├─ invoke('run_module', plugin.id, params)
 │     └─ Runner.spawn  →  ExternalToolModule.run
 │            ├─ resolve binary path  (BinaryResolver)
 │            ├─ resolve output_dir   (default: run_dir/output)
 │            ├─ build argv           (argv.rs)
 │            ├─ spawn child, stream stderr → RunEvent::Log
 │            ├─ select on child.wait() / cancel.cancelled()
 │            ├─ on done: glob outputs.patterns
 │            └─ ModuleResult { output_files, summary, log }
 └─ frontend renders generic plugin result view
```

## Error handling

| Failure                              | Where surfaced                                    | Recovery                                       |
| ------------------------------------ | ------------------------------------------------- | ---------------------------------------------- |
| Manifest TOML parse error            | Settings → Plugins → "ERROR: parse failed at L:C" | Edit file, click "Reload plugins"              |
| Manifest schema validation error     | Same — "ERROR: missing required field 'binary'"   | Same                                           |
| Two manifests with same `id` (user)  | Loader picks first, logs warning for second       | User dedupes manually                          |
| Binary not configured / not on PATH  | Sidebar greyed + guidance card; Settings flagged  | Set path in Settings                           |
| Binary configured but wrong file     | Run fails at `BinaryError::NotExecutable`         | Toast + log; user re-picks in Settings         |
| Tool exits non-zero                  | RunRecord = Failed; full log in panel; exit_code  | User reads stderr, fixes inputs                |
| Cancel mid-run                       | `child.kill().await` → RunRecord = Failed (cancelled) | Same as existing subprocess modules        |
| Output glob matches nothing          | Run still Done; result UI shows "no outputs matched patterns" notice | User checks log / patterns |

## Testing strategy

- **`rb-plugin` unit tests** (in-crate, `cargo test -p rb-plugin`):
  - manifest parse: golden TOML → expected `PluginManifest`
  - validation: each rejection case (duplicate name, missing binary, conflicting CLI rule, etc.) returns a structured error with the right field
  - argv builder: each CliRule shape produces the expected `Vec<String>` for representative params
  - schema derivation: a manifest produces a JSON Schema that matches a hand-written equivalent (snapshot test)
  - loader: bundled-only / user-only / both / collision / parse-error mix, asserting `by_id` content and `errors` content
- **Smoke test** (manual, in `docs/`): a checklist for "ship RustQC bundled, point it at a binary, run on a sample FASTQ" covering: appears in sidebar, form renders, run completes, outputs listed, HTML opens.
- **No new e2e/integration test infra.** First-party modules don't have one either; the gain isn't worth the new dependency in v1.

## Migration / rollout

1. Land `rb-plugin` crate + `BinaryResolver` runtime-registration refactor + `ExternalToolModule` with **no bundled plugins** (empty `crates/rb-app/plugins/`). Verify nothing changes for existing first-party flows.
2. Add `list_modules` Tauri command; switch frontend to fetch dynamically. Verify first-party modules still render identically.
3. Build the generic plugin view + sidebar plugin badge + missing-binary guidance card.
4. Add Settings → Plugins panel + Reload plugins command.
5. Ship the **RustQC** manifest as the first bundled plugin. End-to-end smoke test.
6. (Follow-up, separate spec) Document the manifest format in `docs/` for users who want to author their own.

Each step is independently shippable. A user-visible plugin (RustQC) only appears at step 5; everything before that is invisible refactor.

## Open questions deferred to implementation

- Exact icon-name mapping for `category` (we already have lucide icons per first-party module; reuse those).
- Whether `Reload plugins` should also re-run `BinaryResolver::load()` (probably yes — same UX expectation).
- How to namespace `run_<id>` AI tools when a user plugin shadows a bundled one with the same id (probably: source: User wins for AI invocation too, log a warning).
- Whether to validate manifest TOML against a JSON Schema we publish (nice-to-have for v2; v1 relies on Rust-side validation messages).

# AI Chat Mode — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Phase 1 ("B" level) of the AI chat mode spec: users can create a project, pick an AI or manual default view, and drive any single analysis module through natural-language conversation with a plan-card confirmation gate. Architecture preserves forward compatibility to Phases 2 (C — multi-step pipelines) and 3 (D — full analysis agent) with no breaking changes.

**Architecture:** New `rb-ai` workspace crate owns provider adapters, tool registry, chat session persistence, and the orchestrator main loop. `rb-core::Module` trait gains two optional methods (`params_schema`, `ai_hint`) so each analysis module self-describes to the LLM. `rb-app` gains 14 `chat_*` / `ai_*` Tauri commands and two events (`chat-stream`, `chat-session-updated`). The Runner, RunRecord, and existing `run-progress` event stream are unchanged — AI-initiated runs flow through the exact same pipeline as UI-initiated runs. Provider I/O happens entirely in Rust; API keys stored via `keyring` never cross into the webview. Frontend adds an `#chat` route, session list in the sidebar, a streaming chat window with plan cards and run cards.

**Tech Stack:** Rust 1.75+, tokio, async-trait, reqwest (streaming SSE), eventsource-stream, serde / serde_json, jsonschema (test-time validation), wiremock (tests), keyring, aes-gcm + argon2 (Linux fallback), tempfile, Tauri v2, vanilla JS frontend.

**Spec:** `docs/superpowers/specs/2026-04-19-ai-chat-mode-design.md`

**Scope:** Phase 1 only. All C/D behaviour is explicitly out of scope; this plan only reserves schema fields and registers stub tools so the later phases land as additions.

---

## File Structure

### New Files

**rb-ai crate:**
- `crates/rb-ai/Cargo.toml` — package definition
- `crates/rb-ai/src/lib.rs` — public re-exports, module declarations
- `crates/rb-ai/src/error.rs` — `AiError` enum (thiserror)
- `crates/rb-ai/src/provider/mod.rs` — `ChatProvider` trait, `ChatRequest`, `ProviderEvent`, `FinishReason`, `ProviderError`
- `crates/rb-ai/src/provider/openai_compat.rs` — OpenAI-compatible SSE streaming impl
- `crates/rb-ai/src/provider/anthropic.rs` — feature-flagged stub (returns `Unimplemented`)
- `crates/rb-ai/src/provider/ollama.rs` — feature-flagged stub (returns `Unimplemented`)
- `crates/rb-ai/src/tools/mod.rs` — `ToolRegistry`, registration entry points
- `crates/rb-ai/src/tools/schema.rs` — `ToolDef`, `RiskLevel`, `ToolError`, JSON-Schema helpers
- `crates/rb-ai/src/tools/builtin.rs` — static Read-risk tools
- `crates/rb-ai/src/tools/module_derived.rs` — adapter turning `ModuleRegistry` into Run-risk `ToolDef`s
- `crates/rb-ai/src/tools/stubs.rs` — Phase 3 tools that return `Unimplemented`
- `crates/rb-ai/src/session/mod.rs` — `ChatSession`, `SessionIndex`, `SessionMeta` types
- `crates/rb-ai/src/session/message.rs` — `Message` enum and `ToolCall` / `ToolResult` structs
- `crates/rb-ai/src/session/store.rs` — atomic store for `{project}/chats/`
- `crates/rb-ai/src/orchestrator/mod.rs` — `run_turn` main loop, `OrchestratorCtx`, `ChatStreamEvent`
- `crates/rb-ai/src/orchestrator/snapshot.rs` — project snapshot builder
- `crates/rb-ai/src/orchestrator/plan_card.rs` — pending tool-call state (approvals / rejections)
- `crates/rb-ai/src/orchestrator/prompt.rs` — system prompt loader with {zh,en} includes
- `crates/rb-ai/src/orchestrator/prompts/system_en.md` — English system prompt
- `crates/rb-ai/src/orchestrator/prompts/system_zh.md` — Chinese system prompt
- `crates/rb-ai/src/config/mod.rs` — `AiConfig`, `ProviderConfig`, load/save helpers
- `crates/rb-ai/src/config/keyring.rs` — `KeyStore` trait with keyring + file fallback impls
- `crates/rb-ai/tests/orchestrator_turn.rs` — integration test using `MockProvider`
- `crates/rb-ai/tests/provider_openai_compat.rs` — wiremock-driven SSE test

**rb-app additions:**
- `crates/rb-app/src/commands/chat.rs` — session CRUD + send/approve/reject/cancel
- `crates/rb-app/src/commands/ai_provider.rs` — provider config + `ai_*_api_key`
- `crates/rb-app/src/ai_state.rs` — `AiState` holding `ToolRegistry`, `KeyStore`, active orchestrator handles

**Frontend:**
- `frontend/js/api/chat.js` — thin wrapper around `invoke('chat_*')` + event subscription
- `frontend/js/modules/chat/session-list.js` — sidebar list rendering
- `frontend/js/modules/chat/message-stream.js` — `chat-stream` event dispatcher
- `frontend/js/modules/chat/schema-form.js` — JSON Schema → HTML form renderer
- `frontend/js/modules/chat/plan-card.js` — plan card DOM component
- `frontend/js/modules/chat/run-card.js` — run progress / result card
- `frontend/js/modules/chat/chat-view.js` — view controller (wire-up of all of above)
- `frontend/css/chat.css` — AI chat styles
- `frontend/js/i18n-chat.js` — chat-specific i18n strings (merged into existing dictionaries)

### Modified Files

- `Cargo.toml` (workspace root) — add `"crates/rb-ai"` member
- `crates/rb-core/Cargo.toml` — add `async-trait` if not already (it is)
- `crates/rb-core/src/module.rs` — add default `params_schema()` and `ai_hint()` methods
- `crates/rb-qc/src/lib.rs` — override `params_schema()` + `ai_hint()`
- `crates/rb-trimming/src/lib.rs` — same
- `crates/rb-star-index/src/lib.rs` — same
- `crates/rb-star-align/src/lib.rs` — same
- `crates/rb-gff-convert/src/lib.rs` — same
- `crates/rb-deseq2/src/lib.rs` — same
- `crates/rb-core/src/project.rs` — add `default_view: Option<String>` field to `Project` with serde default
- `crates/rb-app/src/state.rs` — embed `AiState` in `AppState`
- `crates/rb-app/src/main.rs` — register new commands, bootstrap `AiState`
- `crates/rb-app/src/commands/mod.rs` — declare `chat` and `ai_provider` modules
- `crates/rb-app/src/commands/project.rs` — accept `default_view` on create
- `frontend/index.html` — add chat nav entry + mock shims for new `chat_*` / `ai_*` commands
- `frontend/js/app.js` — `navigate()` branch for `#chat`, project-creation wizard radio, settings page AI provider section, sidebar AI Copilot block
- `frontend/js/i18n.js` — merge chat strings
- `frontend/css/style.css` — sidebar AI Copilot block styles, @import `chat.css`

---

## Conventions

- **Every task ends in a commit.** Use Conventional Commit prefixes (`feat`, `fix`, `test`, `refactor`, `docs`).
- **TDD** for Rust: write the failing test, run to confirm it fails, write the minimum code, run to confirm it passes, commit.
- **Frontend** follows existing repo style (no test framework installed). Use manual verification steps with explicit expected behavior; add screenshot / console-check steps where relevant.
- **Never** block on real external APIs. All provider I/O in tests uses `wiremock`. All keyring I/O in tests uses the in-memory mock variant.
- **Cap lints** when running clippy against the whole workspace — submodule deps emit warnings we don't control:
  ```
  RUSTFLAGS="--cap-lints=warn" cargo clippy -p rb-ai -p rb-core -p rb-app -- -D warnings
  ```

---

## Task 1: Extend `Module` trait with `params_schema` + `ai_hint`

**Files:**
- Modify: `crates/rb-core/src/module.rs`

- [ ] **Step 1: Add failing test for the new trait methods**

Append inside `mod tests` at `crates/rb-core/src/module.rs`:

```rust
#[test]
fn module_default_schema_is_none_and_hint_is_empty() {
    let m = DummyModule;
    // Default impls: no schema means not exposed to AI; empty hint.
    assert!(m.params_schema().is_none());
    assert_eq!(m.ai_hint("en"), "");
    assert_eq!(m.ai_hint("zh"), "");
}

struct SchemaModule;

#[async_trait::async_trait]
impl Module for SchemaModule {
    fn id(&self) -> &str { "schema" }
    fn name(&self) -> &str { "Schema" }
    fn validate(&self, _p: &serde_json::Value) -> Vec<ValidationError> { vec![] }
    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": { "foo": { "type": "string" } },
            "required": ["foo"]
        }))
    }
    fn ai_hint(&self, lang: &str) -> String {
        match lang { "zh" => "测试".into(), _ => "test".into() }
    }
    async fn run(
        &self, _p: &serde_json::Value, _d: &std::path::Path,
        _tx: tokio::sync::mpsc::Sender<RunEvent>, _c: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        Ok(ModuleResult { output_files: vec![], summary: serde_json::json!({}), log: "".into() })
    }
}

#[test]
fn module_can_override_schema_and_hint() {
    let m = SchemaModule;
    let schema = m.params_schema().expect("expected Some schema");
    assert_eq!(schema["type"], "object");
    assert_eq!(m.ai_hint("zh"), "测试");
    assert_eq!(m.ai_hint("en"), "test");
}
```

- [ ] **Step 2: Run the test — expect compile failure (`params_schema` / `ai_hint` not defined)**

```
cargo test -p rb-core -- module_default_schema_is_none_and_hint_is_empty
```
Expected: `error[E0599]: no method named 'params_schema' found for reference '&DummyModule'`

- [ ] **Step 3: Add the two methods to the trait with safe defaults**

Edit `crates/rb-core/src/module.rs`, replacing the `Module` trait body:

```rust
#[async_trait::async_trait]
pub trait Module: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError>;
    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        events_tx: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError>;

    /// JSON Schema (draft-07) describing the module's parameters.
    /// Returning `None` means the module is not exposed to the AI tool registry.
    /// Override when you're ready for the LLM to call this module.
    fn params_schema(&self) -> Option<serde_json::Value> {
        None
    }

    /// One-paragraph hint, localized to `lang` (`"en"` or `"zh"`),
    /// telling the LLM when and how to use this module.
    /// Default is empty (safe — AI will see a tool without guidance).
    fn ai_hint(&self, _lang: &str) -> String {
        String::new()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```
cargo test -p rb-core
```
Expected: all tests pass, including the two new tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-core/src/module.rs
git commit -m "feat(rb-core): add Module::params_schema + ai_hint with safe defaults

Prepares the Module trait for AI tool-registry derivation. Defaults
keep existing modules invisible to the AI until they opt in by
overriding params_schema with Some(schema)."
```

---

## Task 2: QC module — `params_schema` + `ai_hint`

**Files:**
- Modify: `crates/rb-qc/src/lib.rs`

- [ ] **Step 1: Inspect current validate() to understand QC params shape**

```
cargo doc --no-deps -p rb-qc --open  # or just read the file
```
Identify the fields QC's existing form collects (typically `input` FASTQ path(s)).

- [ ] **Step 2: Add a failing test**

Append to `crates/rb-qc/src/lib.rs` inside its test module (add one if absent):

```rust
#[cfg(test)]
mod ai_schema_tests {
    use super::*;
    use rb_core::module::Module;

    #[test]
    fn qc_schema_requires_input() {
        let schema = QcModule.params_schema().expect("qc exposes a schema");
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().expect("required list");
        assert!(required.iter().any(|v| v == "input"),
                "QC schema must require 'input'");
    }

    #[test]
    fn qc_hint_mentions_fastq_in_both_languages() {
        let en = QcModule.ai_hint("en").to_lowercase();
        let zh = QcModule.ai_hint("zh");
        assert!(en.contains("fastq"), "en hint should mention fastq");
        assert!(!zh.is_empty(), "zh hint must not be empty");
    }
}
```

- [ ] **Step 3: Run tests — expect failure (schema returns None)**

```
cargo test -p rb-qc -- ai_schema_tests
```
Expected: panic `qc exposes a schema`.

- [ ] **Step 4: Override `params_schema` and `ai_hint`**

Add to the `impl Module for QcModule` block in `crates/rb-qc/src/lib.rs`:

```rust
fn params_schema(&self) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "type": "object",
        "properties": {
            "input": {
                "type": "array",
                "items": { "type": "string", "description": "Absolute path to a FASTQ or FASTQ.gz file." },
                "minItems": 1,
                "description": "Input FASTQ file paths. May be a single file or multiple samples."
            }
        },
        "required": ["input"],
        "additionalProperties": false
    }))
}

fn ai_hint(&self, lang: &str) -> String {
    match lang {
        "zh" => "用 run_qc 对用户提供的 FASTQ 文件做质量评估。通常是流水线的第一步 (修剪之前)。参数 input 接受一个 FASTQ 文件路径数组,每个样本一个条目。".into(),
        _    => "Use run_qc to assess read quality for raw FASTQ input. This is typically the first step of a pipeline, before trimming. The `input` array takes one FASTQ path per sample.".into(),
    }
}
```

- [ ] **Step 5: Run tests**

```
cargo test -p rb-qc
```
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rb-qc/src/lib.rs
git commit -m "feat(rb-qc): expose params_schema + ai_hint for AI tool registry"
```

---

## Task 3: Trimming module — `params_schema` + `ai_hint`

**Files:**
- Modify: `crates/rb-trimming/src/lib.rs`

- [ ] **Step 1: Read the current trimming module to see actual params**

```
cargo doc -p rb-trimming --no-deps && less crates/rb-trimming/src/lib.rs
```
Identify fields (typically `input` list, `adapter` optional string, `quality_cutoff` optional u8, `output_dir` optional).

- [ ] **Step 2: Write the failing test**

Add to `crates/rb-trimming/src/lib.rs`:

```rust
#[cfg(test)]
mod ai_schema_tests {
    use super::*;
    use rb_core::module::Module;

    #[test]
    fn trimming_schema_declares_input_required() {
        let s = TrimmingModule.params_schema().unwrap();
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "input"));
    }

    #[test]
    fn trimming_hint_nonempty_both_languages() {
        assert!(!TrimmingModule.ai_hint("en").is_empty());
        assert!(!TrimmingModule.ai_hint("zh").is_empty());
    }
}
```

- [ ] **Step 3: Run tests to confirm failure**

```
cargo test -p rb-trimming -- ai_schema_tests
```
Expected: panic at `.unwrap()` on the None schema.

- [ ] **Step 4: Implement overrides**

In `impl Module for TrimmingModule`:

```rust
fn params_schema(&self) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "type": "object",
        "properties": {
            "input": {
                "type": "array",
                "items": { "type": "string" },
                "minItems": 1,
                "description": "FASTQ file paths to trim."
            },
            "adapter": {
                "type": "string",
                "description": "Adapter sequence to remove (3' end). Omit to use cutadapt defaults."
            },
            "quality_cutoff": {
                "type": "integer",
                "minimum": 0, "maximum": 40,
                "description": "Phred quality threshold for trimming (default 20)."
            },
            "extra_args": {
                "type": "string",
                "description": "Additional raw CLI args forwarded to cutadapt-rs (power-user escape hatch)."
            }
        },
        "required": ["input"],
        "additionalProperties": false
    }))
}

fn ai_hint(&self, lang: &str) -> String {
    match lang {
        "zh" => "用 run_trimming 调用 cutadapt 去除接头并按质量裁剪 FASTQ。输入是 run_qc 看过的原始 FASTQ,输出会被 run_star_align 使用。参数 adapter 与 quality_cutoff 不确定时省略即可。".into(),
        _    => "Use run_trimming to remove adapters and quality-trim FASTQ via cutadapt. Input is typically the same raw FASTQ QC has already inspected; output feeds run_star_align. Omit adapter / quality_cutoff if unsure — defaults are sensible.".into(),
    }
}
```

- [ ] **Step 5: Run tests**

```
cargo test -p rb-trimming
```
Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rb-trimming/src/lib.rs
git commit -m "feat(rb-trimming): expose params_schema + ai_hint"
```

---

## Task 4: STAR index module — `params_schema` + `ai_hint`

**Files:**
- Modify: `crates/rb-star-index/src/lib.rs`

- [ ] **Step 1: Identify existing params**

Read `crates/rb-star-index/src/lib.rs`. Collect field names (typically `genome_fa`, `gtf`, `sjdb_overhang`, `threads`, `output_dir`).

- [ ] **Step 2: Add failing tests**

```rust
#[cfg(test)]
mod ai_schema_tests {
    use super::*;
    use rb_core::module::Module;

    #[test]
    fn star_index_requires_genome_fa_and_gtf() {
        let s = StarIndexModule.params_schema().unwrap();
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "genome_fa"));
        assert!(req.iter().any(|v| v == "gtf"));
    }
}
```

- [ ] **Step 3: Run tests — expect failure**

```
cargo test -p rb-star-index -- ai_schema_tests
```

- [ ] **Step 4: Implement overrides**

```rust
fn params_schema(&self) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "type": "object",
        "properties": {
            "genome_fa": { "type": "string", "description": "Reference genome FASTA path." },
            "gtf":       { "type": "string", "description": "Matching annotation GTF path (use run_gff_convert if only GFF3 is available)." },
            "sjdb_overhang": { "type": "integer", "minimum": 1, "default": 100,
                              "description": "--sjdbOverhang value; set to read length - 1." },
            "threads":   { "type": "integer", "minimum": 1, "default": 4 },
            "extra_args": { "type": "string" }
        },
        "required": ["genome_fa", "gtf"],
        "additionalProperties": false
    }))
}

fn ai_hint(&self, lang: &str) -> String {
    match lang {
        "zh" => "用 run_star_index 为 STAR 比对生成基因组索引。每个参考基因组只需做一次,生成的索引目录之后喂给 run_star_align 的 genome_dir。".into(),
        _    => "Use run_star_index to build a STAR genome index — a one-time setup per reference. The resulting directory is passed to run_star_align as `genome_dir`.".into(),
    }
}
```

- [ ] **Step 5: Run tests + commit**

```
cargo test -p rb-star-index
git add crates/rb-star-index/src/lib.rs
git commit -m "feat(rb-star-index): expose params_schema + ai_hint"
```

---

## Task 5: STAR align module — `params_schema` + `ai_hint`

**Files:**
- Modify: `crates/rb-star-align/src/lib.rs`

Follow the same pattern as Task 4. Required fields: `genome_dir`, `reads` (array; may be pairs). Optional: `threads`, `output_dir`, `quant_mode`, `extra_args`.

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod ai_schema_tests {
    use super::*;
    use rb_core::module::Module;
    #[test]
    fn star_align_requires_genome_dir_and_reads() {
        let s = StarAlignModule.params_schema().unwrap();
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "genome_dir"));
        assert!(req.iter().any(|v| v == "reads"));
    }
}
```

- [ ] **Step 2: Run — expect failure. Implement overrides:**

```rust
fn params_schema(&self) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "type": "object",
        "properties": {
            "genome_dir": { "type": "string", "description": "Path to STAR index produced by run_star_index." },
            "reads": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "sample": { "type": "string", "description": "Sample name, e.g. 'treated_rep1'." },
                        "r1": { "type": "string" },
                        "r2": { "type": "string", "description": "Mate 2 path for paired-end; omit for single-end." }
                    },
                    "required": ["sample", "r1"]
                },
                "minItems": 1
            },
            "threads": { "type": "integer", "minimum": 1, "default": 8 },
            "extra_args": { "type": "string" }
        },
        "required": ["genome_dir", "reads"],
        "additionalProperties": false
    }))
}

fn ai_hint(&self, lang: &str) -> String {
    match lang {
        "zh" => "用 run_star_align 把测序 reads 比对到基因组,并产出 counts_matrix.tsv 供 run_deseq2 使用。genome_dir 用 run_star_index 的输出目录。reads 数组每个条目是一个样本。".into(),
        _    => "Use run_star_align to align reads to the genome and produce a counts_matrix.tsv consumed by run_deseq2. `genome_dir` is the output of run_star_index. Each entry in `reads` is one sample.".into(),
    }
}
```

- [ ] **Step 3: Run tests + commit**

```
cargo test -p rb-star-align
git add crates/rb-star-align/src/lib.rs
git commit -m "feat(rb-star-align): expose params_schema + ai_hint"
```

---

## Task 6: GFF convert module — `params_schema` + `ai_hint`

**Files:**
- Modify: `crates/rb-gff-convert/src/lib.rs`

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod ai_schema_tests {
    use super::*;
    use rb_core::module::Module;
    #[test]
    fn gff_schema_requires_input_and_target_format() {
        let s = GffConvertModule.params_schema().unwrap();
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "input"));
        assert!(req.iter().any(|v| v == "target_format"));
    }
}
```

- [ ] **Step 2: Implement overrides**

```rust
fn params_schema(&self) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "type": "object",
        "properties": {
            "input":         { "type": "string", "description": "Input GFF3 or GTF path." },
            "target_format": { "type": "string", "enum": ["gff3", "gtf"], "description": "Desired output format." },
            "output_dir":    { "type": "string", "description": "Directory for the converted file. Defaults to run directory." }
        },
        "required": ["input", "target_format"],
        "additionalProperties": false
    }))
}

fn ai_hint(&self, lang: &str) -> String {
    match lang {
        "zh" => "用 run_gff_convert 在 GFF3 和 GTF 之间转换注释文件。STAR index 需要 GTF,用户只提供 GFF3 时先跑这个。".into(),
        _    => "Use run_gff_convert to translate annotation files between GFF3 and GTF. Call this before run_star_index if the user only has GFF3.".into(),
    }
}
```

- [ ] **Step 3: Tests + commit**

```
cargo test -p rb-gff-convert
git add crates/rb-gff-convert/src/lib.rs
git commit -m "feat(rb-gff-convert): expose params_schema + ai_hint"
```

---

## Task 7: DESeq2 module — `params_schema` + `ai_hint`

**Files:**
- Modify: `crates/rb-deseq2/src/lib.rs`

- [ ] **Step 1: Write failing test + implement**

```rust
#[cfg(test)]
mod ai_schema_tests {
    use super::*;
    use rb_core::module::Module;
    #[test]
    fn deseq2_requires_counts_and_coldata() {
        let s = DeseqModule.params_schema().unwrap();
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "counts"));
        assert!(req.iter().any(|v| v == "coldata"));
    }
}
```

Override impl:

```rust
fn params_schema(&self) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "type": "object",
        "properties": {
            "counts":  { "type": "string", "description": "Path to counts matrix TSV (from run_star_align or equivalent)." },
            "coldata": { "type": "string", "description": "Path to sample metadata TSV/CSV with a condition column." },
            "design":  { "type": "string", "default": "~condition", "description": "R-style design formula." }
        },
        "required": ["counts", "coldata"],
        "additionalProperties": false
    }))
}

fn ai_hint(&self, lang: &str) -> String {
    match lang {
        "zh" => "用 run_deseq2 做差异表达分析。counts 通常是 run_star_align 产出的 counts_matrix.tsv; coldata 是用户在项目里提供的样本分组表。".into(),
        _    => "Use run_deseq2 for differential expression analysis. `counts` is typically the counts_matrix.tsv produced by run_star_align; `coldata` is a user-provided sample metadata table.".into(),
    }
}
```

- [ ] **Step 2: Tests + commit**

```
cargo test -p rb-deseq2
git add crates/rb-deseq2/src/lib.rs
git commit -m "feat(rb-deseq2): expose params_schema + ai_hint"
```

---

## Task 8: Add `default_view` to `Project`

**Files:**
- Modify: `crates/rb-core/src/project.rs`

- [ ] **Step 1: Failing test that loading an old project (no `default_view` field) defaults to `"manual"`**

Append to `crates/rb-core/src/project.rs` in a new `#[cfg(test)] mod project_default_view_tests { ... }`:

```rust
#[cfg(test)]
mod project_default_view_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn legacy_project_json_without_default_view_loads_as_manual() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("runs")).unwrap();
        let legacy = r#"{
            "name": "legacy",
            "created_at": "2026-01-01T00:00:00Z",
            "runs": []
        }"#;
        std::fs::write(tmp.path().join("project.json"), legacy).unwrap();
        let proj = Project::load(tmp.path()).unwrap();
        assert_eq!(proj.default_view.as_deref(), Some("manual"));
    }

    #[test]
    fn newly_created_project_persists_default_view() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        p.default_view = Some("ai".into());
        p.save().unwrap();
        let reloaded = Project::load(tmp.path()).unwrap();
        assert_eq!(reloaded.default_view.as_deref(), Some("ai"));
    }
}
```

- [ ] **Step 2: Run — expect compile error (`default_view` field missing)**

```
cargo test -p rb-core -- project_default_view_tests
```

- [ ] **Step 3: Add the field with serde default**

Replace the `Project` struct in `crates/rb-core/src/project.rs`:

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Project {
    pub name: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip)]
    pub root_dir: PathBuf,
    pub runs: Vec<RunRecord>,
    #[serde(default = "default_view_manual")]
    pub default_view: Option<String>,
}

fn default_view_manual() -> Option<String> {
    Some("manual".to_string())
}
```

In `Project::create`, initialize the field:

```rust
let project = Project {
    name: name.to_string(),
    created_at: Utc::now(),
    root_dir: root_dir.to_path_buf(),
    runs: Vec::new(),
    default_view: Some("manual".to_string()),
};
```

- [ ] **Step 4: Run tests**

```
cargo test -p rb-core
```
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-core/src/project.rs
git commit -m "feat(rb-core): add Project::default_view with serde default

Legacy project.json without the field defaults to 'manual' on load,
keeping existing projects backwards-compatible."
```

---

## Task 9: Scaffold `rb-ai` crate

**Files:**
- Create: `crates/rb-ai/Cargo.toml`
- Create: `crates/rb-ai/src/lib.rs`
- Create: `crates/rb-ai/src/error.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Write the Cargo manifest**

Create `crates/rb-ai/Cargo.toml`:

```toml
[package]
name = "rb-ai"
version = "0.1.0"
edition = "2021"

[features]
default = []
anthropic = []
ollama-native = []

[dependencies]
rb-core = { path = "../rb-core" }
async-trait = "0.1"
thiserror = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["sync", "rt", "macros", "time", "fs"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "stream"] }
futures-util = "0.3"
eventsource-stream = "0.2"
url = "2"
keyring = "2"
aes-gcm = "0.10"
argon2 = "0.5"
rand = "0.8"
dirs = "5"
jsonschema = "0.18"
tracing = "0.1"

[dev-dependencies]
tempfile = "3"
wiremock = "0.6"
tokio = { version = "1", features = ["full", "test-util"] }
```

- [ ] **Step 2: Register the crate in the workspace**

Edit the top-level `Cargo.toml`, adding `"crates/rb-ai"` to the `members` list.

- [ ] **Step 3: Scaffold `lib.rs` and `error.rs`**

`crates/rb-ai/src/lib.rs`:

```rust
//! AI orchestration, provider abstraction, and chat session persistence.
//!
//! Depends on `rb-core` for `ModuleRegistry` and `Runner`; does not depend on
//! any Tauri or UI code, so it can be reused in a headless CLI or MCP server.

pub mod config;
pub mod error;
pub mod orchestrator;
pub mod provider;
pub mod session;
pub mod tools;

pub use error::AiError;
```

`crates/rb-ai/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AiError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("tool error: {0}")]
    Tool(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("cancelled")]
    Cancelled,
    #[error("config error: {0}")]
    Config(String),
    #[error("keyring error: {0}")]
    Keyring(String),
    #[error("provider not configured")]
    ProviderNotConfigured,
    #[error("invalid state: {0}")]
    InvalidState(String),
}
```

- [ ] **Step 4: Create stub module files so the crate compiles**

Create each of `config/mod.rs`, `orchestrator/mod.rs`, `provider/mod.rs`, `session/mod.rs`, `tools/mod.rs` with just:

```rust
// placeholder; filled in a later task
```

- [ ] **Step 5: Build the crate**

```
cargo check -p rb-ai
```
Expected: compiles clean.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/rb-ai/
git commit -m "feat(rb-ai): scaffold crate with error types and module skeleton"
```

---

## Task 10: Tool schema types (`ToolDef`, `RiskLevel`, `ToolError`)

**Files:**
- Create: `crates/rb-ai/src/tools/schema.rs`
- Modify: `crates/rb-ai/src/tools/mod.rs`

- [ ] **Step 1: Failing test**

Create `crates/rb-ai/src/tools/schema.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Read,
    Run,
    Destructive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub risk: RiskLevel,
    /// JSON Schema draft-07 for the tool's arguments (object form).
    pub params: serde_json::Value,
}

impl ToolDef {
    pub fn validate_schema(&self) -> Result<(), String> {
        // JSON Schema draft-07 meta-validation. We only accept object-type schemas
        // because every tool receives a JSON object of named arguments.
        let compiled = jsonschema::JSONSchema::options()
            .with_draft(jsonschema::Draft::Draft7)
            .compile(&self.params)
            .map_err(|e| format!("invalid schema for {}: {}", self.name, e))?;
        drop(compiled);
        if self.params.get("type") != Some(&serde_json::json!("object")) {
            return Err(format!(
                "tool {} schema.type must be 'object', got {:?}",
                self.name, self.params.get("type")
            ));
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    Unknown(String),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("not implemented in Phase 1: {0}")]
    Unimplemented(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooldef_serializes_risk_as_lowercase_string() {
        let t = ToolDef {
            name: "x".into(), description: "".into(),
            risk: RiskLevel::Run,
            params: serde_json::json!({"type": "object"}),
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains(r#""risk":"run""#));
    }

    #[test]
    fn validate_schema_rejects_non_object_root() {
        let t = ToolDef {
            name: "bad".into(), description: "".into(),
            risk: RiskLevel::Read,
            params: serde_json::json!({"type": "string"}),
        };
        assert!(t.validate_schema().is_err());
    }

    #[test]
    fn validate_schema_accepts_well_formed_object() {
        let t = ToolDef {
            name: "ok".into(), description: "".into(),
            risk: RiskLevel::Read,
            params: serde_json::json!({
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "required": ["x"]
            }),
        };
        assert!(t.validate_schema().is_ok());
    }
}
```

- [ ] **Step 2: Update `tools/mod.rs` to expose schema types**

```rust
pub mod schema;
pub use schema::{RiskLevel, ToolDef, ToolError};
```

- [ ] **Step 3: Run tests**

```
cargo test -p rb-ai tools::schema
```
Expected: all three tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-ai/src/tools/
git commit -m "feat(rb-ai): define ToolDef / RiskLevel / ToolError primitives"
```

---

## Task 11: `ToolRegistry` with executor dispatch

**Files:**
- Modify: `crates/rb-ai/src/tools/mod.rs`

- [ ] **Step 1: Failing test — registry round-trip**

Update `crates/rb-ai/src/tools/mod.rs`:

```rust
pub mod builtin;
pub mod module_derived;
pub mod schema;
pub mod stubs;

pub use schema::{RiskLevel, ToolDef, ToolError};

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use rb_core::project::Project;
use rb_core::runner::Runner;

/// Context handed to tool executors. Gives them project-level access
/// without leaking `ModuleRegistry` internals.
pub struct ToolContext<'a> {
    pub project: &'a Arc<tokio::sync::Mutex<Project>>,
    pub runner: &'a Runner,
    pub binary_resolver: &'a Arc<tokio::sync::Mutex<rb_core::binary::BinaryResolver>>,
}

/// Outcome of executing a tool.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum ToolOutput {
    Value(Value),
}

/// Executes a registered tool.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError>;
}

pub struct ToolEntry {
    pub def: ToolDef,
    pub executor: Arc<dyn ToolExecutor>,
}

#[derive(Default)]
pub struct ToolRegistry {
    entries: HashMap<String, ToolEntry>,
}

impl ToolRegistry {
    pub fn new() -> Self { Self { entries: HashMap::new() } }

    pub fn register(&mut self, entry: ToolEntry) {
        self.entries.insert(entry.def.name.clone(), entry);
    }

    pub fn get(&self, name: &str) -> Option<&ToolEntry> {
        self.entries.get(name)
    }

    pub fn all_for_ai(&self) -> Vec<ToolDef> {
        let mut v: Vec<_> = self.entries.values().map(|e| e.def.clone()).collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct EchoExec;
    #[async_trait]
    impl ToolExecutor for EchoExec {
        async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::Value(args.clone()))
        }
    }

    #[test]
    fn registry_returns_tools_sorted_and_findable_by_name() {
        let mut r = ToolRegistry::new();
        r.register(ToolEntry {
            def: ToolDef {
                name: "b_tool".into(), description: "b".into(),
                risk: RiskLevel::Read, params: serde_json::json!({"type":"object"}),
            },
            executor: Arc::new(EchoExec),
        });
        r.register(ToolEntry {
            def: ToolDef {
                name: "a_tool".into(), description: "a".into(),
                risk: RiskLevel::Read, params: serde_json::json!({"type":"object"}),
            },
            executor: Arc::new(EchoExec),
        });
        let all = r.all_for_ai();
        assert_eq!(all[0].name, "a_tool");
        assert_eq!(all[1].name, "b_tool");
        assert!(r.get("a_tool").is_some());
        assert!(r.get("missing").is_none());
    }
}
```

- [ ] **Step 2: Create placeholder `builtin.rs`, `module_derived.rs`, `stubs.rs`** so the `mod` declarations compile:

```rust
// placeholder — populated in later tasks
```

- [ ] **Step 3: Run tests**

```
cargo test -p rb-ai tools
```
Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-ai/src/tools/
git commit -m "feat(rb-ai): add ToolRegistry with async executor dispatch"
```

---

## Task 12: Built-in Read-risk tools

**Files:**
- Modify: `crates/rb-ai/src/tools/builtin.rs`

Tools registered: `list_project_files`, `read_table_preview`, `get_project_info`, `get_run_status`, `list_known_binaries`.

- [ ] **Step 1: Failing tests**

Replace `crates/rb-ai/src/tools/builtin.rs` with:

```rust
use super::schema::{RiskLevel, ToolDef, ToolError};
use super::{ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn register_all(registry: &mut ToolRegistry) {
    registry.register(list_project_files_entry());
    registry.register(read_table_preview_entry());
    registry.register(get_project_info_entry());
    registry.register(get_run_status_entry());
    registry.register(list_known_binaries_entry());
}

// ----- list_project_files -----
fn list_project_files_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "list_project_files".into(),
            description: "List files and directories inside the current project, \
                optionally under a subdirectory. Returns at most 200 entries with \
                type (file|dir), size in bytes, and a detected format tag.".into(),
            risk: RiskLevel::Read,
            params: json!({
                "type": "object",
                "properties": {
                    "subdir": { "type": "string",
                        "description": "Optional subdirectory relative to the project root." }
                },
                "additionalProperties": false
            }),
        },
        executor: Arc::new(ListProjectFiles),
    }
}

struct ListProjectFiles;
#[async_trait]
impl ToolExecutor for ListProjectFiles {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let subdir = args.get("subdir").and_then(|v| v.as_str()).unwrap_or("");
        let root = { ctx.project.lock().await.root_dir.clone() };
        let target = if subdir.is_empty() { root.clone() } else { root.join(subdir) };
        if !target.starts_with(&root) {
            return Err(ToolError::InvalidArgs("subdir must stay inside project".into()));
        }
        let mut out = vec![];
        let mut entries = tokio::fs::read_dir(&target).await
            .map_err(|e| ToolError::Execution(format!("read_dir: {e}")))?;
        while let Some(ent) = entries.next_entry().await
            .map_err(|e| ToolError::Execution(format!("next_entry: {e}")))?
        {
            if out.len() >= 200 { break; }
            let meta = ent.metadata().await
                .map_err(|e| ToolError::Execution(format!("metadata: {e}")))?;
            let name = ent.file_name().to_string_lossy().to_string();
            let kind = if meta.is_dir() { "dir" } else { "file" };
            let format = detect_format(&name);
            out.push(json!({
                "name": name, "type": kind,
                "size": meta.len(), "format": format,
            }));
        }
        Ok(ToolOutput::Value(json!({
            "subdir": subdir,
            "entries": out,
            "truncated": out.len() == 200
        })))
    }
}

fn detect_format(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.ends_with(".fastq.gz") || n.ends_with(".fq.gz") || n.ends_with(".fastq") || n.ends_with(".fq") { "fastq" }
    else if n.ends_with(".bam") { "bam" }
    else if n.ends_with(".sam") { "sam" }
    else if n.ends_with(".gtf") { "gtf" }
    else if n.ends_with(".gff3") || n.ends_with(".gff") { "gff" }
    else if n.ends_with(".fa") || n.ends_with(".fasta") || n.ends_with(".fna") { "fasta" }
    else if n.ends_with(".tsv") { "tsv" }
    else if n.ends_with(".csv") { "csv" }
    else { "other" }
}

// ----- read_table_preview -----
fn read_table_preview_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "read_table_preview".into(),
            description: "Read the first N lines of a TSV/CSV/FASTQ file inside the project. \
                Returns raw text — do not request > 200 rows.".into(),
            risk: RiskLevel::Read,
            params: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "rows": { "type": "integer", "minimum": 1, "maximum": 200, "default": 10 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        executor: Arc::new(ReadTablePreview),
    }
}

struct ReadTablePreview;
#[async_trait]
impl ToolExecutor for ReadTablePreview {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let rows = args.get("rows").and_then(|v| v.as_u64()).unwrap_or(10).min(200) as usize;
        let root = { ctx.project.lock().await.root_dir.clone() };
        let full = root.join(path);
        if !full.starts_with(&root) {
            return Err(ToolError::InvalidArgs("path must be inside project".into()));
        }
        let text = tokio::fs::read_to_string(&full).await
            .map_err(|e| ToolError::Execution(format!("read: {e}")))?;
        let preview: Vec<_> = text.lines().take(rows).collect();
        Ok(ToolOutput::Value(json!({
            "path": path, "rows": preview.len(),
            "content": preview.join("\n"),
        })))
    }
}

// ----- get_project_info -----
fn get_project_info_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "get_project_info".into(),
            description: "Return project name, creation time, and a summary of recent runs.".into(),
            risk: RiskLevel::Read,
            params: json!({ "type": "object", "additionalProperties": false }),
        },
        executor: Arc::new(GetProjectInfo),
    }
}

struct GetProjectInfo;
#[async_trait]
impl ToolExecutor for GetProjectInfo {
    async fn execute(&self, _args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let proj = ctx.project.lock().await;
        let runs: Vec<_> = proj.runs.iter().rev().take(10).map(|r| json!({
            "id": r.id, "module_id": r.module_id,
            "status": format!("{:?}", r.status),
            "started_at": r.started_at, "finished_at": r.finished_at,
        })).collect();
        Ok(ToolOutput::Value(json!({
            "name": proj.name, "created_at": proj.created_at,
            "runs_count": proj.runs.len(), "recent_runs": runs,
            "default_view": proj.default_view
        })))
    }
}

// ----- get_run_status -----
fn get_run_status_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "get_run_status".into(),
            description: "Look up a run by id. Returns status, timestamps, result summary, and output files.".into(),
            risk: RiskLevel::Read,
            params: json!({
                "type": "object",
                "properties": { "run_id": { "type": "string" } },
                "required": ["run_id"],
                "additionalProperties": false
            }),
        },
        executor: Arc::new(GetRunStatus),
    }
}

struct GetRunStatus;
#[async_trait]
impl ToolExecutor for GetRunStatus {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let id = args.get("run_id").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("run_id required".into()))?;
        let proj = ctx.project.lock().await;
        let r = proj.runs.iter().find(|r| r.id == id)
            .ok_or_else(|| ToolError::Execution(format!("run {id} not found")))?;
        Ok(ToolOutput::Value(json!({
            "run_id": r.id, "module_id": r.module_id,
            "status": format!("{:?}", r.status),
            "started_at": r.started_at, "finished_at": r.finished_at,
            "result": r.result,
        })))
    }
}

// ----- list_known_binaries -----
fn list_known_binaries_entry() -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: "list_known_binaries".into(),
            description: "Which external tools (STAR, gffread-rs, cutadapt-rs, ...) are available to the app.".into(),
            risk: RiskLevel::Read,
            params: json!({ "type": "object", "additionalProperties": false }),
        },
        executor: Arc::new(ListKnownBinaries),
    }
}

struct ListKnownBinaries;
#[async_trait]
impl ToolExecutor for ListKnownBinaries {
    async fn execute(&self, _args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let resolver = ctx.binary_resolver.lock().await;
        let items = resolver.list_known();
        Ok(ToolOutput::Value(json!({ "binaries": items })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rb_core::binary::BinaryResolver;
    use rb_core::project::Project;
    use rb_core::runner::Runner;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    fn make_ctx() -> (Arc<Mutex<Project>>, Runner, Arc<Mutex<BinaryResolver>>, tempfile::TempDir) {
        let tmp = tempdir().unwrap();
        let project = Project::create("t", tmp.path()).unwrap();
        std::fs::write(tmp.path().join("a.tsv"), "h1\th2\n1\t2\n3\t4\n").unwrap();
        std::fs::create_dir_all(tmp.path().join("data")).unwrap();
        let project = Arc::new(Mutex::new(project));
        let runner = Runner::new(project.clone());
        let resolver = Arc::new(Mutex::new(BinaryResolver::with_defaults_at(
            tmp.path().join("binaries.json"))));
        (project, runner, resolver, tmp)
    }

    #[tokio::test]
    async fn list_project_files_sees_top_level() {
        let (project, runner, resolver, _tmp) = make_ctx();
        let exec = ListProjectFiles;
        let out = exec.execute(&json!({}), ToolContext {
            project: &project, runner: &runner, binary_resolver: &resolver,
        }).await.unwrap();
        let ToolOutput::Value(v) = out;
        let entries = v["entries"].as_array().unwrap();
        assert!(entries.iter().any(|e| e["name"] == "a.tsv"));
    }

    #[tokio::test]
    async fn read_table_preview_limits_rows() {
        let (project, runner, resolver, _tmp) = make_ctx();
        let exec = ReadTablePreview;
        let out = exec.execute(&json!({"path":"a.tsv","rows":2}), ToolContext {
            project: &project, runner: &runner, binary_resolver: &resolver,
        }).await.unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["rows"], 2);
    }

    #[tokio::test]
    async fn read_table_preview_rejects_path_outside_project() {
        let (project, runner, resolver, _tmp) = make_ctx();
        let exec = ReadTablePreview;
        let err = exec.execute(&json!({"path":"../escape.tsv"}), ToolContext {
            project: &project, runner: &runner, binary_resolver: &resolver,
        }).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidArgs(_)));
    }
}
```

- [ ] **Step 2: Run tests**

```
cargo test -p rb-ai tools::builtin
```
Expected: all three builtin tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-ai/src/tools/builtin.rs
git commit -m "feat(rb-ai): builtin read-risk tools (files/preview/project/run_status/binaries)"
```

---

## Task 13: Module-derived Run-risk tools

**Files:**
- Modify: `crates/rb-ai/src/tools/module_derived.rs`

- [ ] **Step 1: Failing test — deriving a tool from a module with a schema**

Replace `crates/rb-ai/src/tools/module_derived.rs`:

```rust
use super::schema::{RiskLevel, ToolDef, ToolError};
use super::{ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use rb_core::module::Module;
use serde_json::Value;
use std::sync::Arc;

/// Iterate all modules in the registry and register `run_{id}` tools for
/// those that opt in by returning `Some(schema)` from `params_schema`.
pub fn register_for_modules(
    registry: &mut ToolRegistry,
    modules: &[Arc<dyn Module>],
    lang: &str,
) {
    for m in modules {
        let Some(schema) = m.params_schema() else { continue };
        let name = format!("run_{}", m.id());
        let description = if m.ai_hint(lang).is_empty() {
            format!("Run the {} module.", m.name())
        } else {
            m.ai_hint(lang)
        };
        registry.register(ToolEntry {
            def: ToolDef {
                name,
                description,
                risk: RiskLevel::Run,
                params: schema,
            },
            executor: Arc::new(ModuleRunExec { module: m.clone() }),
        });
    }
}

pub struct ModuleRunExec {
    pub module: Arc<dyn Module>,
}

#[async_trait]
impl ToolExecutor for ModuleRunExec {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        // Validate before spawning to surface errors in-band to the LLM.
        let errs = self.module.validate(args);
        if !errs.is_empty() {
            return Err(ToolError::InvalidArgs(
                errs.iter().map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>().join("; ")));
        }
        let run_id = ctx.runner.spawn(self.module.clone(), args.clone()).await
            .map_err(|e| ToolError::Execution(e))?;
        Ok(ToolOutput::Value(serde_json::json!({
            "run_id": run_id,
            "status": "started",
            "module_id": self.module.id(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rb_core::cancel::CancellationToken;
    use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
    use rb_core::run_event::RunEvent;
    use serde_json::json;
    use std::path::Path;
    use tokio::sync::mpsc;

    struct OkModule;
    #[async_trait::async_trait]
    impl Module for OkModule {
        fn id(&self) -> &str { "ok" }
        fn name(&self) -> &str { "OK" }
        fn validate(&self, _p: &Value) -> Vec<ValidationError> { vec![] }
        fn params_schema(&self) -> Option<Value> {
            Some(json!({ "type": "object", "properties": {}, "additionalProperties": true }))
        }
        fn ai_hint(&self, _l: &str) -> String { "ok module".into() }
        async fn run(&self, _: &Value, _: &Path, tx: mpsc::Sender<RunEvent>, _: CancellationToken)
            -> Result<ModuleResult, ModuleError>
        {
            let _ = tx.send(RunEvent::Progress { fraction: 1.0, message: "done".into() }).await;
            Ok(ModuleResult { output_files: vec![], summary: json!({}), log: "".into() })
        }
    }

    struct SilentModule;
    #[async_trait::async_trait]
    impl Module for SilentModule {
        fn id(&self) -> &str { "silent" }
        fn name(&self) -> &str { "Silent" }
        fn validate(&self, _p: &Value) -> Vec<ValidationError> { vec![] }
        // params_schema defaults to None — should be skipped.
        async fn run(&self, _: &Value, _: &Path, _: mpsc::Sender<RunEvent>, _: CancellationToken)
            -> Result<ModuleResult, ModuleError>
        {
            Ok(ModuleResult { output_files: vec![], summary: json!({}), log: "".into() })
        }
    }

    #[test]
    fn module_without_schema_is_skipped() {
        let mut reg = ToolRegistry::new();
        let mods: Vec<Arc<dyn Module>> = vec![Arc::new(OkModule), Arc::new(SilentModule)];
        register_for_modules(&mut reg, &mods, "en");
        assert!(reg.get("run_ok").is_some());
        assert!(reg.get("run_silent").is_none(),
                "modules without a schema must not be registered");
    }
}
```

- [ ] **Step 2: Run tests**

```
cargo test -p rb-ai tools::module_derived
```
Expected: pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-ai/src/tools/module_derived.rs
git commit -m "feat(rb-ai): derive run_* tools from ModuleRegistry, skip modules without schema"
```

---

## Task 14: Phase 3 stub tools

**Files:**
- Modify: `crates/rb-ai/src/tools/stubs.rs`

- [ ] **Step 1: Write file**

```rust
use super::schema::{RiskLevel, ToolDef, ToolError};
use super::{ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn register_all(registry: &mut ToolRegistry) {
    registry.register(entry(
        "read_results_table",
        "Read a results table (TSV/CSV/Parquet) with optional projection/filter. \
         Not implemented in Phase 1 — will land in the analysis-agent phase.",
        RiskLevel::Read,
        json!({
            "type": "object",
            "properties": {
                "run_id": { "type": "string" },
                "path":   { "type": "string" },
                "columns": { "type": "array", "items": { "type": "string" } },
                "filter":  { "type": "string", "description": "polars SQL-lite filter expression" },
                "limit":   { "type": "integer", "minimum": 1, "maximum": 10000 }
            },
            "required": ["run_id"],
            "additionalProperties": false
        }),
    ));
    registry.register(entry(
        "summarize_run",
        "Return an LLM-friendly summary of a run's key metrics. \
         Not implemented in Phase 1.",
        RiskLevel::Read,
        json!({
            "type": "object",
            "properties": { "run_id": { "type": "string" } },
            "required": ["run_id"],
            "additionalProperties": false
        }),
    ));
    registry.register(entry(
        "generate_plot",
        "Produce an ECharts JSON spec for a custom visualization. \
         Not implemented in Phase 1.",
        RiskLevel::Read,
        json!({
            "type": "object",
            "properties": {
                "source_run_id": { "type": "string" },
                "kind":          { "type": "string", "enum": ["volcano", "pca", "heatmap"] }
            },
            "required": ["source_run_id", "kind"],
            "additionalProperties": false
        }),
    ));
}

fn entry(name: &str, desc: &str, risk: RiskLevel, params: Value) -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: name.into(), description: desc.into(), risk, params,
        },
        executor: Arc::new(UnimplementedStub { name: name.into() }),
    }
}

pub struct UnimplementedStub { pub name: String }

#[async_trait]
impl ToolExecutor for UnimplementedStub {
    async fn execute(&self, _args: &Value, _ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        Err(ToolError::Unimplemented(format!(
            "{} is reserved for a future release; fall back to run_* tools and describe findings in text.",
            self.name
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolRegistry;
    #[test]
    fn register_all_adds_three_stubs_with_valid_schema() {
        let mut r = ToolRegistry::new();
        register_all(&mut r);
        for n in ["read_results_table", "summarize_run", "generate_plot"] {
            let t = r.get(n).unwrap_or_else(|| panic!("missing {n}"));
            t.def.validate_schema().unwrap();
        }
    }
}
```

- [ ] **Step 2: Tests + commit**

```
cargo test -p rb-ai tools::stubs
git add crates/rb-ai/src/tools/stubs.rs
git commit -m "feat(rb-ai): register Phase 3 stub tools (read_results_table / summarize_run / generate_plot)"
```

---

## Task 15: `ChatProvider` trait and message types

**Files:**
- Modify: `crates/rb-ai/src/provider/mod.rs`

- [ ] **Step 1: Define provider abstraction with failing unit test**

Replace `crates/rb-ai/src/provider/mod.rs`:

```rust
pub mod openai_compat;

#[cfg(feature = "anthropic")]
pub mod anthropic;

#[cfg(feature = "ollama-native")]
pub mod ollama;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

use rb_core::cancel::CancellationToken;

use crate::tools::ToolDef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<ProviderMessage>,
    pub tools: Vec<ToolDef>,
    pub temperature: f32,
}

/// Neutral message shape handed to provider adapters. Distinct from the
/// persisted `session::Message` (which can carry more metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ProviderMessage {
    User { content: String },
    Assistant { content: String, tool_calls: Vec<ProviderToolCall> },
    Tool { call_id: String, name: String, result: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderToolCall {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum ProviderEvent {
    TextDelta(String),
    ToolCall { id: String, name: String, args: serde_json::Value },
    Finish(FinishReason),
}

#[derive(Debug, Clone)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    Error(String),
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("http error: {0}")]
    Http(String),
    #[error("auth error (check API key): {0}")]
    Auth(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("malformed response: {0}")]
    Malformed(String),
    #[error("cancelled")]
    Cancelled,
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn send(
        &self,
        req: ChatRequest,
        sink: mpsc::Sender<ProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ProviderError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_message_roundtrips_via_serde() {
        let m = ProviderMessage::Assistant {
            content: "hi".into(),
            tool_calls: vec![ProviderToolCall {
                id: "tc1".into(), name: "ls".into(),
                args: serde_json::json!({"path":"/"}),
            }],
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: ProviderMessage = serde_json::from_str(&s).unwrap();
        match back {
            ProviderMessage::Assistant { tool_calls, .. } => assert_eq!(tool_calls.len(), 1),
            _ => panic!("wrong variant"),
        }
    }
}
```

- [ ] **Step 2: Create placeholder `openai_compat.rs`**

```rust
// filled in next task
```

- [ ] **Step 3: Build + test**

```
cargo test -p rb-ai provider
```
Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-ai/src/provider/
git commit -m "feat(rb-ai): define ChatProvider trait and neutral message types"
```

---

## Task 16: OpenAI-compatible SSE provider

**Files:**
- Modify: `crates/rb-ai/src/provider/openai_compat.rs`
- Create: `crates/rb-ai/tests/provider_openai_compat.rs`

- [ ] **Step 1: Write wiremock-driven integration test (will fail at compile)**

Create `crates/rb-ai/tests/provider_openai_compat.rs`:

```rust
use rb_ai::provider::{
    openai_compat::OpenAiCompatProvider, ChatProvider, ChatRequest, FinishReason,
    ProviderEvent, ProviderMessage,
};
use rb_core::cancel::CancellationToken;
use tokio::sync::mpsc;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sse(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_raw(body.as_bytes().to_vec(), "text/event-stream")
}

fn basic_req(model: &str) -> ChatRequest {
    ChatRequest {
        model: model.into(), system: "sys".into(),
        messages: vec![ProviderMessage::User { content: "hi".into() }],
        tools: vec![], temperature: 0.0,
    }
}

#[tokio::test]
async fn openai_compat_streams_text_and_finishes() {
    let server = MockServer::start().await;
    let body = "\
data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
data: {\"choices\":[{\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(sse(body))
        .mount(&server).await;

    let p = OpenAiCompatProvider::new(server.uri(), "test-key".into());
    let (tx, mut rx) = mpsc::channel(16);
    let cancel = CancellationToken::new();
    p.send(basic_req("m"), tx, cancel).await.unwrap();

    let mut texts = String::new();
    let mut finish = None;
    while let Some(ev) = rx.recv().await {
        match ev {
            ProviderEvent::TextDelta(s) => texts.push_str(&s),
            ProviderEvent::Finish(r) => finish = Some(r),
            _ => {}
        }
    }
    assert_eq!(texts, "Hello");
    assert!(matches!(finish, Some(FinishReason::Stop)));
}

#[tokio::test]
async fn openai_compat_emits_tool_calls() {
    let server = MockServer::start().await;
    let body = "\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"tc_1\",\"type\":\"function\",\"function\":{\"name\":\"list_project_files\",\"arguments\":\"\"}}]}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"subdir\\\":\\\"data\\\"}\"}}]}}]}\n\n\
data: {\"choices\":[{\"finish_reason\":\"tool_calls\"}]}\n\n\
data: [DONE]\n\n";
    Mock::given(method("POST")).and(path("/chat/completions"))
        .respond_with(sse(body)).mount(&server).await;

    let p = OpenAiCompatProvider::new(server.uri(), "k".into());
    let (tx, mut rx) = mpsc::channel(16);
    p.send(basic_req("m"), tx, CancellationToken::new()).await.unwrap();

    let mut saw_tool = None;
    let mut finish = None;
    while let Some(ev) = rx.recv().await {
        match ev {
            ProviderEvent::ToolCall { id, name, args } => {
                saw_tool = Some((id, name, args));
            }
            ProviderEvent::Finish(r) => finish = Some(r),
            _ => {}
        }
    }
    let (id, name, args) = saw_tool.expect("tool call expected");
    assert_eq!(id, "tc_1");
    assert_eq!(name, "list_project_files");
    assert_eq!(args["subdir"], "data");
    assert!(matches!(finish, Some(FinishReason::ToolCalls)));
}

#[tokio::test]
async fn openai_compat_maps_401_to_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("{\"error\":{\"message\":\"bad key\"}}"))
        .mount(&server).await;
    let p = OpenAiCompatProvider::new(server.uri(), "bad".into());
    let (tx, _rx) = mpsc::channel(4);
    let err = p.send(basic_req("m"), tx, CancellationToken::new()).await.unwrap_err();
    matches!(err, rb_ai::provider::ProviderError::Auth(_)).then_some(()).unwrap();
}
```

- [ ] **Step 2: Run to verify failure**

```
cargo test -p rb-ai --test provider_openai_compat
```
Expected: compile error (no `OpenAiCompatProvider`).

- [ ] **Step 3: Implement the provider**

Replace `crates/rb-ai/src/provider/openai_compat.rs`:

```rust
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::Value;
use std::time::Duration;
use tokio::sync::mpsc;

use rb_core::cancel::CancellationToken;

use super::{
    ChatProvider, ChatRequest, FinishReason, ProviderError, ProviderEvent, ProviderMessage,
    ProviderToolCall,
};

pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiCompatProvider {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("reqwest client");
        Self { base_url: base_url.into(), api_key: api_key.into(), client }
    }
}

fn messages_to_openai(messages: &[ProviderMessage]) -> Vec<Value> {
    messages.iter().map(|m| match m {
        ProviderMessage::User { content } => serde_json::json!({ "role": "user", "content": content }),
        ProviderMessage::Assistant { content, tool_calls } => {
            let mut obj = serde_json::json!({ "role": "assistant", "content": content });
            if !tool_calls.is_empty() {
                obj["tool_calls"] = serde_json::Value::Array(tool_calls.iter().map(|tc| serde_json::json!({
                    "id": tc.id, "type": "function",
                    "function": { "name": tc.name, "arguments": tc.args.to_string() }
                })).collect());
            }
            obj
        }
        ProviderMessage::Tool { call_id, name, result } => serde_json::json!({
            "role": "tool", "tool_call_id": call_id, "name": name, "content": result,
        }),
    }).collect()
}

fn tools_to_openai(tools: &[crate::tools::ToolDef]) -> Vec<Value> {
    tools.iter().map(|t| serde_json::json!({
        "type": "function",
        "function": {
            "name": t.name, "description": t.description, "parameters": t.params,
        }
    })).collect()
}

#[async_trait]
impl ChatProvider for OpenAiCompatProvider {
    fn id(&self) -> &str { "openai-compat" }

    async fn send(
        &self,
        req: ChatRequest,
        sink: mpsc::Sender<ProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ProviderError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut body = serde_json::json!({
            "model": req.model,
            "messages": {
                let mut v = vec![serde_json::json!({"role":"system","content": req.system})];
                v.extend(messages_to_openai(&req.messages));
                serde_json::Value::Array(v)
            },
            "temperature": req.temperature,
            "stream": true,
        });
        if !req.tools.is_empty() {
            body["tools"] = serde_json::Value::Array(tools_to_openai(&req.tools));
        }

        let resp = self.client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(text),
                429       => ProviderError::RateLimited(text),
                _         => ProviderError::Http(format!("{status}: {text}")),
            });
        }

        // Buffers for streaming tool-call assembly.
        #[derive(Default)]
        struct ToolBuf { id: String, name: String, args: String }
        let mut tool_bufs: std::collections::BTreeMap<u64, ToolBuf> = Default::default();
        let mut emitted_finish = false;

        let mut stream = resp.bytes_stream().eventsource();
        while let Some(ev) = stream.next().await {
            if cancel.is_cancelled() {
                let _ = sink.send(ProviderEvent::Finish(FinishReason::Error("cancelled".into()))).await;
                return Err(ProviderError::Cancelled);
            }
            let ev = ev.map_err(|e| ProviderError::Malformed(e.to_string()))?;
            if ev.data.trim() == "[DONE]" { break; }
            let v: Value = serde_json::from_str(&ev.data)
                .map_err(|e| ProviderError::Malformed(format!("bad json: {e}")))?;
            let choice = v["choices"].get(0).cloned().unwrap_or(Value::Null);
            let delta = choice.get("delta").cloned().unwrap_or(Value::Null);

            if let Some(s) = delta.get("content").and_then(|c| c.as_str()) {
                if !s.is_empty() {
                    let _ = sink.send(ProviderEvent::TextDelta(s.to_string())).await;
                }
            }
            if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tcs {
                    let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                    let buf = tool_bufs.entry(idx).or_default();
                    if let Some(id) = tc.get("id").and_then(|s| s.as_str()) {
                        buf.id = id.to_string();
                    }
                    if let Some(name) = tc.pointer("/function/name").and_then(|s| s.as_str()) {
                        buf.name = name.to_string();
                    }
                    if let Some(args) = tc.pointer("/function/arguments").and_then(|s| s.as_str()) {
                        buf.args.push_str(args);
                    }
                }
            }
            if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                // Flush any accumulated tool calls before finishing.
                for (_, buf) in std::mem::take(&mut tool_bufs) {
                    let args: Value = serde_json::from_str(&buf.args).unwrap_or(Value::Null);
                    let _ = sink.send(ProviderEvent::ToolCall {
                        id: buf.id, name: buf.name, args,
                    }).await;
                }
                let reason = match fr {
                    "stop"        => FinishReason::Stop,
                    "tool_calls"  => FinishReason::ToolCalls,
                    "length"      => FinishReason::Length,
                    other         => FinishReason::Error(other.into()),
                };
                let _ = sink.send(ProviderEvent::Finish(reason)).await;
                emitted_finish = true;
            }
        }
        if !emitted_finish {
            let _ = sink.send(ProviderEvent::Finish(FinishReason::Stop)).await;
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Run tests**

```
cargo test -p rb-ai --test provider_openai_compat
```
Expected: all three wiremock tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-ai/src/provider/openai_compat.rs crates/rb-ai/tests/provider_openai_compat.rs
git commit -m "feat(rb-ai): OpenAI-compatible SSE streaming provider

Handles text deltas, streamed tool_calls (accumulated per index),
and finish_reason mapping. Auth errors surfaced as ProviderError::Auth."
```

---

## Task 17: Anthropic + Ollama provider stubs behind features

**Files:**
- Create: `crates/rb-ai/src/provider/anthropic.rs`
- Create: `crates/rb-ai/src/provider/ollama.rs`

- [ ] **Step 1: Write compile-guarded stubs**

`crates/rb-ai/src/provider/anthropic.rs`:

```rust
//! Anthropic provider. Phase 1 ships a stub; feature `anthropic` gates compilation.
use async_trait::async_trait;
use tokio::sync::mpsc;

use rb_core::cancel::CancellationToken;

use super::{ChatProvider, ChatRequest, ProviderError, ProviderEvent};

pub struct AnthropicProvider;

#[async_trait]
impl ChatProvider for AnthropicProvider {
    fn id(&self) -> &str { "anthropic" }
    async fn send(
        &self, _req: ChatRequest, _sink: mpsc::Sender<ProviderEvent>, _c: CancellationToken,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::Http(
            "Anthropic provider is not implemented in Phase 1".into()))
    }
}
```

`crates/rb-ai/src/provider/ollama.rs`:

```rust
use async_trait::async_trait;
use tokio::sync::mpsc;
use rb_core::cancel::CancellationToken;
use super::{ChatProvider, ChatRequest, ProviderError, ProviderEvent};

pub struct OllamaProvider;

#[async_trait]
impl ChatProvider for OllamaProvider {
    fn id(&self) -> &str { "ollama" }
    async fn send(
        &self, _req: ChatRequest, _sink: mpsc::Sender<ProviderEvent>, _c: CancellationToken,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::Http(
            "Native Ollama provider is not implemented in Phase 1; use OpenAI-compat with Ollama's /v1 endpoint instead.".into()))
    }
}
```

- [ ] **Step 2: Build with and without the features**

```
cargo check -p rb-ai
cargo check -p rb-ai --features anthropic
cargo check -p rb-ai --features ollama-native
```
Expected: all three compile.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-ai/src/provider/
git commit -m "feat(rb-ai): stub Anthropic and Ollama providers behind features"
```

---

## Task 18: Session message model

**Files:**
- Create: `crates/rb-ai/src/session/message.rs`
- Modify: `crates/rb-ai/src/session/mod.rs`

- [ ] **Step 1: Failing test covering JSON shape**

Create `crates/rb-ai/src/session/message.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    User   { content: String },
    Assistant {
        content: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        interrupted: bool,
    },
    Tool {
        call_id: String,
        name: String,
        result: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub args: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assistant_without_tool_calls_does_not_serialize_field() {
        let m = Message::Assistant {
            content: "hi".into(), tool_calls: vec![], interrupted: false,
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(!s.contains("tool_calls"), "empty tool_calls must be skipped");
        assert!(!s.contains("interrupted"), "false interrupted must be skipped");
    }
    #[test]
    fn legacy_assistant_message_without_new_fields_still_loads() {
        let legacy = r#"{"role":"assistant","content":"old"}"#;
        let m: Message = serde_json::from_str(legacy).unwrap();
        assert_eq!(m, Message::Assistant { content: "old".into(), tool_calls: vec![], interrupted: false });
    }
}
```

- [ ] **Step 2: Update `session/mod.rs` to re-export**

```rust
pub mod message;
pub mod store;
pub use message::{Message, ToolCall};
```

Create empty `crates/rb-ai/src/session/store.rs`:

```rust
// implemented in next task
```

- [ ] **Step 3: Tests + commit**

```
cargo test -p rb-ai session::message
git add crates/rb-ai/src/session/
git commit -m "feat(rb-ai): define Message / ToolCall types with backwards-compat serde"
```

---

## Task 19: Session store with atomic writes

**Files:**
- Modify: `crates/rb-ai/src/session/mod.rs`
- Modify: `crates/rb-ai/src/session/store.rs`

- [ ] **Step 1: Failing integration tests**

Update `crates/rb-ai/src/session/mod.rs`:

```rust
pub mod message;
pub mod store;

pub use message::{Message, ToolCall};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSnapshot {
    pub provider_id: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub provider_snapshot: Option<ProviderSnapshot>,
    #[serde(default)]
    pub summary: Option<String>,   // reserved for Phase 3
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub provider_snapshot: Option<ProviderSnapshot>,
}

impl ChatSession {
    pub fn new(id: String, title: String, provider_snapshot: Option<ProviderSnapshot>) -> Self {
        let now = Utc::now();
        Self {
            schema_version: 1,
            id, title,
            created_at: now, updated_at: now,
            provider_snapshot, summary: None,
            messages: vec![],
        }
    }
    pub fn meta(&self) -> SessionMeta {
        SessionMeta {
            id: self.id.clone(), title: self.title.clone(),
            created_at: self.created_at, updated_at: self.updated_at,
            message_count: self.messages.len(),
            provider_snapshot: self.provider_snapshot.clone(),
        }
    }
}
```

Replace `crates/rb-ai/src/session/store.rs`:

```rust
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;
use uuid::Uuid;

use crate::error::AiError;
use super::{ChatSession, SessionMeta};

const CHATS_DIR: &str = "chats";
const INDEX_FILE: &str = "index.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionIndex {
    pub schema_version: u32,
    pub sessions: Vec<SessionMeta>,
}

pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    pub fn new(project_root: &Path) -> Self {
        Self { root: project_root.join(CHATS_DIR) }
    }

    pub async fn ensure_dir(&self) -> Result<(), AiError> {
        fs::create_dir_all(&self.root).await?;
        Ok(())
    }

    pub fn generate_session_id() -> String {
        format!("ses_{}", Uuid::new_v4().simple())
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }

    fn index_path(&self) -> PathBuf {
        self.root.join(INDEX_FILE)
    }

    pub async fn list(&self) -> Result<SessionIndex, AiError> {
        let p = self.index_path();
        if !p.exists() {
            return Ok(SessionIndex { schema_version: 1, sessions: vec![] });
        }
        let text = fs::read_to_string(&p).await?;
        Ok(serde_json::from_str(&text)?)
    }

    pub async fn save_session(&self, session: &ChatSession) -> Result<(), AiError> {
        self.ensure_dir().await?;
        atomic_write_json(&self.session_path(&session.id), session).await?;

        // Update index
        let mut index = self.list().await.unwrap_or_else(|_| SessionIndex { schema_version: 1, sessions: vec![] });
        let meta = session.meta();
        if let Some(existing) = index.sessions.iter_mut().find(|s| s.id == meta.id) {
            *existing = meta;
        } else {
            index.sessions.push(meta);
        }
        index.sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        atomic_write_json(&self.index_path(), &index).await?;
        Ok(())
    }

    pub async fn load_session(&self, id: &str) -> Result<ChatSession, AiError> {
        let text = fs::read_to_string(&self.session_path(id)).await
            .map_err(|_| AiError::SessionNotFound(id.to_string()))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub async fn delete_session(&self, id: &str) -> Result<(), AiError> {
        let p = self.session_path(id);
        if p.exists() { fs::remove_file(&p).await?; }
        let mut index = self.list().await?;
        index.sessions.retain(|s| s.id != id);
        atomic_write_json(&self.index_path(), &index).await?;
        Ok(())
    }

    pub async fn rename_session(&self, id: &str, new_title: String) -> Result<(), AiError> {
        let mut s = self.load_session(id).await?;
        s.title = new_title;
        s.updated_at = chrono::Utc::now();
        self.save_session(&s).await
    }
}

async fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AiError> {
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(value)?;
    fs::write(&tmp, text).await?;
    fs::rename(&tmp, path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Message, ToolCall};
    use tempfile::tempdir;

    #[tokio::test]
    async fn roundtrip_save_list_load_delete() {
        let tmp = tempdir().unwrap();
        let store = SessionStore::new(tmp.path());
        let mut s = ChatSession::new("ses_1".into(), "t".into(), None);
        s.messages.push(Message::User { content: "hi".into() });
        store.save_session(&s).await.unwrap();
        let idx = store.list().await.unwrap();
        assert_eq!(idx.sessions.len(), 1);
        assert_eq!(idx.sessions[0].message_count, 1);
        let loaded = store.load_session("ses_1").await.unwrap();
        assert_eq!(loaded.messages.len(), 1);
        store.delete_session("ses_1").await.unwrap();
        assert_eq!(store.list().await.unwrap().sessions.len(), 0);
    }

    #[tokio::test]
    async fn save_is_atomic_no_partial_file() {
        let tmp = tempdir().unwrap();
        let store = SessionStore::new(tmp.path());
        let mut s = ChatSession::new("ses_a".into(), "t".into(), None);
        s.messages.push(Message::User { content: "x".into() });
        store.save_session(&s).await.unwrap();
        // No .tmp sibling left behind
        let entries: Vec<_> = std::fs::read_dir(tmp.path().join("chats")).unwrap()
            .filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(!entries.iter().any(|n| n.ends_with(".tmp")), "tmp file leaked: {entries:?}");
    }
}
```

- [ ] **Step 2: Run tests**

```
cargo test -p rb-ai session
```
Expected: pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rb-ai/src/session/
git commit -m "feat(rb-ai): session store with atomic writes and index.json"
```

---

## Task 20: `AiConfig` with load/save

**Files:**
- Modify: `crates/rb-ai/src/config/mod.rs`

- [ ] **Step 1: Failing test**

Replace `crates/rb-ai/src/config/mod.rs`:

```rust
pub mod keyring;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::error::AiError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiConfig {
    pub default_provider: Option<String>,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_temperature() -> f32 { 0.2 }

impl AiConfig {
    pub fn example_openai() -> Self {
        let mut providers = HashMap::new();
        providers.insert("openai-compat".into(), ProviderConfig {
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            temperature: 0.2,
        });
        Self { default_provider: Some("openai-compat".into()), providers }
    }

    pub async fn load_or_default(path: &Path) -> Result<Self, AiError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path).await?;
        Ok(serde_json::from_str(&text)?)
    }

    pub async fn save(&self, path: &Path) -> Result<(), AiError> {
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).await?; }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?).await?;
        fs::rename(&tmp, path).await?;
        Ok(())
    }

    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .map(|p| p.join("rustbrain").join("ai.json"))
            .unwrap_or_else(|| PathBuf::from("./rustbrain-ai.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn roundtrip_defaults_and_custom() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("ai.json");
        let cfg = AiConfig::example_openai();
        cfg.save(&p).await.unwrap();
        let back = AiConfig::load_or_default(&p).await.unwrap();
        assert_eq!(back.default_provider.as_deref(), Some("openai-compat"));
        assert_eq!(back.providers["openai-compat"].model, "gpt-4o-mini");
    }

    #[tokio::test]
    async fn missing_file_yields_default() {
        let tmp = tempdir().unwrap();
        let cfg = AiConfig::load_or_default(&tmp.path().join("missing.json")).await.unwrap();
        assert!(cfg.default_provider.is_none());
        assert!(cfg.providers.is_empty());
    }
}
```

Create placeholder `crates/rb-ai/src/config/keyring.rs`:

```rust
// filled in next task
```

- [ ] **Step 2: Run tests + commit**

```
cargo test -p rb-ai config
git add crates/rb-ai/src/config/
git commit -m "feat(rb-ai): AiConfig serde types with atomic save and default path"
```

---

## Task 21: `KeyStore` (keyring + encrypted-file fallback)

**Files:**
- Modify: `crates/rb-ai/src/config/keyring.rs`

- [ ] **Step 1: Write the full `KeyStore` abstraction + in-memory test impl**

Replace `crates/rb-ai/src/config/keyring.rs`:

```rust
use std::path::PathBuf;
use std::sync::Mutex;
use std::collections::HashMap;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::error::AiError;

const SERVICE: &str = "rustbrain";
const ACCOUNT_PREFIX: &str = "ai.provider.";

/// Abstracts over OS keyring with an encrypted-file fallback and
/// an in-memory impl for tests.
pub trait KeyStore: Send + Sync {
    fn set(&self, provider_id: &str, key: &str) -> Result<(), AiError>;
    fn get(&self, provider_id: &str) -> Result<Option<String>, AiError>;
    fn clear(&self, provider_id: &str) -> Result<(), AiError>;
    fn backend(&self) -> &'static str;
}

/// keyring-backed implementation. Errors at `set` time cause a fallback.
pub struct KeyringStore;

impl KeyStore for KeyringStore {
    fn set(&self, provider_id: &str, key: &str) -> Result<(), AiError> {
        let account = format!("{ACCOUNT_PREFIX}{provider_id}.api_key");
        let entry = keyring::Entry::new(SERVICE, &account)
            .map_err(|e| AiError::Keyring(e.to_string()))?;
        entry.set_password(key).map_err(|e| AiError::Keyring(e.to_string()))
    }
    fn get(&self, provider_id: &str) -> Result<Option<String>, AiError> {
        let account = format!("{ACCOUNT_PREFIX}{provider_id}.api_key");
        let entry = keyring::Entry::new(SERVICE, &account)
            .map_err(|e| AiError::Keyring(e.to_string()))?;
        match entry.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AiError::Keyring(e.to_string())),
        }
    }
    fn clear(&self, provider_id: &str) -> Result<(), AiError> {
        let account = format!("{ACCOUNT_PREFIX}{provider_id}.api_key");
        let entry = keyring::Entry::new(SERVICE, &account)
            .map_err(|e| AiError::Keyring(e.to_string()))?;
        match entry.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AiError::Keyring(e.to_string())),
        }
    }
    fn backend(&self) -> &'static str { "keyring" }
}

/// AES-GCM encrypted file fallback.
/// 32-byte key derived from a machine id via argon2; file holds {nonce, cipher}.
#[derive(Serialize, Deserialize)]
struct StoredSecret { nonce: [u8; 12], cipher: Vec<u8> }

pub struct EncryptedFileStore {
    path: PathBuf,
    key: Key<Aes256Gcm>,
}

impl EncryptedFileStore {
    pub fn new(path: PathBuf, machine_id: &[u8]) -> Result<Self, AiError> {
        use argon2::{Argon2, Params, Algorithm, Version};
        let params = Params::new(4096, 3, 1, Some(32))
            .map_err(|e| AiError::Keyring(format!("argon2 params: {e}")))?;
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut key_buf = [0u8; 32];
        argon.hash_password_into(machine_id, b"rustbrain.keyfile.salt.v1", &mut key_buf)
            .map_err(|e| AiError::Keyring(format!("argon2 hash: {e}")))?;
        Ok(Self { path, key: Key::<Aes256Gcm>::clone_from_slice(&key_buf) })
    }

    fn load(&self) -> Result<HashMap<String, StoredSecret>, AiError> {
        if !self.path.exists() { return Ok(HashMap::new()); }
        let text = std::fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&text).unwrap_or_default())
    }
    fn save(&self, m: &HashMap<String, StoredSecret>) -> Result<(), AiError> {
        if let Some(parent) = self.path.parent() { std::fs::create_dir_all(parent)?; }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(m)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
    fn cipher(&self) -> Aes256Gcm { Aes256Gcm::new(&self.key) }
}

impl KeyStore for EncryptedFileStore {
    fn set(&self, provider_id: &str, key: &str) -> Result<(), AiError> {
        let mut m = self.load()?;
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = self.cipher().encrypt(Nonce::from_slice(&nonce), key.as_bytes())
            .map_err(|e| AiError::Keyring(format!("encrypt: {e}")))?;
        m.insert(provider_id.to_string(), StoredSecret { nonce, cipher: ct });
        self.save(&m)
    }
    fn get(&self, provider_id: &str) -> Result<Option<String>, AiError> {
        let m = self.load()?;
        let Some(s) = m.get(provider_id) else { return Ok(None) };
        let pt = self.cipher().decrypt(Nonce::from_slice(&s.nonce), s.cipher.as_slice())
            .map_err(|e| AiError::Keyring(format!("decrypt: {e}")))?;
        Ok(Some(String::from_utf8(pt).map_err(|e| AiError::Keyring(e.to_string()))?))
    }
    fn clear(&self, provider_id: &str) -> Result<(), AiError> {
        let mut m = self.load()?;
        m.remove(provider_id);
        self.save(&m)
    }
    fn backend(&self) -> &'static str { "encrypted-file" }
}

/// In-memory fake for tests.
pub struct InMemoryStore { inner: Mutex<HashMap<String, String>> }
impl InMemoryStore {
    pub fn new() -> Self { Self { inner: Mutex::new(HashMap::new()) } }
}
impl KeyStore for InMemoryStore {
    fn set(&self, p: &str, k: &str) -> Result<(), AiError> {
        self.inner.lock().unwrap().insert(p.into(), k.into()); Ok(())
    }
    fn get(&self, p: &str) -> Result<Option<String>, AiError> {
        Ok(self.inner.lock().unwrap().get(p).cloned())
    }
    fn clear(&self, p: &str) -> Result<(), AiError> {
        self.inner.lock().unwrap().remove(p); Ok(())
    }
    fn backend(&self) -> &'static str { "memory" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn in_memory_roundtrip() {
        let s = InMemoryStore::new();
        s.set("openai-compat", "sk-test").unwrap();
        assert_eq!(s.get("openai-compat").unwrap().as_deref(), Some("sk-test"));
        s.clear("openai-compat").unwrap();
        assert_eq!(s.get("openai-compat").unwrap(), None);
    }

    #[test]
    fn encrypted_file_roundtrip_and_ciphertext_differs_from_plaintext() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("secrets.enc.json");
        let s = EncryptedFileStore::new(path.clone(), b"machine-test-id").unwrap();
        s.set("openai-compat", "sk-secret-123").unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("sk-secret-123"), "plaintext leaked into file");
        assert_eq!(s.get("openai-compat").unwrap().as_deref(), Some("sk-secret-123"));
    }
}
```

- [ ] **Step 2: Run tests + commit**

```
cargo test -p rb-ai config::keyring
git add crates/rb-ai/src/config/keyring.rs
git commit -m "feat(rb-ai): KeyStore trait with keyring + encrypted-file + in-memory impls"
```

---

## Task 22: Project snapshot builder

**Files:**
- Modify: `crates/rb-ai/src/orchestrator/mod.rs`
- Create: `crates/rb-ai/src/orchestrator/snapshot.rs`

- [ ] **Step 1: Test + implement snapshot**

Create `crates/rb-ai/src/orchestrator/snapshot.rs`:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;
use rb_core::project::Project;

/// Build a compact, LLM-friendly summary of project state.
/// Target ≤ ~500 tokens (~2000 chars).
pub async fn build(project: &Arc<Mutex<Project>>) -> String {
    let proj = project.lock().await;
    let mut out = String::new();
    out.push_str(&format!("Project: {}\n", proj.name));
    out.push_str(&format!("Default view: {}\n",
        proj.default_view.clone().unwrap_or_else(|| "manual".into())));
    out.push_str(&format!("Created: {}\n\n", proj.created_at.format("%Y-%m-%d")));

    let root = proj.root_dir.clone();
    drop(proj);

    // Top-level listing, limit ~20 entries
    out.push_str("Top-level files:\n");
    if let Ok(rd) = std::fs::read_dir(&root) {
        let mut shown = 0;
        for ent in rd.flatten() {
            if shown >= 20 { out.push_str("  ...\n"); break; }
            let name = ent.file_name().to_string_lossy().to_string();
            let kind = match ent.file_type() {
                Ok(t) if t.is_dir() => "/",
                _ => "",
            };
            out.push_str(&format!("  {name}{kind}\n"));
            shown += 1;
        }
    }
    out.push('\n');

    // Recent runs (last 10), compact format.
    let proj = project.lock().await;
    out.push_str("Recent runs:\n");
    let recent: Vec<_> = proj.runs.iter().rev().take(10).collect();
    if recent.is_empty() {
        out.push_str("  (none yet)\n");
    } else {
        for r in recent {
            out.push_str(&format!("  {}: {:?} {}\n", r.id, r.status,
                r.finished_at.map(|d| d.format("%H:%M").to_string())
                    .unwrap_or_else(|| "-".into())));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rb_core::project::Project;
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn snapshot_includes_project_name_and_runs_header() {
        let tmp = tempdir().unwrap();
        let project = Arc::new(Mutex::new(Project::create("demo", tmp.path()).unwrap()));
        let s = build(&project).await;
        assert!(s.contains("Project: demo"));
        assert!(s.contains("Recent runs:"));
    }
}
```

Update `crates/rb-ai/src/orchestrator/mod.rs`:

```rust
pub mod plan_card;
pub mod prompt;
pub mod snapshot;
```

- [ ] **Step 2: Tests + commit**

```
cargo test -p rb-ai orchestrator::snapshot
git add crates/rb-ai/src/orchestrator/
git commit -m "feat(rb-ai): project snapshot builder for system-prompt injection"
```

---

## Task 23: System prompt loader + zh/en prompt files

**Files:**
- Create: `crates/rb-ai/src/orchestrator/prompts/system_en.md`
- Create: `crates/rb-ai/src/orchestrator/prompts/system_zh.md`
- Modify: `crates/rb-ai/src/orchestrator/prompt.rs`

- [ ] **Step 1: Write the prompt files**

Create `crates/rb-ai/src/orchestrator/prompts/system_en.md`:

```markdown
You are RustBrain's analysis copilot, embedded in a desktop app for transcriptomics analysis.

## Your capabilities
You can discover project data, inspect tables, and trigger analyses by calling tools. Every tool that actually *starts* a run (name prefixed `run_`) returns immediately with a `run_id`; the actual computation may take minutes or hours. **Never claim a run is complete unless you've seen `status: "Done"` from `get_run_status`.** Never invent `run_id`s.

## Plan-card awareness
For run-risk tools, the user sees a plan card with your proposed arguments and can edit them before approval. Propose *minimal, sensible* arguments — don't speculatively add flags. If the user edits arguments, respect the edits: the tool result you receive reflects the edited values.

## Safety rails
- Only call tools that appear in the provided tool list.
- Never instruct the user to run shell commands or modify files outside the project.
- Use `list_project_files` before assuming a file exists.
- For long-running runs, don't poll in a tight loop; respond to the user and let them ask for status.

## Style
- Be concise and direct. Users are technical.
- When a tool fails, explain what went wrong and suggest one concrete next step.
- If data required to proceed is missing, ask *one* clarifying question — don't pile on.
```

Create `crates/rb-ai/src/orchestrator/prompts/system_zh.md`:

```markdown
你是 RustBrain 的分析副驾驶,嵌入在一个用于转录组分析的桌面应用中。

## 你的能力
你可以通过工具来探索项目数据、检查数据表、触发分析。所有以 `run_` 开头的工具在启动分析后会**立刻返回一个 `run_id`**,真正的计算可能需要几分钟到几小时。**除非你从 `get_run_status` 看到 `status: "Done"`,否则不要声称分析已完成。**绝不要凭空编造 `run_id`。

## 关于 Plan Card
对于写入类的工具,用户会看到一张"计划卡片",里面展示你提议的参数,用户可以在点击"执行"前编辑这些参数。因此你应该提议**最小的、合理的**参数集 — 不要猜测性地堆砌选项。如果用户改了参数,工具返回的结果会反映用户修改后的值,请以工具返回为准。

## 安全守则
- 只调用工具列表中出现的工具。
- 绝不指示用户在项目外执行 shell 命令或修改文件。
- 假设某个文件存在之前,先用 `list_project_files` 确认。
- 长任务不要紧密轮询,回复完用户后让用户自己追问状态。

## 风格
- 简洁直接,用户是技术用户。
- 工具失败时,说明发生了什么,并给出一个具体的下一步建议。
- 缺少继续分析所需的信息时,**一次只问一个**澄清问题,不要连发。
```

- [ ] **Step 2: Write loader module**

Replace `crates/rb-ai/src/orchestrator/prompt.rs`:

```rust
const EN: &str = include_str!("prompts/system_en.md");
const ZH: &str = include_str!("prompts/system_zh.md");

pub fn base_prompt(lang: &str) -> &'static str {
    match lang { "zh" => ZH, _ => EN }
}

/// Combine the language-specific base prompt with the project snapshot.
pub fn compose(lang: &str, snapshot: &str) -> String {
    format!("{}\n\n## Project state\n\n{}", base_prompt(lang), snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base_falls_back_to_english() {
        assert_eq!(base_prompt("jp"), base_prompt("en"));
    }
    #[test]
    fn compose_contains_both_sections() {
        let out = compose("en", "snap-x");
        assert!(out.contains("RustBrain"));
        assert!(out.contains("## Project state"));
        assert!(out.contains("snap-x"));
    }
}
```

- [ ] **Step 3: Tests + commit**

```
cargo test -p rb-ai orchestrator::prompt
git add crates/rb-ai/src/orchestrator/
git commit -m "feat(rb-ai): system prompts (zh/en) with project-snapshot composer"
```

---

## Task 24: Plan-card state + orchestrator `run_turn`

**Files:**
- Modify: `crates/rb-ai/src/orchestrator/plan_card.rs`
- Modify: `crates/rb-ai/src/orchestrator/mod.rs`
- Create: `crates/rb-ai/tests/orchestrator_turn.rs`

This is the biggest task — split into three steps: plan-card state, the main loop, and the integration test using a mock provider.

- [ ] **Step 1: Plan-card state machine**

Replace `crates/rb-ai/src/orchestrator/plan_card.rs`:

```rust
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

use crate::error::AiError;

#[derive(Debug)]
pub enum PlanDecision {
    Approve { edited_args: Option<Value> },
    Reject  { reason: Option<String> },
}

#[derive(Default)]
pub struct PendingPlans {
    waiters: HashMap<String, oneshot::Sender<PlanDecision>>,
}

#[derive(Clone, Default)]
pub struct PlanCardRegistry { inner: Arc<Mutex<PendingPlans>> }

impl PlanCardRegistry {
    pub fn new() -> Self { Self::default() }

    /// Register a pending plan card and get back a receiver to await the decision.
    pub async fn register(&self, call_id: String) -> oneshot::Receiver<PlanDecision> {
        let (tx, rx) = oneshot::channel();
        self.inner.lock().await.waiters.insert(call_id, tx);
        rx
    }

    pub async fn approve(&self, call_id: &str, edited_args: Option<Value>) -> Result<(), AiError> {
        let tx = self.inner.lock().await.waiters.remove(call_id)
            .ok_or_else(|| AiError::InvalidState(format!("no pending plan for {call_id}")))?;
        tx.send(PlanDecision::Approve { edited_args })
            .map_err(|_| AiError::InvalidState("plan waiter dropped".into()))
    }

    pub async fn reject(&self, call_id: &str, reason: Option<String>) -> Result<(), AiError> {
        let tx = self.inner.lock().await.waiters.remove(call_id)
            .ok_or_else(|| AiError::InvalidState(format!("no pending plan for {call_id}")))?;
        tx.send(PlanDecision::Reject { reason })
            .map_err(|_| AiError::InvalidState("plan waiter dropped".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn register_then_approve_resolves_waiter() {
        let reg = PlanCardRegistry::new();
        let rx = reg.register("tc_1".into()).await;
        reg.approve("tc_1", Some(serde_json::json!({"foo":"bar"}))).await.unwrap();
        let dec = rx.await.unwrap();
        match dec {
            PlanDecision::Approve { edited_args } => {
                assert_eq!(edited_args.unwrap()["foo"], "bar");
            }
            _ => panic!("expected approve"),
        }
    }
    #[tokio::test]
    async fn double_approve_errors() {
        let reg = PlanCardRegistry::new();
        let _rx = reg.register("tc_2".into()).await;
        reg.approve("tc_2", None).await.unwrap();
        let err = reg.approve("tc_2", None).await.unwrap_err();
        assert!(matches!(err, AiError::InvalidState(_)));
    }
}
```

- [ ] **Step 2: Orchestrator `run_turn` and chat-stream event types**

Replace `crates/rb-ai/src/orchestrator/mod.rs`:

```rust
pub mod plan_card;
pub mod prompt;
pub mod snapshot;

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::project::Project;
use rb_core::runner::Runner;

use crate::error::AiError;
use crate::provider::{
    ChatProvider, ChatRequest, FinishReason, ProviderEvent, ProviderMessage, ProviderToolCall,
};
use crate::session::{ChatSession, Message, ToolCall};
use crate::tools::{RiskLevel, ToolContext, ToolOutput, ToolRegistry};

pub use plan_card::{PlanCardRegistry, PlanDecision};

/// Events streamed to the frontend. Serialize-ready.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind")]
pub enum ChatStreamEvent {
    Text { session_id: String, delta: String },
    ToolCall {
        session_id: String, call_id: String,
        name: String, risk: String,
        args: serde_json::Value,
        status: String, // "pending" | "running" | "done" | "error"
    },
    ToolResult {
        session_id: String, call_id: String,
        result: serde_json::Value,
    },
    Done { session_id: String },
    Error { session_id: String, message: String },
}

pub struct OrchestratorCtx {
    pub project: Arc<Mutex<Project>>,
    pub runner: Arc<Runner>,
    pub binary_resolver: Arc<Mutex<BinaryResolver>>,
    pub tools: Arc<ToolRegistry>,
    pub provider: Arc<dyn ChatProvider>,
    pub model: String,
    pub temperature: f32,
    pub plans: PlanCardRegistry,
    pub lang: String,
}

/// Append a user message and drive the provider until Finish.
/// All interim state is persisted; failures leave the session in a consistent state.
pub async fn run_turn(
    ctx: &OrchestratorCtx,
    session: Arc<Mutex<ChatSession>>,
    user_text: String,
    sink: mpsc::Sender<ChatStreamEvent>,
    cancel: CancellationToken,
    store_save: impl Fn(&ChatSession) -> futures_util::future::BoxFuture<'static, Result<(), AiError>> + Send + Sync,
) -> Result<(), AiError> {
    // 1. Append user message
    {
        let mut s = session.lock().await;
        s.messages.push(Message::User { content: user_text });
        s.updated_at = chrono::Utc::now();
        store_save(&s).await?;
    }

    let session_id = { session.lock().await.id.clone() };

    loop {
        if cancel.is_cancelled() {
            let _ = sink.send(ChatStreamEvent::Error {
                session_id: session_id.clone(), message: "cancelled".into(),
            }).await;
            return Err(AiError::Cancelled);
        }

        // 2. Build ChatRequest
        let snap = snapshot::build(&ctx.project).await;
        let system = prompt::compose(&ctx.lang, &snap);
        let provider_msgs = to_provider_messages(&session.lock().await.messages);
        let req = ChatRequest {
            model: ctx.model.clone(),
            system,
            messages: provider_msgs,
            tools: ctx.tools.all_for_ai(),
            temperature: ctx.temperature,
        };

        // 3. Drive provider
        let (p_tx, mut p_rx) = mpsc::channel::<ProviderEvent>(32);
        let provider = ctx.provider.clone();
        let cancel_for_prov = cancel.clone();
        let sink_for_text = sink.clone();
        let sid_for_text = session_id.clone();

        let prov_handle = tokio::spawn(async move {
            provider.send(req, p_tx, cancel_for_prov).await
        });

        let mut text_buf = String::new();
        let mut tool_calls: Vec<ProviderToolCall> = vec![];
        let mut finish: Option<FinishReason> = None;

        while let Some(ev) = p_rx.recv().await {
            match ev {
                ProviderEvent::TextDelta(s) => {
                    text_buf.push_str(&s);
                    let _ = sink_for_text.send(ChatStreamEvent::Text {
                        session_id: sid_for_text.clone(), delta: s,
                    }).await;
                }
                ProviderEvent::ToolCall { id, name, args } => {
                    tool_calls.push(ProviderToolCall { id, name, args });
                }
                ProviderEvent::Finish(r) => { finish = Some(r); }
            }
        }
        if let Err(e) = prov_handle.await.unwrap_or(Err(crate::provider::ProviderError::Malformed("join failed".into()))) {
            let _ = sink.send(ChatStreamEvent::Error {
                session_id: session_id.clone(), message: format!("{e}"),
            }).await;
            return Err(AiError::Provider(format!("{e}")));
        }

        // 4. Persist assistant message
        let call_list: Vec<ToolCall> = tool_calls.iter().map(|tc| ToolCall {
            call_id: tc.id.clone(), name: tc.name.clone(), args: tc.args.clone(),
        }).collect();
        {
            let mut s = session.lock().await;
            s.messages.push(Message::Assistant {
                content: text_buf.clone(),
                tool_calls: call_list.clone(),
                interrupted: cancel.is_cancelled(),
            });
            s.updated_at = chrono::Utc::now();
            store_save(&s).await?;
        }

        if tool_calls.is_empty() {
            let _ = sink.send(ChatStreamEvent::Done { session_id: session_id.clone() }).await;
            return Ok(());
        }

        // 5. Execute each tool call
        for tc in tool_calls {
            let entry = match ctx.tools.get(&tc.name) {
                Some(e) => e,
                None => {
                    push_tool_result(&ctx, &session, &tc.id, &tc.name,
                        serde_json::json!({"error": format!("unknown tool: {}", tc.name)}),
                        &sink, &session_id, &store_save).await?;
                    continue;
                }
            };
            let risk_s = match entry.def.risk {
                RiskLevel::Read => "read",
                RiskLevel::Run  => "run",
                RiskLevel::Destructive => "destructive",
            };
            // Emit ToolCall event
            let _ = sink.send(ChatStreamEvent::ToolCall {
                session_id: session_id.clone(),
                call_id: tc.id.clone(),
                name: tc.name.clone(),
                risk: risk_s.to_string(),
                args: tc.args.clone(),
                status: match entry.def.risk {
                    RiskLevel::Read => "running".into(),
                    _ => "pending".into(),
                },
            }).await;

            let resolved_args = match entry.def.risk {
                RiskLevel::Read => tc.args.clone(),
                RiskLevel::Run  => {
                    let rx = ctx.plans.register(tc.id.clone()).await;
                    match rx.await.map_err(|_| AiError::InvalidState("plan waiter dropped".into()))? {
                        PlanDecision::Approve { edited_args } => edited_args.unwrap_or_else(|| tc.args.clone()),
                        PlanDecision::Reject { reason } => {
                            let result = serde_json::json!({
                                "error": "rejected_by_user",
                                "reason": reason.unwrap_or_default(),
                            });
                            push_tool_result(&ctx, &session, &tc.id, &tc.name, result,
                                &sink, &session_id, &store_save).await?;
                            continue;
                        }
                    }
                }
                RiskLevel::Destructive => {
                    let result = serde_json::json!({
                        "error": "destructive tools are disabled in Phase 1",
                    });
                    push_tool_result(&ctx, &session, &tc.id, &tc.name, result,
                        &sink, &session_id, &store_save).await?;
                    continue;
                }
            };

            // Execute
            let exec_ctx = ToolContext {
                project: &ctx.project,
                runner: &ctx.runner,
                binary_resolver: &ctx.binary_resolver,
            };
            let exec_result = entry.executor.execute(&resolved_args, exec_ctx).await;
            let result_value = match exec_result {
                Ok(ToolOutput::Value(v)) => v,
                Err(e) => serde_json::json!({ "error": e.to_string() }),
            };
            push_tool_result(&ctx, &session, &tc.id, &tc.name, result_value,
                &sink, &session_id, &store_save).await?;
        }

        match finish {
            Some(FinishReason::ToolCalls) | None => continue, // feed back tool results
            Some(FinishReason::Stop) => {
                let _ = sink.send(ChatStreamEvent::Done { session_id: session_id.clone() }).await;
                return Ok(());
            }
            Some(FinishReason::Length) => {
                let _ = sink.send(ChatStreamEvent::Error {
                    session_id: session_id.clone(),
                    message: "response truncated by model length limit".into(),
                }).await;
                return Ok(());
            }
            Some(FinishReason::Error(e)) => {
                let _ = sink.send(ChatStreamEvent::Error {
                    session_id: session_id.clone(), message: e,
                }).await;
                return Ok(());
            }
        }
    }
}

async fn push_tool_result(
    _ctx: &OrchestratorCtx,
    session: &Arc<Mutex<ChatSession>>,
    call_id: &str, name: &str, result: serde_json::Value,
    sink: &mpsc::Sender<ChatStreamEvent>, session_id: &str,
    store_save: &impl Fn(&ChatSession) -> futures_util::future::BoxFuture<'static, Result<(), AiError>>,
) -> Result<(), AiError> {
    {
        let mut s = session.lock().await;
        s.messages.push(Message::Tool {
            call_id: call_id.into(), name: name.into(), result: result.clone(),
        });
        s.updated_at = chrono::Utc::now();
        store_save(&s).await?;
    }
    let _ = sink.send(ChatStreamEvent::ToolResult {
        session_id: session_id.into(),
        call_id: call_id.into(),
        result,
    }).await;
    Ok(())
}

fn to_provider_messages(messages: &[Message]) -> Vec<ProviderMessage> {
    messages.iter().map(|m| match m {
        Message::User { content } => ProviderMessage::User { content: content.clone() },
        Message::Assistant { content, tool_calls, .. } => ProviderMessage::Assistant {
            content: content.clone(),
            tool_calls: tool_calls.iter().map(|tc| ProviderToolCall {
                id: tc.call_id.clone(), name: tc.name.clone(), args: tc.args.clone(),
            }).collect(),
        },
        Message::Tool { call_id, name, result } => ProviderMessage::Tool {
            call_id: call_id.clone(), name: name.clone(),
            result: serde_json::to_string(result).unwrap_or_else(|_| "{}".into()),
        },
    }).collect()
}
```

- [ ] **Step 3: Integration test with MockProvider**

Create `crates/rb-ai/tests/orchestrator_turn.rs`:

```rust
use async_trait::async_trait;
use rb_ai::orchestrator::{run_turn, ChatStreamEvent, OrchestratorCtx, PlanCardRegistry};
use rb_ai::provider::{ChatProvider, ChatRequest, FinishReason, ProviderError, ProviderEvent};
use rb_ai::session::{ChatSession, Message};
use rb_ai::tools::{builtin, module_derived, ToolRegistry};
use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::project::Project;
use rb_core::runner::Runner;
use std::sync::{Arc, Mutex as StdMutex};
use tempfile::tempdir;
use tokio::sync::{mpsc, Mutex};

/// MockProvider plays a scripted sequence of ProviderEvents.
struct MockProvider {
    script: StdMutex<Vec<Vec<ProviderEvent>>>,
}
impl MockProvider {
    fn new(turns: Vec<Vec<ProviderEvent>>) -> Self {
        Self { script: StdMutex::new(turns) }
    }
}
#[async_trait]
impl ChatProvider for MockProvider {
    fn id(&self) -> &str { "mock" }
    async fn send(
        &self, _req: ChatRequest,
        sink: mpsc::Sender<ProviderEvent>, _c: CancellationToken,
    ) -> Result<(), ProviderError> {
        let next = self.script.lock().unwrap().remove(0);
        for ev in next { let _ = sink.send(ev).await; }
        Ok(())
    }
}

#[tokio::test]
async fn turn_without_tool_calls_emits_text_then_done() {
    let tmp = tempdir().unwrap();
    let project = Arc::new(Mutex::new(Project::create("t", tmp.path()).unwrap()));
    let runner = Arc::new(Runner::new(project.clone()));
    let resolver = Arc::new(Mutex::new(BinaryResolver::with_defaults_at(tmp.path().join("bin.json"))));
    let mut tools = ToolRegistry::new();
    builtin::register_all(&mut tools);

    let provider = Arc::new(MockProvider::new(vec![vec![
        ProviderEvent::TextDelta("Hel".into()),
        ProviderEvent::TextDelta("lo".into()),
        ProviderEvent::Finish(FinishReason::Stop),
    ]]));
    let ctx = OrchestratorCtx {
        project: project.clone(), runner: runner.clone(),
        binary_resolver: resolver.clone(),
        tools: Arc::new(tools),
        provider, model: "m".into(), temperature: 0.0,
        plans: PlanCardRegistry::new(), lang: "en".into(),
    };
    let session = Arc::new(Mutex::new(ChatSession::new("s1".into(), "t".into(), None)));
    let (tx, mut rx) = mpsc::channel(64);

    let session_for_save = session.clone();
    run_turn(&ctx, session.clone(), "hi".into(), tx, CancellationToken::new(),
        move |_s| {
            let _ = &session_for_save;
            Box::pin(async { Ok(()) })
        }
    ).await.unwrap();

    let mut texts = String::new();
    let mut saw_done = false;
    while let Some(ev) = rx.recv().await {
        match ev {
            ChatStreamEvent::Text { delta, .. } => texts.push_str(&delta),
            ChatStreamEvent::Done { .. } => { saw_done = true; break; }
            _ => {}
        }
    }
    assert_eq!(texts, "Hello");
    assert!(saw_done);
    let msgs = &session.lock().await.messages;
    assert_eq!(msgs.len(), 2);
    assert!(matches!(msgs[0], Message::User { .. }));
    assert!(matches!(msgs[1], Message::Assistant { .. }));
}

#[tokio::test]
async fn read_risk_tool_call_feeds_result_back_to_provider() {
    let tmp = tempdir().unwrap();
    let project = Arc::new(Mutex::new(Project::create("t", tmp.path()).unwrap()));
    let runner = Arc::new(Runner::new(project.clone()));
    let resolver = Arc::new(Mutex::new(BinaryResolver::with_defaults_at(tmp.path().join("bin.json"))));
    let mut tools = ToolRegistry::new();
    builtin::register_all(&mut tools);

    // Turn 1: model calls get_project_info; Turn 2: model says "done" after seeing result.
    let provider = Arc::new(MockProvider::new(vec![
        vec![
            ProviderEvent::ToolCall {
                id: "tc_a".into(), name: "get_project_info".into(),
                args: serde_json::json!({}),
            },
            ProviderEvent::Finish(FinishReason::ToolCalls),
        ],
        vec![
            ProviderEvent::TextDelta("Project is t.".into()),
            ProviderEvent::Finish(FinishReason::Stop),
        ],
    ]));

    let ctx = OrchestratorCtx {
        project: project.clone(), runner: runner.clone(),
        binary_resolver: resolver.clone(),
        tools: Arc::new(tools),
        provider, model: "m".into(), temperature: 0.0,
        plans: PlanCardRegistry::new(), lang: "en".into(),
    };
    let session = Arc::new(Mutex::new(ChatSession::new("s2".into(), "t".into(), None)));
    let (tx, mut rx) = mpsc::channel(64);
    run_turn(&ctx, session.clone(), "status".into(), tx, CancellationToken::new(),
        move |_s| Box::pin(async { Ok(()) })).await.unwrap();

    let mut saw_tool_result = false;
    let mut text = String::new();
    while let Some(ev) = rx.recv().await {
        match ev {
            ChatStreamEvent::ToolResult { result, .. } => {
                saw_tool_result = true;
                assert_eq!(result["name"], "t");
            }
            ChatStreamEvent::Text { delta, .. } => text.push_str(&delta),
            ChatStreamEvent::Done { .. } => break,
            _ => {}
        }
    }
    assert!(saw_tool_result);
    assert_eq!(text, "Project is t.");
}
```

- [ ] **Step 4: Run all tests**

```
cargo test -p rb-ai
```
Expected: all unit + integration tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rb-ai/
git commit -m "feat(rb-ai): orchestrator run_turn loop with plan-card gating

Tool calls are dispatched by RiskLevel — Read auto-executes; Run
suspends until chat_approve_tool / chat_reject_tool is invoked.
Tool results are fed back to the provider in the next iteration."
```

---

## Task 25: `AiState` and `chat_session_*` commands

**Files:**
- Create: `crates/rb-app/src/ai_state.rs`
- Create: `crates/rb-app/src/commands/chat.rs`
- Modify: `crates/rb-app/src/state.rs`
- Modify: `crates/rb-app/src/commands/mod.rs`
- Modify: `crates/rb-app/Cargo.toml`

- [ ] **Step 1: Add `rb-ai` dependency**

Edit `crates/rb-app/Cargo.toml` and add under `[dependencies]`:

```toml
rb-ai = { path = "../rb-ai" }
```

- [ ] **Step 2: Define AiState**

Create `crates/rb-app/src/ai_state.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;

use rb_ai::config::{keyring::KeyStore, AiConfig};
use rb_ai::orchestrator::PlanCardRegistry;
use rb_ai::provider::ChatProvider;
use rb_ai::session::store::SessionStore;
use rb_ai::tools::ToolRegistry;

pub struct AiState {
    pub tools_by_lang: std::collections::HashMap<String, Arc<ToolRegistry>>,
    pub keystore: Arc<dyn KeyStore>,
    pub config_path: PathBuf,
    pub config: tokio::sync::Mutex<AiConfig>,
    pub plans: PlanCardRegistry,
    pub provider_cache: tokio::sync::Mutex<Option<Arc<dyn ChatProvider>>>,
    pub active_turns: tokio::sync::Mutex<std::collections::HashMap<String, rb_core::cancel::CancellationToken>>,
}

pub fn build_tool_registry(
    modules: &[Arc<dyn rb_core::module::Module>],
    lang: &str,
) -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    rb_ai::tools::builtin::register_all(&mut reg);
    rb_ai::tools::module_derived::register_for_modules(&mut reg, modules, lang);
    rb_ai::tools::stubs::register_all(&mut reg);
    reg
}

pub fn make_session_store(project_root: &std::path::Path) -> SessionStore {
    SessionStore::new(project_root)
}
```

- [ ] **Step 3: Extend `AppState`**

Edit `crates/rb-app/src/state.rs` — add an `ai` field:

```rust
use crate::ai_state::AiState;

pub struct AppState {
    pub registry: Arc<ModuleRegistry>,
    pub runner: Arc<Mutex<Option<Runner>>>,
    pub recent_projects: Arc<Mutex<Vec<PathBuf>>>,
    pub binary_resolver: Arc<Mutex<rb_core::binary::BinaryResolver>>,
    pub ai: Arc<AiState>,
}

impl AppState {
    pub fn new(registry: ModuleRegistry, ai: Arc<AiState>) -> Self {
        let resolver = /* unchanged */;
        Self {
            registry: Arc::new(registry),
            runner: Arc::new(Mutex::new(None)),
            recent_projects: Arc::new(Mutex::new(Vec::new())),
            binary_resolver: Arc::new(Mutex::new(resolver)),
            ai,
        }
    }
}
```

- [ ] **Step 4: Create `chat.rs` commands (session CRUD only for now)**

Create `crates/rb-app/src/commands/chat.rs`:

```rust
use std::sync::Arc;
use tauri::State;

use rb_ai::session::{store::{SessionIndex, SessionStore}, ChatSession};

use crate::state::AppState;

fn session_store_for(state: &AppState) -> Result<SessionStore, String> {
    let runner_guard = state.runner.try_lock()
        .map_err(|_| "project busy".to_string())?;
    let runner = runner_guard.as_ref().ok_or_else(|| "no open project".to_string())?;
    let root = {
        let proj = runner.project().try_lock()
            .map_err(|_| "project busy".to_string())?;
        proj.root_dir.clone()
    };
    Ok(SessionStore::new(&root))
}

#[tauri::command]
pub async fn chat_list_sessions(state: State<'_, AppState>) -> Result<SessionIndex, String> {
    let store = session_store_for(&state)?;
    store.list().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_create_session(
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<ChatSession, String> {
    let store = session_store_for(&state)?;
    let id = SessionStore::generate_session_id();
    let session = ChatSession::new(id, title.unwrap_or_else(|| "New chat".into()), None);
    store.save_session(&session).await.map_err(|e| e.to_string())?;
    Ok(session)
}

#[tauri::command]
pub async fn chat_get_session(
    state: State<'_, AppState>, session_id: String,
) -> Result<ChatSession, String> {
    let store = session_store_for(&state)?;
    store.load_session(&session_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_delete_session(
    state: State<'_, AppState>, session_id: String,
) -> Result<(), String> {
    let store = session_store_for(&state)?;
    store.delete_session(&session_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_rename_session(
    state: State<'_, AppState>, session_id: String, title: String,
) -> Result<(), String> {
    let store = session_store_for(&state)?;
    store.rename_session(&session_id, title).await.map_err(|e| e.to_string())
}
```

- [ ] **Step 5: Register in `commands/mod.rs`**

Add:

```rust
pub mod ai_provider;
pub mod chat;
```

- [ ] **Step 6: Build — don't register commands yet (that happens in Task 28)**

```
cargo check -p rb-app
```
Expected: compiles (if there are errors in `AppState::new` because main hasn't been updated, we'll fix in Task 28).

- [ ] **Step 7: Commit**

```bash
git add crates/rb-app/
git commit -m "feat(rb-app): AiState + chat_session_* session CRUD commands"
```

---

## Task 26: `chat_send_message` + approve/reject/cancel commands

**Files:**
- Modify: `crates/rb-app/src/commands/chat.rs`

- [ ] **Step 1: Append send/approve/reject/cancel commands**

Append to `crates/rb-app/src/commands/chat.rs`:

```rust
use rb_ai::config::keyring::KeyStore;
use rb_ai::error::AiError;
use rb_ai::orchestrator::{run_turn, ChatStreamEvent, OrchestratorCtx};
use rb_ai::provider::openai_compat::OpenAiCompatProvider;
use rb_ai::provider::ChatProvider;
use rb_core::cancel::CancellationToken;
use tauri::{AppHandle, Emitter};

async fn acquire_provider(state: &AppState) -> Result<(Arc<dyn ChatProvider>, String, f32), String> {
    let cfg = state.ai.config.lock().await.clone();
    let provider_id = cfg.default_provider.as_deref()
        .ok_or_else(|| "no default provider configured".to_string())?;
    let pc = cfg.providers.get(provider_id)
        .ok_or_else(|| format!("provider {provider_id} not found in config"))?.clone();
    let key = state.ai.keystore.get(provider_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "API key not set for provider".to_string())?;
    let provider: Arc<dyn ChatProvider> = Arc::new(OpenAiCompatProvider::new(pc.base_url, key));
    Ok((provider, pc.model, pc.temperature))
}

fn language_for(state: &AppState) -> String {
    // Phase 1: hardcoded to English. Frontend picks by setting a separate command
    // or via app config. Kept simple; wire to real i18n when the frontend supports it.
    std::env::var("RUSTBRAIN_LANG").unwrap_or_else(|_| "en".into())
}

#[tauri::command]
pub async fn chat_send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    text: String,
) -> Result<(), String> {
    let store = session_store_for(&state)?;
    let session = store.load_session(&session_id).await.map_err(|e| e.to_string())?;
    let session = Arc::new(tokio::sync::Mutex::new(session));

    let (provider, model, temperature) = acquire_provider(&state).await?;

    // Pull the runner + project out of AppState.
    let runner = {
        let guard = state.runner.lock().await;
        guard.as_ref().ok_or_else(|| "no open project".to_string())?.project().clone()
    };
    // NOTE: Runner::spawn takes &Runner so we need the Runner value itself. We hold it via
    // an Arc inside AppState::runner (it's Option<Runner>). Re-clone via a helper.
    // See Task 28 for the refactor ensuring Runner is shared via Arc.
    let runner_arc: Arc<rb_core::runner::Runner> = {
        let guard = state.runner.lock().await;
        // SAFETY: we keep AppState::runner as Arc<Mutex<Option<Arc<Runner>>>> after Task 28.
        guard.as_ref().ok_or_else(|| "no open project".to_string())?.clone().into()
    };

    let lang = language_for(&state);
    let tools = state.ai.tools_by_lang.get(&lang).cloned()
        .unwrap_or_else(|| state.ai.tools_by_lang.get("en").cloned().expect("en tools registered"));

    let ctx = OrchestratorCtx {
        project: runner_arc.project().clone(),
        runner: runner_arc.clone(),
        binary_resolver: state.binary_resolver.clone(),
        tools,
        provider,
        model, temperature,
        plans: state.ai.plans.clone(),
        lang,
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<ChatStreamEvent>(64);
    let cancel = CancellationToken::new();
    state.ai.active_turns.lock().await.insert(session_id.clone(), cancel.clone());

    // Forward events to Tauri
    let app_emit = app.clone();
    let session_id_emit = session_id.clone();
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let _ = app_emit.emit("chat-stream", &ev);
        }
        let _ = app_emit.emit("chat-session-updated",
            serde_json::json!({ "session_id": session_id_emit }));
    });

    let session_for_store = session.clone();
    let store_for_closure = store;
    let save_fn = move |s: &rb_ai::session::ChatSession| {
        let store = store_for_closure.clone();
        let snapshot = s.clone();
        Box::pin(async move {
            store.save_session(&snapshot).await.map_err(|e| AiError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))
        }) as futures_util::future::BoxFuture<'static, Result<(), AiError>>
    };

    let session_for_turn = session.clone();
    let active_turns = state.ai.active_turns.clone();
    let sid = session_id.clone();
    tokio::spawn(async move {
        let res = run_turn(&ctx, session_for_turn, text, tx, cancel, save_fn).await;
        active_turns.lock().await.remove(&sid);
        if let Err(e) = res {
            tracing::warn!("run_turn ended with error: {e:?}");
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn chat_approve_tool(
    state: State<'_, AppState>,
    call_id: String,
    edited_args: Option<serde_json::Value>,
) -> Result<(), String> {
    state.ai.plans.approve(&call_id, edited_args).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_reject_tool(
    state: State<'_, AppState>,
    call_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    state.ai.plans.reject(&call_id, reason).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_cancel_turn(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    if let Some(token) = state.ai.active_turns.lock().await.remove(&session_id) {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn chat_cancel_run(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<(), String> {
    let guard = state.runner.lock().await;
    let runner = guard.as_ref().ok_or_else(|| "no open project".to_string())?;
    runner.cancel(&run_id).await;
    Ok(())
}
```

- [ ] **Step 2: Build — expect errors tied to `AppState::runner` still being `Option<Runner>` not `Arc<Runner>`.**

This is intentional; the refactor in Task 28 normalizes the runner shape. Add a TODO comment and move on.

- [ ] **Step 3: Commit partial work**

```bash
git add crates/rb-app/src/commands/chat.rs
git commit -m "feat(rb-app): chat send/approve/reject/cancel commands (Runner wiring pending)"
```

---

## Task 27: `ai_provider_*` + `ai_*_api_key` commands

**Files:**
- Create: `crates/rb-app/src/commands/ai_provider.rs`

- [ ] **Step 1: Write commands**

```rust
use tauri::State;

use rb_ai::config::{keyring::KeyStore, ProviderConfig};

use crate::state::AppState;

#[tauri::command]
pub async fn ai_get_config(
    state: State<'_, AppState>,
) -> Result<rb_ai::config::AiConfig, String> {
    Ok(state.ai.config.lock().await.clone())
}

#[tauri::command]
pub async fn ai_set_provider_config(
    state: State<'_, AppState>,
    provider_id: String,
    config: ProviderConfig,
) -> Result<(), String> {
    let mut cfg = state.ai.config.lock().await;
    cfg.providers.insert(provider_id.clone(), config);
    if cfg.default_provider.is_none() {
        cfg.default_provider = Some(provider_id);
    }
    cfg.save(&state.ai.config_path).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_set_default_provider(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<(), String> {
    let mut cfg = state.ai.config.lock().await;
    if !cfg.providers.contains_key(&provider_id) {
        return Err(format!("unknown provider {provider_id}"));
    }
    cfg.default_provider = Some(provider_id);
    cfg.save(&state.ai.config_path).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_set_api_key(
    state: State<'_, AppState>, provider_id: String, key: String,
) -> Result<(), String> {
    state.ai.keystore.set(&provider_id, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_clear_api_key(
    state: State<'_, AppState>, provider_id: String,
) -> Result<(), String> {
    state.ai.keystore.clear(&provider_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_has_api_key(
    state: State<'_, AppState>, provider_id: String,
) -> Result<bool, String> {
    Ok(state.ai.keystore.get(&provider_id).map_err(|e| e.to_string())?.is_some())
}

#[tauri::command]
pub async fn ai_backend_info(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "keystore_backend": state.ai.keystore.backend(),
        "config_path": state.ai.config_path,
    }))
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/rb-app/src/commands/ai_provider.rs
git commit -m "feat(rb-app): ai_provider commands (config + key management)"
```

---

## Task 28: Wire AiState into `main.rs` and register commands

**Files:**
- Modify: `crates/rb-app/src/main.rs`
- Modify: `crates/rb-app/src/state.rs` (adjust Runner to `Arc<Runner>` via Option)

- [ ] **Step 1: Refactor `AppState::runner` to carry `Arc<Runner>`**

Change in `crates/rb-app/src/state.rs`:

```rust
pub struct AppState {
    pub registry: Arc<ModuleRegistry>,
    pub runner: Arc<Mutex<Option<Arc<Runner>>>>,   // note: Option<Arc<Runner>>
    pub recent_projects: Arc<Mutex<Vec<PathBuf>>>,
    pub binary_resolver: Arc<Mutex<rb_core::binary::BinaryResolver>>,
    pub ai: Arc<crate::ai_state::AiState>,
}
```

Update every place that constructs the runner (in `commands/project.rs`'s `open_project` / `create_project`) to wrap in `Arc`:

```rust
*state.runner.lock().await = Some(Arc::new(runner));
```

- [ ] **Step 2: Update `main.rs`**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai_state;
mod commands;
mod state;

use state::{AppState, ModuleRegistry};
use std::sync::Arc;
use tauri::{path::BaseDirectory, Manager};

fn main() {
    let mut registry = ModuleRegistry::new();
    registry.register(Arc::new(rb_deseq2::DeseqModule));
    registry.register(Arc::new(rb_qc::QcModule));
    registry.register(Arc::new(rb_trimming::TrimmingModule));
    registry.register(Arc::new(rb_gff_convert::GffConvertModule));
    registry.register(Arc::new(rb_star_index::StarIndexModule));
    registry.register(Arc::new(rb_star_align::StarAlignModule));

    // Build AI state
    let modules_vec: Vec<Arc<dyn rb_core::module::Module>> = vec![
        Arc::new(rb_deseq2::DeseqModule), Arc::new(rb_qc::QcModule),
        Arc::new(rb_trimming::TrimmingModule), Arc::new(rb_gff_convert::GffConvertModule),
        Arc::new(rb_star_index::StarIndexModule), Arc::new(rb_star_align::StarAlignModule),
    ];
    let tools_en = Arc::new(ai_state::build_tool_registry(&modules_vec, "en"));
    let tools_zh = Arc::new(ai_state::build_tool_registry(&modules_vec, "zh"));
    let mut tools_by_lang = std::collections::HashMap::new();
    tools_by_lang.insert("en".into(), tools_en);
    tools_by_lang.insert("zh".into(), tools_zh);

    let config_path = rb_ai::config::AiConfig::default_path();
    let ai_config = tauri::async_runtime::block_on(
        rb_ai::config::AiConfig::load_or_default(&config_path)
    ).unwrap_or_default();

    let keystore: Arc<dyn rb_ai::config::keyring::KeyStore> = {
        let k = rb_ai::config::keyring::KeyringStore;
        // Probe: try a harmless get to detect keyring availability.
        match k.get("__probe__") {
            Ok(_) => Arc::new(k),
            Err(_) => {
                let fallback_path = dirs::config_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("rustbrain").join("secrets.enc.json");
                let machine_id = std::fs::read_to_string("/etc/machine-id")
                    .unwrap_or_else(|_| "rustbrain-fallback".into());
                Arc::new(rb_ai::config::keyring::EncryptedFileStore::new(
                    fallback_path, machine_id.trim().as_bytes()
                ).expect("encrypted-file keystore"))
            }
        }
    };

    let ai_state = Arc::new(ai_state::AiState {
        tools_by_lang,
        keystore,
        config_path,
        config: tokio::sync::Mutex::new(ai_config),
        plans: rb_ai::orchestrator::PlanCardRegistry::new(),
        provider_cache: tokio::sync::Mutex::new(None),
        active_turns: tokio::sync::Mutex::new(std::collections::HashMap::new()),
    });

    tauri::Builder::default()
        .manage(AppState::new(registry, ai_state))
        .setup(|app| {
            register_bundled(app, "star", "star");
            register_bundled(app, "gffread-rs", "gffread-rs");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Existing commands
            commands::project::create_project,
            commands::project::open_project,
            commands::project::list_recent_projects,
            commands::modules::validate_params,
            commands::modules::run_module,
            commands::modules::cancel_run,
            commands::modules::get_run_result,
            commands::modules::list_runs,
            commands::files::select_files,
            commands::files::select_directory,
            commands::files::read_table_preview,
            commands::settings::get_binary_paths,
            commands::settings::set_binary_path,
            commands::settings::clear_binary_path,
            // New chat commands
            commands::chat::chat_list_sessions,
            commands::chat::chat_create_session,
            commands::chat::chat_get_session,
            commands::chat::chat_delete_session,
            commands::chat::chat_rename_session,
            commands::chat::chat_send_message,
            commands::chat::chat_approve_tool,
            commands::chat::chat_reject_tool,
            commands::chat::chat_cancel_turn,
            commands::chat::chat_cancel_run,
            commands::ai_provider::ai_get_config,
            commands::ai_provider::ai_set_provider_config,
            commands::ai_provider::ai_set_default_provider,
            commands::ai_provider::ai_set_api_key,
            commands::ai_provider::ai_clear_api_key,
            commands::ai_provider::ai_has_api_key,
            commands::ai_provider::ai_backend_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn register_bundled(app: &tauri::App, binary_id: &str, filename_stem: &str) {
    // unchanged from existing implementation
}
```

- [ ] **Step 2: Update `commands/project.rs`** to also accept `default_view` when creating.

Find `create_project` and add a new optional string param; persist to `Project::default_view`.

```rust
#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    name: String,
    root_dir: PathBuf,
    default_view: Option<String>,  // NEW
) -> Result<(), String> {
    let mut proj = Project::create(&name, &root_dir).map_err(|e| e.to_string())?;
    proj.default_view = default_view.or_else(|| Some("manual".into()));
    proj.save().map_err(|e| e.to_string())?;
    // build runner and stash into AppState as before
    // ...
    Ok(())
}
```

- [ ] **Step 3: Full workspace build**

```
cargo check --workspace
cargo test --workspace
```
Expected: everything compiles, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rb-app/
git commit -m "feat(rb-app): wire AiState into AppState, register 14 new commands

Also adds default_view param to create_project so the frontend wizard can
persist the user's preferred initial view."
```

---

## Task 29: Frontend `api/chat.js` wrapper

**Files:**
- Create: `frontend/js/api/chat.js`

- [ ] **Step 1: Create the wrapper**

```js
// Thin wrapper over Tauri `invoke` for chat_* / ai_* commands and the
// two chat events. Keeps view code from depending on window.__TAURI__ directly.

const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
const listen = (event, cb)  => window.__TAURI__.event.listen(event, cb);

export const chatApi = {
  listSessions:    ()                       => invoke('chat_list_sessions'),
  createSession:   (title)                  => invoke('chat_create_session', { title }),
  getSession:      (sessionId)              => invoke('chat_get_session', { sessionId }),
  deleteSession:   (sessionId)              => invoke('chat_delete_session', { sessionId }),
  renameSession:   (sessionId, title)       => invoke('chat_rename_session', { sessionId, title }),
  sendMessage:     (sessionId, text)        => invoke('chat_send_message', { sessionId, text }),
  approveTool:     (callId, editedArgs)     => invoke('chat_approve_tool', { callId, editedArgs }),
  rejectTool:      (callId, reason)         => invoke('chat_reject_tool', { callId, reason }),
  cancelTurn:      (sessionId)              => invoke('chat_cancel_turn', { sessionId }),
  cancelRun:       (runId)                  => invoke('chat_cancel_run', { runId }),
  subscribeStream: (cb)                     => listen('chat-stream', (e) => cb(e.payload)),
  subscribeUpdated:(cb)                     => listen('chat-session-updated', (e) => cb(e.payload)),
};

export const aiApi = {
  getConfig:          ()                        => invoke('ai_get_config'),
  setProviderConfig:  (providerId, config)      => invoke('ai_set_provider_config', { providerId, config }),
  setDefaultProvider: (providerId)              => invoke('ai_set_default_provider', { providerId }),
  setApiKey:          (providerId, key)         => invoke('ai_set_api_key', { providerId, key }),
  clearApiKey:        (providerId)              => invoke('ai_clear_api_key', { providerId }),
  hasApiKey:          (providerId)              => invoke('ai_has_api_key', { providerId }),
  backendInfo:        ()                        => invoke('ai_backend_info'),
};
```

- [ ] **Step 2: Commit**

```bash
git add frontend/js/api/chat.js
git commit -m "feat(frontend): chat/ai Tauri command wrappers"
```

---

## Task 30: Browser-mode mocks for chat/ai commands

**Files:**
- Modify: `frontend/index.html`

- [ ] **Step 1: Extend the existing mock `invoke` to handle new commands**

Find the `if (!window.__TAURI__) { ... }` block and add these handlers inside the `invoke` function (before the fallthrough):

```js
// --- chat session CRUD ---
const _mockSessions = new Map();
if (cmd === 'chat_list_sessions') {
  return Promise.resolve({
    schema_version: 1,
    sessions: [..._mockSessions.values()].map(s => ({
      id: s.id, title: s.title,
      created_at: s.created_at, updated_at: s.updated_at,
      message_count: s.messages.length,
      provider_snapshot: null,
    })),
  });
}
if (cmd === 'chat_create_session') {
  const id = 'ses_mock_' + Math.random().toString(16).slice(2, 10);
  const now = new Date().toISOString();
  const s = { id, title: args.title || 'New chat', created_at: now, updated_at: now, messages: [], schema_version: 1, provider_snapshot: null, summary: null };
  _mockSessions.set(id, s);
  return Promise.resolve(s);
}
if (cmd === 'chat_get_session')     return Promise.resolve(_mockSessions.get(args.sessionId));
if (cmd === 'chat_delete_session')  { _mockSessions.delete(args.sessionId); return Promise.resolve(null); }
if (cmd === 'chat_rename_session')  {
  const s = _mockSessions.get(args.sessionId); if (s) { s.title = args.title; s.updated_at = new Date().toISOString(); }
  return Promise.resolve(null);
}

// --- chat_send_message — canned streaming via synthetic events ---
if (cmd === 'chat_send_message') {
  setTimeout(() => {
    window.__TAURI__._mockEmit('chat-stream', {
      kind: 'Text', session_id: args.sessionId,
      delta: '(mock) I would call tools here in the real app.',
    });
    window.__TAURI__._mockEmit('chat-stream', { kind: 'Done', session_id: args.sessionId });
  }, 100);
  return Promise.resolve(null);
}
if (cmd === 'chat_approve_tool' || cmd === 'chat_reject_tool'
    || cmd === 'chat_cancel_turn' || cmd === 'chat_cancel_run') {
  return Promise.resolve(null);
}

// --- ai_* ---
if (cmd === 'ai_get_config') {
  return Promise.resolve({ default_provider: null, providers: {} });
}
if (cmd === 'ai_set_provider_config' || cmd === 'ai_set_default_provider'
    || cmd === 'ai_set_api_key' || cmd === 'ai_clear_api_key') {
  return Promise.resolve(null);
}
if (cmd === 'ai_has_api_key') return Promise.resolve(false);
if (cmd === 'ai_backend_info') return Promise.resolve({ keystore_backend: 'memory', config_path: null });
```

Also add a tiny in-process event emitter so mock `chat_send_message` can drive listeners:

```js
window.__TAURI__._listeners = new Map();
window.__TAURI__._mockEmit = (event, payload) => {
  const ls = window.__TAURI__._listeners.get(event) || [];
  ls.forEach(l => l({ payload }));
};
window.__TAURI__.event = {
  listen(event, cb) {
    const ls = window.__TAURI__._listeners.get(event) || [];
    ls.push(cb); window.__TAURI__._listeners.set(event, ls);
    return Promise.resolve(() => {
      const cur = window.__TAURI__._listeners.get(event) || [];
      window.__TAURI__._listeners.set(event, cur.filter(l => l !== cb));
    });
  },
};
```

- [ ] **Step 2: Verify in browser preview**

```
cd frontend && python3 -m http.server 8090
```
Open `http://localhost:8090`. Open console; no mock-related errors on load.

- [ ] **Step 3: Commit**

```bash
git add frontend/index.html
git commit -m "feat(frontend): browser-mode mocks for chat_*/ai_* commands and events"
```

---

## Task 31: Sidebar AI Copilot block

**Files:**
- Modify: `frontend/js/app.js`
- Modify: `frontend/css/style.css`

- [ ] **Step 1: Locate the current sidebar render function**

In `frontend/js/app.js`, find `renderSidebar()` (or equivalent — function that populates the project-wide nav).

- [ ] **Step 2: Insert a new AI Copilot section at the top**

Add a new block rendered *above* the Pipeline and Utilities groups. Its contents depend on two states:

```js
// inside renderSidebar — pseudocode aligned with existing patterns
async function renderAiCopilotBlock(container) {
  const hasKey = await window.__TAURI__.core.invoke('ai_has_api_key', { providerId: 'openai-compat' })
    .catch(() => false);
  const block = document.createElement('section');
  block.className = 'sidebar-group sidebar-ai-copilot';
  block.innerHTML = `
    <h3>💬 AI Copilot</h3>
    <div class="sidebar-ai-body">
      ${hasKey
        ? `<button class="btn-new-chat">+ New session</button>
           <ul class="sidebar-session-list"></ul>`
        : `<div class="sidebar-empty-cta">
             <p>Configure an AI provider to enable chat.</p>
             <button class="btn-configure-ai">Open settings</button>
           </div>`}
    </div>`;

  if (hasKey) {
    block.querySelector('.btn-new-chat').addEventListener('click', async () => {
      const s = await chatApi.createSession(null);
      location.hash = `#chat/${s.id}`;
    });
    const list = block.querySelector('.sidebar-session-list');
    const idx = await chatApi.listSessions();
    (idx.sessions || []).forEach(meta => {
      const li = document.createElement('li');
      li.textContent = meta.title;
      li.addEventListener('click', () => { location.hash = `#chat/${meta.id}`; });
      list.appendChild(li);
    });
  } else {
    block.querySelector('.btn-configure-ai').addEventListener('click', () => {
      location.hash = '#settings';
    });
  }
  container.appendChild(block);
}
```

Call `renderAiCopilotBlock` from `renderSidebar` before the existing Pipeline and Utilities blocks.

- [ ] **Step 3: Add CSS**

Append to `frontend/css/style.css`:

```css
.sidebar-ai-copilot h3 { margin: 0 0 8px; }
.sidebar-ai-body { display: flex; flex-direction: column; gap: 6px; }
.sidebar-session-list { list-style: none; padding: 0; margin: 0; }
.sidebar-session-list li { padding: 4px 8px; border-radius: 4px; cursor: pointer; }
.sidebar-session-list li:hover { background: rgba(0,0,0,0.05); }
.sidebar-empty-cta { padding: 8px; border: 1px dashed #bbb; border-radius: 4px; font-size: 0.9em; }
.btn-new-chat, .btn-configure-ai { padding: 6px 10px; border-radius: 4px; cursor: pointer; }
```

- [ ] **Step 4: Manual verify**

```
cd frontend && python3 -m http.server 8090
```
Open app. Confirm:
- With no key configured, sidebar shows CTA and clicking it navigates to `#settings`.
- After a mocked `ai_has_api_key → true` path (test by temporarily flipping the mock), sidebar shows a "+ New session" button and an empty list.

- [ ] **Step 5: Commit**

```bash
git add frontend/js/app.js frontend/css/style.css
git commit -m "feat(frontend): sidebar AI Copilot block with session list and empty CTA"
```

---

## Task 32: Session list + chat view shell + routing

**Files:**
- Create: `frontend/js/modules/chat/session-list.js`
- Create: `frontend/js/modules/chat/chat-view.js`
- Modify: `frontend/js/app.js` (add router case for `#chat` and `#chat/<id>`)

- [ ] **Step 1: Session list component**

`frontend/js/modules/chat/session-list.js`:

```js
import { chatApi } from '../../api/chat.js';

export async function renderSessionListPage(container) {
  container.innerHTML = `
    <h2>💬 AI Copilot — Sessions</h2>
    <button class="btn-new-chat">+ New chat</button>
    <ul class="session-list"></ul>`;
  container.querySelector('.btn-new-chat').addEventListener('click', async () => {
    const s = await chatApi.createSession(null);
    location.hash = `#chat/${s.id}`;
  });
  const list = container.querySelector('.session-list');
  const idx = await chatApi.listSessions();
  if (!idx.sessions || idx.sessions.length === 0) {
    list.innerHTML = '<li class="empty">No sessions yet. Click "+ New chat".</li>';
    return;
  }
  idx.sessions.forEach(meta => {
    const li = document.createElement('li');
    li.innerHTML = `
      <span class="title">${escapeHtml(meta.title)}</span>
      <span class="count">${meta.message_count} msgs</span>
      <button class="btn-del" title="Delete">×</button>`;
    li.querySelector('.title').addEventListener('click', () => {
      location.hash = `#chat/${meta.id}`;
    });
    li.querySelector('.btn-del').addEventListener('click', async (e) => {
      e.stopPropagation();
      if (confirm('Delete this session?')) {
        await chatApi.deleteSession(meta.id);
        renderSessionListPage(container);
      }
    });
    list.appendChild(li);
  });
}

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;',
                                       '"': '&quot;', "'": '&#39;' })[c]);
}
```

- [ ] **Step 2: Chat view shell (stream handling comes in Task 33)**

`frontend/js/modules/chat/chat-view.js`:

```js
import { chatApi } from '../../api/chat.js';

export async function renderChatView(container, sessionId) {
  const session = await chatApi.getSession(sessionId);
  container.innerHTML = `
    <header class="chat-header">
      <h2>${escapeHtml(session.title)}</h2>
      <span class="provider">${session.provider_snapshot?.model ?? ''}</span>
    </header>
    <div class="chat-messages"></div>
    <footer class="chat-input-bar">
      <textarea class="chat-input" placeholder="Describe what you want to analyse..."></textarea>
      <button class="btn-send">Send</button>
      <button class="btn-stop" hidden>Stop</button>
    </footer>`;

  const messagesEl = container.querySelector('.chat-messages');
  session.messages.forEach(m => appendMessage(messagesEl, m));

  container.querySelector('.btn-send').addEventListener('click', async () => {
    const ta = container.querySelector('.chat-input');
    const text = ta.value.trim();
    if (!text) return;
    appendMessage(messagesEl, { role: 'user', content: text });
    ta.value = '';
    await chatApi.sendMessage(sessionId, text);
    // streaming handled via subscribeStream in Task 33
  });

  container.querySelector('.btn-stop').addEventListener('click', () => {
    chatApi.cancelTurn(sessionId);
  });
}

function appendMessage(container, m) {
  const el = document.createElement('div');
  el.className = `msg msg-${m.role}`;
  el.textContent = m.content || '';
  container.appendChild(el);
  container.scrollTop = container.scrollHeight;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g,
    c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'})[c]);
}
```

- [ ] **Step 3: Hook into router**

In `frontend/js/app.js` `navigate()` (or equivalent), add a case:

```js
if (hash.startsWith('#chat/')) {
  const id = hash.slice('#chat/'.length);
  import('./modules/chat/chat-view.js').then(m => m.renderChatView(contentEl, id));
  return;
}
if (hash === '#chat') {
  import('./modules/chat/session-list.js').then(m => m.renderSessionListPage(contentEl));
  return;
}
```

- [ ] **Step 4: Manual verify**

```
cd frontend && python3 -m http.server 8090
```
- Navigate to `#chat`: session list renders; "+ New chat" creates a session and lands on `#chat/<id>`.
- Type "hi" and Send: the user bubble appears; in mock mode, a `Done` event will fire — actual text rendering lands in Task 33.

- [ ] **Step 5: Commit**

```bash
git add frontend/js/modules/chat/ frontend/js/app.js
git commit -m "feat(frontend): session list, chat view shell, #chat routing"
```

---

## Task 33: Message stream + schema-form helper + plan/run cards

**Files:**
- Create: `frontend/js/modules/chat/message-stream.js`
- Create: `frontend/js/modules/chat/schema-form.js`
- Create: `frontend/js/modules/chat/plan-card.js`
- Create: `frontend/js/modules/chat/run-card.js`
- Modify: `frontend/js/modules/chat/chat-view.js`
- Modify: `frontend/css/style.css` (add `@import 'chat.css';`)
- Create: `frontend/css/chat.css`

- [ ] **Step 1: Schema-to-form helper**

`frontend/js/modules/chat/schema-form.js`:

```js
// Turns a JSON Schema draft-07 object type into an HTML form.
// Supports: string, number, integer, boolean, array of scalars, enum, default.
// Nested objects: rendered inline recursively.

export function renderSchemaForm(schema, initialArgs) {
  const form = document.createElement('form');
  form.className = 'schema-form';
  const state = structuredClone(initialArgs ?? {});

  const props = schema.properties || {};
  const required = new Set(schema.required || []);

  for (const [name, sub] of Object.entries(props)) {
    const row = document.createElement('label');
    row.className = 'form-row';
    const label = document.createElement('span');
    label.textContent = name + (required.has(name) ? ' *' : '');
    label.title = sub.description || '';
    row.appendChild(label);

    const input = makeInput(sub, state[name], v => {
      state[name] = v;
    });
    row.appendChild(input);
    form.appendChild(row);
  }

  return {
    el: form,
    getValues: () => structuredClone(state),
  };
}

function makeInput(schema, currentValue, onChange) {
  if (schema.enum) {
    const sel = document.createElement('select');
    schema.enum.forEach(v => {
      const o = document.createElement('option');
      o.value = o.textContent = String(v);
      sel.appendChild(o);
    });
    if (currentValue != null) sel.value = currentValue;
    sel.addEventListener('change', () => onChange(sel.value));
    return sel;
  }
  const t = schema.type;
  if (t === 'boolean') {
    const cb = document.createElement('input'); cb.type = 'checkbox';
    cb.checked = !!currentValue;
    cb.addEventListener('change', () => onChange(cb.checked));
    return cb;
  }
  if (t === 'integer' || t === 'number') {
    const num = document.createElement('input'); num.type = 'number';
    if (schema.minimum != null) num.min = schema.minimum;
    if (schema.maximum != null) num.max = schema.maximum;
    num.value = currentValue ?? schema.default ?? '';
    num.addEventListener('input', () => {
      const v = num.value === '' ? undefined : (t === 'integer' ? parseInt(num.value) : parseFloat(num.value));
      onChange(v);
    });
    return num;
  }
  if (t === 'array') {
    const ta = document.createElement('textarea'); ta.placeholder = 'one value per line';
    ta.value = Array.isArray(currentValue) ? currentValue.join('\n') : '';
    ta.addEventListener('input', () => {
      onChange(ta.value.split('\n').map(s => s.trim()).filter(Boolean));
    });
    return ta;
  }
  // default string
  const tx = document.createElement('input'); tx.type = 'text';
  tx.value = currentValue ?? schema.default ?? '';
  tx.addEventListener('input', () => onChange(tx.value));
  return tx;
}
```

- [ ] **Step 2: Plan card**

`frontend/js/modules/chat/plan-card.js`:

```js
import { chatApi } from '../../api/chat.js';
import { renderSchemaForm } from './schema-form.js';

export function createPlanCard({ callId, name, args, schema, risk }) {
  const el = document.createElement('div');
  el.className = `plan-card plan-card-${risk}`;
  el.dataset.callId = callId;
  el.innerHTML = `
    <header class="plan-card-header">
      <span class="plan-tool">${name}</span>
      <span class="plan-risk">${risk}</span>
    </header>
    <div class="plan-form"></div>
    <footer class="plan-actions">
      <button class="btn-exec">Execute</button>
      <button class="btn-reject">Reject</button>
    </footer>`;
  const form = renderSchemaForm(schema || { type: 'object', properties: {} }, args);
  el.querySelector('.plan-form').appendChild(form.el);
  el.querySelector('.btn-exec').addEventListener('click', async () => {
    el.querySelector('.btn-exec').disabled = true;
    el.querySelector('.btn-reject').disabled = true;
    await chatApi.approveTool(callId, form.getValues());
    markStatus(el, 'running');
  });
  el.querySelector('.btn-reject').addEventListener('click', async () => {
    const reason = prompt('Optional reason for rejection:');
    await chatApi.rejectTool(callId, reason || null);
    markStatus(el, 'rejected');
  });
  return el;
}

export function markStatus(cardEl, status) {
  cardEl.classList.add(`plan-card-${status}`);
  const footer = cardEl.querySelector('.plan-actions');
  if (footer) footer.hidden = true;
  const badge = document.createElement('span');
  badge.className = 'plan-status';
  badge.textContent = status;
  cardEl.querySelector('header').appendChild(badge);
}
```

- [ ] **Step 3: Run card**

`frontend/js/modules/chat/run-card.js`:

```js
import { chatApi } from '../../api/chat.js';

export function createRunCard({ runId, moduleId }) {
  const el = document.createElement('div');
  el.className = 'run-card';
  el.dataset.runId = runId;
  el.innerHTML = `
    <header>
      <span class="run-module">${moduleId}</span>
      <span class="run-id">${runId}</span>
    </header>
    <div class="run-progress"><div class="run-bar"></div></div>
    <div class="run-status">starting…</div>
    <footer>
      <button class="btn-run-cancel">Cancel run</button>
      <button class="btn-run-open" hidden>View details</button>
    </footer>`;
  el.querySelector('.btn-run-cancel').addEventListener('click', () => chatApi.cancelRun(runId));
  return el;
}

export function bindRunProgress(cardEl) {
  window.__TAURI__.event.listen('run-progress', (e) => {
    if (e.payload.run_id !== cardEl.dataset.runId) return;
    const pct = Math.round((e.payload.fraction ?? 0) * 100);
    cardEl.querySelector('.run-bar').style.width = `${pct}%`;
    cardEl.querySelector('.run-status').textContent = e.payload.message || `${pct}%`;
  });
  window.__TAURI__.event.listen('run-completed', (e) => {
    if (e.payload.run_id !== cardEl.dataset.runId) return;
    cardEl.querySelector('.btn-run-cancel').hidden = true;
    cardEl.querySelector('.btn-run-open').hidden = false;
    cardEl.querySelector('.run-status').textContent = e.payload.status || 'done';
  });
}
```

- [ ] **Step 4: Message stream dispatcher**

`frontend/js/modules/chat/message-stream.js`:

```js
import { chatApi } from '../../api/chat.js';
import { createPlanCard, markStatus } from './plan-card.js';
import { createRunCard, bindRunProgress } from './run-card.js';

export function attachStream({ container, sessionId, toolSchemasByName }) {
  let currentAssistant = null;
  let raf = null;

  const append = el => { container.appendChild(el); container.scrollTop = container.scrollHeight; };

  const ensureAssistantBubble = () => {
    if (!currentAssistant) {
      currentAssistant = document.createElement('div');
      currentAssistant.className = 'msg msg-assistant';
      append(currentAssistant);
    }
    return currentAssistant;
  };

  const scheduleRender = (bubble, text) => {
    bubble.dataset.raw = (bubble.dataset.raw || '') + text;
    if (raf) return;
    raf = requestAnimationFrame(() => {
      bubble.textContent = bubble.dataset.raw;
      raf = null;
    });
  };

  return chatApi.subscribeStream(ev => {
    if (ev.session_id !== sessionId) return;
    if (ev.kind === 'Text') {
      const bubble = ensureAssistantBubble();
      scheduleRender(bubble, ev.delta);
    } else if (ev.kind === 'ToolCall') {
      if (ev.risk === 'read') {
        const row = document.createElement('div');
        row.className = 'tool-auto';
        row.textContent = `🔧 ${ev.name}  (auto)`;
        row.dataset.callId = ev.call_id;
        append(row);
      } else {
        const schema = toolSchemasByName[ev.name];
        append(createPlanCard({ callId: ev.call_id, name: ev.name, args: ev.args, schema, risk: ev.risk }));
      }
    } else if (ev.kind === 'ToolResult') {
      // Update any existing plan/tool card
      const card = container.querySelector(`[data-call-id="${ev.call_id}"]`);
      if (card) markStatus(card, 'done');
      if (ev.result?.run_id) {
        const rc = createRunCard({ runId: ev.result.run_id, moduleId: ev.result.module_id || '' });
        append(rc); bindRunProgress(rc);
      } else {
        // show compact result summary
        const row = document.createElement('pre');
        row.className = 'tool-result';
        row.textContent = JSON.stringify(ev.result, null, 2).slice(0, 500);
        append(row);
      }
    } else if (ev.kind === 'Done') {
      currentAssistant = null;
    } else if (ev.kind === 'Error') {
      const err = document.createElement('div');
      err.className = 'msg-error';
      err.textContent = `Error: ${ev.message}`;
      append(err);
    }
  });
}
```

- [ ] **Step 5: Wire stream into chat view + load tool schemas**

Update `chat-view.js`'s `renderChatView`:

```js
import { attachStream } from './message-stream.js';
// ...
const toolSchemas = {}; // Populated by reading provider-known tools;
// for Phase 1 we opportunistically collect from the session's past tool calls.
attachStream({ container: container.querySelector('.chat-messages'), sessionId, toolSchemasByName: toolSchemas });
```

For Phase 1, `toolSchemas` can remain empty — the schema form gracefully renders whatever keys appear in `args`. Later the frontend can fetch schemas via a `chat_list_tools` command (not in Phase 1 scope).

- [ ] **Step 6: Create `chat.css`**

```css
.chat-messages { display: flex; flex-direction: column; gap: 8px; padding: 12px; overflow-y: auto; }
.msg { padding: 8px 12px; border-radius: 8px; max-width: 70ch; }
.msg-user { align-self: flex-end; background: #e8f0ff; }
.msg-assistant { align-self: flex-start; background: #f6f6f6; }
.msg-error { align-self: center; color: #b00020; }
.tool-auto { align-self: flex-start; color: #666; font-size: 0.9em; font-family: monospace; }
.tool-result { align-self: flex-start; background: #fafafa; border: 1px solid #ddd; padding: 4px 6px; }
.plan-card { border: 1px solid #4a9; border-radius: 8px; padding: 8px; margin: 6px 0; }
.plan-card-header { display: flex; justify-content: space-between; align-items: center; }
.plan-tool { font-weight: 600; }
.plan-risk { background: #ffd; padding: 2px 6px; border-radius: 4px; font-size: 0.8em; }
.plan-form { margin-top: 6px; display: flex; flex-direction: column; gap: 4px; }
.form-row { display: flex; gap: 8px; align-items: center; }
.form-row > span { min-width: 120px; }
.plan-actions { display: flex; gap: 6px; margin-top: 8px; }
.run-card { border: 1px solid #09c; border-radius: 6px; padding: 6px 8px; margin: 6px 0; }
.run-progress { background: #eee; height: 6px; border-radius: 3px; overflow: hidden; margin: 4px 0; }
.run-bar { background: #09c; height: 100%; width: 0%; transition: width 0.3s; }
.chat-input-bar { display: flex; gap: 6px; padding: 8px; border-top: 1px solid #ddd; }
.chat-input { flex: 1; min-height: 2.5em; max-height: 12em; }
```

Add `@import 'chat.css';` at the top of `frontend/css/style.css`.

- [ ] **Step 7: Manual verify**

```
cd frontend && python3 -m http.server 8090
```
- Navigate to a session. Type "test". Confirm bubble render + mock "Done" stream clears the assistant bubble.

- [ ] **Step 8: Commit**

```bash
git add frontend/js/modules/chat/ frontend/css/chat.css frontend/css/style.css frontend/js/app.js
git commit -m "feat(frontend): streaming chat view with plan cards and run cards"
```

---

## Task 34: Project-creation wizard — `default_view` radio

**Files:**
- Modify: `frontend/js/app.js`

- [ ] **Step 1: Find the project-creation form**

Look for `renderNewProjectModal` or similar in `app.js`.

- [ ] **Step 2: Insert the radio group**

Add above the submit button:

```html
<fieldset class="default-view-group">
  <legend>Default view when opening this project</legend>
  <label>
    <input type="radio" name="default_view" value="ai" />
    AI analysis mode <small>(recommended for new users)</small>
  </label>
  <label>
    <input type="radio" name="default_view" value="manual" checked />
    Manual pipeline <small>(recommended if you have a defined SOP)</small>
  </label>
  <p class="hint">Both views are always available. This only sets the initial landing page.</p>
</fieldset>
```

- [ ] **Step 3: Pass to `create_project`**

Update the form submit handler:

```js
const defaultView = form.querySelector('input[name="default_view"]:checked').value;
await invoke('create_project', { name, rootDir, defaultView });
```

- [ ] **Step 4: Use `default_view` to pick initial route on open**

After `open_project` resolves, read the project's `default_view` (returned by the command) and route:

```js
location.hash = (project.default_view === 'ai') ? '#chat' : '#qc';
```

- [ ] **Step 5: Manual verify + commit**

```
cd frontend && python3 -m http.server 8090
```
Create project with "AI" selected; confirm it lands on `#chat`.

```bash
git add frontend/js/app.js
git commit -m "feat(frontend): default_view radio in new-project wizard, initial route honoured"
```

---

## Task 35: Settings AI Provider section

**Files:**
- Modify: `frontend/js/app.js`
- Modify: `frontend/css/style.css`

- [ ] **Step 1: Extend the settings page**

In `renderSettings` (or equivalent), add a new section:

```html
<section class="settings-ai">
  <h3>AI Provider</h3>
  <form class="ai-provider-form">
    <label>Provider
      <select name="provider_id">
        <option value="openai-compat">OpenAI-compatible</option>
      </select>
    </label>
    <label>Base URL
      <input type="url" name="base_url" value="https://api.openai.com/v1" />
    </label>
    <label>Model
      <input type="text" name="model" placeholder="e.g. gpt-4o-mini or deepseek-chat" />
    </label>
    <label>Temperature
      <input type="range" name="temperature" min="0" max="2" step="0.05" value="0.2" />
    </label>
    <label>API Key
      <input type="password" name="api_key" autocomplete="off" />
    </label>
    <div class="ai-provider-actions">
      <button type="button" class="btn-save">Save</button>
      <button type="button" class="btn-clear-key">Clear key</button>
      <span class="ai-key-state"></span>
    </div>
  </form>
</section>
```

- [ ] **Step 2: Wire up handlers**

```js
import { aiApi } from './api/chat.js';

async function bindAiSettings(formEl) {
  const cfg = await aiApi.getConfig();
  const pc = cfg.providers?.['openai-compat'];
  if (pc) {
    formEl.querySelector('[name="base_url"]').value = pc.base_url;
    formEl.querySelector('[name="model"]').value = pc.model;
    formEl.querySelector('[name="temperature"]').value = pc.temperature;
  }
  const hasKey = await aiApi.hasApiKey('openai-compat');
  formEl.querySelector('.ai-key-state').textContent = hasKey ? '✓ Key saved' : 'No key configured';

  formEl.querySelector('.btn-save').addEventListener('click', async () => {
    const config = {
      base_url: formEl.querySelector('[name="base_url"]').value,
      model:    formEl.querySelector('[name="model"]').value,
      temperature: parseFloat(formEl.querySelector('[name="temperature"]').value),
    };
    await aiApi.setProviderConfig('openai-compat', config);
    const key = formEl.querySelector('[name="api_key"]').value;
    if (key) {
      await aiApi.setApiKey('openai-compat', key);
      formEl.querySelector('[name="api_key"]').value = '';
    }
    formEl.querySelector('.ai-key-state').textContent =
      (await aiApi.hasApiKey('openai-compat')) ? '✓ Key saved' : 'No key configured';
    alert('Saved.');
  });
  formEl.querySelector('.btn-clear-key').addEventListener('click', async () => {
    await aiApi.clearApiKey('openai-compat');
    formEl.querySelector('.ai-key-state').textContent = 'No key configured';
  });
}

bindAiSettings(document.querySelector('.ai-provider-form'));
```

- [ ] **Step 3: Verify, commit**

```
cd frontend && python3 -m http.server 8090
```
- Settings page shows the form.
- Save → reload: values persist (in real Tauri mode; mock mode resets).

```bash
git add frontend/js/app.js frontend/css/style.css
git commit -m "feat(frontend): Settings > AI Provider section with save/clear key"
```

---

## Task 36: i18n strings for chat UI

**Files:**
- Modify: `frontend/js/i18n.js`

- [ ] **Step 1: Add chat strings to both dictionaries**

Append keys to the `en` and `zh` objects:

```js
// en
chat_copilot_title: 'AI Copilot',
chat_new_session: '+ New session',
chat_configure_ai_cta: 'Configure an AI provider to enable chat.',
chat_open_settings: 'Open settings',
chat_placeholder: 'Describe what you want to analyse...',
chat_btn_send: 'Send',
chat_btn_stop: 'Stop',
chat_plan_execute: 'Execute',
chat_plan_reject: 'Reject',
chat_default_view_legend: 'Default view when opening this project',
chat_default_view_ai: 'AI analysis mode',
chat_default_view_manual: 'Manual pipeline',
settings_ai_title: 'AI Provider',
settings_ai_base_url: 'Base URL',
settings_ai_model: 'Model',
settings_ai_temperature: 'Temperature',
settings_ai_api_key: 'API Key',
settings_ai_key_saved: '✓ Key saved',
settings_ai_no_key: 'No key configured',
settings_ai_save: 'Save',
settings_ai_clear_key: 'Clear key',
```

Mirror each key in `zh`:

```js
chat_copilot_title: 'AI 副驾驶',
chat_new_session: '+ 新建会话',
chat_configure_ai_cta: '配置一个 AI provider 以启用对话功能。',
chat_open_settings: '打开设置',
chat_placeholder: '描述你想做的分析...',
chat_btn_send: '发送',
chat_btn_stop: '停止',
chat_plan_execute: '执行',
chat_plan_reject: '拒绝',
chat_default_view_legend: '打开此项目时的默认视图',
chat_default_view_ai: 'AI 分析模式',
chat_default_view_manual: '手动流水线',
settings_ai_title: 'AI Provider',
settings_ai_base_url: 'Base URL',
settings_ai_model: '模型',
settings_ai_temperature: '温度',
settings_ai_api_key: 'API Key',
settings_ai_key_saved: '✓ 已保存密钥',
settings_ai_no_key: '尚未配置密钥',
settings_ai_save: '保存',
settings_ai_clear_key: '清除密钥',
```

- [ ] **Step 2: Replace hardcoded strings in the new views with `t('key')` calls**

Scan `sidebar-ai-copilot`, `session-list.js`, `chat-view.js`, `plan-card.js`, `default_view` radio, AI settings section — swap each literal for `t('key')`.

- [ ] **Step 3: Manual verify + commit**

```
cd frontend && python3 -m http.server 8090
```
Toggle language. Confirm every chat string updates.

```bash
git add frontend/js/i18n.js frontend/js/app.js frontend/js/modules/chat/
git commit -m "feat(frontend): chat UI strings localized (zh/en)"
```

---

## Task 37: End-to-end smoke test (manual script)

**Files:**
- Create: `docs/ai-chat-smoke.md` — manual test log

- [ ] **Step 1: Run a full verification pass**

```
RUSTFLAGS="--cap-lints=warn" cargo clippy -p rb-ai -p rb-core -p rb-app -- -D warnings
cargo test --workspace
cd crates/rb-app && cargo tauri dev
```

- [ ] **Step 2: Walk through the golden path**

Create `docs/ai-chat-smoke.md` with the steps you followed. Use this template:

```markdown
# AI Chat Mode — Smoke Test Log

Date: YYYY-MM-DD
Build: <git rev-parse --short HEAD>
Provider: OpenAI-compatible (`base_url: https://api.deepseek.com/v1`, `model: deepseek-chat`)

## Setup
- [ ] cargo tauri dev launches
- [ ] Settings > AI Provider: saved base_url, model, temperature
- [ ] Settings > AI Provider: entered a valid test API key — state shows ✓
- [ ] ai_backend_info reports `keystore_backend` is `keyring` (or `encrypted-file` on headless Linux)

## New project flow
- [ ] New project wizard shows default_view radio
- [ ] Creating with "AI analysis mode" lands on `#chat` with an empty session list
- [ ] Creating with "Manual pipeline" lands on `#qc` as before

## Chat golden path
- [ ] "+ New chat" creates a session, reflected in sidebar and session list
- [ ] Send "run QC on data/*.fastq.gz" → assistant streams text
- [ ] Plan card appears for `run_qc` with editable `input` array
- [ ] Clicking Execute spawns a run; run card appears with live progress
- [ ] After run completes, `[View details]` navigates to traditional QC result page
- [ ] Switching to traditional QC view shows the same run in the list

## Edge cases
- [ ] Reject plan card → assistant continues, explains
- [ ] Cancel button on run card → run status transitions to Cancelled
- [ ] Stop button on streaming → assistant bubble marked (stopped by user)
- [ ] Wrong API key → Settings test shows auth error; no chat crash
- [ ] Delete session → disappears from sidebar and list
- [ ] Restart app → session persists; sidebar still shows it

## i18n
- [ ] Switch language toggle updates all chat UI strings
- [ ] system prompt is chosen based on `RUSTBRAIN_LANG` env var (or current UI lang)

## Known limitations observed
- [ ] (note anything surprising)
```

- [ ] **Step 3: Commit**

```bash
git add docs/ai-chat-smoke.md
git commit -m "docs: AI chat mode manual smoke-test checklist"
```

---

## Task 38: Update main README + CLAUDE.md

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: README** — add one paragraph under Features and one crate row

Under Features, append:

```markdown
- **AI analysis mode (Phase 1)** — create a project in AI mode and drive analyses through natural-language chat. The copilot proposes plans, you approve or edit them, and AI-initiated runs share the same run history as the manual UI. Works with any OpenAI-compatible endpoint (OpenAI / DeepSeek / Moonshot / Qwen / vLLM / Ollama).
```

Under the crate tree, add `rb-ai` with a one-line description.

- [ ] **Step 2: CLAUDE.md** — add a short architecture note

After the existing "Cargo Workspace" section, add:

```markdown
**rb-ai** — AI orchestration crate. Owns provider adapters (OpenAI-compatible in Phase 1; Anthropic/Ollama gated behind features), tool registry (builtin Read-risk tools, module-derived `run_*` tools, Phase 3 stubs), chat session persistence (`<project>/chats/`), and the `run_turn` orchestrator loop. Depends on `rb-core`; does not depend on Tauri. Phase 1 ships "B" (single-module conversational execution) with forward-compatible schema for Phases 2/3 — see `docs/superpowers/specs/2026-04-19-ai-chat-mode-design.md`.
```

- [ ] **Step 3: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: document AI chat mode (Phase 1) in README and CLAUDE.md"
```

---

## Post-Implementation Verification

After all tasks complete, run a full sanity check:

```bash
cargo check --workspace
cargo test --workspace
RUSTFLAGS="--cap-lints=warn" cargo clippy \
  -p rb-ai -p rb-core -p rb-app \
  -p rb-qc -p rb-trimming -p rb-star-index -p rb-star-align \
  -p rb-gff-convert -p rb-deseq2 \
  -- -D warnings
cargo fmt -p rb-ai -p rb-core -p rb-app \
  -p rb-qc -p rb-trimming -p rb-star-index -p rb-star-align \
  -p rb-gff-convert -p rb-deseq2
```

Then follow `docs/ai-chat-smoke.md` end-to-end against a real provider.

**Deliverables checklist:**

- [ ] All 38 tasks completed with their commits
- [ ] `cargo test --workspace` green
- [ ] Clippy clean on our crates
- [ ] Manual smoke test passes
- [ ] Spec reservations still intact (session `summary` field, `provider_snapshot`, stub tools registered)







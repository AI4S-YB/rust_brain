# AI Chat Mode Design

**Date**: 2026-04-19
**Status**: Approved (brainstorming phase)
**Scope**: New `rb-ai` crate + `rb-app` extensions + frontend `#chat` view. Traditional UI retained unchanged alongside.
**Target**: Phase 1 ships "B" (single-module conversational execution); architecture preserves forward compatibility to "C" (multi-step pipelines) and "D" (full analysis agent) with no breaking changes.

## Problem

The current app exposes each analysis module as a form-based view. This works for users with a clear SOP but has friction for:

1. **First-time / non-bioinformatics users**: they know the science question (e.g., "find DE genes between treated and control") but don't know which module to pick or what `--sjdbOverhang` means.
2. **Exploratory analyses**: switching between QC, alignment, and DE across forms is cumbersome.
3. **Power users at the end of a run**: "why did 40% of reads fail to align?" requires opening multiple result files in separate tools.

An AI-driven conversational mode solves all three, but introducing it naively would bifurcate the codebase (two data models, two run histories, two UI trees). The design below keeps **one project, one Runner, one RunRecord** — AI is a new *originator* of module executions, not a parallel universe.

## Goals

1. Users can create a project, optionally select "AI analysis mode" as the default landing view, and drive analyses entirely through natural language for single-module tasks (Phase 1).
2. Traditional UI is fully preserved; users can switch views freely within any project without data loss or duplication.
3. AI-initiated runs appear in the same `runs/` directory and `run-progress` event stream as UI-initiated runs. Any module added later is automatically visible to the AI.
4. The multi-provider abstraction supports OpenAI-compatible endpoints on day one (OpenAI / DeepSeek / Moonshot / Qwen / vLLM / Ollama OpenAI-compat), with Anthropic and native-Ollama slots reserved behind the same trait.
5. API keys never traverse the webview boundary. All provider I/O happens in Rust; the frontend only sees redacted state.
6. Long-running tool calls (STAR alignment, etc.) return immediately with a `run_id`; the chat session does not block on completion.
7. The architecture supports Phase 2 (C) and Phase 3 (D) as pure additions — no tool-schema changes, no persistence migrations, no provider-contract revisions.

## Non-Goals (Phase 1)

- Multi-step automatic pipelines (QC → trim → align → DE in one turn). Tool schema and orchestrator loop support this; Phase 1 UX restricts to single-module plans.
- Reading analysis results (TSVs, QC HTML) from inside the AI. Stub tools registered (`read_results_table`, `summarize_run`, `generate_plot`) return `Unimplemented`. Implementations land in Phase 3.
- Session forking, conversation summarization, and cross-session references.
- Real-time multi-provider switching within a session. Provider is chosen at app level; `provider_snapshot` is recorded per session for future use.
- A separate "AI Copilot" floating drawer inside traditional views.

## Architecture Overview

### Crate Layout

```
crates/
├── rb-core/           unchanged public surface; Module trait gains params_schema() + ai_hint()
├── rb-app/            new chat_* Tauri commands + chat-stream events
├── rb-qc/             adds params_schema() + ai_hint()
├── rb-trimming/       ditto
├── rb-star-index/     ditto
├── rb-star-align/     ditto
├── rb-gff-convert/    ditto
├── rb-deseq2/         ditto
└── rb-ai/             NEW
    ├── src/
    │   ├── lib.rs
    │   ├── provider/
    │   │   ├── mod.rs          ChatProvider trait + ProviderEvent enum
    │   │   ├── openai_compat.rs OpenAI-compatible SSE streaming impl (Phase 1)
    │   │   ├── anthropic.rs    (Phase 1 ships compile-guarded stub behind `anthropic` feature, default off)
    │   │   └── ollama.rs       (Phase 1 ships compile-guarded stub behind `ollama-native` feature, default off)
    │   ├── tools/
    │   │   ├── mod.rs          ToolRegistry
    │   │   ├── schema.rs       ToolDef, RiskLevel, JSON Schema helpers
    │   │   ├── builtin.rs      static tools (list_project_files, etc.)
    │   │   ├── module_derived.rs adapters: Module → ToolDef
    │   │   └── stubs.rs        Phase 3 tools returning Unimplemented
    │   ├── session/
    │   │   ├── mod.rs          ChatSession, SessionIndex
    │   │   ├── message.rs      Message enum (User/Assistant/Tool/System)
    │   │   └── store.rs        atomic file IO for chats/index.json + {id}.json
    │   ├── orchestrator/
    │   │   ├── mod.rs          run_turn() main loop
    │   │   ├── snapshot.rs     project snapshot injection
    │   │   ├── plan_card.rs    pending tool-call state machine
    │   │   └── prompt.rs       system prompt templates (zh/en)
    │   └── config/
    │       ├── mod.rs          AiConfig serde types
    │       └── keyring.rs      keyring crate wrapper + encrypted-file fallback
```

`rb-ai` depends on `rb-core` (to access `ModuleRegistry`, `Runner`, `RunRecord`, `Project`). It does **not** depend on `rb-app` or Tauri — so it could be reused for a future headless CLI or MCP server.

### Data Flow

```
Frontend  invoke('chat_send_message', {session_id, text})
  ↓
rb-app   chat_send_message command
  ↓
rb-ai    orchestrator::run_turn()
  ├─ session.load() → append UserMsg → session.save()
  ├─ build ChatRequest {
  │     system: lang-specific prompt + project snapshot
  │     messages: session.messages
  │     tools: ToolRegistry.all_for_ai()
  │  }
  ├─ loop:
  │    provider.send(req, sink, cancel_token)
  │    sink events:
  │      TextDelta(s)    → emit chat-stream(Text), accumulate
  │      ToolCall{...}   → dispatch by RiskLevel:
  │         Read         → execute immediately, emit ToolCall+ToolResult,
  │                        append ToolMsg, provider.send() again
  │         Run          → emit ToolCall(pending), PAUSE loop,
  │                        await chat_approve_tool or chat_reject_tool,
  │                        on approve: Runner.spawn(module, args) → run_id,
  │                        tool_result = {run_id, status:"started"},
  │                        append ToolMsg, provider.send() again
  │         Destructive  → (not registered in Phase 1)
  │      Finish          → emit Done, session.save()
```

The Runner, RunRecord, and Tauri `run-progress` / `run-completed` event stream are unchanged. The frontend's existing chart/result pages render AI-initiated runs with zero modification.

### Tool Protocol (neutral schema)

```rust
pub enum RiskLevel { Read, Run, Destructive }

pub struct ToolDef {
    pub name: String,
    pub description: String,      // localized
    pub risk: RiskLevel,
    pub params: serde_json::Value, // JSON Schema draft-07
}
```

Provider adapters serialize `ToolDef` into each vendor's format (OpenAI `tools[].function`, Anthropic `tools`, Ollama `tools`). Clients never see the vendor-specific wire format.

**Registry sources**:
- **Static builtins** (`tools/builtin.rs`): `list_project_files`, `read_table_preview`, `get_project_info`, `get_run_status`, `list_known_binaries`. All Read-risk.
- **Module-derived** (`tools/module_derived.rs`): for each `Module` in `ModuleRegistry`, emit a `run_{module.id}` tool. `params` comes from `module.params_schema()`; `description` from `module.ai_hint()`; risk is always `Run`.
- **Stubs** (`tools/stubs.rs`): `read_results_table`, `summarize_run`, `generate_plot`. Registered so LLMs trained on broader schemas can discover them, but the execution layer returns `ToolError::Unimplemented` with a message explaining they land in a future version.

### Plan Card Protocol

When the LLM emits a `Run`-risk tool call, the orchestrator does not execute. Instead it:

1. Records a `PendingToolCall { call_id, tool_name, args, risk }` keyed by `call_id`.
2. Emits `chat-stream` with `{kind: "ToolCall", call_id, name, risk: "run", args, status: "pending"}`.
3. Suspends the orchestrator loop for this session.

The frontend renders a plan card: tool name, rendered form from the tool's JSON Schema (user can edit), and two buttons. User interaction maps to:

- `chat_approve_tool(call_id, edited_args?)` — resumes orchestrator with possibly-edited args; `Runner.spawn()` is called; `tool_result = {run_id, status: "started"}` feeds back to LLM.
- `chat_reject_tool(call_id, reason?)` — resumes orchestrator; `tool_result = {error: "rejected_by_user", reason}` feeds back; LLM typically explains alternatives.

## Components

### rb-core `Module` Trait Extension

```rust
#[async_trait]
pub trait Module: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn validate(&self, params: &Value) -> Result<()>;
    async fn run(&self, ctx: ModuleContext) -> Result<()>;

    // NEW — default returns None; modules without a schema are NOT registered as AI tools.
    fn params_schema(&self) -> Option<Value> { None }

    // NEW — default returns empty; modules fill this to guide the LLM. `lang` is "en" or "zh".
    fn ai_hint(&self, lang: &str) -> String { String::new() }
}
```

Each existing module gets both methods in Phase 1. `params_schema()` returns a JSON Schema draft-07 object describing required and optional params with types and constraints (mirroring what the existing UI form collects). A module whose `params_schema()` returns `None` is skipped during tool-registry derivation — this guards against accidentally exposing half-specified modules to the AI. `ai_hint()` returns one paragraph in the requested language (`"en"` or `"zh"`, matching the app's current i18n state) describing when and how the LLM should choose this tool (e.g., "Use `run_qc` after receiving raw FASTQ files to assess per-base quality before trimming.").

### Provider Abstraction

```rust
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

pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub temperature: f32,
}

pub enum ProviderEvent {
    TextDelta(String),
    ToolCall { id: String, name: String, args: Value },
    Finish(FinishReason),
}

pub enum FinishReason { Stop, ToolCalls, Length, Error(String) }
```

`OpenAiCompatProvider` streams SSE (`text/event-stream`), parses `data:` frames into `ProviderEvent`. Supports custom `base_url`; model name is user-configured.

### Session Persistence

```
<project>/
├── project.json          ← adds "default_view": "ai" | "manual"
├── runs/                 ← unchanged
└── chats/                ← NEW
    ├── index.json        ← [{id, title, created_at, updated_at, message_count, provider_snapshot}]
    └── {session_id}.json
```

`{session_id}.json`:
```json
{
  "schema_version": 1,
  "id": "ses_01hx...",
  "title": "QC troubleshooting 2026-04-19",
  "created_at": "2026-04-19T10:12:00Z",
  "updated_at": "2026-04-19T10:43:00Z",
  "provider_snapshot": { "provider_id": "openai-compat", "model": "deepseek-chat" },
  "summary": null,
  "messages": [
    { "role": "user", "content": "..." },
    { "role": "assistant", "content": "...", "tool_calls": [ { "call_id": "tc_...", "name": "run_qc", "args": {...} } ] },
    { "role": "tool", "call_id": "tc_...", "name": "run_qc", "result": { "run_id": "..." } }
  ]
}
```

Every append triggers an atomic write (`write temp file → fsync → rename`). `index.json` updates afterward; partial failure recovery walks `chats/*.json` and rebuilds the index.

Session titles auto-generate from the first user message (truncated to ~40 chars); user can rename.

### Configuration

**App-level config** at `{tauri::app_config_dir}/config.json`:
```json
{
  "ai": {
    "default_provider": "openai-compat",
    "providers": {
      "openai-compat": {
        "base_url": "https://api.deepseek.com/v1",
        "model": "deepseek-chat",
        "temperature": 0.2
      }
    }
  }
}
```

**API keys** stored separately via `keyring` crate:
- service = `rustbrain`, account = `ai.provider.{provider_id}.api_key`
- Tauri commands: `ai_set_api_key(provider_id, key)` / `ai_clear_api_key(provider_id)` / `ai_has_api_key(provider_id) -> bool`
- **Never returns plaintext to the frontend.**

**Linux fallback** when `keyring` reports unavailable (headless servers without libsecret): `~/.config/rustbrain/secrets.enc` encrypted with AES-GCM using a key derived from `/etc/machine-id` + app salt. UI labels the state "keyring unavailable — using encrypted file fallback" so users know.

### Orchestrator Main Loop

Located at `rb-ai/src/orchestrator/mod.rs`. Single entry point:

```rust
pub async fn run_turn(
    ctx: &OrchestratorCtx,
    session: Arc<Mutex<ChatSession>>,
    user_text: String,
    event_sink: mpsc::Sender<ChatStreamEvent>,
    cancel: CancellationToken,
) -> Result<(), TurnError>;
```

Loop body (pseudocode):
```
1. session.append(User(text)) → session.save()
2. loop {
     req = build_chat_request(session, snapshot(), tool_registry)
     (text_buf, tool_calls) = drive_provider(req, event_sink, cancel)
     session.append(Assistant(text_buf, tool_calls))
     if tool_calls.is_empty() {
         session.save()
         event_sink.send(Done)
         break
     }
     for tc in tool_calls {
         match tool.risk {
           Read:  result = execute_read_tool(tc.name, tc.args)
                  session.append(Tool(result))
           Run:   result = await_user_decision(tc)  // suspends loop
                  if approved: run_id = Runner.spawn(...); result = {run_id}
                  session.append(Tool(result))
         }
     }
     session.save()
     continue  // feed tool results back to provider
   }
```

Cancellation token is checked between iterations and passed into `provider.send()` to abort streaming HTTP mid-flight.

### Project Snapshot Injection

On every turn, `snapshot.rs` builds a compact string appended to the system prompt:

```
Current project: "rnaseq_2026q2"
Top-level files: data/ (14 files, 2 fastq.gz samples), raw/samples.csv (5 lines), references/
Recent runs (last 10):
- qc_a1b2     Done       2026-04-19 09:31
- trim_c3d4   Done       2026-04-19 09:52
- star_e5f6   Running    started 10:05
Available binaries: star (bundled), gffread-rs (bundled), cutadapt-rs (PATH)
```

Target ~500 tokens. If the project is large, the snapshot shows only top-level directories with counts; the LLM can call `list_project_files(subdir)` for detail.

### System Prompt

Two files under `rb-ai/src/orchestrator/prompts/`: `system_zh.md` and `system_en.md`. Chosen at runtime based on app i18n. Key sections:

1. **Identity**: "You are RustBrain's analysis copilot for transcriptomics."
2. **Capabilities**: "You can discover project data, inspect tables, and trigger analyses via tools."
3. **Long-running semantics**: "Tools that start runs return a `run_id` immediately. Never claim a run is complete unless you've seen `status: \"Done\"` from `get_run_status`. Never invent `run_id`s."
4. **Plan-card awareness**: "For Run-risk tools, the user sees a plan card and may edit your parameters before executing. Propose sensible defaults but keep args minimal; don't pack extra speculative flags."
5. **Safety rails**: "Never instruct the user to run shell commands or modify files outside the project. Never call tools that are not in the provided tool list."
6. **Project snapshot** (dynamic injection).

## Frontend

### Routing Additions

```
#chat                    ← session list for current project
#chat/{session_id}       ← specific chat window
#settings                ← extended with "AI Provider" section
```

Existing routes (`#qc`, `#trimming`, `#star-index`, `#star-align`, `#differential`, `#gff-convert`) unchanged.

### Sidebar Layout

```
📁 Project: rnaseq_2026q2

💬 AI Copilot          ← NEW, at top
    QC troubleshooting
    Run DESeq2
    + New session

━━ Pipeline ━━
  Quality Control
  Trimming
  STAR Index
  STAR Align
  Differential (DESeq2)

━━ Utilities ━━
  GFF Convert
```

The AI Copilot block is always visible. If no provider is configured, the session list is replaced with a call-to-action card that links to the settings page.

### Chat Window

Structure:
```
[session title] [provider: deepseek-chat]                     [⋯ menu]
─────────────────────────────────────────────────────────────────────
(message stream)
  User bubble
  Assistant bubble (Markdown + code blocks, streaming)
  🔧 Tool call card (Read): collapsed, shows name + brief result
  📋 Plan Card (Run): expanded, form rendered from JSON Schema,
                     [Execute] [Reject] buttons
  ⏳ Run progress card: run_id, status, progress bar, [Cancel]
─────────────────────────────────────────────────────────────────────
[auto-growing text input]                         [Send] [Stop]
```

### Plan Card

Generated from the tool's `params` JSON Schema via a schema-to-form helper (`plan-card.js`). Supported JSON Schema features in Phase 1: `type: string | number | integer | boolean | array`, `enum`, `minimum`/`maximum`, `default`, `required`, `description`, `items`. Nested objects render as inline sub-forms.

After approval, the card collapses into a **Run progress card** that subscribes to `run-progress` events for that `run_id`. On completion (`run-completed` event), becomes a **Run result card** with a "View details" button that navigates to the traditional view's result page for that module.

### Streaming Event Handling

The frontend subscribes to `chat-stream` once per open session:
- `Text`: append to current assistant bubble; Markdown re-render throttled to `requestAnimationFrame` (avoid re-parsing on every token).
- `ToolCall`: insert card DOM element with `call_id` as key.
- `ToolResult`: update matching card to completed state.
- `Done`: stop "thinking" indicator, mark message finalized.
- `Error`: append red text with `[Retry]` button.

The browser-mode Tauri shim in `index.html` gains mock handlers for `chat_*` commands so `python3 -m http.server` preview still works without a backend (returns canned streaming text).

### New Frontend Files

Following the frontend-modularization plan (spec `2026-04-19-frontend-modularization-design.md`). If that migration has not yet landed when AI work starts, AI work proceeds against the existing monolithic `app.js` and integrates by exporting functions through the existing `window.*` shims, then migrates with the modularization effort. Either ordering works.
```
frontend/js/
├── views/
│   └── chat.js
├── modules/
│   └── chat/
│       ├── message-stream.js
│       ├── plan-card.js
│       ├── run-card.js
│       ├── session-list.js
│       └── schema-form.js
└── api/
    └── chat.js        ← wraps chat_* Tauri commands
```

### Project Creation Wizard

The current "New project" form gains a radio group:
```
Default view when opening this project:
  ( ) AI analysis mode    — recommended for new users; drive analyses by chat
  ( ) Manual pipeline     — recommended if you have a defined SOP
```
Persisted as `project.json.default_view`. Switchable later from project settings. Label text stresses that both views remain available.

### Settings Page — AI Provider Section

- Provider dropdown (Phase 1: only "OpenAI-compatible")
- Base URL (text input, default `https://api.openai.com/v1`)
- Model (text input with suggested values)
- Temperature (slider 0.0–2.0, default 0.2)
- API Key (password input + "Test connection" button, shows ✓/✗ without revealing key)
- State indicator: `✓ Key saved` or `Key not configured`

## Error Handling

| Error source | Handling | UI behavior |
|---|---|---|
| Provider 4xx (invalid key, no quota, unknown model) | Immediate structured error | Red message + "Open settings" link |
| Provider 5xx / network timeout before any stream event | Exponential backoff, 3 retries | "Service unavailable, [Retry]" |
| Network drop mid-stream (after events already received) | No retry — partial assistant message saved as `interrupted`, user clicks [Retry] to re-send | "Connection lost mid-response, [Retry]" |
| Tool args schema mismatch | Feed error as `tool_result`, LLM self-corrects | Collapsed "tool failed, AI notified" card |
| Module run failure | Feed failure as `tool_result` | Same as above; LLM explains |
| Orchestrator panic | Log + close stream + preserve persisted messages | Red "Internal error, [View logs]" |
| User stops stream | Mark message `interrupted: true` | Subtle "(stopped by user)" suffix |

**Principle**: recoverable errors (schema / transient tool failure) feed back to the LLM without terminating the session. Only infrastructure-level errors (auth, network, panic) interrupt streaming.

## Cancellation Matrix

| Action | Effect |
|---|---|
| "Stop" button on chat input | Cancel provider HTTP stream. Already-launched runs keep running. Session preserved. |
| "Reject" on plan card | `tool_result = {error: "rejected_by_user"}` feeds back. |
| "Cancel" on run progress card | `Runner.cancel(run_id)` → status becomes `Cancelled` → tool_result reports cancellation. |
| Close AI view tab | Nothing stops. Re-opening resumes live state. |
| Delete session | Remove files. Does NOT cancel runs that session started. |
| Quit app | Tokio runtime drops; in-memory streams and runs terminate. Persisted data retained. |

## Testing Strategy

**`rb-ai` unit tests** (primary coverage):
- `tools/schema.rs`: at test time, validate that every builtin and every module-derived tool exposes a well-formed JSON Schema (draft-07) using `jsonschema` crate.
- `provider/openai_compat.rs`: `wiremock` crate mocks the OpenAI SSE endpoint. Cases: simple text, tool call, tool-call-then-final, network error, malformed SSE frame, mid-stream disconnect, cancellation mid-stream.
- `session/store.rs`: append + atomic write, corrupted file recovery, index rebuild.
- `orchestrator/`: `MockProvider` trait impl drives scripted responses. Cases: Read-risk auto-execute, Run-risk plan-card-approve with edited args, Run-risk reject, tool_result feedback loop, cancel at each stage.
- `config/keyring.rs`: in-memory keyring for test; encrypted-file fallback roundtrip.

**`rb-app` integration tests**:
- Bootstrap `AppState` with a real `ModuleRegistry` and `MockProvider`; drive `chat_send_message → approve → run_qc` end-to-end using a temp project directory. Verify `run-progress` events fire, session is persisted, and run appears in `runs/`.

**Frontend**:
- Phase 1 relies on manual regression (consistent with current frontend strategy).
- Optional vitest suite for `schema-form.js` (most error-prone: JSON Schema → form rendering).

**No real API calls** in CI. All provider tests use `wiremock`.

## Phased Roadmap

### Phase 1 (B) — this design

Ships:
- Complete `rb-ai` crate per above layout.
- `Module` trait extended; 6 existing modules implement `params_schema()` and `ai_hint()`.
- 14 new Tauri commands, 2 new Tauri events.
- Frontend `#chat` view, session list, plan card, run card, streaming.
- Project creation wizard extended with `default_view`.
- Settings page AI Provider section with keyring-backed key storage.
- i18n (zh/en system prompts + UI strings).
- Test suite using `wiremock` + `MockProvider`.

Capability: user describes a task → AI picks a module + args → plan card → approve → run launches → AI reports `run_id` → user follows up to check status.

Rough size: `rb-ai` ~2500 LOC Rust; `rb-app` extensions ~600 LOC; frontend ~1500 LOC JS/CSS; docs + i18n ~300 lines.

### Phase 2 (C) — multi-step pipelines

Adds:
- Orchestrator accepts parallel / sequential tool calls in one assistant turn. (Phase 1's "feed tool_result, re-invoke provider" loop already accommodates this; only UX gating changes.)
- Batch plan card: one card showing multiple proposed steps with individual checkboxes.
- System prompt extended with planning guidance + few-shot pipeline examples.
- Inter-run dependencies via `from_run: run_id` argument passing (requires `get_run_result` to return enough metadata for the LLM to chain).

Phase 1 reservations that enable this:
- ✅ Neutral tool schema, not provider-locked.
- ✅ `call_id` on plan cards — batch is just multiple cards.
- ✅ Every run has a stable `run_id`.
- ✅ Orchestrator is already a tool-call loop.

### Phase 3 (D) — full analysis agent

Adds:
- Implement stub tools: `read_results_table` (polars-backed, projection/filter), `summarize_run`, `generate_plot` (ECharts JSON output rendered in plan card).
- Session-level provider override (some sessions use GPT-4o, others DeepSeek).
- Session summarization for long conversations.
- Possibly: "Copilot drawer" in traditional views (optional extension).

Phase 1 reservations that enable this:
- ✅ Stub tool schemas are registered; implementation replaces `Unimplemented`.
- ✅ Session schema has `summary` field.
- ✅ Session has `provider_snapshot` field.

## Open Questions

None at design-approval time. All decisions logged in brainstorming transcript:

1. Phase target: start B, migrate to D — multi-provider abstraction from day one.
2. Mode coexistence: both views in every project; `default_view` is a soft preference.
3. Safety: tiered risk model (Read auto / Run plan-card / Destructive not registered).
4. Orchestrator in Rust (API keys never leave the process; zero IPC for tool dispatch).
5. Module-derived tools via new `params_schema()` + `ai_hint()` methods on `Module` trait.
6. Plan card is editable before approval.
7. Long-running tools return `run_id` immediately; no blocking.
8. Sessions persisted at `project/chats/`; schema allows Phase 3 summary extension.
9. API keys via `keyring`, Linux fallback to AES-GCM encrypted file.
10. Error philosophy: recoverable errors feed back to LLM, only infra errors interrupt.
11. Tests use `wiremock` + `MockProvider` — no real API calls in CI.

## Appendix A — Known Schema Fragments Reserved for Phase 2/3

Reserved in Phase 1 data model (already accepted by serde with `default`):
- `ChatSession.summary: Option<String>` — Phase 3 summarization target.
- `ChatSession.provider_snapshot: ProviderSnapshot` — Phase 3 per-session provider.
- Tool registry includes `read_results_table`, `summarize_run`, `generate_plot` as `Unimplemented` — Phase 3 fills these.

## Appendix B — i18n Strings Scope

Phase 1 adds (both zh + en):
- Chat view chrome: new session, delete, rename, stop, retry, placeholder text.
- Plan card: execute, reject, edit args, field labels derived from schema `description`.
- Run card: status labels, cancel, view details.
- Settings AI section: all field labels, connection test states.
- System prompts: two separate files `system_zh.md` and `system_en.md`.
- Module `ai_hint()` returns localized text per language arg.

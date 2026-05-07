# Changelog

## v0.11.0 — 2026-05-07

### New

- New crate `rb-ai-bio` bridges the generic `rb-ai` agent framework with
  `rb-core`'s rust_brain workflow model. Houses the bio-specific tools
  (`project_state` family + `run_<module>` wrapper). Hosts register them
  alongside `rb_ai::tools::builtin::register_all`.

### Refactor

- `rb-ai` is now fully decoupled from `rb-core` — `cargo tree -p rb-ai`
  shows zero `rb-*` workspace deps, only crates.io packages. The crate is
  publishable as a standalone agent framework reusable by other projects.
- `ToolContext` / `ExecCtx` / `RunSessionCtx` no longer carry
  `project / runner / binary_resolver`. Tools that need bio-specific state
  carry their own `Arc<...>` set at registration time. The agent loop's
  system prompt accepts a host-pre-computed `system_context: String`.
- `rb_core::cancel::CancellationToken` and `rb_core::subprocess::harden_for_gui`
  references throughout `rb-ai` swapped for `tokio_util::sync::CancellationToken`
  and a small inlined `crate::subprocess::harden_for_gui`.

### Migration (other Rust crates depending on rb-ai)

- `rb_ai::tools::module_derived::*` → `rb_ai_bio::module_derived::*`
- `rb_ai::tools::builtin::project_state::*` → `rb_ai_bio::project_state::*`
- `RunSessionCtx { project, runner, binary_resolver, .. }` → drop those
  three fields; provide `system_context: String` instead (host computes
  the project summary it wants to feed the system prompt).

## v0.3.0 — 2026-05-06

### BREAKING

- Replaces single-turn `chat_*` commands and `#chat` view with the self-evolving
  agent: `agent_*` commands and the new `#agent` view. The `#chat` route alias
  redirects to `#agent` and will be removed after 2026-06-05.
- Old `<project>/chats/` session data is deprecated and unused; you may delete
  the directory at your own discretion.
- `rb-ai` public surface rewritten: `session/`, `orchestrator/` removed;
  `memory/`, `sandbox/`, `agent_loop/` added. `RiskLevel::Run` split into
  `RunLow` / `RunMid`.

### New

- Self-evolving agent core (`rb-ai`): perceive → reason → execute → record loop
  with layered memory (L0–L4), sandboxed tool execution, BM25 + flash-LLM
  memory recall, pixi-driven `code_run`, and crystallization of finished
  sessions into reusable skills.
- 10 new Tauri commands (`agent_start_session`, `agent_send`, `agent_approve`,
  `agent_reject`, `agent_answer`, `agent_cancel`, `agent_set_full_permission`,
  `agent_list_archives`, `agent_load_archive`, `agent_list_skills`,
  `agent_edit_memory`).
- Frontend `#agent` view: three-column research interface with archives list,
  conversation flow, working checkpoint, sandbox tree, network log, and
  approval cards.
- Per-project agent isolation: only one active session per project; concurrent
  starts return an error.
- Full permission mode for trusted users (toggleable from the toolbar).

### Migration

No automatic migration. Re-open each project in v0.3.0 and run a research
session via `#agent`; old chat history will not be carried over.

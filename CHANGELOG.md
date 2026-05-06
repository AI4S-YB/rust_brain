# Changelog

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

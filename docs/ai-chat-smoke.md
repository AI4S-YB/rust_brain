# AI Chat Mode — Smoke Test Checklist

Manual verification pass for Phase 1. Run against a real OpenAI-compatible endpoint before tagging a release.

**Build info** (fill when running):
- Date:
- Git HEAD: `git rev-parse --short HEAD`
- Provider: OpenAI-compatible (`base_url`, `model`)

## Setup
- [ ] `cargo tauri dev` launches without runtime errors.
- [ ] Settings → AI Provider: saved base_url, model, temperature persist across restart.
- [ ] Settings → AI Provider: valid API key → status shows `✓ Key saved`.
- [ ] `ai_backend_info` reports `keystore_backend` = `keyring` (or `encrypted-file` on headless Linux).

## New project flow
- [ ] New-project wizard shows both `default_view` radios with the expected hint text.
- [ ] Creating with **AI analysis mode** lands on `#chat` with an empty session list.
- [ ] Creating with **Manual pipeline** lands on the dashboard (legacy behavior).

## Chat golden path (single-module)
- [ ] `+ New chat` creates a session, visible in `#chat` list.
- [ ] Send `run QC on data/*.fastq.gz` — assistant streams text progressively (no wall-clock pauses).
- [ ] A **Plan Card** appears for `run_qc` with the `input_files` field editable.
- [ ] Editing the FASTQ list and clicking **Execute** spawns a run; card collapses; a **Run Card** appears with live progress.
- [ ] Run completes → `[View details]` navigates to the traditional QC result view showing the same run.
- [ ] Switching to traditional `#qc` shows the same run in the module's run history.

## Edge cases
- [ ] **Reject plan card** — assistant explains and continues (no crash).
- [ ] **Cancel run card** — Runner marks the run Cancelled; status reflected in run card and in traditional view.
- [ ] **Stop streaming (chat_cancel_turn)** — last assistant bubble marked with a subdued "(stopped by user)" style.
- [ ] **Wrong API key** — 401 surfaces as an error bubble with a "Settings" link; no session corruption.
- [ ] **Delete session** — removed from `#chat` list and `index.json`; active runs continue unaffected.
- [ ] **Restart app** — `#chat` still lists all prior sessions; opening one restores full message history.

## i18n
- [ ] Switch language → sidebar AI Copilot label, settings panel, plan card text all localized.
- [ ] `RUSTBRAIN_LANG=zh cargo tauri dev` — system prompt is the Chinese version (visible indirectly via tone/phrasing of AI responses).

## Multi-step flow (partial — full in Phase 2)
- [ ] Describe full RNA-seq pipeline in one message — AI proposes per-step plan cards one at a time.

## Known limitations observed
- [ ] (record anything surprising)

---

Reference: `docs/superpowers/specs/2026-04-19-ai-chat-mode-design.md`, `docs/superpowers/plans/2026-04-19-ai-chat-mode.md`.

# Self-Evolving Agent — Plan 2：rb-app + Frontend `#agent` 视图

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Plan 1 落地的 rb-ai 之上接通 Tauri 命令层 + frontend `#agent` 视图，让用户在桌面端能真正驱动一次自进化研究 session。

**Architecture:** rb-app 侧新增 `AgentRuntime`（项目级 handle map + 全局 MemoryStore + provider 工厂），暴露 10 个 `agent_*` Tauri 命令；run_session 派发到 tokio task，agent_loop 输出的 `AgentEvent` mpsc 流由专用 forwarder task 翻译成 Tauri 事件。frontend 用 vanilla JS 三栏布局：左 archive 列表、中对话流、右 working checkpoint + sandbox + net log；删除现有 `frontend/js/modules/chat/`，路由 `#chat` 30 天 alias 重定向。

**Tech Stack:** Tauri v2, tokio, serde_json, rb-ai (Plan 1 产物), 现有 vanilla HTML/CSS/JS frontend (无构建步)。

**对应 Spec：** `docs/superpowers/specs/2026-05-06-self-evolving-agent-design.md` §UI + §Tauri 命令 + §并发
**前置：** Plan 1（commit `6ab8eeb` on `feat/self-evolving-agent`）已落地

---

## 文件布局

**新建（rb-app 侧）**

```
crates/rb-app/src/
├── agent_runtime.rs          AgentRuntime + AgentHandle + 工厂方法
└── commands/
    └── agent.rs              10 个 agent_* Tauri 命令 + event forwarder
```

**新建（frontend 侧）**

```
frontend/js/modules/agent/
├── view.js                   入口：渲染三栏 + 装事件监听
├── state.js                  per-session 状态（messages, todo, recalled, ...）
├── left-pane.js              archive 列表 + 新研究按钮
├── middle-pane.js            对话流（user/assistant/tool_call/tool_result）
├── right-pane.js             checkpoint + sandbox tree + net log tail
├── approval-card.js          风险确认弹卡（含 always-allow 勾选）
├── full-permission.js        toolbar full permission toggle
└── api.js                    invoke 包装 + 事件订阅
```

**修改（rb-app 侧）**

```
crates/rb-app/src/ai_state.rs            +AgentRuntime 字段（或独立 state）
crates/rb-app/src/commands/mod.rs        +pub mod agent
crates/rb-app/src/main.rs                +agent_* 命令注册 + AgentRuntime 初始化
crates/rb-app/Cargo.toml                 (无新依赖，rb-ai 已含全部需要)
```

**修改（frontend 侧）**

```
frontend/index.html                      sidebar nav: chat → agent
frontend/js/core/router.js               navigate('chat/...') 兼容 alias + 加 'agent' 分支
frontend/js/core/constants.js            KNOWN_VIEWS 加 agent
frontend/js/main.js                      首次启动时也走 agent 路径
frontend/css/...                         agent 三栏样式（沿用现 ECHART_THEME 配色）
```

**删除**

```
frontend/js/modules/chat/                整目录（chat-view, message-stream, plan-card, run-card, schema-form, session-list）
```

**版本与 CHANGELOG**

```
crates/{rb-core,rb-ai,rb-app,...}/Cargo.toml  version → "0.3.0"
CHANGELOG.md                                  +v0.3.0 entry（如不存在，本 plan 创建）
```

---

## 总体数据流（实现要点）

```
Frontend                          rb-app                                rb-ai
─────────                         ─────                                 ─────
invoke('agent_start_session')  →  AgentRuntime.start(project_id)
                                   - 建 SharedSession (空)
                                   - 建 SandboxPolicy
                                   - 建 ToolRegistry (builtin + module + skill)
                                   - 存 AgentHandle
                                  ←  {session_id}

invoke('agent_send', text)     →  AgentRuntime.send(session_id, text)
                                   - 取 handle
                                   - 派 forwarder task 监听 event_rx → emit('agent-stream')
                                   - 派 ask_user forwarder → emit('agent-ask-user')
                                   - tokio::spawn run_session(...)              →  perceive→reason→execute→record
                                  ←  ()                                              ↓
                                                                                  AgentEvent → mpsc
                                                                                  AskUserRequest → mpsc

frontend listens('agent-stream') ← emit (per AgentEvent)
frontend listens('agent-ask-user')← emit (when run_session 调 ask_user)

invoke('agent_approve', call_id)→ approval_tx.send((call_id, Approve))     →  execute 解锁继续

invoke('agent_cancel')         →  handle.cancel.cancel()                     →  run_session 退出 + Cancelled archive
```

---

## Phase A — rb-app 后端

> 6 个 task：runtime 类型 → 10 个命令拆 4 组分别落地。

### Task 1：AgentRuntime + AgentHandle 类型骨架

**Files:**
- Create: `crates/rb-app/src/agent_runtime.rs`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: 写失败测试（单元测）**

新建 `crates/rb-app/src/agent_runtime.rs`：

```rust
//! Per-project AgentRuntime: holds the global MemoryStore + a map of
//! per-project AgentHandles. One AgentHandle = one in-flight or paused
//! agent session. The runtime is shared via Tauri State.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rb_ai::agent_loop::{AgentSession, ApprovalVerdict, SharedSession};
use rb_ai::memory::MemoryStore;
use rb_ai::sandbox::SandboxPolicy;
use rb_ai::AiError;
use rb_core::cancel::CancellationToken;
use tokio::sync::{mpsc, Mutex};

/// One active agent session. The map key is `project_root.display().to_string()`.
pub struct AgentHandle {
    pub session_id: String,
    pub session: SharedSession,
    pub policy: Arc<SandboxPolicy>,
    pub cancel: CancellationToken,
    /// Sender into the approval channel that execute_call awaits.
    pub approval_tx: mpsc::Sender<(String, ApprovalVerdict)>,
    /// Sender into the ask_user channel that AskUserExec writes into. The
    /// receiver is owned by a forwarder task that emits "agent-ask-user".
    pub ask_user_tx: mpsc::Sender<rb_ai::tools::AskUserRequest>,
    /// `Some` while a run_session task is active; `None` when the session
    /// is paused awaiting the next user turn.
    pub run_join: Mutex<Option<tokio::task::JoinHandle<Result<(), AiError>>>>,
}

pub struct AgentRuntime {
    pub memory: Arc<MemoryStore>,
    /// Per-project handles. Project root → handle.
    pub active: Mutex<HashMap<String, Arc<AgentHandle>>>,
}

impl AgentRuntime {
    pub fn new() -> Result<Self, AiError> {
        Ok(Self {
            memory: Arc::new(MemoryStore::open_default()?),
            active: Mutex::new(HashMap::new()),
        })
    }

    /// Test-only constructor that lets us point the global root at a tempdir.
    #[doc(hidden)]
    pub fn with_memory_root(root: PathBuf) -> Result<Self, AiError> {
        Ok(Self {
            memory: Arc::new(MemoryStore::open(root)?),
            active: Mutex::new(HashMap::new()),
        })
    }

    pub async fn handle_for(&self, project_root: &str) -> Option<Arc<AgentHandle>> {
        self.active.lock().await.get(project_root).cloned()
    }

    pub async fn insert(&self, project_root: String, handle: Arc<AgentHandle>) {
        self.active.lock().await.insert(project_root, handle);
    }

    pub async fn remove(&self, project_root: &str) -> Option<Arc<AgentHandle>> {
        self.active.lock().await.remove(project_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn runtime_insert_and_lookup() {
        let tmp = tempdir().unwrap();
        let rt = AgentRuntime::with_memory_root(tmp.path().join("global")).unwrap();
        assert!(rt.handle_for("p").await.is_none());

        let session = Arc::new(tokio::sync::Mutex::new(AgentSession::new("p".into())));
        let policy = Arc::new(SandboxPolicy::new(tmp.path().to_path_buf(), "sandbox"));
        let cancel = CancellationToken::new();
        let (appr_tx, _) = mpsc::channel(1);
        let (ask_tx, _) = mpsc::channel(1);
        let h = Arc::new(AgentHandle {
            session_id: "s1".into(),
            session,
            policy,
            cancel,
            approval_tx: appr_tx,
            ask_user_tx: ask_tx,
            run_join: Mutex::new(None),
        });
        rt.insert("p".into(), h.clone()).await;
        let got = rt.handle_for("p").await.unwrap();
        assert_eq!(got.session_id, "s1");
        rt.remove("p").await;
        assert!(rt.handle_for("p").await.is_none());
    }
}
```

`crates/rb-app/src/main.rs` 顶部加 `mod agent_runtime;`。**不要**在 generate_handler! 加任何东西 —— 命令在 Task 2-7 逐个加。

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-app --lib agent_runtime
```

预期：1 PASS。

- [ ] **Step 3: 全工作区编译**

```
cargo check --workspace
```

预期：通过。

- [ ] **Step 4: 提交**

```bash
git add crates/rb-app/src/agent_runtime.rs crates/rb-app/src/main.rs
git commit -m "$(cat <<'EOF'
feat(app): AgentRuntime + AgentHandle skeleton (no commands yet)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2：commands/agent.rs 骨架 + agent_start_session

**Files:**
- Create: `crates/rb-app/src/commands/agent.rs`
- Modify: `crates/rb-app/src/commands/mod.rs`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: 实现命令 + AppState 接线**

新建 `crates/rb-app/src/commands/agent.rs`：

```rust
//! Tauri commands for the self-evolving agent.
//!
//! Pattern: each command takes the AppState (which embeds AgentRuntime),
//! does its work async, returns Result<T, String> (Tauri serializes errors
//! as strings). Heavy lifting (running provider, executing tools) happens
//! in a background tokio task spawned by agent_send; commands themselves
//! return quickly.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rb_ai::agent_loop::{AgentSession, ApprovalVerdict};
use rb_ai::sandbox::SandboxPolicy;
use rb_ai::tools::{builtin, ToolRegistry};
use rb_core::cancel::CancellationToken;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::{mpsc, Mutex};

use crate::agent_runtime::{AgentHandle, AgentRuntime};

#[derive(Debug, Serialize)]
pub struct StartSessionResp {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct StartSessionArgs {
    pub project_root: String,
    pub full_permission: Option<bool>,
}

#[tauri::command]
pub async fn agent_start_session(
    args: StartSessionArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<StartSessionResp, String> {
    // Reject if a session is already active for this project.
    if runtime.handle_for(&args.project_root).await.is_some() {
        return Err(format!(
            "agent already running for project {}",
            args.project_root
        ));
    }
    let project_root = PathBuf::from(&args.project_root);
    runtime
        .memory
        .ensure_project(&project_root)
        .map_err(|e| e.to_string())?;
    let session = Arc::new(Mutex::new(AgentSession::new(args.project_root.clone())));
    let session_id = session.lock().await.id.clone();
    let mut policy = SandboxPolicy::new(project_root.clone(), "sandbox");
    if args.full_permission.unwrap_or(false) {
        policy = policy.full_permission();
    }
    let policy = Arc::new(policy);
    let (approval_tx, _approval_rx_unused) = mpsc::channel::<(String, ApprovalVerdict)>(8);
    let (ask_user_tx, _ask_user_rx_unused) = mpsc::channel::<rb_ai::tools::AskUserRequest>(8);
    // The unused-rx are dropped here on purpose. agent_send replaces them
    // before each turn so the receivers are bound to the run task that
    // owns them.
    let handle = Arc::new(AgentHandle {
        session_id: session_id.clone(),
        session,
        policy,
        cancel: CancellationToken::new(),
        approval_tx,
        ask_user_tx,
        run_join: Mutex::new(None),
    });
    runtime.insert(args.project_root, handle).await;
    Ok(StartSessionResp { session_id })
}

/// Build a per-session ToolRegistry (builtin + module-derived + skill loader).
/// Module-derived tools come from the AppState ModuleRegistry; skill loader
/// reads global L3 + project L3-local at startup.
pub(crate) fn build_registry(
    modules: &[Arc<dyn rb_core::module::Module>],
    lang: &str,
    memory_global: &std::path::Path,
    project_root: &std::path::Path,
) -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    builtin::register_all(&mut reg);
    rb_ai::tools::module_derived::register_modules(&mut reg, modules, lang);
    let _ = rb_ai::tools::skill::register_dir(&mut reg, &memory_global.join("L3_skills"));
    let _ = rb_ai::tools::skill::register_dir(&mut reg, &project_root.join("agent/L3_local"));
    reg
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn build_registry_includes_builtin_and_skill_dirs() {
        let tmp = tempdir().unwrap();
        let global = tmp.path().join("global");
        std::fs::create_dir_all(global.join("L3_skills")).unwrap();
        std::fs::write(
            global.join("L3_skills/rna-seq-de.md"),
            "---\nname: rna-seq-de\ndescription: x\n---\nbody",
        )
        .unwrap();
        let proot = tmp.path().join("proj");
        std::fs::create_dir_all(proot.join("agent/L3_local")).unwrap();
        let reg = build_registry(&[], "en", &global, &proot);
        assert!(reg.get("file_read").is_some());
        assert!(reg.get("skill_rna_seq_de").is_some());
    }
}
```

更新 `crates/rb-app/src/commands/mod.rs`：

```rust
pub mod agent;
pub mod ai_provider;
pub mod assets;
pub mod files;
pub mod inputs;
pub mod modules;
pub mod plugins;
pub mod project;
pub mod samples;
pub mod settings;
```

> **注意**：此处 `rb_ai::tools::module_derived::register_modules` 是现有 API；不存在则按现有签名调整（搜 `module_derived` 看实际签名）。如果签名是 `register(reg, registry: &ModuleRegistry, lang)`，相应调整。

更新 `crates/rb-app/src/main.rs`：在 `tauri::Builder::default()` 链上靠近 `.manage(...)` 处加：

```rust
let agent_runtime = std::sync::Arc::new(
    crate::agent_runtime::AgentRuntime::new()
        .map_err(|e| anyhow::anyhow!("init AgentRuntime: {e}"))?,
);
```

并在后续 `.manage(agent_runtime.clone())` 注册（具体位置看 main.rs 现有 manage 调用模式）。在 `tauri::generate_handler!` 加：

```rust
commands::agent::agent_start_session,
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-app --lib commands::agent
cargo test -p rb-app --lib agent_runtime
cargo check --workspace
```

预期：全 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-app/src
git commit -m "$(cat <<'EOF'
feat(app): agent_start_session command + ToolRegistry builder

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3：agent_send 命令 + event forwarder

**Files:**
- Modify: `crates/rb-app/src/commands/agent.rs`
- Modify: `crates/rb-app/src/main.rs`

> 这是 Plan 2 最复杂的命令——把 run_session 跑起来 + 两个 forwarder task（event sink → Tauri emit；ask_user → Tauri emit）。

- [ ] **Step 1: 实现 agent_send**

在 `crates/rb-app/src/commands/agent.rs` 末尾追加：

```rust
use rb_ai::agent_loop::{run_session, AgentEvent, RunConfig, RunSessionCtx};
use rb_ai::memory::Bm25Recaller;
use rb_ai::sandbox::NetLogger;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Deserialize)]
pub struct SendArgs {
    pub project_root: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct AskUserOutbound {
    pub session_id: String,
    pub call_id: String,
    pub prompt: String,
}

#[tauri::command]
pub async fn agent_send(
    args: SendArgs,
    app: AppHandle,
    runtime: State<'_, Arc<AgentRuntime>>,
    ai_state: State<'_, Arc<crate::ai_state::AiState>>,
    module_state: State<'_, Arc<crate::AppState>>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or_else(|| format!("no agent session for project {}", args.project_root))?;

    // Refuse a second concurrent run_session for the same handle.
    {
        let mut join_slot = handle.run_join.lock().await;
        if let Some(j) = join_slot.take() {
            if !j.is_finished() {
                *join_slot = Some(j);
                return Err("a run is already in flight; cancel it or wait".into());
            }
        }
    }

    let session_id = handle.session_id.clone();
    let project_root_pb = PathBuf::from(&args.project_root);
    let memory = runtime.memory.clone();

    // Build the registry once per send.
    let modules: Vec<Arc<dyn rb_core::module::Module>> =
        module_state.modules.list_all().into_iter().collect();
    let lang = ai_state.config.lock().await.lang.clone().unwrap_or_else(|| "en".into());
    let registry = Arc::new(build_registry(
        &modules,
        &lang,
        &memory.global_root,
        &project_root_pb,
    ));

    // Build provider from AiConfig (best-effort; surface error to frontend).
    let provider = build_provider(&ai_state)
        .await
        .map_err(|e| format!("provider init: {e}"))?;

    // Channels.
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(64);
    let (approval_tx, approval_rx) = mpsc::channel::<(String, ApprovalVerdict)>(8);
    let (ask_user_tx, mut ask_user_rx) = mpsc::channel::<rb_ai::tools::AskUserRequest>(8);
    let approval_rx = Arc::new(Mutex::new(approval_rx));

    // Replace the handle's tx ends so agent_approve / ask_user respond from the right channel.
    // Safety: we hold &Arc<AgentHandle>; the field is `mpsc::Sender` which is Clone but
    // we replace via interior mutability — the field must be wrapped. The simplest
    // working shape is to keep handle.approval_tx / ask_user_tx behind their own
    // Mutex<Sender<...>>. Update the AgentHandle definition (Task 1) accordingly.
    {
        let mut atx = handle.approval_tx_slot.lock().await;
        *atx = approval_tx.clone();
        let mut utx = handle.ask_user_tx_slot.lock().await;
        *utx = ask_user_tx.clone();
    }

    // Forwarder: AgentEvent → Tauri emit("agent-stream").
    let app_for_events = app.clone();
    let forwarder_events = tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            let _ = app_for_events.emit("agent-stream", &ev);
        }
    });

    // Forwarder: AskUserRequest → Tauri emit("agent-ask-user"), and parallel
    // path that wires the responder back to a per-call channel keyed by call_id
    // so agent_answer (Task 5) can find it.
    let app_for_ask = app.clone();
    let session_id_for_ask = session_id.clone();
    let pending_asks: Arc<tokio::sync::Mutex<HashMap<String, mpsc::Sender<String>>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let pending_for_handle = pending_asks.clone();
    {
        let mut slot = handle.pending_asks_slot.lock().await;
        *slot = Some(pending_for_handle);
    }
    let pending_for_forwarder = pending_asks.clone();
    let forwarder_ask = tokio::spawn(async move {
        while let Some(req) = ask_user_rx.recv().await {
            pending_for_forwarder
                .lock()
                .await
                .insert(req.call_id.clone(), req.responder);
            let _ = app_for_ask.emit(
                "agent-ask-user",
                &AskUserOutbound {
                    session_id: session_id_for_ask.clone(),
                    call_id: req.call_id,
                    prompt: req.prompt,
                },
            );
        }
    });

    // Build NetLogger (per-session) and Recaller (BM25 baseline).
    let net_log_enabled = !handle.policy.is_full_permission();
    let net_log = Arc::new(
        NetLogger::new(&project_root_pb, &session_id, net_log_enabled)
            .map_err(|e| e.to_string())?,
    );
    let recaller: Arc<dyn rb_ai::memory::Recaller> = Arc::new(Bm25Recaller::new(5));

    // Spawn run_session.
    let ctx = RunSessionCtx {
        project: module_state.runner.project_arc(),
        runner: module_state.runner.clone(),
        binary_resolver: module_state.binary_resolver.clone(),
        registry,
        policy: handle.policy.clone(),
        memory,
        recaller,
        provider,
        net_log,
        project_root: project_root_pb,
        config: RunConfig::default(),
    };
    let session_arc = handle.session.clone();
    let cancel = handle.cancel.clone();
    let join = tokio::spawn(async move {
        let r = run_session(
            ctx,
            args.text,
            session_arc,
            event_tx,
            ask_user_tx,
            approval_rx,
            cancel,
        )
        .await;
        // Drain forwarders by dropping the senders (channels close when the
        // run_session task drops them on exit).
        forwarder_events.abort();
        forwarder_ask.abort();
        r
    });
    *handle.run_join.lock().await = Some(join);
    Ok(())
}

async fn build_provider(
    ai_state: &Arc<crate::ai_state::AiState>,
) -> Result<Arc<dyn rb_ai::provider::ChatProvider>, String> {
    // Resolve the configured default provider via existing ai_provider helpers.
    crate::ai_provider::resolve_chat_provider(ai_state)
        .await
        .map_err(|e| e.to_string())
}
```

> **AgentHandle 接口扩展**：上面用到 `handle.approval_tx_slot`、`handle.ask_user_tx_slot`、`handle.pending_asks_slot`、`handle.policy.is_full_permission()`。Task 1 的 `AgentHandle` 不含这些；这个 step 必须先回 Task 1 文件加：

```rust
pub struct AgentHandle {
    // existing fields ...
    pub approval_tx_slot: Mutex<mpsc::Sender<(String, ApprovalVerdict)>>,
    pub ask_user_tx_slot: Mutex<mpsc::Sender<rb_ai::tools::AskUserRequest>>,
    pub pending_asks_slot:
        Mutex<Option<Arc<tokio::sync::Mutex<HashMap<String, mpsc::Sender<String>>>>>>,
}
```

把 Task 1 旧的 `approval_tx` / `ask_user_tx` 字段改名 `approval_tx_slot` / `ask_user_tx_slot` 并把类型从 `Sender<...>` 改成 `Mutex<Sender<...>>`，让 agent_send 能就地替换。

`SandboxPolicy::is_full_permission` 不存在，加一个 inline accessor：

`crates/rb-ai/src/sandbox/policy.rs` `impl SandboxPolicy` 末尾追加：

```rust
pub fn is_full_permission(&self) -> bool {
    matches!(self.mode, PolicyMode::FullPermission)
}
```

`Runner` 需要暴露 `project_arc()`。`crates/rb-core/src/runner.rs` 中查 `pub fn project(&self)` 之类，若返回 `&Arc<Mutex<Project>>`，新增 `pub fn project_arc(&self) -> Arc<Mutex<Project>>`：

```rust
pub fn project_arc(&self) -> Arc<Mutex<Project>> {
    self.project.clone()
}
```

`AppState` 需要暴露 `binary_resolver: Arc<Mutex<BinaryResolver>>`、`modules.list_all()`：检查 `crates/rb-app/src/lib.rs` 或 `main.rs` 中 AppState 定义；如缺少 `list_all()`，在 ModuleRegistry impl 加返回 `Vec<Arc<dyn Module>>` 的方法。

`crate::ai_provider::resolve_chat_provider` 是 stub 名——`ai_provider.rs` 现有 `effective_thinking`/`resolve_api_key`，添一个公共函数 `resolve_chat_provider(ai_state)` 返回 `Arc<dyn ChatProvider>`：拼合 base_url + model + api_key 走现有 `OpenAiCompat::new`（看现有 provider/openai_compat.rs）。

- [ ] **Step 2: 注册命令**

`crates/rb-app/src/main.rs` 的 `generate_handler!` 加 `commands::agent::agent_send,`。

- [ ] **Step 3: 编译**

```
cargo check --workspace
```

预期：通过（如有报错按上面提示加缺的 method/字段）。

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(app): agent_send command + event/ask-user forwarders + handle slots

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4：agent_approve / agent_reject / agent_answer

**Files:**
- Modify: `crates/rb-app/src/commands/agent.rs`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: 实现三个命令**

在 `crates/rb-app/src/commands/agent.rs` 末尾追加：

```rust
#[derive(Debug, Deserialize)]
pub struct ApproveArgs {
    pub project_root: String,
    pub call_id: String,
    pub edited_args: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn agent_approve(
    args: ApproveArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or("no agent session")?;
    let tx = handle.approval_tx_slot.lock().await.clone();
    tx.send((
        args.call_id,
        ApprovalVerdict::Approve {
            edited_args: args.edited_args,
        },
    ))
    .await
    .map_err(|e| format!("approve send: {e}"))
}

#[derive(Debug, Deserialize)]
pub struct RejectArgs {
    pub project_root: String,
    pub call_id: String,
    pub reason: Option<String>,
}

#[tauri::command]
pub async fn agent_reject(
    args: RejectArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or("no agent session")?;
    let tx = handle.approval_tx_slot.lock().await.clone();
    tx.send((
        args.call_id,
        ApprovalVerdict::Reject {
            reason: args.reason,
        },
    ))
    .await
    .map_err(|e| format!("reject send: {e}"))
}

#[derive(Debug, Deserialize)]
pub struct AnswerArgs {
    pub project_root: String,
    pub call_id: String,
    pub reply: String,
}

#[tauri::command]
pub async fn agent_answer(
    args: AnswerArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or("no agent session")?;
    let pending_lock = handle.pending_asks_slot.lock().await;
    let pending = pending_lock
        .as_ref()
        .ok_or("no in-flight run; ask_user only valid during agent_send")?;
    let mut p = pending.lock().await;
    let tx = p
        .remove(&args.call_id)
        .ok_or_else(|| format!("no pending ask_user with call_id {}", args.call_id))?;
    tx.send(args.reply).await.map_err(|e| format!("answer send: {e}"))
}
```

`crates/rb-app/src/main.rs` `generate_handler!` 加：

```rust
commands::agent::agent_approve,
commands::agent::agent_reject,
commands::agent::agent_answer,
```

- [ ] **Step 2: 编译**

```
cargo check --workspace
```

预期：通过。

- [ ] **Step 3: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(app): agent_approve / agent_reject / agent_answer commands

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5：agent_cancel + agent_set_full_permission

**Files:**
- Modify: `crates/rb-app/src/commands/agent.rs`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: 实现两个命令**

在 `crates/rb-app/src/commands/agent.rs` 末尾追加：

```rust
#[derive(Debug, Deserialize)]
pub struct CancelArgs {
    pub project_root: String,
}

#[tauri::command]
pub async fn agent_cancel(
    args: CancelArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or("no agent session")?;
    handle.cancel.cancel();
    // Wait briefly for run_session to wind down; abort if it hangs.
    let mut slot = handle.run_join.lock().await;
    if let Some(j) = slot.take() {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), j).await;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct FullPermArgs {
    pub project_root: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn agent_set_full_permission(
    args: FullPermArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<(), String> {
    let handle = runtime
        .handle_for(&args.project_root)
        .await
        .ok_or("no agent session")?;
    handle.policy.set_full_permission(args.enabled);
    Ok(())
}
```

> **SandboxPolicy 接口扩展**：`policy.set_full_permission(bool)` 不存在。`policy.mode` 是直接字段；改成 `Mutex<PolicyMode>` 太破坏；最少侵入是把 `mode` 改成 `AtomicU8` 或 `Mutex<PolicyMode>`。最简单：mode 改成 `std::sync::Mutex<PolicyMode>`，加 setter 与 getter。

`crates/rb-ai/src/sandbox/policy.rs` 改 `pub mode: PolicyMode` → `pub mode: std::sync::Mutex<PolicyMode>`，相应 `should_run`、`is_full_permission`、`full_permission()` 内部改读写 mutex。新增：

```rust
pub fn set_full_permission(&self, enabled: bool) {
    let mut m = self.mode.lock().unwrap();
    *m = if enabled { PolicyMode::FullPermission } else { PolicyMode::Normal };
}
```

把现有 `is_full_permission` / `should_run` 内部都通过 lock 读：

```rust
pub fn is_full_permission(&self) -> bool {
    matches!(*self.mode.lock().unwrap(), PolicyMode::FullPermission)
}

pub fn should_run(&self, bucket: &Bucket, decision: &Decision) -> bool {
    match *self.mode.lock().unwrap() {
        PolicyMode::FullPermission => true,
        PolicyMode::Normal => match decision {
            Decision::Allow => true,
            Decision::ApproveOnce => self.approved.lock().unwrap().contains(bucket),
            Decision::AlwaysAsk => false,
        },
    }
}
```

构造函数对应改：

```rust
mode: std::sync::Mutex::new(PolicyMode::Normal),
```

`pub fn full_permission(self) -> Self` 改为：

```rust
pub fn full_permission(self) -> Self {
    *self.mode.lock().unwrap() = PolicyMode::FullPermission;
    self
}
```

`crates/rb-app/src/main.rs` `generate_handler!` 加：

```rust
commands::agent::agent_cancel,
commands::agent::agent_set_full_permission,
```

- [ ] **Step 2: 跑全部测试确认 SandboxPolicy 改造无回归**

```
cargo test -p rb-ai --lib sandbox::policy
cargo test -p rb-ai --test agent_session_e2e
cargo test --workspace
```

预期：全 PASS（policy 改 Mutex 后 8 个旧 sandbox::policy 测试 + 3 个 e2e 仍通过）。

- [ ] **Step 3: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(app): agent_cancel + agent_set_full_permission

SandboxPolicy.mode now lives behind a Mutex so mode flips at runtime.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6：agent_list_archives / agent_load_archive / agent_list_skills / agent_edit_memory

**Files:**
- Modify: `crates/rb-app/src/commands/agent.rs`
- Modify: `crates/rb-app/src/main.rs`

- [ ] **Step 1: 实现四个命令**

在 `crates/rb-app/src/commands/agent.rs` 末尾追加：

```rust
#[derive(Debug, Serialize)]
pub struct ArchiveListEntry {
    pub id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub summary: String,
    pub outcome: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListArchivesArgs {
    pub project_root: String,
}

#[tauri::command]
pub async fn agent_list_archives(
    args: ListArchivesArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<Vec<ArchiveListEntry>, String> {
    let path = rb_ai::memory::MemoryStore::project_root(&PathBuf::from(&args.project_root))
        .join("L4_archives/_index.json");
    let entries = runtime.memory.read_index(&path).map_err(|e| e.to_string())?;
    let mut out = vec![];
    for e in entries {
        if let rb_ai::memory::IndexEntry::Archive {
            id,
            started_at,
            ended_at,
            summary,
            outcome,
            tags,
        } = e
        {
            out.push(ArchiveListEntry {
                id,
                started_at: started_at.to_rfc3339(),
                ended_at: ended_at.map(|d| d.to_rfc3339()),
                summary,
                outcome: format!("{outcome:?}").to_lowercase(),
                tags,
            });
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
pub struct LoadArchiveArgs {
    pub project_root: String,
    pub archive_id: String,
}

#[tauri::command]
pub async fn agent_load_archive(
    args: LoadArchiveArgs,
    _runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<rb_ai::memory::Archive, String> {
    let path = rb_ai::memory::MemoryStore::project_root(&PathBuf::from(&args.project_root))
        .join("L4_archives")
        .join(format!("{}.json", args.archive_id));
    let bytes = std::fs::read(&path).map_err(|e| format!("read archive: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse archive: {e}"))
}

#[derive(Debug, Serialize)]
pub struct SkillsList {
    pub global: Vec<SkillSummary>,
    pub project: Vec<SkillSummary>,
}

#[derive(Debug, Serialize)]
pub struct SkillSummary {
    pub name: String,
    pub path: String,
    pub triggers: Vec<String>,
    pub hits: u64,
}

#[derive(Debug, Deserialize)]
pub struct ListSkillsArgs {
    pub project_root: String,
}

#[tauri::command]
pub async fn agent_list_skills(
    args: ListSkillsArgs,
    runtime: State<'_, Arc<AgentRuntime>>,
) -> Result<SkillsList, String> {
    let global_idx = runtime
        .memory
        .read_index(&runtime.memory.global_root.join("L3_skills/_index.json"))
        .map_err(|e| e.to_string())?;
    let project_idx = runtime
        .memory
        .read_index(
            &rb_ai::memory::MemoryStore::project_root(&PathBuf::from(&args.project_root))
                .join("L3_local/_index.json"),
        )
        .map_err(|e| e.to_string())?;
    fn to_summary(entries: Vec<rb_ai::memory::IndexEntry>) -> Vec<SkillSummary> {
        entries
            .into_iter()
            .filter_map(|e| match e {
                rb_ai::memory::IndexEntry::Skill {
                    name,
                    path,
                    triggers,
                    hits,
                    ..
                } => Some(SkillSummary {
                    name,
                    path,
                    triggers,
                    hits,
                }),
                _ => None,
            })
            .collect()
    }
    Ok(SkillsList {
        global: to_summary(global_idx),
        project: to_summary(project_idx),
    })
}

#[derive(Debug, Deserialize)]
pub struct EditMemoryArgs {
    pub path: String,
    pub content: String,
}

#[tauri::command]
pub async fn agent_edit_memory(args: EditMemoryArgs) -> Result<(), String> {
    // Trust the frontend to pass a path returned by agent_list_skills or
    // resolved via well-known L0/L2 file names. Reject paths that escape
    // either the global memory root or any project's agent dir.
    let path = std::path::Path::new(&args.path);
    let canon = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let global = dirs::data_local_dir()
        .ok_or("no data_local_dir")?
        .join("rust_brain/agent");
    let allowed = canon.starts_with(&global) || canon.to_string_lossy().contains("/agent/");
    if !allowed {
        return Err(format!("refused to write outside agent dirs: {}", canon.display()));
    }
    std::fs::write(&canon, args.content).map_err(|e| e.to_string())
}
```

`crates/rb-app/src/main.rs` `generate_handler!` 加四行。

- [ ] **Step 2: 编译**

```
cargo check --workspace
```

预期：通过。

- [ ] **Step 3: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(app): agent_list_archives / load_archive / list_skills / edit_memory

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase B — Frontend `#agent` 视图

> 6 个 task：删旧 chat → 新建 agent 模块骨架 → 三栏布局 → 中栏对话流 → 右栏面板 → approval 卡。

### Task 7：删除旧 chat 模块

**Files:**
- Delete: `frontend/js/modules/chat/` 整目录

- [ ] **Step 1: 删除文件**

```
git rm -r frontend/js/modules/chat
```

- [ ] **Step 2: 找出所有引用并清理**

```
grep -rn "modules/chat\|chat-view\|chat_session\|message-stream\|plan-card\|run-card\|schema-form\|session-list" frontend/
```

每个命中按下面分类处理：
- `frontend/js/main.js` / `core/router.js` 中对 chat 视图的 import → 删（agent 模块在后续 task 加）。
- `frontend/index.html` 中的 sidebar `data-view="chat"` 项 → 暂留，下一 task 改成 `data-view="agent"`。
- `core/constants.js` 中如有 `chat` view id → 暂留。

- [ ] **Step 3: 浏览器烟雾测**

```bash
cd frontend && python3 -m http.server 8090 &
sleep 2
curl -s http://localhost:8090/ | grep -c '<title'  # 必须 ≥1
kill %1
```

预期：index.html 仍能加载（即便 chat 路由暂时坏掉也无所谓——下一 task 修）。

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
chore(frontend): delete old chat module (replaced by agent in next task)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 8：agent 模块骨架 + 路由

**Files:**
- Create: `frontend/js/modules/agent/view.js`
- Create: `frontend/js/modules/agent/state.js`
- Create: `frontend/js/modules/agent/api.js`
- Modify: `frontend/js/core/router.js`
- Modify: `frontend/js/core/constants.js`
- Modify: `frontend/index.html` (sidebar entry chat → agent)

- [ ] **Step 1: 创建 state/api/view 三件套**

`frontend/js/modules/agent/state.js`:
```js
// Per-session in-memory state. Only ONE active session shown at a time;
// multi-session UX is out of scope for v0.3.
export const agentState = {
  projectRoot: null,
  sessionId: null,
  messages: [],     // {role, content, tool_calls?, ...} normalized for render
  recalled: [],     // RecallCandidate[]
  todo: [],         // TodoEntry[]
  pendingAsks: {},  // call_id -> prompt
  archives: [],     // ArchiveListEntry[]
  skills: { global: [], project: [] },
  fullPermission: false,
};
```

`frontend/js/modules/agent/api.js`:
```js
const { invoke } = window.__TAURI__?.core ?? { invoke: async () => { throw new Error('Tauri not available'); } };
const { listen } = window.__TAURI__?.event ?? { listen: async () => () => {} };

export const agentApi = {
  startSession(projectRoot, fullPermission = false) {
    return invoke('agent_start_session', { args: { projectRoot, fullPermission } });
  },
  send(projectRoot, text)        { return invoke('agent_send', { args: { projectRoot, text } }); },
  approve(projectRoot, callId, editedArgs = null) {
    return invoke('agent_approve', { args: { projectRoot, callId, editedArgs } });
  },
  reject(projectRoot, callId, reason = null) {
    return invoke('agent_reject', { args: { projectRoot, callId, reason } });
  },
  answer(projectRoot, callId, reply) {
    return invoke('agent_answer', { args: { projectRoot, callId, reply } });
  },
  cancel(projectRoot)            { return invoke('agent_cancel', { args: { projectRoot } }); },
  setFullPermission(projectRoot, enabled) {
    return invoke('agent_set_full_permission', { args: { projectRoot, enabled } });
  },
  listArchives(projectRoot)      { return invoke('agent_list_archives', { args: { projectRoot } }); },
  loadArchive(projectRoot, archiveId) {
    return invoke('agent_load_archive', { args: { projectRoot, archiveId } });
  },
  listSkills(projectRoot)        { return invoke('agent_list_skills', { args: { projectRoot } }); },
  editMemory(path, content)      { return invoke('agent_edit_memory', { args: { path, content } }); },
};

export function onAgentStream(handler)   { return listen('agent-stream', e => handler(e.payload)); }
export function onAgentAskUser(handler)  { return listen('agent-ask-user', e => handler(e.payload)); }
```

`frontend/js/modules/agent/view.js`:
```js
import { agentState } from './state.js';
import { agentApi, onAgentStream, onAgentAskUser } from './api.js';
import { state as appState } from '../../core/state.js';

export async function renderAgentView(content) {
  content.innerHTML = `
    <div class="agent-shell">
      <aside class="agent-left" id="agent-left">left pane (archives) — wired in Task 9</aside>
      <section class="agent-mid" id="agent-mid">
        <div class="agent-msgs" id="agent-msgs"></div>
        <form class="agent-input" id="agent-input-form">
          <textarea id="agent-input" placeholder="Ask the agent..." rows="3"></textarea>
          <button type="submit">Send</button>
        </form>
      </section>
      <aside class="agent-right" id="agent-right">right pane — wired in Task 11</aside>
    </div>`;
  const projectRoot = appState.project?.root_dir;
  if (!projectRoot) {
    content.querySelector('#agent-msgs').innerHTML = '<p>Open a project first.</p>';
    return;
  }
  agentState.projectRoot = projectRoot;
  // Lazily start a session.
  if (!agentState.sessionId) {
    const r = await agentApi.startSession(projectRoot);
    agentState.sessionId = r.session_id;
  }
  // Subscribe to events (idempotent — guard via window flag).
  if (!window.__agentListening) {
    window.__agentListening = true;
    onAgentStream(ev => handleStream(ev));
    onAgentAskUser(req => handleAskUser(req));
  }
  document.getElementById('agent-input-form').addEventListener('submit', async e => {
    e.preventDefault();
    const ta = document.getElementById('agent-input');
    const text = ta.value.trim();
    if (!text) return;
    ta.value = '';
    pushMessage({ role: 'user', content: text });
    await agentApi.send(agentState.projectRoot, text);
  });
}

function handleStream(ev) {
  // Switch on ev.kind. Minimal in this task; full rendering lands in Task 10.
  const m = document.getElementById('agent-msgs');
  if (!m) return;
  const line = document.createElement('div');
  line.className = 'agent-event-debug';
  line.textContent = JSON.stringify(ev);
  m.appendChild(line);
  m.scrollTop = m.scrollHeight;
}

function handleAskUser(req) {
  agentState.pendingAsks[req.call_id] = req.prompt;
  const m = document.getElementById('agent-msgs');
  if (!m) return;
  const line = document.createElement('div');
  line.className = 'agent-ask-user';
  line.innerHTML = `<strong>Agent asks:</strong> ${escapeHtml(req.prompt)} <button data-call-id="${req.call_id}">Reply</button>`;
  m.appendChild(line);
}

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}

function pushMessage(m) {
  agentState.messages.push(m);
  const target = document.getElementById('agent-msgs');
  if (!target) return;
  const div = document.createElement('div');
  div.className = `agent-msg agent-msg-${m.role}`;
  div.textContent = `[${m.role}] ${m.content}`;
  target.appendChild(div);
  target.scrollTop = target.scrollHeight;
}
```

- [ ] **Step 2: 路由接通**

`frontend/js/core/router.js` 中的 `parseChatRoute` / chat 分支替换。具体：
- import：`import { renderAgentView } from '../modules/agent/view.js';`
- 主分支 `if (view === 'agent') { renderAgentView(content); }` 加在现有 settings/dashboard 分支之间。
- 旧 `view.startsWith('chat/')` / `chatRoute` 处理改为：`if (view === 'chat' || view.startsWith('chat/')) { window.location.hash = '#agent'; return; }`（30 天 alias）。

`frontend/js/core/constants.js`：找 `KNOWN_VIEWS` 集合（如有），加 `'agent'`；如有 `chat` 留着，加注释 `// alias to agent until 2026-06-05`。

`frontend/index.html`：sidebar nav `data-view="chat"` 改 `data-view="agent"`，icon 换 `<i data-lucide="brain">`，label `Research`（zh: `研究`）。

- [ ] **Step 3: 浏览器烟雾测**

```bash
cd frontend && python3 -m http.server 8090 &
sleep 2
echo "open http://localhost:8090/#agent in browser, expect 'Open a project first.'"
kill %1
```

`agent-shell` 三栏 div 出现即过。

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(frontend): #agent view skeleton + router alias from #chat

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 9：左栏 archive 列表

**Files:**
- Create: `frontend/js/modules/agent/left-pane.js`
- Modify: `frontend/js/modules/agent/view.js`

- [ ] **Step 1: 实现 left-pane**

新建 `frontend/js/modules/agent/left-pane.js`:
```js
import { agentState } from './state.js';
import { agentApi } from './api.js';

export async function renderLeftPane(root) {
  if (!agentState.projectRoot) {
    root.innerHTML = '<p>Open a project to see archives.</p>';
    return;
  }
  agentState.archives = await agentApi.listArchives(agentState.projectRoot);
  root.innerHTML = `
    <div class="agent-left-header">
      <h3>Research history</h3>
      <button id="agent-new-research">New research</button>
    </div>
    <ul class="agent-archive-list">
      ${agentState.archives.map(a => `
        <li class="agent-archive-item" data-id="${a.id}">
          <div class="agent-archive-summary">${escapeHtml(a.summary)}</div>
          <div class="agent-archive-meta">
            <span class="agent-archive-outcome agent-archive-outcome-${a.outcome}">${a.outcome}</span>
            <span class="agent-archive-time">${a.started_at.slice(0,16).replace('T',' ')}</span>
          </div>
        </li>`).join('')}
    </ul>`;
  root.querySelector('#agent-new-research').addEventListener('click', async () => {
    agentState.sessionId = null;
    const r = await agentApi.startSession(agentState.projectRoot);
    agentState.sessionId = r.session_id;
    document.getElementById('agent-msgs').innerHTML = '';
    agentState.messages = [];
  });
}

function escapeHtml(s) {
  return (s||'').replace(/[&<>"']/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
```

`view.js` `renderAgentView` 中调用：

```js
import { renderLeftPane } from './left-pane.js';
// ...inside renderAgentView, after agentState.projectRoot set:
await renderLeftPane(content.querySelector('#agent-left'));
```

- [ ] **Step 2: 浏览器烟雾测**

打开 `#agent`，左栏显示「Research history」+ New research 按钮 + 现有 archive（如已运行过）。

- [ ] **Step 3: 提交**

```bash
git add frontend/js/modules/agent/left-pane.js frontend/js/modules/agent/view.js
git commit -m "$(cat <<'EOF'
feat(frontend): agent left pane — archives list + new research button

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 10：中栏对话流 + 工具调用渲染

**Files:**
- Create: `frontend/js/modules/agent/middle-pane.js`
- Modify: `frontend/js/modules/agent/view.js`

- [ ] **Step 1: 实现 middle-pane**

新建 `frontend/js/modules/agent/middle-pane.js`:
```js
import { agentState } from './state.js';

const COLOR_BY_BUCKET = {
  read_fs: 'gray',
  sandbox_write: 'green',
  code_run_sandbox: 'green',
  web: 'teal',
  memory_write: 'blue',
};

export function appendStreamEvent(ev) {
  const m = document.getElementById('agent-msgs');
  if (!m) return;
  switch (ev.kind) {
    case 'text':       appendText(m, ev.delta); break;
    case 'reasoning':  appendReasoning(m, ev.delta); break;
    case 'tool_call':  appendToolCall(m, ev); break;
    case 'tool_result': appendToolResult(m, ev); break;
    case 'memory':     appendMemory(m, ev.recalled); break;
    case 'checkpoint': /* handled by right-pane */ break;
    case 'crystallize': appendCrystallize(m, ev); break;
    case 'done':       appendDone(m); break;
    case 'error':      appendError(m, ev.message); break;
  }
  m.scrollTop = m.scrollHeight;
}

function lastAssistantBlock(m) {
  const last = m.querySelector('.agent-msg-assistant:last-child .agent-text');
  if (last) return last;
  const block = document.createElement('div');
  block.className = 'agent-msg agent-msg-assistant';
  block.innerHTML = '<div class="agent-text"></div>';
  m.appendChild(block);
  return block.querySelector('.agent-text');
}
function appendText(m, delta) {
  const target = lastAssistantBlock(m);
  target.textContent += delta;
}
function appendReasoning(m, delta) {
  let r = m.querySelector('.agent-msg-assistant:last-child .agent-reasoning');
  if (!r) {
    const block = m.querySelector('.agent-msg-assistant:last-child') || lastAssistantBlock(m).parentElement;
    const div = document.createElement('details');
    div.className = 'agent-reasoning';
    div.innerHTML = '<summary>thinking…</summary><pre></pre>';
    block.appendChild(div);
    r = div;
  }
  r.querySelector('pre').textContent += delta;
}
function appendToolCall(m, ev) {
  const c = document.createElement('div');
  c.className = `agent-tool-call agent-tool-call-${ev.decision}`;
  c.dataset.callId = ev.call_id;
  c.dataset.bucket = ev.bucket;
  c.dataset.name = ev.name;
  const color = COLOR_BY_BUCKET[ev.bucket?.split(':')[0]] || 'slate';
  c.innerHTML = `
    <div class="agent-tool-head">
      <span class="agent-tool-name">${escapeHtml(ev.name)}</span>
      <span class="agent-bucket agent-bucket-${color}">${escapeHtml(ev.bucket)}</span>
      <span class="agent-decision">${ev.decision}</span>
    </div>
    <details class="agent-tool-args"><summary>args</summary><pre>${escapeHtml(JSON.stringify(ev.args, null, 2))}</pre></details>
    <div class="agent-tool-status">${ev.decision === 'allow' ? 'running…' : 'awaiting approval'}</div>`;
  m.appendChild(c);
}
function appendToolResult(m, ev) {
  const c = m.querySelector(`[data-call-id="${ev.call_id}"]`);
  if (c) {
    c.querySelector('.agent-tool-status').textContent = ev.result?.error ? `error: ${ev.result.error}` : 'done';
    const det = document.createElement('details');
    det.className = 'agent-tool-result';
    det.innerHTML = `<summary>result</summary><pre>${escapeHtml(JSON.stringify(ev.result, null, 2))}</pre>`;
    c.appendChild(det);
  }
}
function appendMemory(m, recalled) {
  agentState.recalled = recalled || [];
  if (!recalled?.length) return;
  const c = document.createElement('details');
  c.className = 'agent-memory-card';
  c.innerHTML = `<summary>Recalled ${recalled.length} memory entries</summary>
    <ul>${recalled.map(r => `<li>[${escapeHtml(r.scope)}|${escapeHtml(r.kind)}] ${escapeHtml(r.text)}</li>`).join('')}</ul>`;
  m.appendChild(c);
}
function appendCrystallize(m, ev) {
  const c = document.createElement('div');
  c.className = 'agent-crystallize';
  c.textContent = `Crystallized to ${ev.layer}/${ev.scope}: ${ev.path}`;
  m.appendChild(c);
}
function appendDone(m) {
  const c = document.createElement('div');
  c.className = 'agent-done';
  c.textContent = '— task done —';
  m.appendChild(c);
}
function appendError(m, msg) {
  const c = document.createElement('div');
  c.className = 'agent-error';
  c.textContent = `error: ${msg}`;
  m.appendChild(c);
}
function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
```

`view.js` 替换 Task 8 里的 stub `handleStream(ev)` 为：
```js
import { appendStreamEvent } from './middle-pane.js';
function handleStream(ev) {
  appendStreamEvent(ev);
  if (ev.kind === 'checkpoint') updateRightPaneCheckpoint(ev);  // implemented in Task 11
}
```

- [ ] **Step 2: 浏览器烟雾测**

启动 Tauri dev (`cd crates/rb-app && cargo tauri dev`，需要装 `tauri-cli`)，开 project，进 `#agent`，输入 "echo hello in shell" 风格指令；预期看到 tool_call/tool_result 卡片。如本地没装 LLM provider，跳过实际跑——CSS+渲染逻辑可看静态结构。

- [ ] **Step 3: 提交**

```bash
git add frontend/js/modules/agent/middle-pane.js frontend/js/modules/agent/view.js
git commit -m "$(cat <<'EOF'
feat(frontend): agent middle pane — message stream + tool call cards

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 11：右栏（checkpoint todo + sandbox tree + net log）

**Files:**
- Create: `frontend/js/modules/agent/right-pane.js`
- Modify: `frontend/js/modules/agent/view.js`

- [ ] **Step 1: 实现 right-pane**

新建 `frontend/js/modules/agent/right-pane.js`:
```js
import { agentState } from './state.js';

export function renderRightPane(root) {
  root.innerHTML = `
    <section class="agent-right-section" id="agent-todo-section">
      <h4>Working checkpoint</h4>
      <ul class="agent-todo-list" id="agent-todo-list"><li class="empty">no todo yet</li></ul>
    </section>
    <section class="agent-right-section" id="agent-sandbox-section">
      <h4>Sandbox</h4>
      <pre class="agent-sandbox-tree" id="agent-sandbox-tree">(empty)</pre>
    </section>
    <section class="agent-right-section" id="agent-net-section">
      <h4>Network log</h4>
      <pre class="agent-net-log" id="agent-net-log">(disabled)</pre>
    </section>`;
}

export function updateCheckpointTodo(todo) {
  agentState.todo = todo;
  const ul = document.getElementById('agent-todo-list');
  if (!ul) return;
  if (!todo?.length) {
    ul.innerHTML = '<li class="empty">no todo yet</li>';
    return;
  }
  ul.innerHTML = todo.map(t =>
    `<li class="${t.done ? 'done' : 'open'}">${t.done ? '✔' : '○'} ${escapeHtml(t.text)}</li>`
  ).join('');
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
```

`view.js` 在 `renderAgentView` 中调用：
```js
import { renderRightPane, updateCheckpointTodo } from './right-pane.js';
// ...after renderLeftPane(...):
renderRightPane(content.querySelector('#agent-right'));
```

并把 `updateRightPaneCheckpoint` 实现成：
```js
function updateRightPaneCheckpoint(ev) { updateCheckpointTodo(ev.todo); }
```

- [ ] **Step 2: 提交**

```bash
git add frontend/js/modules/agent/right-pane.js frontend/js/modules/agent/view.js
git commit -m "$(cat <<'EOF'
feat(frontend): agent right pane — todo + sandbox + net log placeholders

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 12：Approval 卡 + ask_user reply

**Files:**
- Create: `frontend/js/modules/agent/approval-card.js`
- Modify: `frontend/js/modules/agent/middle-pane.js`
- Modify: `frontend/js/modules/agent/view.js`

- [ ] **Step 1: 实现 approval-card**

新建 `frontend/js/modules/agent/approval-card.js`:
```js
import { agentApi } from './api.js';
import { agentState } from './state.js';

export function attachApprovalHandlers(root) {
  root.addEventListener('click', async e => {
    const approveBtn = e.target.closest('[data-approve]');
    if (approveBtn) {
      const card = approveBtn.closest('.agent-tool-call');
      await agentApi.approve(agentState.projectRoot, card.dataset.callId);
      card.querySelector('.agent-tool-status').textContent = 'approved — running…';
      return;
    }
    const rejectBtn = e.target.closest('[data-reject]');
    if (rejectBtn) {
      const card = rejectBtn.closest('.agent-tool-call');
      const reason = prompt('Reason (optional)?') || null;
      await agentApi.reject(agentState.projectRoot, card.dataset.callId, reason);
      card.querySelector('.agent-tool-status').textContent = 'rejected';
      return;
    }
    const askReply = e.target.closest('.agent-ask-user [data-call-id]');
    if (askReply) {
      const callId = askReply.dataset.callId;
      const reply = prompt(`Agent asks: ${agentState.pendingAsks[callId] || ''}`) || '';
      await agentApi.answer(agentState.projectRoot, callId, reply);
      delete agentState.pendingAsks[callId];
      askReply.closest('.agent-ask-user').textContent = `(replied) ${reply}`;
    }
  });
}
```

`middle-pane.js` 中 `appendToolCall` 在 `awaiting approval` 状态下额外加 approve/reject 按钮：

替换 `appendToolCall` 末尾：
```js
  c.innerHTML = `
    <div class="agent-tool-head">...</div>
    <details class="agent-tool-args">...</details>
    <div class="agent-tool-status">${ev.decision === 'allow' ? 'running…' : 'awaiting approval'}</div>
    ${ev.decision === 'approve_once' || ev.decision === 'always_ask' ? `
      <div class="agent-approval-actions">
        <button data-approve>Approve</button>
        <button data-reject>Reject</button>
      </div>` : ''}
    `;
```

`view.js` 在 setup events 处加：
```js
import { attachApprovalHandlers } from './approval-card.js';
// ...after renderRightPane(...):
attachApprovalHandlers(content.querySelector('#agent-msgs'));
```

- [ ] **Step 2: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(frontend): approval cards + ask_user reply prompt

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase C — 收尾：CSS、Full Permission toggle、CHANGELOG、版本

### Task 13：CSS — 三栏布局 + 卡片样式

**Files:**
- Create: `frontend/css/agent.css`
- Modify: `frontend/index.html` (link agent.css)

- [ ] **Step 1: 写 CSS**

新建 `frontend/css/agent.css`:
```css
.agent-shell {
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr) 280px;
  gap: 16px;
  height: calc(100vh - 80px);
}
.agent-left, .agent-right {
  background: var(--surface, #fdf8f0);
  border: 1px solid var(--line, #e4dccd);
  border-radius: 8px;
  padding: 12px;
  overflow-y: auto;
}
.agent-mid { display: flex; flex-direction: column; min-height: 0; }
.agent-msgs { flex: 1; overflow-y: auto; padding: 12px; background: var(--surface, #fdf8f0); border-radius: 8px; border: 1px solid var(--line, #e4dccd); }
.agent-input { display: flex; gap: 8px; padding-top: 12px; }
.agent-input textarea { flex: 1; padding: 8px; font: inherit; }
.agent-input button { padding: 8px 16px; }

.agent-msg { margin-bottom: 12px; padding: 8px 12px; border-radius: 6px; }
.agent-msg-user { background: #fff8e7; }
.agent-msg-assistant { background: #f3f9f5; }
.agent-text { white-space: pre-wrap; }
.agent-reasoning { color: #777; font-size: 0.85rem; margin-top: 6px; }
.agent-reasoning pre { white-space: pre-wrap; }

.agent-tool-call { margin: 8px 0; padding: 8px; border: 1px solid #d4cdb6; border-radius: 6px; background: #fff; }
.agent-tool-head { display: flex; gap: 8px; align-items: center; }
.agent-tool-name { font-weight: 600; }
.agent-bucket { font-size: 0.75rem; padding: 2px 6px; border-radius: 4px; }
.agent-bucket-green { background: #dff5e3; color: #2c6e3a; }
.agent-bucket-teal  { background: #d6f0ee; color: #2a6e6e; }
.agent-bucket-blue  { background: #d8e4f5; color: #2a4d80; }
.agent-bucket-gray  { background: #ececec; color: #555; }
.agent-bucket-slate { background: #e4e4ea; color: #444; }
.agent-decision { font-size: 0.75rem; color: #888; margin-left: auto; }
.agent-tool-args summary, .agent-tool-result summary { cursor: pointer; font-size: 0.85rem; }
.agent-tool-args pre, .agent-tool-result pre { font-size: 0.78rem; max-height: 200px; overflow: auto; }
.agent-tool-status { margin-top: 6px; font-size: 0.85rem; color: #555; }
.agent-approval-actions { margin-top: 6px; display: flex; gap: 8px; }
.agent-approval-actions button { padding: 4px 12px; font-size: 0.85rem; }

.agent-memory-card { margin: 8px 0; font-size: 0.85rem; color: #555; }
.agent-crystallize { margin: 8px 0; padding: 6px; background: #efe7d4; border-radius: 4px; font-size: 0.85rem; }
.agent-done { margin: 12px 0; padding: 8px; text-align: center; background: #ebf3eb; border-radius: 4px; }
.agent-error { margin: 8px 0; padding: 8px; background: #fbe7e2; color: #8a3324; border-radius: 4px; }

.agent-archive-list { list-style: none; padding: 0; }
.agent-archive-item { padding: 6px; margin-bottom: 4px; cursor: pointer; border-radius: 4px; }
.agent-archive-item:hover { background: #ece4ce; }
.agent-archive-summary { font-size: 0.9rem; }
.agent-archive-meta { display: flex; gap: 8px; font-size: 0.75rem; color: #888; }
.agent-archive-outcome-done       { color: #2c6e3a; }
.agent-archive-outcome-cancelled  { color: #b07a16; }
.agent-archive-outcome-failed     { color: #8a3324; }
.agent-archive-outcome-interrupted{ color: #5d5d8a; }

.agent-right-section { margin-bottom: 16px; }
.agent-right-section h4 { margin: 0 0 6px 0; font-size: 0.85rem; color: #555; }
.agent-todo-list { list-style: none; padding: 0; font-size: 0.85rem; }
.agent-todo-list li.done { color: #888; text-decoration: line-through; }
.agent-sandbox-tree, .agent-net-log { font-size: 0.78rem; max-height: 180px; overflow: auto; background: #fdfaf2; padding: 6px; border-radius: 4px; }
```

`frontend/index.html` `<head>` 加：
```html
<link rel="stylesheet" href="css/agent.css">
```

- [ ] **Step 2: 浏览器烟雾测**

打开 `#agent`，三栏布局对齐、字体颜色匹配 "Warm Botanical Lab" 主题。

- [ ] **Step 3: 提交**

```bash
git add frontend/css/agent.css frontend/index.html
git commit -m "$(cat <<'EOF'
feat(frontend): agent.css — three-column layout + tool call card styling

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 14：Full Permission toggle + 顶部工具条

**Files:**
- Create: `frontend/js/modules/agent/full-permission.js`
- Modify: `frontend/js/modules/agent/view.js`
- Modify: `frontend/css/agent.css`

- [ ] **Step 1: 实现 toggle**

新建 `frontend/js/modules/agent/full-permission.js`:
```js
import { agentApi } from './api.js';
import { agentState } from './state.js';

export function renderToolbar(root) {
  root.innerHTML = `
    <div class="agent-toolbar">
      <label class="agent-fp-toggle">
        <input type="checkbox" id="agent-fp-checkbox">
        <span>Full permission</span>
      </label>
      <button id="agent-cancel-btn">Cancel run</button>
    </div>`;
  const cb = root.querySelector('#agent-fp-checkbox');
  cb.addEventListener('change', async () => {
    if (cb.checked) {
      const ok = confirm('Full permission disables every approval gate AND turns off network logging. Proceed?');
      if (!ok) { cb.checked = false; return; }
    }
    agentState.fullPermission = cb.checked;
    await agentApi.setFullPermission(agentState.projectRoot, cb.checked);
  });
  root.querySelector('#agent-cancel-btn').addEventListener('click', async () => {
    await agentApi.cancel(agentState.projectRoot);
  });
}
```

`view.js` 顶部布局调整：在 `agent-mid` 上方加 `<div id="agent-toolbar"></div>`，调用 `renderToolbar(content.querySelector('#agent-toolbar'))`。

`agent.css` 加：
```css
.agent-shell { grid-template-rows: 40px 1fr; grid-template-areas: "toolbar toolbar toolbar" "left mid right"; }
#agent-toolbar { grid-area: toolbar; display: flex; gap: 12px; align-items: center; padding: 0 8px; }
.agent-fp-toggle { display: flex; gap: 6px; align-items: center; cursor: pointer; }
```

- [ ] **Step 2: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(frontend): agent toolbar — full permission toggle + cancel button

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 15：CHANGELOG + 版本 0.3.0

**Files:**
- Create or modify: `CHANGELOG.md` (项目根)
- Modify: 所有 `crates/*/Cargo.toml`：`version = "0.3.0"`

- [ ] **Step 1: 加 CHANGELOG**

如果 `CHANGELOG.md` 不存在则创建：

```markdown
# Changelog

## v0.3.0 — 2026-05-XX

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
```

- [ ] **Step 2: 改 Cargo.toml 版本**

每个 `crates/*/Cargo.toml` 顶部 `version = "0.x"` → `version = "0.3.0"`。一次性命令：

```
for f in crates/*/Cargo.toml; do
  sed -i 's/^version = ".*"/version = "0.3.0"/' "$f"
done
```

确认改对：
```
grep '^version' crates/*/Cargo.toml
```

- [ ] **Step 3: 编译 + 全测**

```
cargo check --workspace
cargo test --workspace
```

预期：通过。

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
chore: v0.3.0 — self-evolving agent + CHANGELOG

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Plan 2 终止条件 / 验收标准

完成 15 个 task 后：

1. `cargo test --workspace` 全 PASS（包括 Plan 1 的 311 + Plan 2 加的几个单元）。
2. `cargo clippy -p rb-app -- -D warnings` 通过（rb-ai 已 clean）。
3. `cargo tauri dev`（在装了 tauri-cli 的机器上）能启动：
   - 进入 `#agent`，左栏显示 archive 列表，中栏可输入消息。
   - 配过 OpenAI-compat provider 后，发消息能看到 streaming text + tool_call 卡片。
   - approve/reject/cancel 按钮均可工作。
   - Full permission toggle 弹确认对话框，开后 sandbox 桶不再要审批。
4. `#chat` URL 自动重定向到 `#agent`。
5. v0.3.0 版本号已更新；CHANGELOG 记录所有 BREAKING + 新功能。

---

## Self-Review

**Spec coverage:**

| Spec section | Plan task |
|---|---|
| 10 Tauri commands | T2–T6 |
| `agent-stream` 单 channel + 各种 event variants | T3 forwarder + T10 渲染 |
| 3 栏布局 | T8 (skeleton) + T9 (left) + T10 (mid) + T11 (right) |
| approval card + always-allow 复选 | T12 |
| Memory recall 卡 | T10 `appendMemory` |
| Crystallize 卡 | T10 `appendCrystallize` |
| Full Permission toggle | T14 |
| 单 project 并发护栏 | T2 (start_session 拒绝) |
| `#chat` 30 天 alias | T8 路由分支 |
| CHANGELOG / v0.3.0 | T15 |
| 删旧 chat 模块 | T7 |
| AskUser pause/resume | T3 forwarder + T4 (`agent_answer`) + T12 prompt |
| Frontend approval channel | T12 |
| net log tail in right pane | T11（占位 `<pre>`，本 plan 暂不实时尾随；下一迭代再做实时 stream） |
| Sandbox file tree | T11（占位 `<pre>`，同上） |

> **Open scope notes**：T11 的 sandbox tree / net log 是静态占位；实时刷新需要新增 `agent_get_sandbox_tree` / `agent_tail_net_log` 命令——本 plan 不展开，保留为 v0.3.1 增量。Spec 中 §UI 描述的「实时尾随」是 v0.3.x 增量也接受的。

**Placeholder scan:** 无 TBD/TODO。所有 step 含完整代码或具体命令。

**Type consistency:**
- `AgentHandle` 字段在 T1 定义，T3 中扩展（slot 化）；T1 实现需按 T3 注释更新——这是 plan 内部一致的，但要求 T1 的 subagent 在 T3 时回头改 T1 的 struct 定义。注意此点。
- 所有命令的 args 结构体（`StartSessionArgs` 等）、返回类型在各 task 内一致。
- frontend `agentApi.*` 与 rust `agent_*` 命令一一对应。
- `AgentEvent` 枚举 variants（10 个）与 `appendStreamEvent` switch 各 case 对齐。

**Scope check:** 15 task 一次落地是大但合理；可在 Phase A 完成后做一次 review 间断点（比如 T6 完成后 manual-test 一遍 Tauri 命令再进 Phase B）。

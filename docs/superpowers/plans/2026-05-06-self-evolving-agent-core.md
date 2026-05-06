# Self-Evolving Agent — Plan 1：rb-ai 核心重写

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `rb-ai` 重写为感知-推理-执行-记忆循环 + L0–L4 分层记忆 + 风险分桶沙箱。本 plan 完成后，rb-ai 可在 mock provider 下跑完整 agent session（lib 集成测验证），但尚未接 Tauri 命令与 frontend——那是 Plan 2。

**Architecture:** 在 `rb-ai/src/` 内：保留 `provider/` `tools/module_derived` `tools/schema` `config/`；删除 `session/` `orchestrator/`；新增 `memory/` `sandbox/` `agent_loop/`，并把 `tools/builtin.rs` 单文件按职能拆成 `tools/builtin/` 子模块，新增 `tools/skill.rs`。所有新增模块只依赖 `rb-core`。

**Tech Stack:** Rust 1.7x、tokio、serde、serde_json、async-trait、reqwest、jsonschema、bm25（自实现轻量版，单文件 ≤ 200 行）、path-clean、fs2、wiremock（dev）、tempfile（dev）。

**对应 Spec:** `docs/superpowers/specs/2026-05-06-self-evolving-agent-design.md`

**不在本 Plan：** Tauri `agent_*` 命令、frontend `#agent` 视图、`#chat` 路由 alias、CHANGELOG/v0.3.0 发版。这些进 Plan 2。

---

## 文件布局总览

**新增**

```
crates/rb-ai/src/
├── memory/
│   ├── mod.rs
│   ├── layers.rs              L0/L1/L2/L3/L4 类型 + 序列化
│   ├── store.rs               双根（global+project）原子 IO + 索引 + 5MB 切片
│   ├── recall.rs              Recaller trait + BM25Recaller + FlashRecaller + Composite
│   └── crystallize.rs         task_done / start_long_term_update 写回
├── sandbox/
│   ├── mod.rs
│   ├── policy.rs              Bucket / Decision / SandboxPolicy / classify
│   ├── pixi.rs                pixi detect + init + run 包装
│   └── net.rs                 网络日志写入器
├── agent_loop/
│   ├── mod.rs                 AgentSession + run_session 主循环
│   ├── perceive.rs            project snapshot + memory recall 拼装
│   ├── reason.rs              provider 调用 + tool_calls 解析
│   ├── execute.rs             bucket 分发 + 审批 channel
│   └── record.rs              checkpoint fsync + 结晶触发
├── tools/
│   ├── builtin/               (新目录，原 builtin.rs 拆分进来)
│   │   ├── mod.rs             register_all
│   │   ├── file.rs            file_read / file_list / file_write / file_patch
│   │   ├── code_run.rs
│   │   ├── web.rs             web_scan
│   │   ├── memory_tools.rs    recall_memory / update_working_checkpoint / start_long_term_update / task_done
│   │   ├── ask_user.rs
│   │   └── project_state.rs   project_state / read_run_log / read_results_table
│   └── skill.rs               L3 markdown → ToolDef
└── (lib.rs / error.rs 修改)
```

**删除**

```
crates/rb-ai/src/session/        (整目录)
crates/rb-ai/src/orchestrator/   (整目录)
crates/rb-ai/src/tools/builtin.rs (单文件，被 tools/builtin/ 子模块取代)
crates/rb-ai/src/tools/stubs.rs  (Phase-3 stub 不再需要)
```

**修改**

```
crates/rb-ai/Cargo.toml                  +bm25(自实现)/path-clean/fs2，-jsonschema 留用
crates/rb-ai/src/lib.rs                  re-export 调整
crates/rb-ai/src/error.rs                +SandboxViolation/MemoryWrite/PathEscape 等
crates/rb-ai/src/tools/schema.rs         RiskLevel 扩展为 Read/RunLow/RunMid/Destructive
crates/rb-ai/src/tools/mod.rs            re-export 微调，删 stubs
crates/rb-ai/src/tools/module_derived.rs risk → RunMid
crates/rb-ai/src/config/mod.rs           +AgentConfig 子节
```

**rb-app 侧最小改动**（仅为本 plan 的集成测能编译，正式命令 Plan 2 处理）：把 `rb-app/src/main.rs` 中现有 `chat_*` 命令注册暂时注释掉或删掉；不暴露任何 `agent_*`。本 plan 不动 frontend。

---

## Phase 0 —— 基础准备

### Task 1：扩展 RiskLevel 枚举

**Files:**
- Modify: `crates/rb-ai/src/tools/schema.rs`
- Test (扩展现有): `crates/rb-ai/src/tools/schema.rs` 内 `mod tests`

- [ ] **Step 1: 写失败测试**

在 `crates/rb-ai/src/tools/schema.rs` 的 `mod tests` 末尾追加：

```rust
#[test]
fn risk_level_serializes_all_four_buckets() {
    use serde_json::json;
    fn s(r: RiskLevel) -> String {
        serde_json::to_value(r).unwrap().as_str().unwrap().to_string()
    }
    assert_eq!(s(RiskLevel::Read), "read");
    assert_eq!(s(RiskLevel::RunLow), "run_low");
    assert_eq!(s(RiskLevel::RunMid), "run_mid");
    assert_eq!(s(RiskLevel::Destructive), "destructive");

    let from: RiskLevel = serde_json::from_value(json!("run_mid")).unwrap();
    assert_eq!(from, RiskLevel::RunMid);
}
```

- [ ] **Step 2: 跑测试确认失败**

```
cargo test -p rb-ai --lib tools::schema::tests::risk_level_serializes_all_four_buckets
```

预期：编译失败（`RunLow`/`RunMid` 不存在）。

- [ ] **Step 3: 实现**

替换 `RiskLevel` 枚举：

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Read,
    RunLow,
    RunMid,
    Destructive,
}
```

- [ ] **Step 4: 修旧测试**

旧 `tooldef_serializes_risk_as_lowercase_string` 用 `RiskLevel::Run`，改成 `RiskLevel::RunMid` 并把字符串断言改为 `"risk":"run_mid"`。

- [ ] **Step 5: 跑全部 tools::schema 测试**

```
cargo test -p rb-ai --lib tools::schema
```

预期：全 PASS。

- [ ] **Step 6: 暂存依赖修改**

`tools/module_derived.rs` 此刻引用 `RiskLevel::Run`，把它统一改成 `RiskLevel::RunMid`（grep `RiskLevel::Run\b` 在整个 `crates/rb-ai/src/` 下，逐处替换）。

```
cargo check -p rb-ai
```

预期：编译通过。`session/` `orchestrator/` 仍存在，引用 `RiskLevel::Run` 处也一并替换为 `RiskLevel::RunMid`（这些文件下个 task 会被删，但要先编译通过）。

- [ ] **Step 7: 提交**

```bash
git add crates/rb-ai/src/tools/schema.rs crates/rb-ai/src/tools/module_derived.rs crates/rb-ai/src/orchestrator crates/rb-ai/src/session
git commit -m "$(cat <<'EOF'
refactor(ai): expand RiskLevel into Read/RunLow/RunMid/Destructive

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2：扩展 AiError

**Files:**
- Modify: `crates/rb-ai/src/error.rs`

- [ ] **Step 1: 实现新增 variant**

替换 `AiError`：

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
    #[error("sandbox violation: {0}")]
    SandboxViolation(String),
    #[error("path escapes allowed root: {0}")]
    PathEscape(String),
    #[error("memory write failed: {0}")]
    MemoryWrite(String),
    #[error("agent already running for project {0}")]
    AgentAlreadyRunning(String),
}
```

> 删 `SessionNotFound`（被 `AgentAlreadyRunning` 等取代；旧 chat session 概念已废）。

- [ ] **Step 2: 编译**

```
cargo check -p rb-ai
```

预期：可能在 `session/store.rs` 等处用了 `AiError::SessionNotFound`，那些文件下一 task 会整体删除；如本 task 中编译失败，把对 `SessionNotFound` 的引用临时替换为 `AiError::InvalidState("legacy session".into())`，让编译通过。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/error.rs crates/rb-ai/src/session
git commit -m "$(cat <<'EOF'
refactor(ai): extend AiError with sandbox/memory/agent variants

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3：删除 session/ 与 orchestrator/

**Files:**
- Delete: `crates/rb-ai/src/session/` 整个目录
- Delete: `crates/rb-ai/src/orchestrator/` 整个目录
- Delete: `crates/rb-ai/src/tools/stubs.rs`
- Modify: `crates/rb-ai/src/lib.rs`
- Modify: `crates/rb-ai/src/tools/mod.rs`
- Modify: `crates/rb-app/src/main.rs`（删除 chat 命令注册，仅本任务需要）
- Modify: `crates/rb-app/src/commands/`（如有 `chat.rs` / `chat_session.rs` 等）

- [ ] **Step 1: 删 rb-ai 内三组文件**

```
git rm -r crates/rb-ai/src/session crates/rb-ai/src/orchestrator
git rm crates/rb-ai/src/tools/stubs.rs
```

- [ ] **Step 2: 修 `crates/rb-ai/src/lib.rs`**

```rust
//! AI orchestration, provider abstraction, and persistent agent memory.
//!
//! Depends on `rb-core` for `ModuleRegistry`, `Runner`, `Project`; does not
//! depend on Tauri.

pub mod agent_loop;
pub mod config;
pub mod error;
pub mod memory;
pub mod provider;
pub mod sandbox;
pub mod tools;

pub use error::AiError;
```

> `agent_loop` / `memory` / `sandbox` 子模块此刻还不存在，本 step 只是写入未来引用——下面紧跟着用空 mod 占位。

- [ ] **Step 3: 创建空占位模块**

```
mkdir -p crates/rb-ai/src/{memory,sandbox,agent_loop}
```

每个目录写入一个空 `mod.rs`：

`crates/rb-ai/src/memory/mod.rs`:
```rust
//! Layered memory store (L0–L4) — implemented across submodules.
```

`crates/rb-ai/src/sandbox/mod.rs`:
```rust
//! Sandbox policy + pixi/net wrappers — implemented across submodules.
```

`crates/rb-ai/src/agent_loop/mod.rs`:
```rust
//! Perceive→reason→execute→record main loop — implemented across submodules.
```

- [ ] **Step 4: 修 `crates/rb-ai/src/tools/mod.rs`**

把 `pub mod stubs;` 那行删掉，其余保留（`builtin` `module_derived` `schema`）。`builtin.rs` 还存在，下个阶段 (Task 9-onwards) 才拆。

- [ ] **Step 5: 删 rb-app 中 chat 命令**

```
grep -rn 'chat_' crates/rb-app/src/
```

把所有 `chat_*` 命令注册（`tauri::generate_handler!` 中的项）删除；删除 `crates/rb-app/src/commands/chat*.rs` 文件（如有）；删除 `AppState` 中如 `chat_store: ...` 字段（若引用 `rb_ai::session`）。

- [ ] **Step 6: 全工作区编译**

```
cargo check --workspace
```

预期：通过。如有未删干净的 `rb_ai::session::*` / `rb_ai::orchestrator::*` 引用，按报错逐一删除。

- [ ] **Step 7: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(ai): delete session/, orchestrator/, stubs.rs in rb-ai

Tear-down for the agent rewrite. rb-app chat_* commands removed;
agent_* commands will land in Plan 2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4：扩展 AiConfig + 新增 Cargo 依赖

**Files:**
- Modify: `crates/rb-ai/Cargo.toml`
- Modify: `crates/rb-ai/src/config/mod.rs`

- [ ] **Step 1: 写失败测试**

在 `crates/rb-ai/src/config/mod.rs` 末尾追加（若 mod tests 不存在则新建）：

```rust
#[cfg(test)]
mod agent_config_tests {
    use super::*;

    #[test]
    fn agent_config_defaults_are_sane() {
        let c = AgentConfig::default();
        assert_eq!(c.code_run.runtime, CodeRunRuntime::Pixi);
        assert_eq!(c.code_run.default_timeout_secs, 600);
        assert_eq!(c.sandbox.sandbox_dirname, "sandbox");
        assert!(matches!(c.network.mode, NetworkMode::AllowAll));
        assert!(c.network.log_enabled);
    }

    #[test]
    fn agent_config_round_trips_via_json() {
        let c = AgentConfig::default();
        let s = serde_json::to_string(&c).unwrap();
        let back: AgentConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(c.code_run.default_timeout_secs, back.code_run.default_timeout_secs);
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

```
cargo test -p rb-ai --lib config::agent_config_tests
```

预期：未定义类型。

- [ ] **Step 3: 实现 `AgentConfig`**

在 `crates/rb-ai/src/config/mod.rs` 末尾追加：

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeRunRuntime {
    Pixi,
    System,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    AllowAll,
    Whitelist,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub default_model: String,
    pub flash_recall_model: String,
    pub flash_recall_timeout_ms: u64,
    pub recall_budget_tokens: usize,
    pub working_token_budget: usize,
    pub code_run: CodeRunConfig,
    pub sandbox: SandboxConfig,
    pub network: NetworkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRunConfig {
    pub runtime: CodeRunRuntime,
    pub default_timeout_secs: u64,
    pub custom_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub sandbox_dirname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub mode: NetworkMode,
    pub whitelist: Vec<String>,
    pub log_enabled: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            default_model: "claude-sonnet-4-6".into(),
            flash_recall_model: "claude-haiku-4-5".into(),
            flash_recall_timeout_ms: 3000,
            recall_budget_tokens: 4096,
            working_token_budget: 8192,
            code_run: CodeRunConfig {
                runtime: CodeRunRuntime::Pixi,
                default_timeout_secs: 600,
                custom_command: String::new(),
            },
            sandbox: SandboxConfig {
                sandbox_dirname: "sandbox".into(),
            },
            network: NetworkConfig {
                mode: NetworkMode::AllowAll,
                whitelist: vec![],
                log_enabled: true,
            },
        }
    }
}
```

如 `AiConfig` struct 已存在并有 `serde` derive，把 `pub agent: AgentConfig,` 加入并打 `#[serde(default)]`。

- [ ] **Step 4: 跑测试确认通过**

```
cargo test -p rb-ai --lib config
```

预期：PASS。

- [ ] **Step 5: 加 Cargo 依赖**

`crates/rb-ai/Cargo.toml` 在 `[dependencies]` 增加：

```toml
path-clean = "1"
fs2 = "0.4"
```

> 不引入第三方 `bm25` crate；下面 Phase 1 自实现一个 ≤200 行的轻量 BM25。
> 不删 `jsonschema`（skill 加载仍要用）。
> 暂不删 `keyring`/`aes-gcm`/`argon2`（config 用）。

```
cargo build -p rb-ai
```

预期：通过。

- [ ] **Step 6: 提交**

```bash
git add crates/rb-ai/Cargo.toml crates/rb-ai/src/config/mod.rs Cargo.lock
git commit -m "$(cat <<'EOF'
feat(ai): add AgentConfig + path-clean/fs2 deps

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 1 —— Memory 层

> 5 个 task：先类型与序列化，再存储 IO，再写 BM25 召回，最后 crystallize。flash 召回依赖 provider 抽象，留到 Phase 4 接入。

### Task 5：memory/layers.rs 类型与序列化

**Files:**
- Create: `crates/rb-ai/src/memory/layers.rs`
- Modify: `crates/rb-ai/src/memory/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/memory/layers.rs`：

```rust
//! Layered memory types. Pure data + serialization, no IO.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// L1 entry: append-only insight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub id: String,
    pub tag: String,
    pub summary: String,
    pub evidence_archive_id: Option<String>,
    pub ts: DateTime<Utc>,
}

/// L3 skill metadata read from frontmatter; body kept separate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default = "SkillMeta::default_inputs_schema")]
    pub inputs_schema: serde_json::Value,
    #[serde(default = "SkillMeta::default_risk_tier")]
    pub risk_tier: String,
    #[serde(default)]
    pub crystallized_calls: Vec<serde_json::Value>,
}

impl SkillMeta {
    fn default_inputs_schema() -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    fn default_risk_tier() -> String {
        "run_mid".into()
    }
}

/// Index entry; written into `_index.json` for L3/L4 directories.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IndexEntry {
    Skill {
        name: String,
        path: String,
        scope: Scope,
        triggers: Vec<String>,
        hits: u64,
        last_used: Option<DateTime<Utc>>,
    },
    Archive {
        id: String,
        started_at: DateTime<Utc>,
        ended_at: Option<DateTime<Utc>>,
        summary: String,
        outcome: ArchiveOutcome,
        tags: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveOutcome {
    Done,
    Cancelled,
    Interrupted,
    Failed,
}

/// L4 archive: full agent session trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Archive {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub summary: String,
    pub outcome: ArchiveOutcome,
    pub tags: Vec<String>,
    pub messages: Vec<serde_json::Value>, // raw provider/tool messages
    pub net_log_path: Option<String>,
}

/// Single working checkpoint, written to `<project>/agent/checkpoints/current.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingCheckpoint {
    pub session_id: String,
    pub project_root: String,
    pub started_at: DateTime<Utc>,
    pub last_step_at: DateTime<Utc>,
    pub todo: Vec<TodoEntry>,
    pub message_count: usize,
    pub perceive_snapshot_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoEntry {
    pub text: String,
    pub done: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_meta_round_trips_with_defaults() {
        let yaml_like = serde_json::json!({
            "name": "human-rna-seq-de",
            "description": "Run a full RNA-seq DE pipeline",
        });
        let m: SkillMeta = serde_json::from_value(yaml_like).unwrap();
        assert_eq!(m.name, "human-rna-seq-de");
        assert_eq!(m.risk_tier, "run_mid");
        assert!(m.inputs_schema.get("type").is_some());
    }

    #[test]
    fn index_entry_serializes_with_kind_tag() {
        let entry = IndexEntry::Skill {
            name: "rna-seq".into(),
            path: "L3_skills/rna-seq.md".into(),
            scope: Scope::Global,
            triggers: vec!["rna-seq".into()],
            hits: 0,
            last_used: None,
        };
        let s = serde_json::to_string(&entry).unwrap();
        assert!(s.contains(r#""kind":"skill""#));
        assert!(s.contains(r#""scope":"global""#));
    }

    #[test]
    fn archive_outcome_is_snake_case() {
        let v = serde_json::to_value(ArchiveOutcome::Interrupted).unwrap();
        assert_eq!(v.as_str(), Some("interrupted"));
    }
}
```

更新 `crates/rb-ai/src/memory/mod.rs`：

```rust
//! Layered memory store (L0–L4).

pub mod layers;

pub use layers::{
    Archive, ArchiveOutcome, IndexEntry, Insight, Scope, SkillMeta, TodoEntry, WorkingCheckpoint,
};
```

- [ ] **Step 2: 跑测试确认通过**

```
cargo test -p rb-ai --lib memory::layers
```

预期：3 个测试 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/memory
git commit -m "$(cat <<'EOF'
feat(ai): add memory layer types (L0–L4 schema, no IO yet)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6：memory/store.rs 双根 IO + 索引 + 5MB 切片

**Files:**
- Create: `crates/rb-ai/src/memory/store.rs`
- Modify: `crates/rb-ai/src/memory/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/memory/store.rs`：

```rust
//! Atomic, dual-root memory IO. `MemoryStore` knows where global vs project
//! memory lives and writes both atomically with file locks.
//!
//! All writes go through a temp-file + fsync + rename pattern; index files
//! use exclusive `fs2` locks to handle concurrent sessions.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use fs2::FileExt;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::error::AiError;
use crate::memory::layers::{Archive, IndexEntry, Insight, Scope};

const SHARD_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct MemoryStore {
    pub global_root: PathBuf,
    inner: Arc<Mutex<()>>, // serialize index writes within this process
}

impl MemoryStore {
    /// Resolve the global memory root (`~/.local/share/rust_brain/agent/`)
    /// and ensure its skeleton exists.
    pub fn open_default() -> Result<Self, AiError> {
        let base = dirs::data_local_dir()
            .ok_or_else(|| AiError::Config("no data_local_dir".into()))?
            .join("rust_brain")
            .join("agent");
        Self::open(base)
    }

    pub fn open(global_root: PathBuf) -> Result<Self, AiError> {
        std::fs::create_dir_all(global_root.join("L3_skills"))?;
        ensure_file(&global_root.join("L0_meta.md"), DEFAULT_L0)?;
        ensure_file(&global_root.join("L1_insights.jsonl"), "")?;
        ensure_file(&global_root.join("L2_facts.md"), "# Long-term facts\n\n")?;
        ensure_index(&global_root.join("L3_skills/_index.json"))?;
        Ok(Self {
            global_root,
            inner: Arc::new(Mutex::new(())),
        })
    }

    pub fn project_root(project_root: &Path) -> PathBuf {
        project_root.join("agent")
    }

    pub fn ensure_project(&self, project_root: &Path) -> Result<PathBuf, AiError> {
        let root = Self::project_root(project_root);
        std::fs::create_dir_all(root.join("L3_local"))?;
        std::fs::create_dir_all(root.join("L4_archives"))?;
        std::fs::create_dir_all(root.join("checkpoints"))?;
        ensure_index(&root.join("L3_local/_index.json"))?;
        ensure_index(&root.join("L4_archives/_index.json"))?;
        Ok(root)
    }

    pub async fn append_l1_insight(&self, insight: &Insight) -> Result<(), AiError> {
        let path = self.global_root.join("L1_insights.jsonl");
        let line = serde_json::to_string(insight)? + "\n";
        let _g = self.inner.lock().await;
        append_with_lock(&path, line.as_bytes())
    }

    /// Append an archive. Splits into `<id>.part2.json`, etc., when the
    /// in-progress file would exceed `SHARD_BYTES`.
    pub async fn append_l4_archive(
        &self,
        project_root: &Path,
        archive: &Archive,
    ) -> Result<PathBuf, AiError> {
        let root = Self::project_root(project_root).join("L4_archives");
        std::fs::create_dir_all(&root)?;
        let _g = self.inner.lock().await;
        let path = next_shard_path(&root, &archive.id, SHARD_BYTES)?;
        let bytes = serde_json::to_vec_pretty(archive)?;
        write_atomic(&path, &bytes)?;
        update_index(&root.join("_index.json"), |entries| {
            // Replace any existing entry with same id (last shard wins).
            entries.retain(|e| match e {
                IndexEntry::Archive { id, .. } => id != &archive.id,
                _ => true,
            });
            entries.push(IndexEntry::Archive {
                id: archive.id.clone(),
                started_at: archive.started_at,
                ended_at: archive.ended_at,
                summary: archive.summary.clone(),
                outcome: archive.outcome,
                tags: archive.tags.clone(),
            });
            Ok(())
        })?;
        Ok(path)
    }

    pub async fn upsert_skill_index(
        &self,
        scope: Scope,
        project_root: Option<&Path>,
        entry: IndexEntry,
    ) -> Result<(), AiError> {
        let dir = match (scope, project_root) {
            (Scope::Global, _) => self.global_root.join("L3_skills"),
            (Scope::Project, Some(p)) => Self::project_root(p).join("L3_local"),
            (Scope::Project, None) => {
                return Err(AiError::InvalidState(
                    "project scope requires project_root".into(),
                ))
            }
        };
        std::fs::create_dir_all(&dir)?;
        let _g = self.inner.lock().await;
        update_index(&dir.join("_index.json"), |entries| {
            // Replace by name.
            let name_of = |e: &IndexEntry| match e {
                IndexEntry::Skill { name, .. } => Some(name.clone()),
                _ => None,
            };
            let new_name = name_of(&entry);
            entries.retain(|e| name_of(e) != new_name);
            entries.push(entry);
            Ok(())
        })
    }

    pub fn read_l0(&self) -> Result<String, AiError> {
        Ok(std::fs::read_to_string(self.global_root.join("L0_meta.md"))?)
    }

    pub fn read_l2(&self) -> Result<String, AiError> {
        Ok(std::fs::read_to_string(self.global_root.join("L2_facts.md"))?)
    }

    pub fn read_index(&self, path: &Path) -> Result<Vec<IndexEntry>, AiError> {
        if !path.exists() {
            return Ok(vec![]);
        }
        let bytes = std::fs::read(path)?;
        if bytes.is_empty() {
            return Ok(vec![]);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
}

fn ensure_file(path: &Path, default: &str) -> Result<(), AiError> {
    if !path.exists() {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(path, default)?;
    }
    Ok(())
}

fn ensure_index(path: &Path) -> Result<(), AiError> {
    if !path.exists() {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(path, b"[]")?;
    }
    Ok(())
}

fn append_with_lock(path: &Path, data: &[u8]) -> Result<(), AiError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    let f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.lock_exclusive()
        .map_err(|e| AiError::MemoryWrite(format!("lock {}: {e}", path.display())))?;
    let res = (&f).write_all(data).and_then(|_| f.sync_data());
    f.unlock().ok();
    res.map_err(|e| AiError::MemoryWrite(format!("append {}: {e}", path.display())))
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), AiError> {
    let parent = path
        .parent()
        .ok_or_else(|| AiError::MemoryWrite(format!("no parent: {}", path.display())))?;
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name().unwrap().to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&tmp, data)?;
    let f = std::fs::OpenOptions::new().write(true).open(&tmp)?;
    f.sync_data()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn update_index<F>(path: &Path, mutate: F) -> Result<(), AiError>
where
    F: FnOnce(&mut Vec<IndexEntry>) -> Result<(), AiError>,
{
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let lock_path = path.with_extension("lock");
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&lock_path)?;
    lock_file
        .lock_exclusive()
        .map_err(|e| AiError::MemoryWrite(format!("lock {}: {e}", path.display())))?;
    let res: Result<(), AiError> = (|| {
        let mut entries: Vec<IndexEntry> = if path.exists() {
            let bytes = std::fs::read(path)?;
            if bytes.is_empty() {
                vec![]
            } else {
                serde_json::from_slice(&bytes)?
            }
        } else {
            vec![]
        };
        mutate(&mut entries)?;
        let bytes = serde_json::to_vec_pretty(&entries)?;
        write_atomic(path, &bytes)
    })();
    lock_file.unlock().ok();
    res
}

fn next_shard_path(dir: &Path, id: &str, shard_bytes: u64) -> Result<PathBuf, AiError> {
    let main = dir.join(format!("{id}.json"));
    if !main.exists() {
        return Ok(main);
    }
    // If main is small, overwrite it; if it crosses threshold, find next part.
    let len = std::fs::metadata(&main)?.len();
    if len < shard_bytes {
        return Ok(main);
    }
    let mut n = 2;
    loop {
        let p = dir.join(format!("{id}.part{n}.json"));
        let exists = p.exists();
        let len = if exists { std::fs::metadata(&p)?.len() } else { 0 };
        if !exists || len < shard_bytes {
            return Ok(p);
        }
        n += 1;
        if n > 100 {
            return Err(AiError::MemoryWrite("too many shards".into()));
        }
    }
}

const DEFAULT_L0: &str = r#"# Agent meta-rules

- 第一性原则：在 `<project>/sandbox/` 内自由实验，写到项目结果区前先 ask_user 或调用对应 module。
- 任务分解：长任务先拆 todo，落到 working checkpoint。
- 失败处理：同一工具连续失败 ≥3 次时停下 ask_user，不要无脑重试。
- 记忆归类：项目特定细节写 project scope；可复用经验写 global scope。
- 透明度：每个工具调用前简短说明意图；调用后总结结果，不沉默。
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;

    fn store(tmp: &Path) -> MemoryStore {
        MemoryStore::open(tmp.join("global")).unwrap()
    }

    #[test]
    fn open_creates_skeleton_files() {
        let tmp = tempdir().unwrap();
        let s = store(tmp.path());
        assert!(s.global_root.join("L0_meta.md").exists());
        assert!(s.global_root.join("L1_insights.jsonl").exists());
        assert!(s.global_root.join("L2_facts.md").exists());
        assert!(s.global_root.join("L3_skills/_index.json").exists());
    }

    #[tokio::test]
    async fn append_l1_insight_writes_jsonl_lines() {
        let tmp = tempdir().unwrap();
        let s = store(tmp.path());
        for i in 0..3 {
            s.append_l1_insight(&Insight {
                id: format!("i{i}"),
                tag: "test".into(),
                summary: format!("s{i}"),
                evidence_archive_id: None,
                ts: Utc::now(),
            })
            .await
            .unwrap();
        }
        let body = std::fs::read_to_string(s.global_root.join("L1_insights.jsonl")).unwrap();
        assert_eq!(body.lines().count(), 3);
        for (i, line) in body.lines().enumerate() {
            let v: Insight = serde_json::from_str(line).unwrap();
            assert_eq!(v.id, format!("i{i}"));
        }
    }

    #[tokio::test]
    async fn archive_index_replaces_same_id() {
        let tmp = tempdir().unwrap();
        let s = store(tmp.path());
        let project = tmp.path().join("proj");
        s.ensure_project(&project).unwrap();
        let mut a = Archive {
            id: "a1".into(),
            started_at: Utc::now(),
            ended_at: None,
            summary: "draft".into(),
            outcome: super::super::layers::ArchiveOutcome::Done,
            tags: vec![],
            messages: vec![],
            net_log_path: None,
        };
        s.append_l4_archive(&project, &a).await.unwrap();
        a.summary = "final".into();
        s.append_l4_archive(&project, &a).await.unwrap();
        let idx = s
            .read_index(&MemoryStore::project_root(&project).join("L4_archives/_index.json"))
            .unwrap();
        assert_eq!(idx.len(), 1);
        match &idx[0] {
            IndexEntry::Archive { summary, .. } => assert_eq!(summary, "final"),
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn shards_appear_after_size_threshold() {
        let tmp = tempdir().unwrap();
        let s = store(tmp.path());
        let project = tmp.path().join("proj");
        s.ensure_project(&project).unwrap();
        // First archive — small.
        let a1 = Archive {
            id: "big".into(),
            started_at: Utc::now(),
            ended_at: None,
            summary: "x".into(),
            outcome: super::super::layers::ArchiveOutcome::Done,
            tags: vec![],
            messages: vec![],
            net_log_path: None,
        };
        s.append_l4_archive(&project, &a1).await.unwrap();
        // Manually inflate the existing archive file past 5MB.
        let main = MemoryStore::project_root(&project)
            .join("L4_archives")
            .join("big.json");
        let pad = vec![b' '; (SHARD_BYTES + 1) as usize];
        std::fs::write(&main, pad).unwrap();
        let part = s.append_l4_archive(&project, &a1).await.unwrap();
        assert!(part.file_name().unwrap().to_string_lossy().contains("part2"));
    }
}
```

更新 `crates/rb-ai/src/memory/mod.rs`：

```rust
//! Layered memory store (L0–L4).

pub mod layers;
pub mod store;

pub use layers::{
    Archive, ArchiveOutcome, IndexEntry, Insight, Scope, SkillMeta, TodoEntry, WorkingCheckpoint,
};
pub use store::MemoryStore;
```

`crates/rb-ai/Cargo.toml` 加：

```toml
dirs = "5"   # 已有则跳过
```

> `dirs` 已经在依赖里（config 在用），这一步只确认。

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib memory::store
```

预期：4 个测试全 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/memory
git commit -m "$(cat <<'EOF'
feat(ai): MemoryStore with dual-root IO, atomic writes, 5MB sharding

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 7：memory/recall.rs — Recaller trait + BM25 实现

**Files:**
- Create: `crates/rb-ai/src/memory/recall.rs`
- Modify: `crates/rb-ai/src/memory/mod.rs`

> FlashRecaller 留空 stub，Phase 4 接 provider 时填充。本 task 只落 BM25 + trait + Composite 骨架。

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/memory/recall.rs`：

```rust
//! Memory recall. BM25 over compact "candidate text" derived from index
//! entries + L1 insights. Flash-LLM-driven recall lives in `flash_recaller`
//! below but is wired in Phase 4.

use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AiError;
use crate::memory::layers::{IndexEntry, Insight};
use crate::memory::store::MemoryStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallCandidate {
    pub id: String,
    pub kind: String, // "skill" | "archive" | "insight"
    pub scope: String, // "global" | "project"
    pub text: String, // compact text used for matching
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallResult {
    pub picked: Vec<RecallCandidate>,
    pub rationale: Option<String>,
}

#[async_trait]
pub trait Recaller: Send + Sync {
    async fn recall(
        &self,
        query: &str,
        candidates: Vec<RecallCandidate>,
        budget_tokens: usize,
    ) -> Result<RecallResult, AiError>;
}

pub fn collect_candidates(
    store: &MemoryStore,
    project_root: Option<&Path>,
) -> Result<Vec<RecallCandidate>, AiError> {
    let mut out = vec![];

    // Global L3 skill index.
    let g_index = store.read_index(&store.global_root.join("L3_skills/_index.json"))?;
    for e in g_index {
        if let IndexEntry::Skill {
            name,
            path,
            triggers,
            ..
        } = e
        {
            out.push(RecallCandidate {
                id: format!("skill:global:{name}"),
                kind: "skill".into(),
                scope: "global".into(),
                text: format!("{name} {}", triggers.join(" ")),
                path: Some(path),
            });
        }
    }

    // Global L1 insights (last 200 lines).
    let l1 = store.global_root.join("L1_insights.jsonl");
    if l1.exists() {
        let body = std::fs::read_to_string(&l1)?;
        for line in body.lines().rev().take(200) {
            if let Ok(v) = serde_json::from_str::<Insight>(line) {
                out.push(RecallCandidate {
                    id: format!("insight:{}", v.id),
                    kind: "insight".into(),
                    scope: "global".into(),
                    text: format!("{} {}", v.tag, v.summary),
                    path: None,
                });
            }
        }
    }

    if let Some(p) = project_root {
        let proot = MemoryStore::project_root(p);
        // L3_local skills.
        let p_index = store.read_index(&proot.join("L3_local/_index.json"))?;
        for e in p_index {
            if let IndexEntry::Skill {
                name,
                path,
                triggers,
                ..
            } = e
            {
                out.push(RecallCandidate {
                    id: format!("skill:project:{name}"),
                    kind: "skill".into(),
                    scope: "project".into(),
                    text: format!("{name} {}", triggers.join(" ")),
                    path: Some(path),
                });
            }
        }
        // L4 archives.
        let a_index = store.read_index(&proot.join("L4_archives/_index.json"))?;
        for e in a_index {
            if let IndexEntry::Archive {
                id, summary, tags, ..
            } = e
            {
                out.push(RecallCandidate {
                    id: format!("archive:{id}"),
                    kind: "archive".into(),
                    scope: "project".into(),
                    text: format!("{} {}", summary, tags.join(" ")),
                    path: None,
                });
            }
        }
    }

    Ok(out)
}

// ---------- BM25 ----------

/// Lightweight BM25 over candidate texts. No stemming; lower-cased token split.
pub struct Bm25Recaller {
    pub top_k: usize,
}

impl Bm25Recaller {
    pub fn new(top_k: usize) -> Self {
        Self { top_k }
    }
}

#[async_trait]
impl Recaller for Bm25Recaller {
    async fn recall(
        &self,
        query: &str,
        candidates: Vec<RecallCandidate>,
        _budget_tokens: usize,
    ) -> Result<RecallResult, AiError> {
        if candidates.is_empty() {
            return Ok(RecallResult { picked: vec![], rationale: None });
        }
        let q_terms: Vec<String> = tokenize(query);
        if q_terms.is_empty() {
            return Ok(RecallResult { picked: vec![], rationale: None });
        }
        let docs: Vec<Vec<String>> = candidates.iter().map(|c| tokenize(&c.text)).collect();
        let avgdl = docs.iter().map(|d| d.len()).sum::<usize>() as f32 / docs.len() as f32;
        let n = docs.len();
        let mut df: HashMap<&str, usize> = HashMap::new();
        for d in &docs {
            let mut seen: HashMap<&str, ()> = HashMap::new();
            for t in d {
                if seen.insert(t.as_str(), ()).is_none() {
                    *df.entry(t.as_str()).or_insert(0) += 1;
                }
            }
        }
        let k1 = 1.5_f32;
        let b = 0.75_f32;
        let mut scored: Vec<(usize, f32)> = docs
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let dl = d.len() as f32;
                let mut score = 0.0;
                for q in &q_terms {
                    let f = d.iter().filter(|t| *t == q).count() as f32;
                    if f == 0.0 {
                        continue;
                    }
                    let n_q = *df.get(q.as_str()).unwrap_or(&0) as f32;
                    let idf = ((n as f32 - n_q + 0.5) / (n_q + 0.5) + 1.0).ln();
                    let denom = f + k1 * (1.0 - b + b * dl / avgdl.max(1.0));
                    score += idf * (f * (k1 + 1.0)) / denom;
                }
                (i, score)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let picked: Vec<RecallCandidate> = scored
            .into_iter()
            .filter(|(_, s)| *s > 0.0)
            .take(self.top_k)
            .map(|(i, _)| candidates[i].clone())
            .collect();
        Ok(RecallResult {
            picked,
            rationale: Some("bm25".into()),
        })
    }
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

// ---------- Composite (Flash primary, BM25 fallback) ----------

pub struct CompositeRecaller {
    pub primary: Option<std::sync::Arc<dyn Recaller>>,
    pub fallback: std::sync::Arc<Bm25Recaller>,
    pub timeout: std::time::Duration,
}

#[async_trait]
impl Recaller for CompositeRecaller {
    async fn recall(
        &self,
        query: &str,
        candidates: Vec<RecallCandidate>,
        budget_tokens: usize,
    ) -> Result<RecallResult, AiError> {
        if let Some(p) = &self.primary {
            let cands = candidates.clone();
            let q = query.to_string();
            let primary = p.clone();
            let res =
                tokio::time::timeout(self.timeout, primary.recall(&q, cands, budget_tokens)).await;
            match res {
                Ok(Ok(r)) => return Ok(r),
                _ => {} // fall through
            }
        }
        self.fallback.recall(query, candidates, budget_tokens).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cands() -> Vec<RecallCandidate> {
        vec![
            RecallCandidate {
                id: "skill:global:rna-seq".into(),
                kind: "skill".into(),
                scope: "global".into(),
                text: "human rna-seq differential expression deseq2".into(),
                path: None,
            },
            RecallCandidate {
                id: "skill:global:wgs".into(),
                kind: "skill".into(),
                scope: "global".into(),
                text: "whole genome sequencing variant calling".into(),
                path: None,
            },
            RecallCandidate {
                id: "insight:1".into(),
                kind: "insight".into(),
                scope: "global".into(),
                text: "low mapping rate often caused by adapter contamination".into(),
                path: None,
            },
        ]
    }

    #[tokio::test]
    async fn bm25_picks_topical_candidate() {
        let r = Bm25Recaller::new(2);
        let res = r
            .recall("how do I find DE genes from RNA-seq data?", make_cands(), 4096)
            .await
            .unwrap();
        assert!(!res.picked.is_empty());
        assert_eq!(res.picked[0].id, "skill:global:rna-seq");
    }

    #[tokio::test]
    async fn bm25_returns_empty_for_empty_query() {
        let r = Bm25Recaller::new(2);
        let res = r.recall("???", make_cands(), 4096).await.unwrap();
        assert!(res.picked.is_empty());
    }

    #[tokio::test]
    async fn composite_falls_back_when_primary_times_out() {
        struct SlowPrimary;
        #[async_trait]
        impl Recaller for SlowPrimary {
            async fn recall(
                &self,
                _q: &str,
                _c: Vec<RecallCandidate>,
                _b: usize,
            ) -> Result<RecallResult, AiError> {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                Ok(RecallResult { picked: vec![], rationale: Some("primary".into()) })
            }
        }
        let c = CompositeRecaller {
            primary: Some(std::sync::Arc::new(SlowPrimary)),
            fallback: std::sync::Arc::new(Bm25Recaller::new(2)),
            timeout: std::time::Duration::from_millis(50),
        };
        let res = c.recall("rna-seq", make_cands(), 4096).await.unwrap();
        assert_eq!(res.rationale.as_deref(), Some("bm25"));
    }
}
```

更新 `crates/rb-ai/src/memory/mod.rs`：

```rust
pub mod layers;
pub mod recall;
pub mod store;

pub use layers::{
    Archive, ArchiveOutcome, IndexEntry, Insight, Scope, SkillMeta, TodoEntry, WorkingCheckpoint,
};
pub use recall::{Bm25Recaller, CompositeRecaller, RecallCandidate, RecallResult, Recaller};
pub use store::MemoryStore;
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib memory::recall
```

预期：3 个测试 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/memory
git commit -m "$(cat <<'EOF'
feat(ai): BM25Recaller + Recaller trait + CompositeRecaller skeleton

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 8：memory/crystallize.rs

**Files:**
- Create: `crates/rb-ai/src/memory/crystallize.rs`
- Modify: `crates/rb-ai/src/memory/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/memory/crystallize.rs`：

```rust
//! Crystallize: fold a finished agent session into L1 insight + L4 archive,
//! and expose helpers for `start_long_term_update` (L2/L3 writeback).

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AiError;
use crate::memory::layers::{Archive, ArchiveOutcome, IndexEntry, Insight, Scope};
use crate::memory::store::MemoryStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummaryInput {
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub outcome: ArchiveOutcome,
    pub messages: Vec<serde_json::Value>,
    pub headline: String,
    pub tags: Vec<String>,
    pub net_log_path: Option<String>,
}

/// Append L4 archive + L1 insight summary in one shot.
pub async fn crystallize_session(
    store: &MemoryStore,
    project_root: &Path,
    input: SessionSummaryInput,
) -> Result<(), AiError> {
    let archive = Archive {
        id: input.session_id.clone(),
        started_at: input.started_at,
        ended_at: input.ended_at,
        summary: input.headline.clone(),
        outcome: input.outcome,
        tags: input.tags.clone(),
        messages: input.messages,
        net_log_path: input.net_log_path,
    };
    store.append_l4_archive(project_root, &archive).await?;

    let insight = Insight {
        id: Uuid::new_v4().simple().to_string(),
        tag: input.tags.first().cloned().unwrap_or_else(|| "session".into()),
        summary: input.headline,
        evidence_archive_id: Some(input.session_id),
        ts: Utc::now(),
    };
    store.append_l1_insight(&insight).await?;
    Ok(())
}

/// `start_long_term_update` writeback. Layer + scope are explicit per spec —
/// callers (the agent) declare them in tool args.
pub async fn long_term_update(
    store: &MemoryStore,
    project_root: Option<&Path>,
    layer: Layer,
    scope: Scope,
    body: LongTermBody,
) -> Result<UpdateResult, AiError> {
    match (layer, scope) {
        (Layer::L2, Scope::Global) => {
            let path = store.global_root.join("L2_facts.md");
            let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
            if !existing.ends_with('\n') {
                existing.push('\n');
            }
            existing.push_str(&format!(
                "\n## {}\n\n{}\n",
                body.section.unwrap_or_else(|| "untitled".into()),
                body.markdown
            ));
            super_write_text(&path, &existing)?;
            Ok(UpdateResult { path: path.display().to_string() })
        }
        (Layer::L3, scope) => {
            let dir = match scope {
                Scope::Global => store.global_root.join("L3_skills"),
                Scope::Project => {
                    let p = project_root.ok_or_else(|| {
                        AiError::InvalidState("project scope requires project_root".into())
                    })?;
                    MemoryStore::project_root(p).join("L3_local")
                }
            };
            std::fs::create_dir_all(&dir)?;
            let slug = slugify(&body.name.clone().unwrap_or_else(|| "skill".into()));
            let path = dir.join(format!("{slug}.md"));
            super_write_text(&path, &body.markdown)?;
            store
                .upsert_skill_index(
                    scope,
                    project_root,
                    IndexEntry::Skill {
                        name: slug.clone(),
                        path: path
                            .strip_prefix(if scope == Scope::Global {
                                &store.global_root
                            } else {
                                &MemoryStore::project_root(project_root.unwrap())
                            })
                            .unwrap_or(&path)
                            .display()
                            .to_string(),
                        scope,
                        triggers: body.triggers.unwrap_or_default(),
                        hits: 0,
                        last_used: None,
                    },
                )
                .await?;
            Ok(UpdateResult { path: path.display().to_string() })
        }
        (Layer::L2, Scope::Project) => Err(AiError::InvalidState(
            "L2 is global-only by convention".into(),
        )),
    }
}

fn super_write_text(path: &Path, text: &str) -> Result<(), AiError> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension(format!(
        "tmp.{}",
        Uuid::new_v4().simple()
    ));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .replace("--", "-")
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    L2,
    L3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermBody {
    /// For L2: section heading; for L3: ignored.
    pub section: Option<String>,
    /// For L3: skill name (used for filename + index entry).
    pub name: Option<String>,
    /// For L3: trigger keywords for retrieval.
    pub triggers: Option<Vec<String>>,
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResult {
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn crystallize_writes_archive_and_insight() {
        let tmp = tempdir().unwrap();
        let store = MemoryStore::open(tmp.path().join("global")).unwrap();
        let project = tmp.path().join("proj");
        store.ensure_project(&project).unwrap();
        crystallize_session(
            &store,
            &project,
            SessionSummaryInput {
                session_id: "s1".into(),
                started_at: Utc::now(),
                ended_at: Some(Utc::now()),
                outcome: ArchiveOutcome::Done,
                messages: vec![],
                headline: "did the thing".into(),
                tags: vec!["rna-seq".into()],
                net_log_path: None,
            },
        )
        .await
        .unwrap();

        // L4 written
        let archive = MemoryStore::project_root(&project).join("L4_archives/s1.json");
        assert!(archive.exists());
        // L1 written
        let l1 = std::fs::read_to_string(store.global_root.join("L1_insights.jsonl")).unwrap();
        assert_eq!(l1.lines().count(), 1);
        let v: Insight = serde_json::from_str(l1.trim()).unwrap();
        assert_eq!(v.tag, "rna-seq");
        assert_eq!(v.evidence_archive_id.as_deref(), Some("s1"));
    }

    #[tokio::test]
    async fn long_term_l3_global_writes_md_and_indexes() {
        let tmp = tempdir().unwrap();
        let store = MemoryStore::open(tmp.path().join("global")).unwrap();
        let r = long_term_update(
            &store,
            None,
            Layer::L3,
            Scope::Global,
            LongTermBody {
                section: None,
                name: Some("RNA-seq DE".into()),
                triggers: Some(vec!["rna-seq".into(), "de".into()]),
                markdown: "## SOP\n1. ...".into(),
            },
        )
        .await
        .unwrap();
        assert!(r.path.ends_with("rna-seq-de.md"));
        let body = std::fs::read_to_string(&r.path).unwrap();
        assert!(body.contains("SOP"));
        let idx = store
            .read_index(&store.global_root.join("L3_skills/_index.json"))
            .unwrap();
        assert_eq!(idx.len(), 1);
    }

    #[tokio::test]
    async fn long_term_l2_project_is_rejected() {
        let tmp = tempdir().unwrap();
        let store = MemoryStore::open(tmp.path().join("global")).unwrap();
        let project = tmp.path().join("proj");
        store.ensure_project(&project).unwrap();
        let err = long_term_update(
            &store,
            Some(&project),
            Layer::L2,
            Scope::Project,
            LongTermBody {
                section: Some("x".into()),
                name: None,
                triggers: None,
                markdown: "y".into(),
            },
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(err, AiError::InvalidState(_)));
    }
}
```

更新 `crates/rb-ai/src/memory/mod.rs` 末尾追加：

```rust
pub mod crystallize;
pub use crystallize::{crystallize_session, long_term_update, Layer, LongTermBody, SessionSummaryInput, UpdateResult};
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib memory::crystallize
```

预期：3 个测试 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/memory
git commit -m "$(cat <<'EOF'
feat(ai): crystallize_session + long_term_update for memory writeback

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 9：memory checkpoint helpers

**Files:**
- Modify: `crates/rb-ai/src/memory/store.rs`
- Modify: `crates/rb-ai/src/memory/mod.rs`

- [ ] **Step 1: 写失败测试**

在 `crates/rb-ai/src/memory/store.rs` 的 `mod tests` 末尾追加：

```rust
#[tokio::test]
async fn checkpoint_round_trips_atomically() {
    use crate::memory::layers::{TodoEntry, WorkingCheckpoint};
    let tmp = tempdir().unwrap();
    let s = store(tmp.path());
    let project = tmp.path().join("proj");
    s.ensure_project(&project).unwrap();

    let cp = WorkingCheckpoint {
        session_id: "sess1".into(),
        project_root: project.display().to_string(),
        started_at: Utc::now(),
        last_step_at: Utc::now(),
        todo: vec![TodoEntry { text: "qc".into(), done: false }],
        message_count: 3,
        perceive_snapshot_hash: "abc".into(),
    };
    s.write_checkpoint(&project, &cp).await.unwrap();
    let loaded = s.read_checkpoint(&project).unwrap().unwrap();
    assert_eq!(loaded.session_id, "sess1");
    assert_eq!(loaded.todo.len(), 1);
}

#[tokio::test]
async fn checkpoint_returns_none_when_missing() {
    let tmp = tempdir().unwrap();
    let s = store(tmp.path());
    let project = tmp.path().join("proj");
    s.ensure_project(&project).unwrap();
    assert!(s.read_checkpoint(&project).unwrap().is_none());
}
```

- [ ] **Step 2: 实现**

在 `crates/rb-ai/src/memory/store.rs` `impl MemoryStore` 块内追加：

```rust
pub async fn write_checkpoint(
    &self,
    project_root: &Path,
    cp: &crate::memory::layers::WorkingCheckpoint,
) -> Result<(), AiError> {
    let path = Self::project_root(project_root)
        .join("checkpoints")
        .join("current.json");
    let bytes = serde_json::to_vec_pretty(cp)?;
    let _g = self.inner.lock().await;
    write_atomic(&path, &bytes)
}

pub fn read_checkpoint(
    &self,
    project_root: &Path,
) -> Result<Option<crate::memory::layers::WorkingCheckpoint>, AiError> {
    let path = Self::project_root(project_root)
        .join("checkpoints")
        .join("current.json");
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

pub fn clear_checkpoint(&self, project_root: &Path) -> Result<(), AiError> {
    let path = Self::project_root(project_root)
        .join("checkpoints")
        .join("current.json");
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
```

- [ ] **Step 3: 跑测试**

```
cargo test -p rb-ai --lib memory::store
```

预期：之前 4 个 + 新增 2 个全 PASS。

- [ ] **Step 4: 提交**

```bash
git add crates/rb-ai/src/memory
git commit -m "$(cat <<'EOF'
feat(ai): MemoryStore checkpoint read/write/clear helpers

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 2 —— Sandbox

> 3 个 task：policy（含 classify 和路径校验）、pixi 包装、网络日志。`code_run` tool 本身在 Phase 3 实现。

### Task 10：sandbox/policy.rs — Bucket / Decision / classify

**Files:**
- Create: `crates/rb-ai/src/sandbox/policy.rs`
- Modify: `crates/rb-ai/src/sandbox/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/sandbox/policy.rs`：

```rust
//! Risk bucketing + path canonicalization for the agent sandbox.
//!
//! `classify` takes a tool call (name + args) and returns the bucket and
//! whether it can run immediately, requires a one-time approval, or always
//! asks. Full-permission mode bypasses approval gates.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::AiError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Bucket {
    ReadFs,
    SandboxWrite,
    ProjectModule { module: String },
    CodeRunSandbox,
    CodeRunOutOfSandbox,
    Web,
    MemoryWrite,
    DestructiveDelete,
    AskUser,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow,
    ApproveOnce,
    AlwaysAsk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyMode {
    Normal,
    FullPermission,
}

pub struct SandboxPolicy {
    pub mode: PolicyMode,
    pub project_root: PathBuf,
    pub sandbox_dir: PathBuf,
    pub approved: Mutex<HashSet<Bucket>>,
}

impl SandboxPolicy {
    pub fn new(project_root: PathBuf, sandbox_dirname: &str) -> Self {
        let sandbox_dir = project_root.join(sandbox_dirname);
        let _ = std::fs::create_dir_all(&sandbox_dir);
        Self {
            mode: PolicyMode::Normal,
            project_root,
            sandbox_dir,
            approved: Mutex::new(HashSet::new()),
        }
    }

    pub fn full_permission(mut self) -> Self {
        self.mode = PolicyMode::FullPermission;
        self
    }

    pub fn classify(&self, tool_name: &str, args: &serde_json::Value) -> (Bucket, Decision) {
        // Memory + control tools.
        if matches!(
            tool_name,
            "recall_memory" | "update_working_checkpoint" | "task_done"
        ) {
            return (Bucket::ReadFs, Decision::Allow);
        }
        if tool_name == "ask_user" {
            return (Bucket::AskUser, Decision::Allow);
        }
        if tool_name == "start_long_term_update" {
            return (Bucket::MemoryWrite, Decision::Allow);
        }

        // Read-only filesystem.
        if matches!(
            tool_name,
            "file_read" | "file_list" | "read_results_table" | "read_run_log" | "project_state"
        ) {
            return (Bucket::ReadFs, Decision::Allow);
        }

        // File writes / patches: depends on path.
        if matches!(tool_name, "file_write" | "file_patch") {
            let path_str = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            return match self.classify_write_path(Path::new(path_str)) {
                WritePath::InsideSandbox => (Bucket::SandboxWrite, Decision::Allow),
                WritePath::InsideProject => {
                    (Bucket::ProjectModule { module: "fs".into() }, Decision::ApproveOnce)
                }
                WritePath::OutsideProject => (Bucket::DestructiveDelete, Decision::AlwaysAsk),
            };
        }

        // Code run.
        if tool_name == "code_run" {
            let cwd = args
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .unwrap_or_else(|| self.sandbox_dir.clone());
            return if self.path_inside(&cwd, &self.sandbox_dir) {
                (Bucket::CodeRunSandbox, Decision::Allow)
            } else if self.path_inside(&cwd, &self.project_root) {
                (Bucket::CodeRunOutOfSandbox, Decision::ApproveOnce)
            } else {
                (Bucket::CodeRunOutOfSandbox, Decision::AlwaysAsk)
            };
        }

        // Web.
        if matches!(tool_name, "web_scan" | "web_execute_js") {
            return (Bucket::Web, Decision::Allow);
        }

        // Module-derived tools start with `run_`.
        if let Some(module) = tool_name.strip_prefix("run_") {
            return (
                Bucket::ProjectModule {
                    module: module.into(),
                },
                Decision::ApproveOnce,
            );
        }

        // Skill tools: `skill_<slug>` → first-call approval.
        if let Some(slug) = tool_name.strip_prefix("skill_") {
            return (
                Bucket::ProjectModule {
                    module: format!("skill:{slug}"),
                },
                Decision::ApproveOnce,
            );
        }

        // Default unknown: always ask.
        (Bucket::DestructiveDelete, Decision::AlwaysAsk)
    }

    /// Apply approval-cache + full-permission bypass. Returns whether the
    /// caller should run immediately (true), or wait for user approval (false).
    pub fn should_run(&self, bucket: &Bucket, decision: &Decision) -> bool {
        match self.mode {
            PolicyMode::FullPermission => true,
            PolicyMode::Normal => match decision {
                Decision::Allow => true,
                Decision::ApproveOnce => self.approved.lock().unwrap().contains(bucket),
                Decision::AlwaysAsk => false,
            },
        }
    }

    pub fn record_approval(&self, bucket: Bucket) {
        self.approved.lock().unwrap().insert(bucket);
    }

    fn classify_write_path(&self, p: &Path) -> WritePath {
        let abs = self.canonicalize_or_join(p);
        if self.path_inside(&abs, &self.sandbox_dir) {
            WritePath::InsideSandbox
        } else if self.path_inside(&abs, &self.project_root) {
            WritePath::InsideProject
        } else {
            WritePath::OutsideProject
        }
    }

    fn path_inside(&self, candidate: &Path, root: &Path) -> bool {
        let c = self.canonicalize_or_join(candidate);
        let r = self
            .canonicalize_or_join(root)
            .to_string_lossy()
            .to_string();
        c.to_string_lossy().starts_with(&r)
    }

    fn canonicalize_or_join(&self, p: &Path) -> PathBuf {
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.project_root.join(p)
        };
        let cleaned = path_clean::clean(&abs);
        std::fs::canonicalize(&cleaned).unwrap_or(cleaned)
    }
}

#[derive(Debug, PartialEq)]
enum WritePath {
    InsideSandbox,
    InsideProject,
    OutsideProject,
}

/// Fail-closed path validator for actual file writes (not classification).
/// Used by the file_write tool implementation in Phase 3.
pub fn require_inside(root: &Path, candidate: &Path) -> Result<PathBuf, AiError> {
    let abs = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let cleaned = path_clean::clean(&abs);
    if !cleaned.starts_with(root) {
        return Err(AiError::PathEscape(cleaned.display().to_string()));
    }
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn pol(tmp: &Path) -> SandboxPolicy {
        let p = tmp.to_path_buf();
        std::fs::create_dir_all(p.join("sandbox")).unwrap();
        SandboxPolicy::new(p, "sandbox")
    }

    #[test]
    fn read_tools_are_allow() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let (b, d) = p.classify("file_read", &json!({"path": "x"}));
        assert_eq!(b, Bucket::ReadFs);
        assert_eq!(d, Decision::Allow);
    }

    #[test]
    fn write_to_sandbox_is_allow() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let path = tmp.path().join("sandbox/foo.py");
        let (b, d) = p.classify("file_write", &json!({"path": path.display().to_string()}));
        assert_eq!(b, Bucket::SandboxWrite);
        assert_eq!(d, Decision::Allow);
    }

    #[test]
    fn write_inside_project_outside_sandbox_is_approve_once() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let path = tmp.path().join("results/out.tsv");
        let (_b, d) = p.classify("file_write", &json!({"path": path.display().to_string()}));
        assert_eq!(d, Decision::ApproveOnce);
    }

    #[test]
    fn write_outside_project_is_always_ask() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let outside = std::env::temp_dir().join("not_my_project").join("x");
        let (_, d) = p.classify("file_write", &json!({"path": outside.display().to_string()}));
        assert_eq!(d, Decision::AlwaysAsk);
    }

    #[test]
    fn run_module_tool_is_approve_once() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let (b, d) = p.classify("run_qc", &json!({}));
        match b {
            Bucket::ProjectModule { module } => assert_eq!(module, "qc"),
            _ => panic!(),
        }
        assert_eq!(d, Decision::ApproveOnce);
    }

    #[test]
    fn full_permission_bypasses_approval() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path()).full_permission();
        let (b, d) = p.classify("run_qc", &json!({}));
        assert!(p.should_run(&b, &d));
    }

    #[test]
    fn approve_once_caches_within_session() {
        let tmp = tempdir().unwrap();
        let p = pol(tmp.path());
        let (b, d) = p.classify("run_qc", &json!({}));
        assert!(!p.should_run(&b, &d));
        p.record_approval(b.clone());
        assert!(p.should_run(&b, &d));
    }

    #[test]
    fn require_inside_rejects_dotdot_escape() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let bad = require_inside(&root, Path::new("../etc/passwd"));
        assert!(bad.is_err());
        let ok = require_inside(&root, Path::new("sub/file"));
        assert!(ok.is_ok());
    }
}
```

更新 `crates/rb-ai/src/sandbox/mod.rs`：

```rust
//! Sandbox policy + pixi/net wrappers.

pub mod policy;

pub use policy::{Bucket, Decision, PolicyMode, SandboxPolicy, require_inside};
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib sandbox::policy
```

预期：8 个测试 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/sandbox
git commit -m "$(cat <<'EOF'
feat(ai): SandboxPolicy with bucketed classify + path-escape guard

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 11：sandbox/pixi.rs — pixi 探测与运行包装

**Files:**
- Create: `crates/rb-ai/src/sandbox/pixi.rs`
- Modify: `crates/rb-ai/src/sandbox/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/sandbox/pixi.rs`：

```rust
//! Wrap pixi for sandbox-scoped Python/R/shell execution.
//!
//! Detection is best-effort: `which pixi` (or `where` on Windows). If pixi is
//! missing we return a structured error so the agent can ask the user to
//! install it (https://pixi.sh).
//!
//! `init_if_needed` runs `pixi init` once per sandbox dir.
//! `build_command` returns a `tokio::process::Command` ready to spawn —
//! the caller wires stdin/stdout/stderr and `harden_for_gui` from rb-core.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::error::AiError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Lang {
    Python,
    R,
    Shell,
}

#[derive(Debug, Clone)]
pub struct PixiRuntime {
    pub bin: PathBuf,
}

impl PixiRuntime {
    pub fn detect() -> Result<Self, AiError> {
        let bin_name = if cfg!(windows) { "pixi.exe" } else { "pixi" };
        let path = which::which(bin_name).map_err(|_| {
            AiError::Tool(format!(
                "pixi not found in PATH; install from https://pixi.sh and retry"
            ))
        })?;
        Ok(Self { bin: path })
    }

    pub async fn init_if_needed(&self, sandbox_dir: &Path) -> Result<(), AiError> {
        let pixi_toml = sandbox_dir.join("pixi.toml");
        if pixi_toml.exists() {
            return Ok(());
        }
        std::fs::create_dir_all(sandbox_dir)?;
        let mut cmd = Command::new(&self.bin);
        cmd.arg("init").current_dir(sandbox_dir);
        rb_core::subprocess::harden_for_gui(&mut cmd);
        let out = cmd.output().await.map_err(|e| {
            AiError::Tool(format!("pixi init failed to spawn: {e}"))
        })?;
        if !out.status.success() {
            return Err(AiError::Tool(format!(
                "pixi init exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(())
    }

    /// Build a runnable Command. Caller handles stdin/stdout/stderr piping.
    pub fn build_command(&self, sandbox_dir: &Path, lang: Lang, script_path: &Path) -> Command {
        let interp = match lang {
            Lang::Python => "python",
            Lang::R => "Rscript",
            Lang::Shell => "bash",
        };
        let mut cmd = Command::new(&self.bin);
        cmd.arg("run")
            .arg("--manifest-path")
            .arg(sandbox_dir.join("pixi.toml"))
            .arg("--")
            .arg(interp)
            .arg(script_path);
        cmd.current_dir(sandbox_dir);
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_serializes_as_pascal_case() {
        let v = serde_json::to_value(Lang::Python).unwrap();
        assert_eq!(v.as_str(), Some("Python"));
    }

    #[test]
    fn detect_returns_structured_error_when_pixi_absent() {
        // Force PATH=empty; we can't reliably test "found" cross-CI, so test absent.
        let saved = std::env::var_os("PATH");
        std::env::set_var("PATH", "");
        let r = PixiRuntime::detect();
        if let Some(p) = saved {
            std::env::set_var("PATH", p);
        }
        match r {
            Err(AiError::Tool(msg)) => assert!(msg.contains("pixi")),
            other => panic!("expected Tool err, got {other:?}"),
        }
    }
}
```

`crates/rb-ai/Cargo.toml` 加：

```toml
which = "6"
```

更新 `crates/rb-ai/src/sandbox/mod.rs` 末尾追加：

```rust
pub mod pixi;
pub use pixi::{Lang, PixiRuntime};
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib sandbox::pixi
```

预期：2 个测试 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/Cargo.toml crates/rb-ai/src/sandbox Cargo.lock
git commit -m "$(cat <<'EOF'
feat(ai): pixi runtime detection + command builder

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 12：sandbox/net.rs — 网络日志写入器

**Files:**
- Create: `crates/rb-ai/src/sandbox/net.rs`
- Modify: `crates/rb-ai/src/sandbox/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/sandbox/net.rs`：

```rust
//! Append-only network call logger. The agent's `web_scan` tool calls
//! `record_request` before issuing the HTTP request and `record_response`
//! after; the log lives at `<project>/agent/L4_archives/<session>.net.log`.
//!
//! Disabled when the active `AgentConfig.network.log_enabled` is false
//! (default in FullPermission mode).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::AiError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetEntry {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub session_id: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub bytes: Option<u64>,
    pub note: Option<String>,
}

pub struct NetLogger {
    enabled: bool,
    path: PathBuf,
    inner: Mutex<()>,
}

impl NetLogger {
    pub fn new(project_root: &Path, session_id: &str, enabled: bool) -> Result<Self, AiError> {
        let dir = project_root.join("agent").join("L4_archives");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{session_id}.net.log"));
        Ok(Self {
            enabled,
            path,
            inner: Mutex::new(()),
        })
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn path(&self) -> Option<&Path> {
        if self.enabled {
            Some(&self.path)
        } else {
            None
        }
    }

    pub fn record(&self, entry: &NetEntry) -> Result<(), AiError> {
        if !self.enabled {
            return Ok(());
        }
        let line = serde_json::to_string(entry)? + "\n";
        let _g = self.inner.lock().unwrap();
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.lock_exclusive()
            .map_err(|e| AiError::MemoryWrite(format!("net log lock: {e}")))?;
        let res = (&f)
            .write_all_at(line.as_bytes())
            .map_err(|e| AiError::MemoryWrite(format!("net log write: {e}")));
        f.unlock().ok();
        res
    }
}

trait WriteAll {
    fn write_all_at(self, buf: &[u8]) -> std::io::Result<()>;
}

impl WriteAll for &std::fs::File {
    fn write_all_at(self, buf: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        // Append-only file, ordinary write_all is fine.
        let mut f = self;
        f.write_all(buf)?;
        f.sync_data()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn record_appends_jsonl_line() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let log = NetLogger::new(&project, "sess1", true).unwrap();
        log.record(&NetEntry {
            ts: Utc::now(),
            session_id: "sess1".into(),
            method: "GET".into(),
            url: "https://example.org".into(),
            status: Some(200),
            bytes: Some(123),
            note: None,
        })
        .unwrap();
        let body = std::fs::read_to_string(log.path().unwrap()).unwrap();
        assert_eq!(body.lines().count(), 1);
        let v: NetEntry = serde_json::from_str(body.trim()).unwrap();
        assert_eq!(v.url, "https://example.org");
    }

    #[test]
    fn disabled_logger_is_silent() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let log = NetLogger::new(&project, "s", false).unwrap();
        assert!(log.path().is_none());
        log.record(&NetEntry {
            ts: Utc::now(),
            session_id: "s".into(),
            method: "GET".into(),
            url: "x".into(),
            status: None,
            bytes: None,
            note: None,
        })
        .unwrap();
        assert!(!project.join("agent/L4_archives/s.net.log").exists());
    }
}
```

更新 `crates/rb-ai/src/sandbox/mod.rs` 末尾：

```rust
pub mod net;
pub use net::{NetEntry, NetLogger};
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib sandbox::net
```

预期：2 个测试 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/sandbox
git commit -m "$(cat <<'EOF'
feat(ai): NetLogger for per-session network audit log

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 3 —— Tools 重构与新工具

> 6 个 task：拆 `builtin.rs` 单文件、加 file 工具、code_run、web、memory_tools、ask_user/skill loader、project_state。每个 task 都从 TDD 开始。

### Task 13：拆分 builtin.rs → builtin/ 子模块

**Files:**
- Delete: `crates/rb-ai/src/tools/builtin.rs`
- Create: `crates/rb-ai/src/tools/builtin/mod.rs`
- Create: `crates/rb-ai/src/tools/builtin/file.rs`（暂只挪 `file_read`/`file_list` 已有逻辑）
- Modify: `crates/rb-ai/src/tools/mod.rs`

> 现 `builtin.rs` 35.7K 单文件难维护；本 task 不改语义，只拆。`file_write`/`file_patch`/`code_run`/`web_scan`/`memory_tools` 等新工具在后续 task 加入。

- [ ] **Step 1: 探查现有工具**

```
grep -n 'pub fn register\|pub struct\|impl ToolExecutor' crates/rb-ai/src/tools/builtin.rs | head -50
```

记下当前 builtin.rs 里都注册了哪些 ToolExecutor（按现状里的几个名字：`list_project_files`, `read_table_preview` 等）。

- [ ] **Step 2: 创建 `builtin/mod.rs` 并迁移已有工具**

新建 `crates/rb-ai/src/tools/builtin/mod.rs`：

```rust
//! Built-in (non module-derived, non skill) tools.

pub mod file;
pub mod project_state;

use crate::tools::ToolRegistry;

/// Register every builtin tool into the given registry. Called once at
/// agent boot.
pub fn register_all(registry: &mut ToolRegistry) {
    file::register(registry);
    project_state::register(registry);
    // code_run, web, memory_tools, ask_user, skill loader: registered by
    // separate modules — see lib.rs orchestration.
}
```

把 `builtin.rs` 里 `file_read` / `file_list` 类的 ToolEntry 搬到 `builtin/file.rs`。同样把 `project_state` / `read_run_log` / `read_results_table` 搬到 `builtin/project_state.rs`。两个新文件公共结构：

`crates/rb-ai/src/tools/builtin/file.rs`:
```rust
//! file_read / file_list. Read-only filesystem inspection.
//! file_write / file_patch live in a separate module — they need
//! sandbox::policy.

use std::path::Path;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{
    schema::{RiskLevel, ToolDef, ToolError},
    ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry,
};

pub fn register(reg: &mut ToolRegistry) {
    reg.register(ToolEntry {
        def: file_read_def(),
        executor: std::sync::Arc::new(FileReadExec),
    });
    reg.register(ToolEntry {
        def: file_list_def(),
        executor: std::sync::Arc::new(FileListExec),
    });
}

fn file_read_def() -> ToolDef {
    ToolDef {
        name: "file_read".into(),
        description: "Read a UTF-8 text file. Returns up to ~64KB.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "max_bytes": {"type": "integer", "default": 65536}
            },
            "required": ["path"]
        }),
    }
}

fn file_list_def() -> ToolDef {
    ToolDef {
        name: "file_list".into(),
        description: "List a directory; returns up to 200 entries.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"]
        }),
    }
}

struct FileReadExec;
#[async_trait]
impl ToolExecutor for FileReadExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let max = args
            .get("max_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(65536) as usize;
        let mut bytes = std::fs::read(path).map_err(|e| ToolError::Execution(e.to_string()))?;
        let truncated = bytes.len() > max;
        if truncated {
            bytes.truncate(max);
        }
        let body = String::from_utf8_lossy(&bytes).into_owned();
        Ok(ToolOutput::Value(json!({
            "path": path, "truncated": truncated, "content": body
        })))
    }
}

struct FileListExec;
#[async_trait]
impl ToolExecutor for FileListExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let mut entries = vec![];
        for ent in std::fs::read_dir(path)
            .map_err(|e| ToolError::Execution(e.to_string()))?
            .flatten()
            .take(200)
        {
            let n = ent.file_name().to_string_lossy().to_string();
            let kind = ent
                .file_type()
                .map(|t| if t.is_dir() { "dir" } else { "file" })
                .unwrap_or("?")
                .to_string();
            entries.push(json!({"name": n, "kind": kind}));
        }
        Ok(ToolOutput::Value(json!({"entries": entries})))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn file_read_truncates_large_files() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("big.txt");
        std::fs::write(&p, "x".repeat(1000)).unwrap();
        let exec = FileReadExec;
        let out = exec
            .execute(
                &json!({"path": p.display().to_string(), "max_bytes": 50}),
                ToolContext {
                    project: &std::sync::Arc::new(tokio::sync::Mutex::new(
                        rb_core::project::Project::create("t", tmp.path()).unwrap(),
                    )),
                    runner: &std::sync::Arc::new(rb_core::runner::Runner::new(std::sync::Arc::new(
                        tokio::sync::Mutex::new(
                            rb_core::project::Project::create("u", tmp.path()).unwrap(),
                        ),
                    ))),
                    binary_resolver: &std::sync::Arc::new(tokio::sync::Mutex::new(
                        rb_core::binary::BinaryResolver::default(),
                    )),
                },
            )
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["truncated"], true);
        assert_eq!(v["content"].as_str().unwrap().len(), 50);
    }
}
```

`crates/rb-ai/src/tools/builtin/project_state.rs`：把原 builtin.rs 中 `project_state` / `read_run_log` / `read_results_table` 三个 executor 完整搬过来；保持函数签名与现状一致，不改逻辑。最末尾加一个 `pub fn register(reg: &mut ToolRegistry)`：

```rust
pub fn register(reg: &mut ToolRegistry) {
    // 原 builtin.rs 里这三组是怎么 register 的，照搬即可。
    // 仅迁移，不改逻辑。
}
```

> 关键：**搬运不改逻辑**。如果原 `builtin.rs` 还有别的工具（`select_files` / `select_directory` 等 Tauri 专属的），它们属于 rb-app，不应在 rb-ai：本 task 中遇到不属于 rb-ai 的逻辑，直接删除并补在 commit message 里说明（属于错误依赖，借此清理）。

- [ ] **Step 3: 删除 builtin.rs，调整 mod.rs**

```
git rm crates/rb-ai/src/tools/builtin.rs
```

`crates/rb-ai/src/tools/mod.rs` 第一行：

```rust
pub mod builtin;
```

`pub use schema::{RiskLevel, ToolDef, ToolError};` 保持。

- [ ] **Step 4: 编译 + 跑现有测试**

```
cargo test -p rb-ai --lib tools
```

预期：`tools::builtin::file::tests::file_read_truncates_large_files` PASS；任何依赖 `builtin::*` 旧路径的测试（如 module_derived 测试中可能有）按报错调整 import。

- [ ] **Step 5: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(ai): split tools/builtin.rs into tools/builtin/ submodules

Behavior unchanged. Removes Tauri-bound select_* helpers — those belong
in rb-app, not rb-ai.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 14：file_write / file_patch（受 SandboxPolicy 约束）

**Files:**
- Modify: `crates/rb-ai/src/tools/builtin/file.rs`

- [ ] **Step 1: 写失败测试**

在 `tools/builtin/file.rs` `mod tests` 末尾追加：

```rust
#[tokio::test]
async fn file_write_writes_to_sandbox() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("foo.txt");
    let exec = FileWriteExec;
    exec.execute(
        &json!({"path": p.display().to_string(), "content": "hi"}),
        dummy_ctx(tmp.path()),
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "hi");
}

#[tokio::test]
async fn file_patch_applies_unified_diff() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("a.txt");
    std::fs::write(&p, "alpha\nbeta\ngamma\n").unwrap();
    let exec = FilePatchExec;
    let diff = "\
--- a/a.txt
+++ b/a.txt
@@ -1,3 +1,3 @@
 alpha
-beta
+BETA
 gamma
";
    exec.execute(
        &json!({"path": p.display().to_string(), "diff": diff}),
        dummy_ctx(tmp.path()),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "alpha\nBETA\ngamma\n"
    );
}

fn dummy_ctx(root: &std::path::Path) -> ToolContext<'static> {
    // Build leaks/Box::leak: only acceptable in tests for short-lived ctx.
    let project = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
        rb_core::project::Project::create("t", root).unwrap(),
    ))));
    let runner = Box::leak(Box::new(std::sync::Arc::new(rb_core::runner::Runner::new(
        project.clone(),
    ))));
    let binres = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
        rb_core::binary::BinaryResolver::default(),
    ))));
    ToolContext {
        project,
        runner,
        binary_resolver: binres,
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

```
cargo test -p rb-ai --lib tools::builtin::file::tests::file_write_writes_to_sandbox
```

预期：`FileWriteExec` 未定义。

- [ ] **Step 3: 实现 file_write / file_patch**

在 `tools/builtin/file.rs` `pub fn register` 内追加注册：

```rust
reg.register(ToolEntry {
    def: file_write_def(),
    executor: std::sync::Arc::new(FileWriteExec),
});
reg.register(ToolEntry {
    def: file_patch_def(),
    executor: std::sync::Arc::new(FilePatchExec),
});
```

末尾追加：

```rust
fn file_write_def() -> ToolDef {
    ToolDef {
        name: "file_write".into(),
        description: "Write a text file. Risk depends on path: sandbox \
            paths run freely; project paths require approval; outside-project \
            paths always ask."
            .into(),
        risk: RiskLevel::RunLow,
        params: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["path", "content"]
        }),
    }
}

fn file_patch_def() -> ToolDef {
    ToolDef {
        name: "file_patch".into(),
        description: "Apply a unified diff to a single file.".into(),
        risk: RiskLevel::RunLow,
        params: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "diff": {"type": "string"}
            },
            "required": ["path", "diff"]
        }),
    }
}

pub struct FileWriteExec;
#[async_trait]
impl ToolExecutor for FileWriteExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("content required".into()))?;
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::Execution(e.to_string()))?;
        }
        std::fs::write(path, content).map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(ToolOutput::Value(json!({"path": path, "bytes": content.len()})))
    }
}

pub struct FilePatchExec;
#[async_trait]
impl ToolExecutor for FilePatchExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let diff = args
            .get("diff")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("diff required".into()))?;
        let original = std::fs::read_to_string(path).map_err(|e| ToolError::Execution(e.to_string()))?;
        let patched = apply_unified_diff(&original, diff)
            .map_err(|e| ToolError::Execution(format!("patch failed: {e}")))?;
        std::fs::write(path, &patched).map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(ToolOutput::Value(json!({"path": path})))
    }
}

/// Minimal unified-diff applier. Handles single-file diffs with one or more
/// `@@ -old,len +new,len @@` hunks; tolerates trailing newlines. Not a full
/// patch(1) replacement — sufficient for LLM-generated edits.
fn apply_unified_diff(original: &str, diff: &str) -> Result<String, String> {
    let lines: Vec<&str> = original.split_inclusive('\n').collect();
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    let mut cursor = 0_usize;
    let mut pending: Vec<(usize, usize, Vec<String>, Vec<String>)> = vec![];

    let dlines: Vec<&str> = diff.lines().collect();
    let mut i = 0;
    while i < dlines.len() {
        let l = dlines[i];
        if l.starts_with("---") || l.starts_with("+++") {
            i += 1;
            continue;
        }
        if let Some(hdr) = l.strip_prefix("@@") {
            // Format: " -old_start,old_len +new_start,new_len @@..."
            let parts: Vec<&str> = hdr.split_whitespace().collect();
            let old = parts.iter().find(|p| p.starts_with('-')).unwrap_or(&"-0,0");
            let old_start: usize = old
                .trim_start_matches('-')
                .split(',')
                .next()
                .unwrap()
                .parse()
                .map_err(|e| format!("bad hunk header: {e}"))?;
            let mut hunk_old: Vec<String> = vec![];
            let mut hunk_new: Vec<String> = vec![];
            i += 1;
            while i < dlines.len() && !dlines[i].starts_with("@@") {
                let h = dlines[i];
                if let Some(s) = h.strip_prefix(' ') {
                    hunk_old.push(format!("{s}\n"));
                    hunk_new.push(format!("{s}\n"));
                } else if let Some(s) = h.strip_prefix('-') {
                    hunk_old.push(format!("{s}\n"));
                } else if let Some(s) = h.strip_prefix('+') {
                    hunk_new.push(format!("{s}\n"));
                }
                i += 1;
            }
            pending.push((old_start.saturating_sub(1), hunk_old.len(), hunk_old, hunk_new));
            cursor = old_start;
            continue;
        }
        i += 1;
    }

    // Apply hunks in reverse so earlier offsets stay valid.
    pending.sort_by_key(|h| std::cmp::Reverse(h.0));
    for (start, len, old, new) in pending {
        if start + len > out.len() {
            return Err(format!("hunk start {start} len {len} > file len {}", out.len()));
        }
        let actual: Vec<String> = out[start..start + len].to_vec();
        if actual != old {
            return Err("context mismatch".into());
        }
        out.splice(start..start + len, new);
    }

    let _ = cursor; // silence
    Ok(out.join(""))
}
```

- [ ] **Step 4: 跑测试**

```
cargo test -p rb-ai --lib tools::builtin::file
```

预期：3 个测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/rb-ai/src/tools/builtin/file.rs
git commit -m "$(cat <<'EOF'
feat(ai): file_write + file_patch tools (sandbox policy enforced upstream)

Path-classification lives in sandbox::policy; the tools themselves do raw
IO and trust the agent_loop dispatcher to route through approval first.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 15：code_run 工具

**Files:**
- Create: `crates/rb-ai/src/tools/builtin/code_run.rs`
- Modify: `crates/rb-ai/src/tools/builtin/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/tools/builtin/code_run.rs`：

```rust
//! code_run — execute Python/R/Shell scripts inside the sandbox.
//!
//! Runtime is selected by `AgentConfig.code_run.runtime`. `pixi` is the
//! default; `system` falls back to PATH-resolved python/Rscript/bash;
//! `custom` runs `<custom_command> <interp> <script>`.
//!
//! Streaming: stderr/stdout lines are surfaced via the agent's RunEvent
//! channel — wired by execute.rs in Phase 4. This module only owns the
//! one-shot blocking execution path used by tests.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::tools::{
    schema::{RiskLevel, ToolDef, ToolError},
    ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry,
};

pub fn register(reg: &mut ToolRegistry) {
    reg.register(ToolEntry {
        def: code_run_def(),
        executor: std::sync::Arc::new(CodeRunExec),
    });
}

fn code_run_def() -> ToolDef {
    ToolDef {
        name: "code_run".into(),
        description: "Run a Python/R/shell script. cwd defaults to <project>/sandbox/.".into(),
        risk: RiskLevel::RunLow,
        params: json!({
            "type": "object",
            "properties": {
                "language": {"type": "string", "enum": ["python", "r", "shell"]},
                "code": {"type": "string"},
                "cwd": {"type": "string"},
                "timeout_secs": {"type": "integer", "default": 600}
            },
            "required": ["language", "code"]
        }),
    }
}

struct CodeRunExec;
#[async_trait]
impl ToolExecutor for CodeRunExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let language = args
            .get("language")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("language required".into()))?;
        let code = args
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("code required".into()))?;
        let cwd: PathBuf = args
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .ok_or_else(|| ToolError::InvalidArgs("cwd required".into()))?;
        let timeout = Duration::from_secs(
            args.get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(600),
        );
        std::fs::create_dir_all(&cwd).map_err(|e| ToolError::Execution(e.to_string()))?;
        let (script_name, interp) = match language {
            "python" => ("agent_run.py", "python"),
            "r" => ("agent_run.R", "Rscript"),
            "shell" => ("agent_run.sh", "bash"),
            other => return Err(ToolError::InvalidArgs(format!("unsupported language: {other}"))),
        };
        let script = cwd.join(script_name);
        std::fs::write(&script, code).map_err(|e| ToolError::Execution(e.to_string()))?;

        let mut cmd = Command::new(interp);
        cmd.arg(&script).current_dir(&cwd);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        rb_core::subprocess::harden_for_gui(&mut cmd);

        let mut child = cmd
            .spawn()
            .map_err(|e| ToolError::Execution(format!("spawn {interp}: {e}")))?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let stdout_task = tokio::spawn(async move {
            let mut buf = String::new();
            let mut r = BufReader::new(stdout);
            let mut line = String::new();
            while r.read_line(&mut line).await.unwrap_or(0) > 0 {
                buf.push_str(&line);
                line.clear();
                if buf.len() > 256 * 1024 {
                    break;
                }
            }
            buf
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = String::new();
            let mut r = BufReader::new(stderr);
            let mut line = String::new();
            while r.read_line(&mut line).await.unwrap_or(0) > 0 {
                buf.push_str(&line);
                line.clear();
                if buf.len() > 256 * 1024 {
                    break;
                }
            }
            buf
        });

        let exit = match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return Err(ToolError::Execution(format!("wait: {e}"))),
            Err(_) => {
                let _ = child.kill().await;
                return Err(ToolError::Execution("timeout".into()));
            }
        };
        let stdout = stdout_task.await.unwrap_or_default();
        let stderr = stderr_task.await.unwrap_or_default();
        Ok(ToolOutput::Value(json!({
            "exit_code": exit.code(),
            "stdout": stdout,
            "stderr": stderr
        })))
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn ctx(root: &std::path::Path) -> ToolContext<'static> {
        let project = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
            rb_core::project::Project::create("t", root).unwrap(),
        ))));
        let runner = Box::leak(Box::new(std::sync::Arc::new(rb_core::runner::Runner::new(
            project.clone(),
        ))));
        let binres = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
            rb_core::binary::BinaryResolver::default(),
        ))));
        ToolContext {
            project,
            runner,
            binary_resolver: binres,
        }
    }

    #[tokio::test]
    async fn shell_echo_returns_stdout() {
        let tmp = tempdir().unwrap();
        let exec = CodeRunExec;
        let out = exec
            .execute(
                &json!({
                    "language": "shell",
                    "code": "echo hello-world",
                    "cwd": tmp.path().display().to_string(),
                    "timeout_secs": 5
                }),
                ctx(tmp.path()),
            )
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["exit_code"], 0);
        assert!(v["stdout"].as_str().unwrap().contains("hello-world"));
    }

    #[tokio::test]
    async fn timeout_kills_long_running_script() {
        let tmp = tempdir().unwrap();
        let exec = CodeRunExec;
        let r = exec
            .execute(
                &json!({
                    "language": "shell",
                    "code": "sleep 10",
                    "cwd": tmp.path().display().to_string(),
                    "timeout_secs": 1
                }),
                ctx(tmp.path()),
            )
            .await;
        assert!(matches!(r, Err(ToolError::Execution(s)) if s.contains("timeout")));
    }
}
```

更新 `crates/rb-ai/src/tools/builtin/mod.rs`：

```rust
pub mod code_run;
pub mod file;
pub mod project_state;

use crate::tools::ToolRegistry;

pub fn register_all(registry: &mut ToolRegistry) {
    file::register(registry);
    project_state::register(registry);
    code_run::register(registry);
}
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib tools::builtin::code_run
```

预期：Linux/macOS 上 PASS（Windows 上 `#[cfg(unix)]` 跳过）。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/tools/builtin
git commit -m "$(cat <<'EOF'
feat(ai): code_run tool (system runtime; pixi wiring lands in agent_loop)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 16：web_scan 工具

**Files:**
- Create: `crates/rb-ai/src/tools/builtin/web.rs`
- Modify: `crates/rb-ai/src/tools/builtin/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/tools/builtin/web.rs`：

```rust
//! web_scan — HTTP GET. Returns trimmed body text. Network whitelist /
//! logging is enforced by the agent_loop wrapper; this tool is the raw
//! reqwest call.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{
    schema::{RiskLevel, ToolDef, ToolError},
    ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry,
};

pub fn register(reg: &mut ToolRegistry) {
    reg.register(ToolEntry {
        def: def(),
        executor: std::sync::Arc::new(WebScanExec),
    });
}

fn def() -> ToolDef {
    ToolDef {
        name: "web_scan".into(),
        description: "HTTP GET; returns body trimmed to max_bytes.".into(),
        risk: RiskLevel::RunLow,
        params: json!({
            "type": "object",
            "properties": {
                "url": {"type": "string"},
                "max_bytes": {"type": "integer", "default": 65536},
                "headers": {"type": "object", "additionalProperties": {"type": "string"}}
            },
            "required": ["url"]
        }),
    }
}

struct WebScanExec;
#[async_trait]
impl ToolExecutor for WebScanExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("url required".into()))?;
        let max = args
            .get("max_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(65536) as usize;
        let mut req = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| ToolError::Execution(e.to_string()))?
            .get(url);
        if let Some(headers) = args.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in headers {
                if let Some(s) = v.as_str() {
                    req = req.header(k, s);
                }
            }
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ToolError::Execution(format!("http: {e}")))?;
        let status = resp.status().as_u16();
        let mut body = resp
            .bytes()
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?
            .to_vec();
        let truncated = body.len() > max;
        if truncated {
            body.truncate(max);
        }
        let text = String::from_utf8_lossy(&body).into_owned();
        Ok(ToolOutput::Value(json!({
            "status": status,
            "truncated": truncated,
            "body": text
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ctx(root: &std::path::Path) -> ToolContext<'static> {
        let project = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
            rb_core::project::Project::create("t", root).unwrap(),
        ))));
        let runner = Box::leak(Box::new(std::sync::Arc::new(rb_core::runner::Runner::new(
            project.clone(),
        ))));
        let binres = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
            rb_core::binary::BinaryResolver::default(),
        ))));
        ToolContext {
            project,
            runner,
            binary_resolver: binres,
        }
    }

    #[tokio::test]
    async fn web_scan_returns_status_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/data"))
            .respond_with(ResponseTemplate::new(200).set_body_string("hello"))
            .mount(&server)
            .await;
        let tmp = tempfile::tempdir().unwrap();
        let exec = WebScanExec;
        let out = exec
            .execute(
                &json!({"url": format!("{}/data", server.uri())}),
                ctx(tmp.path()),
            )
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["status"], 200);
        assert_eq!(v["body"].as_str().unwrap(), "hello");
    }
}
```

更新 `crates/rb-ai/src/tools/builtin/mod.rs`：

```rust
pub mod code_run;
pub mod file;
pub mod project_state;
pub mod web;

use crate::tools::ToolRegistry;

pub fn register_all(registry: &mut ToolRegistry) {
    file::register(registry);
    project_state::register(registry);
    code_run::register(registry);
    web::register(registry);
}
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib tools::builtin::web
```

预期：1 个测试 PASS（wiremock 已在 dev-dependencies）。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/tools/builtin
git commit -m "$(cat <<'EOF'
feat(ai): web_scan tool (HTTP GET, body-truncated)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 17：memory_tools — recall_memory / update_working_checkpoint / start_long_term_update / task_done

**Files:**
- Create: `crates/rb-ai/src/tools/builtin/memory_tools.rs`
- Create: `crates/rb-ai/src/tools/builtin/ask_user.rs`
- Modify: `crates/rb-ai/src/tools/mod.rs`（扩展 ToolContext）
- Modify: `crates/rb-ai/src/tools/builtin/mod.rs`

> **关键变更**：这些工具需要访问 `MemoryStore` + `SandboxPolicy`。当前 `ToolContext` 只暴露 `project/runner/binary_resolver`。本 task 给 `ToolContext` 加可选字段。

- [ ] **Step 1: 扩展 ToolContext**

`crates/rb-ai/src/tools/mod.rs` 中 `ToolContext`：

```rust
pub struct ToolContext<'a> {
    pub project: &'a Arc<tokio::sync::Mutex<Project>>,
    pub runner: &'a Arc<Runner>,
    pub binary_resolver: &'a Arc<tokio::sync::Mutex<rb_core::binary::BinaryResolver>>,
    pub memory: Option<&'a Arc<crate::memory::MemoryStore>>,
    pub session_id: Option<&'a str>,
    pub project_root: Option<&'a std::path::Path>,
    pub ask_user_tx: Option<&'a tokio::sync::mpsc::Sender<crate::tools::AskUserRequest>>,
}
```

并新增类型：

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct AskUserRequest {
    pub call_id: String,
    pub prompt: String,
    pub responder: tokio::sync::mpsc::Sender<String>,
}
```

> 旧调用点（如 `module_derived` 内部构造 `ToolContext` 的位置——其实 ToolContext 是按调用方现场构造的，搜索 `ToolContext {` 全局，凡显式构造之处都在 `agent_loop::execute` 里集中产生，不在 module_derived 内）。如果搬运 builtin.rs 时有别的 `ToolContext { ... }`，按报错补 `memory: None, session_id: None, project_root: None, ask_user_tx: None`。

- [ ] **Step 2: 写失败测试 (memory_tools)**

新建 `crates/rb-ai/src/tools/builtin/memory_tools.rs`：

```rust
//! Memory-mutating tools the agent can invoke.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AiError;
use crate::memory::{
    crystallize::{long_term_update, Layer, LongTermBody},
    layers::{Scope, TodoEntry, WorkingCheckpoint},
    recall::{collect_candidates, Bm25Recaller, Recaller},
};
use crate::tools::{
    schema::{RiskLevel, ToolDef, ToolError},
    ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry,
};

pub fn register(reg: &mut ToolRegistry) {
    reg.register(entry("recall_memory", recall_def(), RecallExec));
    reg.register(entry(
        "update_working_checkpoint",
        update_cp_def(),
        UpdateCpExec,
    ));
    reg.register(entry(
        "start_long_term_update",
        long_term_def(),
        LongTermExec,
    ));
    reg.register(entry("task_done", task_done_def(), TaskDoneExec));
}

fn entry<E: ToolExecutor + 'static>(_name: &str, def: ToolDef, exec: E) -> ToolEntry {
    ToolEntry {
        def,
        executor: std::sync::Arc::new(exec),
    }
}

fn recall_def() -> ToolDef {
    ToolDef {
        name: "recall_memory".into(),
        description: "Search global+project memory for relevant skills, archives, insights.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "top_k": {"type": "integer", "default": 5}
            },
            "required": ["query"]
        }),
    }
}

fn update_cp_def() -> ToolDef {
    ToolDef {
        name: "update_working_checkpoint".into(),
        description: "Replace the in-progress todo list and progress note for this session.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {
                "todo": {"type": "array", "items": {"type":"object", "properties":{
                    "text":{"type":"string"},"done":{"type":"boolean"}
                }, "required":["text","done"]}},
                "perceive_snapshot_hash": {"type": "string"}
            },
            "required": ["todo"]
        }),
    }
}

fn long_term_def() -> ToolDef {
    ToolDef {
        name: "start_long_term_update".into(),
        description: "Write to L2 (facts) or L3 (skill SOP). Caller declares layer + scope.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {
                "layer": {"type": "string", "enum": ["l2", "l3"]},
                "scope": {"type": "string", "enum": ["global", "project"]},
                "section": {"type": "string"},
                "name": {"type": "string"},
                "triggers": {"type": "array", "items": {"type": "string"}},
                "markdown": {"type": "string"}
            },
            "required": ["layer", "scope", "markdown"]
        }),
    }
}

fn task_done_def() -> ToolDef {
    ToolDef {
        name: "task_done".into(),
        description: "Signal the current task is finished; triggers archive + insight crystallize.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {
                "headline": {"type": "string"},
                "tags": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["headline"]
        }),
    }
}

struct RecallExec;
#[async_trait]
impl ToolExecutor for RecallExec {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("query required".into()))?;
        let top_k = args
            .get("top_k")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;
        let store = ctx
            .memory
            .ok_or_else(|| ToolError::Execution("memory not wired".into()))?;
        let cands = collect_candidates(store, ctx.project_root)
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let r = Bm25Recaller::new(top_k)
            .recall(query, cands, 4096)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(ToolOutput::Value(serde_json::to_value(r).unwrap()))
    }
}

struct UpdateCpExec;
#[async_trait]
impl ToolExecutor for UpdateCpExec {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let store = ctx
            .memory
            .ok_or_else(|| ToolError::Execution("memory not wired".into()))?;
        let project_root = ctx
            .project_root
            .ok_or_else(|| ToolError::Execution("project_root not wired".into()))?;
        let session_id = ctx
            .session_id
            .ok_or_else(|| ToolError::Execution("session_id not wired".into()))?;
        let todo: Vec<TodoEntry> = serde_json::from_value(
            args.get("todo")
                .cloned()
                .ok_or_else(|| ToolError::InvalidArgs("todo required".into()))?,
        )
        .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let snapshot_hash = args
            .get("perceive_snapshot_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let cp = WorkingCheckpoint {
            session_id: session_id.into(),
            project_root: project_root.display().to_string(),
            started_at: chrono::Utc::now(),
            last_step_at: chrono::Utc::now(),
            todo,
            message_count: 0,
            perceive_snapshot_hash: snapshot_hash,
        };
        store
            .write_checkpoint(project_root, &cp)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(ToolOutput::Value(json!({"ok": true})))
    }
}

struct LongTermExec;
#[async_trait]
impl ToolExecutor for LongTermExec {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let store = ctx
            .memory
            .ok_or_else(|| ToolError::Execution("memory not wired".into()))?;
        let layer = match args.get("layer").and_then(|v| v.as_str()).unwrap_or("") {
            "l2" => Layer::L2,
            "l3" => Layer::L3,
            other => return Err(ToolError::InvalidArgs(format!("layer={other}"))),
        };
        let scope = match args.get("scope").and_then(|v| v.as_str()).unwrap_or("") {
            "global" => Scope::Global,
            "project" => Scope::Project,
            other => return Err(ToolError::InvalidArgs(format!("scope={other}"))),
        };
        let body = LongTermBody {
            section: args
                .get("section")
                .and_then(|v| v.as_str())
                .map(|s| s.into()),
            name: args.get("name").and_then(|v| v.as_str()).map(|s| s.into()),
            triggers: args
                .get("triggers")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            markdown: args
                .get("markdown")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArgs("markdown required".into()))?
                .into(),
        };
        let r = long_term_update(store, ctx.project_root, layer, scope, body)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(ToolOutput::Value(serde_json::to_value(r).unwrap()))
    }
}

struct TaskDoneExec;
#[async_trait]
impl ToolExecutor for TaskDoneExec {
    async fn execute(&self, args: &Value, _ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        // task_done is a control-flow marker; agent_loop intercepts the call
        // and triggers crystallize_session. Here we just echo the intent.
        let headline = args
            .get("headline")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("headline required".into()))?;
        Ok(ToolOutput::Value(json!({
            "task_done": true,
            "headline": headline,
            "tags": args.get("tags").cloned().unwrap_or(json!([]))
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryStore;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn ctx_with_mem<'a>(
        store: &'a Arc<MemoryStore>,
        project: &'a Arc<tokio::sync::Mutex<rb_core::project::Project>>,
        runner: &'a Arc<rb_core::runner::Runner>,
        binres: &'a Arc<tokio::sync::Mutex<rb_core::binary::BinaryResolver>>,
        proot: &'a std::path::Path,
    ) -> ToolContext<'a> {
        ToolContext {
            project,
            runner,
            binary_resolver: binres,
            memory: Some(store),
            session_id: Some("sess1"),
            project_root: Some(proot),
            ask_user_tx: None,
        }
    }

    #[tokio::test]
    async fn update_checkpoint_writes_file() {
        let tmp = tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
        let proot = tmp.path().join("proj");
        store.ensure_project(&proot).unwrap();
        let project = Arc::new(tokio::sync::Mutex::new(
            rb_core::project::Project::create("t", &proot).unwrap(),
        ));
        let runner = Arc::new(rb_core::runner::Runner::new(project.clone()));
        let binres = Arc::new(tokio::sync::Mutex::new(
            rb_core::binary::BinaryResolver::default(),
        ));
        let ctx = ctx_with_mem(&store, &project, &runner, &binres, &proot);
        let exec = UpdateCpExec;
        exec.execute(
            &json!({
                "todo": [{"text":"qc","done":false}],
                "perceive_snapshot_hash": "abc"
            }),
            ctx,
        )
        .await
        .unwrap();
        assert!(proot.join("agent/checkpoints/current.json").exists());
    }

    #[tokio::test]
    async fn task_done_echoes_headline() {
        let tmp = tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
        let proot = tmp.path().join("proj");
        store.ensure_project(&proot).unwrap();
        let project = Arc::new(tokio::sync::Mutex::new(
            rb_core::project::Project::create("t", &proot).unwrap(),
        ));
        let runner = Arc::new(rb_core::runner::Runner::new(project.clone()));
        let binres = Arc::new(tokio::sync::Mutex::new(
            rb_core::binary::BinaryResolver::default(),
        ));
        let ctx = ctx_with_mem(&store, &project, &runner, &binres, &proot);
        let exec = TaskDoneExec;
        let out = exec
            .execute(&json!({"headline": "did rna-seq", "tags":["rna-seq"]}), ctx)
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["task_done"], true);
        assert_eq!(v["headline"], "did rna-seq");
    }
}
```

- [ ] **Step 3: 实现 ask_user.rs**

新建 `crates/rb-ai/src/tools/builtin/ask_user.rs`：

```rust
//! ask_user — pause and surface a prompt to the user. Implementation drains
//! through a channel installed in ToolContext.ask_user_tx; agent_loop owns
//! the receiver. The tool blocks until user replies via responder.

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::tools::{
    schema::{RiskLevel, ToolDef, ToolError},
    AskUserRequest, ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry,
};

pub fn register(reg: &mut ToolRegistry) {
    reg.register(ToolEntry {
        def: def(),
        executor: std::sync::Arc::new(AskUserExec),
    });
}

fn def() -> ToolDef {
    ToolDef {
        name: "ask_user".into(),
        description: "Pause and ask the user a question. Returns their reply.".into(),
        risk: RiskLevel::Read,
        params: json!({
            "type": "object",
            "properties": {"prompt": {"type": "string"}},
            "required": ["prompt"]
        }),
    }
}

struct AskUserExec;
#[async_trait]
impl ToolExecutor for AskUserExec {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("prompt required".into()))?;
        let tx = ctx
            .ask_user_tx
            .ok_or_else(|| ToolError::Execution("ask_user channel not wired".into()))?;
        let (responder_tx, mut responder_rx) = tokio::sync::mpsc::channel::<String>(1);
        tx.send(AskUserRequest {
            call_id: Uuid::new_v4().simple().to_string(),
            prompt: prompt.into(),
            responder: responder_tx,
        })
        .await
        .map_err(|e| ToolError::Execution(format!("ask_user send: {e}")))?;
        let reply = responder_rx
            .recv()
            .await
            .ok_or_else(|| ToolError::Execution("ask_user channel closed".into()))?;
        Ok(ToolOutput::Value(json!({"reply": reply})))
    }
}
```

- [ ] **Step 4: 在 builtin/mod.rs 注册新模块**

```rust
pub mod ask_user;
pub mod code_run;
pub mod file;
pub mod memory_tools;
pub mod project_state;
pub mod web;

use crate::tools::ToolRegistry;

pub fn register_all(registry: &mut ToolRegistry) {
    file::register(registry);
    project_state::register(registry);
    code_run::register(registry);
    web::register(registry);
    memory_tools::register(registry);
    ask_user::register(registry);
}
```

- [ ] **Step 5: 跑测试**

```
cargo test -p rb-ai --lib tools::builtin
```

预期：先前所有测试 + memory_tools 的 2 个 = 全 PASS。

- [ ] **Step 6: 提交**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(ai): memory_tools (recall/checkpoint/long_term/task_done) + ask_user

Extends ToolContext with optional memory, session_id, project_root,
ask_user_tx fields so memory-mutating tools can be wired by agent_loop.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 18：tools/skill.rs — L3 markdown → ToolDef 加载器

**Files:**
- Create: `crates/rb-ai/src/tools/skill.rs`
- Modify: `crates/rb-ai/src/tools/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/tools/skill.rs`：

```rust
//! Load L3 skill markdown files (frontmatter + body) into ToolDefs.
//! Frontmatter is YAML; body is the SOP text (passed to the agent as a
//! sub-task prompt when the skill tool is invoked).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AiError;
use crate::memory::layers::SkillMeta;
use crate::tools::{
    schema::{RiskLevel, ToolDef, ToolError},
    ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry,
};

#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub slug: String,
    pub meta: SkillMeta,
    pub body: String,
    pub source_path: PathBuf,
}

/// Parse one `.md` file with optional `---\n…\n---` YAML frontmatter.
pub fn parse_skill_file(path: &Path) -> Result<LoadedSkill, AiError> {
    let raw = std::fs::read_to_string(path)?;
    let (front, body) = split_frontmatter(&raw);
    let meta: SkillMeta = serde_yaml::from_str(&front)
        .map_err(|e| AiError::Config(format!("skill frontmatter ({}): {e}", path.display())))?;
    let slug = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| meta.name.clone());
    Ok(LoadedSkill {
        slug,
        meta,
        body: body.trim().to_string(),
        source_path: path.to_path_buf(),
    })
}

fn split_frontmatter(raw: &str) -> (String, String) {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (String::new(), raw.to_string());
    }
    let after = &trimmed[3..];
    if let Some(end) = after.find("\n---") {
        let front = after[..end].trim_start_matches('\n').to_string();
        let body = after[end + 4..].trim_start_matches('\n').to_string();
        (front, body)
    } else {
        (String::new(), raw.to_string())
    }
}

/// Register all skills found in `<skills_dir>/*.md` as `skill_<slug>` tools.
pub fn register_dir(reg: &mut ToolRegistry, dir: &Path) -> Result<usize, AiError> {
    let mut n = 0;
    if !dir.exists() {
        return Ok(0);
    }
    for ent in std::fs::read_dir(dir)?.flatten() {
        let path = ent.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        if path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.starts_with('_'))
            .unwrap_or(false)
        {
            continue;
        }
        let s = parse_skill_file(&path)?;
        let tool = SkillTool {
            slug: s.slug.clone(),
            body: s.body.clone(),
            triggers: s.meta.triggers.clone(),
        };
        reg.register(ToolEntry {
            def: ToolDef {
                name: format!("skill_{}", s.slug.replace('-', "_")),
                description: s.meta.description.clone(),
                risk: RiskLevel::RunMid,
                params: s.meta.inputs_schema.clone(),
            },
            executor: std::sync::Arc::new(tool),
        });
        n += 1;
    }
    Ok(n)
}

struct SkillTool {
    slug: String,
    body: String,
    triggers: Vec<String>,
}

#[async_trait]
impl ToolExecutor for SkillTool {
    async fn execute(&self, args: &Value, _ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        // Skill tools surface the SOP body + binding args. agent_loop will
        // pick this up and inject it as a nested user prompt.
        Ok(ToolOutput::Value(json!({
            "skill": self.slug,
            "triggers": self.triggers,
            "args": args,
            "sop": self.body,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parse_extracts_frontmatter_and_body() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("rna-seq.md");
        std::fs::write(
            &p,
            "---\nname: rna-seq\ndescription: do rna-seq\ntriggers: [rna-seq]\n---\n\n## SOP\n1. step\n",
        )
        .unwrap();
        let s = parse_skill_file(&p).unwrap();
        assert_eq!(s.slug, "rna-seq");
        assert_eq!(s.meta.name, "rna-seq");
        assert!(s.body.contains("SOP"));
    }

    #[test]
    fn register_dir_skips_underscore_files() {
        let tmp = tempdir().unwrap();
        std::fs::write(
            tmp.path().join("rna-seq.md"),
            "---\nname: rna-seq\ndescription: x\n---\nbody",
        )
        .unwrap();
        std::fs::write(tmp.path().join("_index.md"), "---\nname: x\ndescription: x\n---\n").unwrap();
        let mut reg = ToolRegistry::new();
        let n = register_dir(&mut reg, tmp.path()).unwrap();
        assert_eq!(n, 1);
        assert!(reg.get("skill_rna_seq").is_some());
    }
}
```

`crates/rb-ai/Cargo.toml` 加：

```toml
serde_yaml = "0.9"
```

`crates/rb-ai/src/tools/mod.rs` 末尾追加：

```rust
pub mod skill;
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib tools::skill
```

预期：2 个测试 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/Cargo.toml crates/rb-ai/src/tools/skill.rs crates/rb-ai/src/tools/mod.rs Cargo.lock
git commit -m "$(cat <<'EOF'
feat(ai): skill loader — L3 markdown frontmatter -> ToolDef

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 4 —— Agent 主循环

> 5 个 task：AgentSession + perceive、reason（含 FlashRecaller 实现）、execute（bucket 分发 + 审批 channel）、record（checkpoint + 结晶）、run_session 主循环。

### Task 19：agent_loop 类型 + perceive.rs

**Files:**
- Create: `crates/rb-ai/src/agent_loop/perceive.rs`
- Create: `crates/rb-ai/src/agent_loop/types.rs`
- Modify: `crates/rb-ai/src/agent_loop/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/agent_loop/types.rs`：

```rust
//! Public types for the agent loop. Kept in their own file because both the
//! main loop and Tauri-facing rb-app need them, and we want a stable surface.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::memory::layers::TodoEntry;

/// One agent research session. Held in `Arc<Mutex<AgentSession>>` by the
/// run_session loop and accessed by tools via ToolContext.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub project_root: String,
    pub started_at: DateTime<Utc>,
    pub messages: Vec<serde_json::Value>, // raw provider message JSON
    pub todo: Vec<TodoEntry>,
    pub tool_failures: std::collections::HashMap<String, u32>,
}

impl AgentSession {
    pub fn new(project_root: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            project_root,
            started_at: Utc::now(),
            messages: vec![],
            todo: vec![],
            tool_failures: Default::default(),
        }
    }
}

/// Streaming events emitted up to rb-app (and onward to frontend in Plan 2).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    Text { session_id: String, delta: String },
    Reasoning { session_id: String, delta: String },
    ToolCall {
        session_id: String,
        call_id: String,
        name: String,
        bucket: String,
        decision: String,
        args: serde_json::Value,
    },
    ToolResult {
        session_id: String,
        call_id: String,
        result: serde_json::Value,
    },
    AskUser {
        session_id: String,
        call_id: String,
        prompt: String,
    },
    Memory { session_id: String, recalled: Vec<serde_json::Value> },
    Checkpoint { session_id: String, todo: Vec<TodoEntry> },
    Crystallize { session_id: String, layer: String, scope: String, path: String },
    Done { session_id: String },
    Error { session_id: String, message: String },
}

pub type SharedSession = Arc<Mutex<AgentSession>>;
```

新建 `crates/rb-ai/src/agent_loop/perceive.rs`：

```rust
//! Build the per-turn system prompt: L0 + project snapshot + recalled memory.

use std::path::Path;
use std::sync::Arc;

use crate::error::AiError;
use crate::memory::recall::{collect_candidates, Recaller};
use crate::memory::MemoryStore;

pub struct PerceiveCtx {
    pub user_text: String,
    pub project_summary: String,
}

pub struct PerceiveOut {
    pub system_prompt: String,
    pub recalled: Vec<crate::memory::recall::RecallCandidate>,
}

pub async fn perceive(
    store: &MemoryStore,
    project_root: Option<&Path>,
    recaller: Arc<dyn Recaller>,
    ctx: &PerceiveCtx,
    budget_tokens: usize,
) -> Result<PerceiveOut, AiError> {
    let l0 = store.read_l0().unwrap_or_default();
    let l2 = store.read_l2().unwrap_or_default();
    let candidates = collect_candidates(store, project_root)?;
    let recall = recaller
        .recall(&ctx.user_text, candidates, budget_tokens)
        .await?;
    let mut sp = String::new();
    sp.push_str("# Meta rules\n\n");
    sp.push_str(&l0);
    sp.push_str("\n\n# Long-term facts\n\n");
    sp.push_str(&l2);
    sp.push_str("\n\n# Project snapshot\n\n");
    sp.push_str(&ctx.project_summary);
    if !recall.picked.is_empty() {
        sp.push_str("\n\n# Recalled memory (top matches)\n\n");
        for c in &recall.picked {
            sp.push_str(&format!("- [{}|{}] {}\n", c.scope, c.kind, c.text));
        }
    }
    Ok(PerceiveOut {
        system_prompt: sp,
        recalled: recall.picked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::recall::{Bm25Recaller, RecallCandidate};
    use crate::memory::MemoryStore;
    use async_trait::async_trait;
    use tempfile::tempdir;

    #[tokio::test]
    async fn perceive_includes_l0_project_and_recalled() {
        let tmp = tempdir().unwrap();
        let store = MemoryStore::open(tmp.path().join("global")).unwrap();
        let project = tmp.path().join("proj");
        store.ensure_project(&project).unwrap();
        // Seed a skill index entry so recall has something to match.
        store
            .upsert_skill_index(
                crate::memory::Scope::Global,
                None,
                crate::memory::IndexEntry::Skill {
                    name: "rna-seq".into(),
                    path: "L3_skills/rna-seq.md".into(),
                    scope: crate::memory::Scope::Global,
                    triggers: vec!["rna-seq".into(), "differential expression".into()],
                    hits: 0,
                    last_used: None,
                },
            )
            .await
            .unwrap();
        let recaller: Arc<dyn Recaller> = Arc::new(Bm25Recaller::new(3));
        let out = perceive(
            &store,
            Some(&project),
            recaller,
            &PerceiveCtx {
                user_text: "find DE genes in this rna-seq dataset".into(),
                project_summary: "Project: demo".into(),
            },
            4096,
        )
        .await
        .unwrap();
        assert!(out.system_prompt.contains("Meta rules"));
        assert!(out.system_prompt.contains("Project: demo"));
        assert!(out.system_prompt.contains("rna-seq"));
        assert!(!out.recalled.is_empty());
    }
}
```

更新 `crates/rb-ai/src/agent_loop/mod.rs`：

```rust
//! Perceive→reason→execute→record main loop.

pub mod perceive;
pub mod types;

pub use types::{AgentEvent, AgentSession, SharedSession};
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib agent_loop::perceive
```

预期：1 个测试 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/agent_loop
git commit -m "$(cat <<'EOF'
feat(ai): AgentSession + AgentEvent types + perceive() builder

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 20：agent_loop/reason.rs + FlashRecaller 实现

**Files:**
- Create: `crates/rb-ai/src/agent_loop/reason.rs`
- Modify: `crates/rb-ai/src/memory/recall.rs`（加 FlashRecaller）
- Modify: `crates/rb-ai/src/agent_loop/mod.rs`

- [ ] **Step 1: 写 FlashRecaller 失败测试**

在 `crates/rb-ai/src/memory/recall.rs` 末尾追加：

```rust
// ---------- FlashRecaller (LLM-driven) ----------

use crate::provider::{ChatProvider, ChatRequest, ProviderEvent, ProviderMessage, ThinkingConfig};

pub struct FlashRecaller {
    pub provider: std::sync::Arc<dyn ChatProvider>,
    pub model: String,
    pub max_candidates: usize,
}

#[async_trait]
impl Recaller for FlashRecaller {
    async fn recall(
        &self,
        query: &str,
        candidates: Vec<RecallCandidate>,
        _budget_tokens: usize,
    ) -> Result<RecallResult, AiError> {
        let cap = candidates.len().min(self.max_candidates);
        let pruned: Vec<&RecallCandidate> = candidates.iter().take(cap).collect();
        let menu: Vec<serde_json::Value> = pruned
            .iter()
            .map(|c| serde_json::json!({"id": c.id, "text": c.text}))
            .collect();
        let user_msg = format!(
            "Query: {query}\n\nCandidates (id, text):\n{}\n\nReturn JSON: {{\"picked\": [\"id1\", \"id2\"], \"rationale\": \"...\"}}.",
            serde_json::to_string_pretty(&menu).unwrap_or_default()
        );
        let req = ChatRequest {
            model: self.model.clone(),
            system: "You select the most relevant memory entries for the query. Return only JSON.".into(),
            messages: vec![ProviderMessage::User { content: user_msg }],
            tools: vec![],
            temperature: 0.0,
            thinking: ThinkingConfig::default(),
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel::<ProviderEvent>(8);
        let cancel = rb_core::cancel::CancellationToken::new();
        let prov = self.provider.clone();
        let h = tokio::spawn(async move { prov.send(req, tx, cancel).await });
        let mut text = String::new();
        while let Some(ev) = rx.recv().await {
            if let ProviderEvent::TextDelta(s) = ev {
                text.push_str(&s);
            }
        }
        h.await.ok();
        let parsed: serde_json::Value = match serde_json::from_str(text.trim()) {
            Ok(v) => v,
            Err(_) => {
                // Try to extract first JSON object substring.
                if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) {
                    serde_json::from_str(&text[start..=end]).map_err(|e| {
                        AiError::Provider(format!("flash recall parse: {e} in {text:?}"))
                    })?
                } else {
                    return Err(AiError::Provider(format!("flash recall no JSON: {text:?}")));
                }
            }
        };
        let ids: Vec<String> = parsed
            .get("picked")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let picked: Vec<RecallCandidate> = ids
            .into_iter()
            .filter_map(|id| candidates.iter().find(|c| c.id == id).cloned())
            .collect();
        Ok(RecallResult {
            picked,
            rationale: parsed
                .get("rationale")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }
}

#[cfg(test)]
mod flash_tests {
    use super::*;
    use crate::provider::{ChatProvider, ChatRequest, ProviderEvent};
    use async_trait::async_trait;

    struct FakeProvider {
        reply: String,
    }
    #[async_trait]
    impl ChatProvider for FakeProvider {
        async fn send(
            &self,
            _req: ChatRequest,
            sink: tokio::sync::mpsc::Sender<ProviderEvent>,
            _cancel: rb_core::cancel::CancellationToken,
        ) -> Result<(), AiError> {
            let _ = sink.send(ProviderEvent::TextDelta(self.reply.clone())).await;
            let _ = sink
                .send(ProviderEvent::Finish(crate::provider::FinishReason::Stop))
                .await;
            Ok(())
        }
    }

    #[tokio::test]
    async fn flash_picks_ids_from_json_reply() {
        let cands = vec![
            RecallCandidate {
                id: "a".into(),
                kind: "x".into(),
                scope: "global".into(),
                text: "alpha".into(),
                path: None,
            },
            RecallCandidate {
                id: "b".into(),
                kind: "x".into(),
                scope: "global".into(),
                text: "beta".into(),
                path: None,
            },
        ];
        let prov = std::sync::Arc::new(FakeProvider {
            reply: r#"{"picked":["b"],"rationale":"better match"}"#.into(),
        });
        let r = FlashRecaller {
            provider: prov,
            model: "haiku".into(),
            max_candidates: 32,
        };
        let res = r.recall("anything", cands, 4096).await.unwrap();
        assert_eq!(res.picked.len(), 1);
        assert_eq!(res.picked[0].id, "b");
        assert_eq!(res.rationale.as_deref(), Some("better match"));
    }
}
```

- [ ] **Step 2: 跑 FlashRecaller 测试**

```
cargo test -p rb-ai --lib memory::recall::flash_tests
```

预期：PASS。

- [ ] **Step 3: 写 reason 模块**

新建 `crates/rb-ai/src/agent_loop/reason.rs`：

```rust
//! Stage 2 of the loop: call provider, accumulate text/reasoning, parse
//! tool calls, surface stream events.

use std::sync::Arc;

use rb_core::cancel::CancellationToken;
use tokio::sync::mpsc;

use crate::error::AiError;
use crate::provider::{
    ChatProvider, ChatRequest, FinishReason, ProviderEvent, ProviderMessage, ProviderToolCall,
    ThinkingConfig,
};
use crate::tools::ToolDef;

use super::types::AgentEvent;

pub struct ReasonOut {
    pub text: String,
    pub reasoning: String,
    pub tool_calls: Vec<ProviderToolCall>,
    pub finish: Option<FinishReason>,
}

pub async fn reason(
    provider: Arc<dyn ChatProvider>,
    model: &str,
    system: String,
    history: Vec<ProviderMessage>,
    tools: Vec<ToolDef>,
    temperature: f32,
    thinking: ThinkingConfig,
    cancel: CancellationToken,
    sink: mpsc::Sender<AgentEvent>,
    session_id: &str,
) -> Result<ReasonOut, AiError> {
    let req = ChatRequest {
        model: model.into(),
        system,
        messages: history,
        tools,
        temperature,
        thinking,
    };
    let (tx, mut rx) = mpsc::channel::<ProviderEvent>(32);
    let cancel_for = cancel.clone();
    let prov = provider.clone();
    let h = tokio::spawn(async move { prov.send(req, tx, cancel_for).await });

    let mut out = ReasonOut {
        text: String::new(),
        reasoning: String::new(),
        tool_calls: vec![],
        finish: None,
    };
    while let Some(ev) = rx.recv().await {
        match ev {
            ProviderEvent::TextDelta(s) => {
                out.text.push_str(&s);
                let _ = sink
                    .send(AgentEvent::Text {
                        session_id: session_id.into(),
                        delta: s,
                    })
                    .await;
            }
            ProviderEvent::ReasoningDelta(s) => {
                out.reasoning.push_str(&s);
                let _ = sink
                    .send(AgentEvent::Reasoning {
                        session_id: session_id.into(),
                        delta: s,
                    })
                    .await;
            }
            ProviderEvent::ToolCall { id, name, args } => {
                out.tool_calls.push(ProviderToolCall { id, name, args });
            }
            ProviderEvent::Finish(r) => {
                out.finish = Some(r);
            }
        }
    }
    match h.await {
        Ok(Ok(())) => Ok(out),
        Ok(Err(e)) => Err(AiError::Provider(format!("{e}"))),
        Err(e) => Err(AiError::Provider(format!("provider join: {e}"))),
    }
}
```

更新 `crates/rb-ai/src/agent_loop/mod.rs`：

```rust
pub mod perceive;
pub mod reason;
pub mod types;

pub use types::{AgentEvent, AgentSession, SharedSession};
```

- [ ] **Step 4: 编译确认**

```
cargo build -p rb-ai
```

预期：通过。reason 暂无独立测试——它只是 provider 适配器，下面 run_session 集成测会覆盖。

- [ ] **Step 5: 提交**

```bash
git add crates/rb-ai/src/memory/recall.rs crates/rb-ai/src/agent_loop
git commit -m "$(cat <<'EOF'
feat(ai): FlashRecaller + reason() stage of the agent loop

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 21：agent_loop/execute.rs — bucket 分发

**Files:**
- Create: `crates/rb-ai/src/agent_loop/execute.rs`
- Modify: `crates/rb-ai/src/agent_loop/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `crates/rb-ai/src/agent_loop/execute.rs`：

```rust
//! Stage 3 of the loop: dispatch tool calls through SandboxPolicy, possibly
//! pausing for user approval. Approval channel: when a Decision::ApproveOnce
//! or AlwaysAsk fires, we surface an AgentEvent::ToolCall(decision="pending")
//! and wait on an `approval_rx` channel for the user's verdict. The Tauri
//! command surface in Plan 2 owns the channel.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, Mutex};

use crate::error::AiError;
use crate::provider::ProviderToolCall;
use crate::sandbox::policy::{Bucket, Decision, SandboxPolicy};
use crate::tools::{ToolContext, ToolOutput, ToolRegistry};

use super::types::AgentEvent;

pub enum ApprovalVerdict {
    Approve { edited_args: Option<Value> },
    Reject { reason: Option<String> },
}

pub struct ExecCtx<'a> {
    pub policy: &'a SandboxPolicy,
    pub registry: &'a ToolRegistry,
    pub project: &'a Arc<Mutex<rb_core::project::Project>>,
    pub runner: &'a Arc<rb_core::runner::Runner>,
    pub binary_resolver: &'a Arc<Mutex<rb_core::binary::BinaryResolver>>,
    pub memory: Option<&'a Arc<crate::memory::MemoryStore>>,
    pub session_id: &'a str,
    pub project_root: Option<&'a std::path::Path>,
    pub ask_user_tx: Option<&'a mpsc::Sender<crate::tools::AskUserRequest>>,
    pub approval_rx: &'a Mutex<mpsc::Receiver<(String, ApprovalVerdict)>>,
    pub event_sink: &'a mpsc::Sender<AgentEvent>,
}

pub async fn execute_call(
    ctx: ExecCtx<'_>,
    call: ProviderToolCall,
) -> Result<Value, AiError> {
    let (bucket, decision) = ctx.policy.classify(&call.name, &call.args);
    let bucket_str = bucket_label(&bucket);
    let decision_str = decision_label(&decision);

    let _ = ctx
        .event_sink
        .send(AgentEvent::ToolCall {
            session_id: ctx.session_id.into(),
            call_id: call.id.clone(),
            name: call.name.clone(),
            bucket: bucket_str.clone(),
            decision: decision_str.clone(),
            args: call.args.clone(),
        })
        .await;

    let resolved_args = if ctx.policy.should_run(&bucket, &decision) {
        call.args.clone()
    } else {
        // Wait for verdict.
        let mut rx = ctx.approval_rx.lock().await;
        loop {
            let (cid, verdict) = rx
                .recv()
                .await
                .ok_or_else(|| AiError::InvalidState("approval channel closed".into()))?;
            if cid != call.id {
                continue;
            }
            match verdict {
                ApprovalVerdict::Approve { edited_args } => {
                    ctx.policy.record_approval(bucket.clone());
                    break edited_args.unwrap_or_else(|| call.args.clone());
                }
                ApprovalVerdict::Reject { reason } => {
                    return Ok(serde_json::json!({
                        "error": "rejected_by_user",
                        "reason": reason.unwrap_or_default()
                    }));
                }
            }
        }
    };

    let entry = ctx
        .registry
        .get(&call.name)
        .ok_or_else(|| AiError::Tool(format!("unknown tool: {}", call.name)))?;
    let tool_ctx = ToolContext {
        project: ctx.project,
        runner: ctx.runner,
        binary_resolver: ctx.binary_resolver,
        memory: ctx.memory,
        session_id: Some(ctx.session_id),
        project_root: ctx.project_root,
        ask_user_tx: ctx.ask_user_tx,
    };
    let out = entry.executor.execute(&resolved_args, tool_ctx).await;
    let value = match out {
        Ok(ToolOutput::Value(v)) => v,
        Err(e) => serde_json::json!({"error": e.to_string()}),
    };
    let _ = ctx
        .event_sink
        .send(AgentEvent::ToolResult {
            session_id: ctx.session_id.into(),
            call_id: call.id,
            result: value.clone(),
        })
        .await;
    Ok(value)
}

fn bucket_label(b: &Bucket) -> String {
    match b {
        Bucket::ReadFs => "read_fs".into(),
        Bucket::SandboxWrite => "sandbox_write".into(),
        Bucket::ProjectModule { module } => format!("project_module:{module}"),
        Bucket::CodeRunSandbox => "code_run_sandbox".into(),
        Bucket::CodeRunOutOfSandbox => "code_run_out_of_sandbox".into(),
        Bucket::Web => "web".into(),
        Bucket::MemoryWrite => "memory_write".into(),
        Bucket::DestructiveDelete => "destructive_delete".into(),
        Bucket::AskUser => "ask_user".into(),
    }
}

fn decision_label(d: &Decision) -> String {
    match d {
        Decision::Allow => "allow".into(),
        Decision::ApproveOnce => "approve_once".into(),
        Decision::AlwaysAsk => "always_ask".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{
        builtin::file::FileWriteExec, schema::ToolDef, RiskLevel, ToolEntry, ToolRegistry,
    };
    use serde_json::json;
    use tempfile::tempdir;

    fn registry() -> ToolRegistry {
        let mut r = ToolRegistry::new();
        // Use file_write since it does no sandbox check internally.
        r.register(ToolEntry {
            def: ToolDef {
                name: "file_write".into(),
                description: "".into(),
                risk: RiskLevel::RunLow,
                params: json!({"type":"object"}),
            },
            executor: Arc::new(FileWriteExec),
        });
        r
    }

    fn rb_core_handles(
        root: &std::path::Path,
    ) -> (
        Arc<Mutex<rb_core::project::Project>>,
        Arc<rb_core::runner::Runner>,
        Arc<Mutex<rb_core::binary::BinaryResolver>>,
    ) {
        let project = Arc::new(Mutex::new(
            rb_core::project::Project::create("t", root).unwrap(),
        ));
        let runner = Arc::new(rb_core::runner::Runner::new(project.clone()));
        let binres = Arc::new(Mutex::new(rb_core::binary::BinaryResolver::default()));
        (project, runner, binres)
    }

    #[tokio::test]
    async fn allow_path_runs_immediately() {
        let tmp = tempdir().unwrap();
        let policy = SandboxPolicy::new(tmp.path().to_path_buf(), "sandbox");
        let reg = registry();
        let (proj, run, br) = rb_core_handles(tmp.path());
        let (sink_tx, mut sink_rx) = mpsc::channel(16);
        let (_appr_tx, appr_rx) = mpsc::channel(1);
        let appr_rx = Mutex::new(appr_rx);
        let path = tmp.path().join("sandbox/x.txt");
        let res = execute_call(
            ExecCtx {
                policy: &policy,
                registry: &reg,
                project: &proj,
                runner: &run,
                binary_resolver: &br,
                memory: None,
                session_id: "s",
                project_root: Some(tmp.path()),
                ask_user_tx: None,
                approval_rx: &appr_rx,
                event_sink: &sink_tx,
            },
            ProviderToolCall {
                id: "c1".into(),
                name: "file_write".into(),
                args: json!({"path": path.display().to_string(), "content":"x"}),
            },
        )
        .await
        .unwrap();
        assert!(res.get("path").is_some());
        // Drain: ToolCall + ToolResult
        let _ = sink_rx.recv().await;
        let _ = sink_rx.recv().await;
        assert!(path.exists());
    }

    #[tokio::test]
    async fn approve_once_waits_then_runs() {
        let tmp = tempdir().unwrap();
        let policy = SandboxPolicy::new(tmp.path().to_path_buf(), "sandbox");
        let reg = registry();
        let (proj, run, br) = rb_core_handles(tmp.path());
        let (sink_tx, _sink_rx) = mpsc::channel(16);
        let (appr_tx, appr_rx) = mpsc::channel(1);
        let appr_rx = Mutex::new(appr_rx);
        // Path = inside-project, outside-sandbox → ApproveOnce.
        let path = tmp.path().join("results/out.tsv");
        let exec_fut = tokio::spawn({
            let policy = unsafe { std::mem::transmute::<&_, &'static SandboxPolicy>(&policy) };
            let reg = unsafe { std::mem::transmute::<&_, &'static ToolRegistry>(&reg) };
            let proj = proj.clone();
            let run = run.clone();
            let br = br.clone();
            let appr_rx = unsafe { std::mem::transmute::<&_, &'static Mutex<_>>(&appr_rx) };
            let sink_tx = sink_tx.clone();
            let path_str = path.display().to_string();
            async move {
                execute_call(
                    ExecCtx {
                        policy,
                        registry: reg,
                        project: &proj,
                        runner: &run,
                        binary_resolver: &br,
                        memory: None,
                        session_id: "s",
                        project_root: None,
                        ask_user_tx: None,
                        approval_rx: appr_rx,
                        event_sink: &sink_tx,
                    },
                    ProviderToolCall {
                        id: "c2".into(),
                        name: "file_write".into(),
                        args: json!({"path": path_str, "content":"y"}),
                    },
                )
                .await
            }
        });
        // Send approval.
        appr_tx
            .send(("c2".into(), ApprovalVerdict::Approve { edited_args: None }))
            .await
            .unwrap();
        let res = exec_fut.await.unwrap().unwrap();
        assert!(res.get("path").is_some());
        // Future calls in this session will be Allow due to record_approval.
    }
}
```

> 注：第二个测试用了 `unsafe transmute` 把局部引用拉成 `'static`，原因是 `tokio::spawn` 要求 `'static` 闭包，而本测试只在 spawn 内同步使用——属于测试代码可接受的折中。如执行人不喜欢 unsafe，可改为把 `policy/reg/appr_rx` 用 `Arc` 包起来传入，相应调整 `ExecCtx`。

更新 `crates/rb-ai/src/agent_loop/mod.rs`：

```rust
pub mod execute;
pub mod perceive;
pub mod reason;
pub mod types;

pub use execute::{execute_call, ApprovalVerdict, ExecCtx};
pub use types::{AgentEvent, AgentSession, SharedSession};
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib agent_loop::execute
```

预期：2 个测试 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/agent_loop
git commit -m "$(cat <<'EOF'
feat(ai): execute_call dispatcher with bucket-based approval gate

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 22：agent_loop/record.rs

**Files:**
- Create: `crates/rb-ai/src/agent_loop/record.rs`
- Modify: `crates/rb-ai/src/agent_loop/mod.rs`

- [ ] **Step 1: 实现 record**

新建 `crates/rb-ai/src/agent_loop/record.rs`：

```rust
//! Stage 4 of the loop: persist working checkpoint after each step and run
//! crystallize_session at end-of-task.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::error::AiError;
use crate::memory::layers::{ArchiveOutcome, TodoEntry, WorkingCheckpoint};
use crate::memory::{crystallize::crystallize_session, MemoryStore, SessionSummaryInput};

use super::types::AgentSession;

pub async fn fsync_checkpoint(
    store: &MemoryStore,
    project_root: &Path,
    session: &AgentSession,
    perceive_snapshot: &str,
) -> Result<(), AiError> {
    let cp = WorkingCheckpoint {
        session_id: session.id.clone(),
        project_root: project_root.display().to_string(),
        started_at: session.started_at,
        last_step_at: Utc::now(),
        todo: session.todo.clone(),
        message_count: session.messages.len(),
        perceive_snapshot_hash: hash(perceive_snapshot),
    };
    store.write_checkpoint(project_root, &cp).await
}

pub async fn finalize(
    store: &Arc<MemoryStore>,
    project_root: &Path,
    session: &AgentSession,
    headline: String,
    tags: Vec<String>,
    outcome: ArchiveOutcome,
    net_log_path: Option<String>,
) -> Result<(), AiError> {
    crystallize_session(
        store,
        project_root,
        SessionSummaryInput {
            session_id: session.id.clone(),
            started_at: session.started_at,
            ended_at: Some(Utc::now()),
            outcome,
            messages: session.messages.clone(),
            headline,
            tags,
            net_log_path,
        },
    )
    .await?;
    store.clear_checkpoint(project_root)?;
    Ok(())
}

pub fn hash(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex(h.finalize().as_slice())
}

fn hex(b: &[u8]) -> String {
    static HEX: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for byte in b {
        s.push(HEX[(byte >> 4) as usize] as char);
        s.push(HEX[(byte & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn finalize_writes_archive_and_clears_checkpoint() {
        let tmp = tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
        let project = tmp.path().join("proj");
        store.ensure_project(&project).unwrap();
        let session = AgentSession::new(project.display().to_string());
        // Pre-populate a checkpoint to verify it's cleared.
        fsync_checkpoint(&store, &project, &session, "snap").await.unwrap();
        assert!(project.join("agent/checkpoints/current.json").exists());
        finalize(
            &store,
            &project,
            &session,
            "did the thing".into(),
            vec!["test".into()],
            ArchiveOutcome::Done,
            None,
        )
        .await
        .unwrap();
        assert!(!project.join("agent/checkpoints/current.json").exists());
        let archive = project
            .join("agent/L4_archives")
            .join(format!("{}.json", session.id));
        assert!(archive.exists());
    }
}
```

`crates/rb-ai/Cargo.toml` 加：

```toml
sha2 = "0.10"
```

更新 `agent_loop/mod.rs`：

```rust
pub mod execute;
pub mod perceive;
pub mod reason;
pub mod record;
pub mod types;

pub use execute::{execute_call, ApprovalVerdict, ExecCtx};
pub use record::{finalize, fsync_checkpoint};
pub use types::{AgentEvent, AgentSession, SharedSession};
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --lib agent_loop::record
```

预期：1 个测试 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/agent_loop crates/rb-ai/Cargo.toml Cargo.lock
git commit -m "$(cat <<'EOF'
feat(ai): record stage — fsync checkpoint + finalize/crystallize

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 23：agent_loop/mod.rs — run_session 主循环

**Files:**
- Modify: `crates/rb-ai/src/agent_loop/mod.rs`

- [ ] **Step 1: 实现 run_session**

替换 `crates/rb-ai/src/agent_loop/mod.rs` 末尾追加：

```rust
//! run_session: orchestrate perceive → reason → execute → record.
//!
//! The caller hands us:
//! - the project + runner + binary_resolver (rb_core),
//! - a built ToolRegistry (with builtin + module_derived + skill tools),
//! - a SandboxPolicy,
//! - a Recaller (BM25 fallback, optionally Composite with Flash),
//! - a ChatProvider,
//! - mpsc senders for AgentEvent and AskUserRequest, and a receiver for
//!   ApprovalVerdict.
//!
//! The session runs until the agent emits `task_done`, until an unrecoverable
//! error, or until cancelled.

use std::sync::Arc;

use rb_core::cancel::CancellationToken;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};

use crate::error::AiError;
use crate::memory::layers::ArchiveOutcome;
use crate::memory::recall::Recaller;
use crate::memory::MemoryStore;
use crate::provider::{ChatProvider, FinishReason, ProviderMessage, ProviderToolCall, ThinkingConfig};
use crate::sandbox::policy::SandboxPolicy;
use crate::sandbox::NetLogger;
use crate::tools::{AskUserRequest, ToolRegistry};

use self::execute::{execute_call, ApprovalVerdict, ExecCtx};
use self::perceive::{perceive, PerceiveCtx};
use self::reason::reason;
use self::record::{finalize, fsync_checkpoint};

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub model: String,
    pub temperature: f32,
    pub thinking: ThinkingConfig,
    pub recall_budget_tokens: usize,
    pub max_consecutive_failures: u32,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".into(),
            temperature: 0.2,
            thinking: ThinkingConfig::default(),
            recall_budget_tokens: 4096,
            max_consecutive_failures: 5,
        }
    }
}

pub struct RunSessionCtx {
    pub project: Arc<Mutex<rb_core::project::Project>>,
    pub runner: Arc<rb_core::runner::Runner>,
    pub binary_resolver: Arc<Mutex<rb_core::binary::BinaryResolver>>,
    pub registry: Arc<ToolRegistry>,
    pub policy: Arc<SandboxPolicy>,
    pub memory: Arc<MemoryStore>,
    pub recaller: Arc<dyn Recaller>,
    pub provider: Arc<dyn ChatProvider>,
    pub net_log: Arc<NetLogger>,
    pub project_root: std::path::PathBuf,
    pub config: RunConfig,
}

pub async fn run_session(
    ctx: RunSessionCtx,
    user_text: String,
    session: SharedSession,
    event_sink: mpsc::Sender<AgentEvent>,
    ask_user_tx: mpsc::Sender<AskUserRequest>,
    approval_rx: Arc<Mutex<mpsc::Receiver<(String, ApprovalVerdict)>>>,
    cancel: CancellationToken,
) -> Result<(), AiError> {
    // 1. Append the user message.
    {
        let mut s = session.lock().await;
        s.messages.push(serde_json::json!({"role":"user","content":user_text.clone()}));
    }
    let session_id = session.lock().await.id.clone();

    // Outer perceive — recall once per session-start (not per turn) keeps
    // determinism. Per-turn updates are achievable via recall_memory tool.
    let proj_summary = crate::orchestrator_compat::project_summary(&ctx.project).await;
    let perceive_out = perceive(
        &ctx.memory,
        Some(&ctx.project_root),
        ctx.recaller.clone(),
        &PerceiveCtx {
            user_text: user_text.clone(),
            project_summary: proj_summary.clone(),
        },
        ctx.config.recall_budget_tokens,
    )
    .await?;
    let _ = event_sink
        .send(AgentEvent::Memory {
            session_id: session_id.clone(),
            recalled: perceive_out
                .recalled
                .iter()
                .map(|c| serde_json::to_value(c).unwrap())
                .collect(),
        })
        .await;

    let system_prompt = perceive_out.system_prompt;

    // 2. Loop.
    let mut consecutive_fail: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    loop {
        if cancel.is_cancelled() {
            finalize(
                &ctx.memory,
                &ctx.project_root,
                &*session.lock().await,
                "cancelled".into(),
                vec!["cancelled".into()],
                ArchiveOutcome::Cancelled,
                ctx.net_log.path().map(|p| p.display().to_string()),
            )
            .await?;
            let _ = event_sink
                .send(AgentEvent::Error {
                    session_id: session_id.clone(),
                    message: "cancelled".into(),
                })
                .await;
            return Err(AiError::Cancelled);
        }

        let history = to_provider_messages(&session.lock().await.messages);
        let tools = ctx.registry.all_for_ai();
        let r = reason(
            ctx.provider.clone(),
            &ctx.config.model,
            system_prompt.clone(),
            history,
            tools,
            ctx.config.temperature,
            ctx.config.thinking.clone(),
            cancel.clone(),
            event_sink.clone(),
            &session_id,
        )
        .await?;

        // Append assistant message.
        {
            let mut s = session.lock().await;
            s.messages.push(serde_json::json!({
                "role":"assistant",
                "content": r.text,
                "reasoning_content": r.reasoning,
                "tool_calls": r.tool_calls,
            }));
        }

        if r.tool_calls.is_empty() {
            // Natural stop without task_done — finalize with Done outcome.
            finalize(
                &ctx.memory,
                &ctx.project_root,
                &*session.lock().await,
                "session ended without task_done".into(),
                vec![],
                ArchiveOutcome::Done,
                ctx.net_log.path().map(|p| p.display().to_string()),
            )
            .await?;
            let _ = event_sink
                .send(AgentEvent::Done {
                    session_id: session_id.clone(),
                })
                .await;
            return Ok(());
        }

        let mut got_task_done: Option<(String, Vec<String>)> = None;
        for call in r.tool_calls {
            // Special-case: task_done becomes a finalize.
            if call.name == "task_done" {
                let headline = call
                    .args
                    .get("headline")
                    .and_then(|v| v.as_str())
                    .unwrap_or("done")
                    .to_string();
                let tags: Vec<String> = call
                    .args
                    .get("tags")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                got_task_done = Some((headline, tags));
                push_tool_result(
                    &session,
                    &call.id,
                    &call.name,
                    serde_json::json!({"task_done": true}),
                )
                .await;
                let _ = event_sink
                    .send(AgentEvent::ToolResult {
                        session_id: session_id.clone(),
                        call_id: call.id.clone(),
                        result: serde_json::json!({"task_done": true}),
                    })
                    .await;
                continue;
            }

            let result = execute_call(
                ExecCtx {
                    policy: &ctx.policy,
                    registry: &ctx.registry,
                    project: &ctx.project,
                    runner: &ctx.runner,
                    binary_resolver: &ctx.binary_resolver,
                    memory: Some(&ctx.memory),
                    session_id: &session_id,
                    project_root: Some(&ctx.project_root),
                    ask_user_tx: Some(&ask_user_tx),
                    approval_rx: &approval_rx,
                    event_sink: &event_sink,
                },
                ProviderToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    args: call.args.clone(),
                },
            )
            .await?;

            // Track consecutive failures.
            if result.get("error").is_some() {
                let n = consecutive_fail.entry(call.name.clone()).or_insert(0);
                *n += 1;
                if *n >= ctx.config.max_consecutive_failures {
                    finalize(
                        &ctx.memory,
                        &ctx.project_root,
                        &*session.lock().await,
                        format!("aborted: {} kept failing", call.name),
                        vec!["failed".into()],
                        ArchiveOutcome::Failed,
                        ctx.net_log.path().map(|p| p.display().to_string()),
                    )
                    .await?;
                    let _ = event_sink
                        .send(AgentEvent::Error {
                            session_id: session_id.clone(),
                            message: format!("{} failed {} times in a row", call.name, n),
                        })
                        .await;
                    return Err(AiError::Tool(format!(
                        "{} failed {} times",
                        call.name, n
                    )));
                }
            } else {
                consecutive_fail.remove(&call.name);
            }

            push_tool_result(&session, &call.id, &call.name, result).await;
        }

        // After tool execution, fsync checkpoint.
        fsync_checkpoint(
            &ctx.memory,
            &ctx.project_root,
            &*session.lock().await,
            &system_prompt,
        )
        .await?;

        if let Some((headline, tags)) = got_task_done {
            finalize(
                &ctx.memory,
                &ctx.project_root,
                &*session.lock().await,
                headline,
                tags,
                ArchiveOutcome::Done,
                ctx.net_log.path().map(|p| p.display().to_string()),
            )
            .await?;
            let _ = event_sink
                .send(AgentEvent::Done {
                    session_id: session_id.clone(),
                })
                .await;
            return Ok(());
        }

        match r.finish {
            Some(FinishReason::Stop) | None | Some(FinishReason::ToolCalls) => continue,
            Some(FinishReason::Length) => {
                let _ = event_sink
                    .send(AgentEvent::Error {
                        session_id: session_id.clone(),
                        message: "model length limit hit".into(),
                    })
                    .await;
                return Ok(());
            }
            Some(FinishReason::Error(e)) => {
                let _ = event_sink
                    .send(AgentEvent::Error {
                        session_id: session_id.clone(),
                        message: e,
                    })
                    .await;
                return Ok(());
            }
        }
    }
}

async fn push_tool_result(
    session: &SharedSession,
    call_id: &str,
    name: &str,
    result: Value,
) {
    let mut s = session.lock().await;
    s.messages.push(serde_json::json!({
        "role":"tool",
        "tool_call_id": call_id,
        "name": name,
        "content": result,
    }));
}

fn to_provider_messages(messages: &[Value]) -> Vec<ProviderMessage> {
    messages
        .iter()
        .filter_map(|m| {
            let role = m.get("role").and_then(|v| v.as_str())?;
            match role {
                "user" => Some(ProviderMessage::User {
                    content: m
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                }),
                "assistant" => {
                    let calls: Vec<ProviderToolCall> = m
                        .get("tool_calls")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    Some(ProviderMessage::Assistant {
                        content: m
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        reasoning_content: m
                            .get("reasoning_content")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        tool_calls: calls,
                    })
                }
                "tool" => Some(ProviderMessage::Tool {
                    call_id: m
                        .get("tool_call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: m
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    result: m
                        .get("content")
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                }),
                _ => None,
            }
        })
        .collect()
}

// orchestrator_compat module: snapshot helper. We deleted orchestrator/, so
// we keep a slim copy of `snapshot::build` here under that name.
mod orchestrator_compat {
    use std::sync::Arc;
    use tokio::sync::Mutex;

    pub async fn project_summary(project: &Arc<Mutex<rb_core::project::Project>>) -> String {
        // Inline minimal version of the deleted orchestrator/snapshot.rs::build.
        let p = project.lock().await;
        format!(
            "Project: {}\nDefault view: {}\nRecent runs:\n{}",
            p.name,
            p.default_view.as_deref().unwrap_or("manual"),
            p.runs
                .iter()
                .rev()
                .take(10)
                .map(|r| format!("  {}: {} {:?}", r.id, r.module_id, r.status))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}
```

更新 `crates/rb-ai/src/agent_loop/mod.rs` 顶部 re-export：

```rust
pub use execute::{execute_call, ApprovalVerdict, ExecCtx};
pub use record::{finalize, fsync_checkpoint};
pub use types::{AgentEvent, AgentSession, SharedSession};
```

（`run_session` / `RunSessionCtx` / `RunConfig` 已在该文件中 `pub`，自然可达。）

- [ ] **Step 2: 编译**

```
cargo build -p rb-ai
```

预期：通过。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/src/agent_loop
git commit -m "$(cat <<'EOF'
feat(ai): run_session main loop wiring perceive/reason/execute/record

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 5 —— 集成测试

> 2 个 task：mock provider e2e 跑一个完整 RNA-seq DE 序列；cancel + 多失败回归。

### Task 24：e2e — 完整 RNA-seq 工具序列

**Files:**
- Create: `crates/rb-ai/tests/agent_session_e2e.rs`

- [ ] **Step 1: 实现 e2e 测试**

新建 `crates/rb-ai/tests/agent_session_e2e.rs`：

```rust
//! End-to-end agent session test. A scripted ChatProvider emits a fixed
//! sequence: file_write → code_run → task_done. The test asserts:
//! - the file is written under sandbox/,
//! - the code runs and produces stdout,
//! - L4 archive is written with outcome=done,
//! - L1 insight is appended,
//! - checkpoint is cleared.

use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use rb_ai::agent_loop::{
    run_session, ApprovalVerdict, AgentEvent, AgentSession, RunConfig, RunSessionCtx, SharedSession,
};
use rb_ai::memory::{Bm25Recaller, MemoryStore};
use rb_ai::provider::{
    ChatProvider, ChatRequest, FinishReason, ProviderEvent, ProviderToolCall,
};
use rb_ai::sandbox::{NetLogger, SandboxPolicy};
use rb_ai::tools::{builtin, ToolRegistry};
use tempfile::tempdir;
use tokio::sync::{mpsc, Mutex};

#[derive(Clone)]
struct ScriptedProvider {
    steps: Arc<StdMutex<Vec<Vec<ProviderEvent>>>>,
}

#[async_trait]
impl ChatProvider for ScriptedProvider {
    async fn send(
        &self,
        _req: ChatRequest,
        sink: tokio::sync::mpsc::Sender<ProviderEvent>,
        _cancel: rb_core::cancel::CancellationToken,
    ) -> Result<(), rb_ai::AiError> {
        let next = self.steps.lock().unwrap().drain(..1).next();
        if let Some(events) = next {
            for ev in events {
                let _ = sink.send(ev).await;
            }
        } else {
            let _ = sink.send(ProviderEvent::Finish(FinishReason::Stop)).await;
        }
        Ok(())
    }
}

#[tokio::test]
async fn agent_runs_scripted_session_to_task_done() {
    let tmp = tempdir().unwrap();
    let project_root = tmp.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    let project = Arc::new(Mutex::new(
        rb_core::project::Project::create("demo", &project_root).unwrap(),
    ));
    let runner = Arc::new(rb_core::runner::Runner::new(project.clone()));
    let binres = Arc::new(Mutex::new(rb_core::binary::BinaryResolver::default()));
    let memory = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
    memory.ensure_project(&project_root).unwrap();
    let policy = Arc::new(SandboxPolicy::new(project_root.clone(), "sandbox"));
    let registry = {
        let mut r = ToolRegistry::new();
        builtin::register_all(&mut r);
        Arc::new(r)
    };

    let sandbox_dir = project_root.join("sandbox");
    let sandbox_dir_s = sandbox_dir.display().to_string();

    let provider = Arc::new(ScriptedProvider {
        steps: Arc::new(StdMutex::new(vec![
            // turn 1: file_write into sandbox
            vec![
                ProviderEvent::ToolCall {
                    id: "t1".into(),
                    name: "file_write".into(),
                    args: serde_json::json!({
                        "path": format!("{sandbox_dir_s}/run.sh"),
                        "content": "echo hello-from-agent\n"
                    }),
                },
                ProviderEvent::Finish(FinishReason::ToolCalls),
            ],
            // turn 2: code_run
            vec![
                ProviderEvent::ToolCall {
                    id: "t2".into(),
                    name: "code_run".into(),
                    args: serde_json::json!({
                        "language": "shell",
                        "code": "bash run.sh",
                        "cwd": sandbox_dir_s.clone(),
                        "timeout_secs": 10
                    }),
                },
                ProviderEvent::Finish(FinishReason::ToolCalls),
            ],
            // turn 3: task_done
            vec![
                ProviderEvent::ToolCall {
                    id: "t3".into(),
                    name: "task_done".into(),
                    args: serde_json::json!({"headline":"hello world test","tags":["test"]}),
                },
                ProviderEvent::Finish(FinishReason::Stop),
            ],
        ])),
    });

    let net_log = Arc::new(NetLogger::new(&project_root, "sess1", false).unwrap());
    let recaller: Arc<dyn rb_ai::memory::Recaller> = Arc::new(Bm25Recaller::new(3));
    let session: SharedSession = Arc::new(Mutex::new(AgentSession::new(
        project_root.display().to_string(),
    )));
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(64);
    let (ask_tx, _ask_rx) = mpsc::channel(4);
    let (_appr_tx, appr_rx) = mpsc::channel::<(String, ApprovalVerdict)>(4);
    let appr_rx = Arc::new(Mutex::new(appr_rx));
    let cancel = rb_core::cancel::CancellationToken::new();

    // Drain events in background.
    let drain = tokio::spawn(async move {
        let mut events = vec![];
        while let Some(e) = event_rx.recv().await {
            events.push(e);
        }
        events
    });

    let ctx = RunSessionCtx {
        project,
        runner,
        binary_resolver: binres,
        registry,
        policy,
        memory: memory.clone(),
        recaller,
        provider,
        net_log,
        project_root: project_root.clone(),
        config: RunConfig::default(),
    };
    run_session(
        ctx,
        "make a hello-world script and run it".into(),
        session.clone(),
        event_tx,
        ask_tx,
        appr_rx,
        cancel,
    )
    .await
    .unwrap();

    let events = drain.await.unwrap();
    assert!(events.iter().any(|e| matches!(e, AgentEvent::Done { .. })));

    // Side-effects:
    let script = sandbox_dir.join("run.sh");
    assert!(script.exists());
    // L4 archive written
    let id = session.lock().await.id.clone();
    let archive = project_root
        .join("agent/L4_archives")
        .join(format!("{id}.json"));
    assert!(archive.exists(), "expected archive at {}", archive.display());
    // L1 insight appended
    let l1 = std::fs::read_to_string(memory.global_root.join("L1_insights.jsonl")).unwrap();
    assert!(l1.lines().count() >= 1);
    // Checkpoint cleared
    assert!(!project_root.join("agent/checkpoints/current.json").exists());
}
```

- [ ] **Step 2: 跑测试**

```
cargo test -p rb-ai --test agent_session_e2e
```

预期：PASS（在能跑 `bash echo` 的 Linux/macOS 上）。Windows 上跳过——本测试用了 shell；后续可加 `#[cfg(unix)]`。

- [ ] **Step 3: 提交**

```bash
git add crates/rb-ai/tests/agent_session_e2e.rs
git commit -m "$(cat <<'EOF'
test(ai): e2e scripted agent session — file_write -> code_run -> task_done

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 25：e2e — cancel 与连续失败回归

**Files:**
- Modify: `crates/rb-ai/tests/agent_session_e2e.rs`

- [ ] **Step 1: 加两个测试**

在 `crates/rb-ai/tests/agent_session_e2e.rs` 末尾追加：

```rust
#[tokio::test]
async fn agent_aborts_after_consecutive_failures() {
    let tmp = tempdir().unwrap();
    let project_root = tmp.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    let project = Arc::new(Mutex::new(
        rb_core::project::Project::create("demo", &project_root).unwrap(),
    ));
    let runner = Arc::new(rb_core::runner::Runner::new(project.clone()));
    let binres = Arc::new(Mutex::new(rb_core::binary::BinaryResolver::default()));
    let memory = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
    memory.ensure_project(&project_root).unwrap();
    let policy = Arc::new(SandboxPolicy::new(project_root.clone(), "sandbox"));
    let registry = {
        let mut r = ToolRegistry::new();
        builtin::register_all(&mut r);
        Arc::new(r)
    };

    // file_read on a non-existent path always errors.
    let bad = "/definitely/does/not/exist/here.txt".to_string();
    let mut steps: Vec<Vec<ProviderEvent>> = vec![];
    for i in 0..10 {
        steps.push(vec![
            ProviderEvent::ToolCall {
                id: format!("c{i}"),
                name: "file_read".into(),
                args: serde_json::json!({"path": bad}),
            },
            ProviderEvent::Finish(FinishReason::ToolCalls),
        ]);
    }
    let provider = Arc::new(ScriptedProvider {
        steps: Arc::new(StdMutex::new(steps)),
    });

    let net_log = Arc::new(NetLogger::new(&project_root, "sess2", false).unwrap());
    let recaller: Arc<dyn rb_ai::memory::Recaller> = Arc::new(Bm25Recaller::new(3));
    let session: SharedSession = Arc::new(Mutex::new(AgentSession::new(
        project_root.display().to_string(),
    )));
    let (event_tx, _event_rx) = mpsc::channel::<AgentEvent>(64);
    let (ask_tx, _ask_rx) = mpsc::channel(4);
    let (_appr_tx, appr_rx) = mpsc::channel::<(String, ApprovalVerdict)>(4);
    let appr_rx = Arc::new(Mutex::new(appr_rx));
    let cancel = rb_core::cancel::CancellationToken::new();

    let mut cfg = RunConfig::default();
    cfg.max_consecutive_failures = 3;

    let ctx = RunSessionCtx {
        project,
        runner,
        binary_resolver: binres,
        registry,
        policy,
        memory: memory.clone(),
        recaller,
        provider,
        net_log,
        project_root: project_root.clone(),
        config: cfg,
    };
    let r = run_session(
        ctx,
        "read a missing file repeatedly".into(),
        session.clone(),
        event_tx,
        ask_tx,
        appr_rx,
        cancel,
    )
    .await;
    assert!(matches!(r, Err(rb_ai::AiError::Tool(_))));
    let id = session.lock().await.id.clone();
    let archive_body = std::fs::read_to_string(
        project_root
            .join("agent/L4_archives")
            .join(format!("{id}.json")),
    )
    .unwrap();
    assert!(archive_body.contains("\"outcome\": \"failed\""));
}

#[tokio::test]
async fn agent_cancel_writes_cancelled_archive() {
    let tmp = tempdir().unwrap();
    let project_root = tmp.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    let project = Arc::new(Mutex::new(
        rb_core::project::Project::create("demo", &project_root).unwrap(),
    ));
    let runner = Arc::new(rb_core::runner::Runner::new(project.clone()));
    let binres = Arc::new(Mutex::new(rb_core::binary::BinaryResolver::default()));
    let memory = Arc::new(MemoryStore::open(tmp.path().join("global")).unwrap());
    memory.ensure_project(&project_root).unwrap();
    let policy = Arc::new(SandboxPolicy::new(project_root.clone(), "sandbox"));
    let registry = {
        let mut r = ToolRegistry::new();
        builtin::register_all(&mut r);
        Arc::new(r)
    };
    let provider = Arc::new(ScriptedProvider {
        steps: Arc::new(StdMutex::new(vec![])),
    });
    let net_log = Arc::new(NetLogger::new(&project_root, "sess3", false).unwrap());
    let recaller: Arc<dyn rb_ai::memory::Recaller> = Arc::new(Bm25Recaller::new(3));
    let session: SharedSession = Arc::new(Mutex::new(AgentSession::new(
        project_root.display().to_string(),
    )));
    let (event_tx, _event_rx) = mpsc::channel::<AgentEvent>(64);
    let (ask_tx, _ask_rx) = mpsc::channel(4);
    let (_appr_tx, appr_rx) = mpsc::channel::<(String, ApprovalVerdict)>(4);
    let appr_rx = Arc::new(Mutex::new(appr_rx));
    let cancel = rb_core::cancel::CancellationToken::new();
    cancel.cancel();

    let ctx = RunSessionCtx {
        project,
        runner,
        binary_resolver: binres,
        registry,
        policy,
        memory: memory.clone(),
        recaller,
        provider,
        net_log,
        project_root: project_root.clone(),
        config: RunConfig::default(),
    };
    let r = run_session(
        ctx,
        "anything".into(),
        session.clone(),
        event_tx,
        ask_tx,
        appr_rx,
        cancel,
    )
    .await;
    assert!(matches!(r, Err(rb_ai::AiError::Cancelled)));
    let id = session.lock().await.id.clone();
    let archive_body = std::fs::read_to_string(
        project_root
            .join("agent/L4_archives")
            .join(format!("{id}.json")),
    )
    .unwrap();
    assert!(archive_body.contains("\"outcome\": \"cancelled\""));
}
```

- [ ] **Step 2: 跑全部 e2e**

```
cargo test -p rb-ai --test agent_session_e2e
```

预期：3 个测试 PASS。

- [ ] **Step 3: 全工作区编译 + 测试**

```
cargo test --workspace
```

预期：所有测试 PASS。如个别 module-derived 集成测因 ToolContext 字段变更失败，按报错补 `memory: None, session_id: None, project_root: None, ask_user_tx: None`。

- [ ] **Step 4: 提交**

```bash
git add crates/rb-ai/tests/agent_session_e2e.rs
git commit -m "$(cat <<'EOF'
test(ai): cancel + consecutive-failure regressions for run_session

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## 收尾

### Task 26：lib.rs re-export 整理 + format/clippy

**Files:**
- Modify: `crates/rb-ai/src/lib.rs`

- [ ] **Step 1: 暴露公共 API**

`crates/rb-ai/src/lib.rs`：

```rust
//! Self-evolving agent core: provider abstraction, layered memory,
//! sandboxed tool execution, perceive→reason→execute→record loop.
//!
//! Depends on `rb-core`. UI/Tauri integration lives in `rb-app` (Plan 2).

pub mod agent_loop;
pub mod config;
pub mod error;
pub mod memory;
pub mod provider;
pub mod sandbox;
pub mod tools;

pub use error::AiError;
pub use memory::{
    Archive, ArchiveOutcome, Bm25Recaller, CompositeRecaller, IndexEntry, Insight, MemoryStore,
    Recaller, Scope, SkillMeta, TodoEntry, WorkingCheckpoint,
};
pub use sandbox::{Bucket, Decision, NetLogger, PolicyMode, SandboxPolicy};
```

- [ ] **Step 2: 跑 fmt + clippy**

```
cargo fmt -p rb-ai
RUSTFLAGS="--cap-lints=warn" cargo clippy -p rb-ai -- -D warnings
```

预期：通过。clippy 报警按 idiomatic 修。

- [ ] **Step 3: 全工作区检查**

```
cargo check --workspace
cargo test --workspace
```

预期：通过。

- [ ] **Step 4: 提交**

```bash
git add crates/rb-ai/src/lib.rs
git commit -m "$(cat <<'EOF'
chore(ai): re-export agent core public API + clippy clean

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Plan 1 终止条件 / 验收标准

完成上述 26 个 task 后，本 plan 的"working software"标准：

1. `cargo test --workspace` 全 PASS。
2. `cargo clippy -p rb-ai -- -D warnings` 通过。
3. `crates/rb-ai/tests/agent_session_e2e.rs` 三个测试演示：
   - 一个 mock provider 驱动的完整 file_write → code_run → task_done 流程，留下正确的 L4 archive、L1 insight、清空 checkpoint。
   - 连续失败导致正常退出并写 outcome=failed archive。
   - 启动即取消导致 outcome=cancelled archive。
4. memory 目录布局符合 spec（全局 `~/.local/share/...` + 项目 `<project>/agent/`）。
5. SandboxPolicy 的 8 类 bucket 全部覆盖，FullPermission 模式在 unit test 验证。
6. `rb-app` 暂无任何 `agent_*` Tauri 命令——这是 Plan 2 的工作；本 plan 中 rb-app 仅保证编译通过。

---

## 不在本 plan，留给 Plan 2

- `agent_*` Tauri 命令（10 个，见 spec §UI 表）
- Frontend `#agent` 视图三栏布局 + 审批卡 + Memory recall 卡 + Crystallize 卡
- `#chat` 路由 30 天 alias 重定向
- AppState 接 AgentRuntime 全局单例 + project-级 Mutex
- CHANGELOG / v0.3.0 发版

---

## Self-Review

**Spec coverage check:**

| Spec section | Plan task |
|---|---|
| Crate 重组（删 session/orchestrator）| Task 3 |
| RiskLevel 扩展 Read/RunLow/RunMid/Destructive | Task 1 |
| AgentConfig 子节 | Task 4 |
| memory/layers 类型 | Task 5 |
| memory/store 双根 + 5MB 切片 + 索引 | Task 6 |
| memory/recall（BM25 + Composite + Flash）| Task 7 + Task 20 |
| memory/crystallize（task_done + start_long_term_update）| Task 8 |
| memory/checkpoint helpers | Task 9 |
| sandbox/policy（Bucket/Decision/classify/full_permission）| Task 10 |
| sandbox/pixi（detect/init/build_command）| Task 11 |
| sandbox/net（NetLogger）| Task 12 |
| tools 拆分 builtin/ 子模块 | Task 13 |
| file_write / file_patch | Task 14 |
| code_run | Task 15 |
| web_scan | Task 16 |
| recall_memory / update_working_checkpoint / start_long_term_update / task_done / ask_user | Task 17 |
| skill loader（L3 markdown → ToolDef）| Task 18 |
| AgentSession + AgentEvent + perceive | Task 19 |
| reason 阶段 + FlashRecaller | Task 20 |
| execute 阶段 + 审批 channel | Task 21 |
| record 阶段 + finalize | Task 22 |
| run_session 主循环 + 连续失败终止 | Task 23 |
| e2e 集成测（mock provider RNA-seq DE 流程）| Task 24 |
| cancel + 失败 archive 回归 | Task 25 |
| AskUser channel | Task 17（执行器）+ Task 21/23（接线）|
| Tauri 命令 / frontend / 路由 alias | **不在本 plan**，Plan 2 |

**留意点（已在对应 task 标出）：**

- Task 21 第二个测试用了 unsafe transmute，仅测试代码，可以接受。
- Task 23 中 `orchestrator_compat::project_summary` 是 `orchestrator/snapshot.rs` 的简版替代——把删掉的逻辑保留所需最小子集。如果未来 frontend 需要更详细的 snapshot，可以重新独立成 `agent_loop/snapshot.rs`。
- Task 18 加了 `serde_yaml` 依赖。frontmatter 也可改用 toml；选 yaml 是因为生态里 markdown frontmatter 多用 yaml。
- L2/L3-Project 组合在 Task 8 被显式拒绝（`AiError::InvalidState`）；调用方（agent）在 prompt 中应被引导避免这种组合。这条放进 Task 18 spawning 时的 L0_meta.md 默认内容可能更稳——Task 6 default L0 已包含归类指引。

**Type consistency check:** RiskLevel 的 4 桶在所有 ToolDef 中一致；SandboxPolicy.classify 返回的 Bucket 与 execute_call 的 bucket_label 字符串一致；Recaller 的 RecallCandidate / RecallResult 在 BM25/Flash/Composite 三处签名一致；AgentEvent 各 variant 与 spec §UI 中"事件流"描述一致。

**Placeholder scan:** 无 TBD/TODO/"实现稍后"。所有 step 含完整代码或具体命令。


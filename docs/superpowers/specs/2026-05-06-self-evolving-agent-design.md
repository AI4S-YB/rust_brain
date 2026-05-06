# Self-Evolving Agent (rb-ai 重写) 设计

**Date**: 2026-05-06
**Status**: Brainstorming approved，待 spec review
**Scope**: 把 `rb-ai` crate 从"单回合 chat + plan card 审批"重写为受 [GenericAgent](https://github.com/lsdefine/GenericAgent) 启发的自进化 agent；删除现有 `chat_*` Tauri 命令、`session/`、`orchestrator/`，新增 `memory/` `sandbox/` `agent_loop/`；frontend `#chat` 视图替换为 `#agent`。

> 因仍处 0.x 探索期，本次允许破坏性变更：旧 chat session 数据不迁移、Tauri 命令名变更、frontend 路由替换。

## Problem

现有 AI 是"对话 + 单 module 表单驱动"形态：每个 Run 风险工具弹一次审批卡，session 仅保留消息序列，不积累跨会话经验，研究类任务（"为什么这批 reads 比对率低？"）难以驱动。我们希望 agent 能：

1. **自主多步研究**：按"感知 → 推理 → 执行 → 记忆"循环跑，单次任务可触发几十上百个工具调用。
2. **接入项目模块工具**：现有 `run_qc` / `run_star_align` / `run_deseq2` 等 module-derived tool 继续可用，零改动新模块。
3. **持有持久记忆**：跨项目积累 SOP、失败教训、习惯参数；项目内归档完整 trace。
4. **代码 + 网络能力**：在项目沙箱里跑 Python/R/shell（通过 pixi 管环境），可联网抓公共数据。
5. **可控审批**：风险分桶，研究流畅但破坏性操作必须确认；提供 "full permission" 模式给信任用户。

## Non-Goals

- 不做 embedding 检索：第一版用 flash model 批量召回 + BM25 fallback。
- 不做容器化沙箱：信任 `<project>/sandbox/**`，仅用路径白名单 + 子进程 hardening。
- 不打包 Python/R 运行时：用户自行装 pixi（推荐）或系统解释器。
- 不迁移旧 chat session 数据。
- 不做多 agent 并发：同一 project 同时只允许一个 active agent session。
- 不在 Phase 1 引入 RAG / 向量库 / 知识图谱。

## 总体架构

### Crate 重组（rb-ai）

```
crates/rb-ai/src/
├── lib.rs
├── error.rs
├── config/                 ✓ 保留（AiConfig + keyring）扩展 agent.* 子节
├── provider/               ✓ 保留（OpenAI compat / Anthropic / Ollama）+ flash provider trait
│
├── tools/
│   ├── mod.rs              ToolRegistry（保留）
│   ├── schema.rs           ToolDef / RiskLevel（扩展 risk: Read|RunLow|RunMid|Destructive）
│   ├── builtin/            扩展：file_read/write/patch/list、code_run、web_scan、ask_user、recall_memory 等
│   ├── module_derived.rs   ✓ 保留（自动派生 run_*）
│   └── skill.rs            ✶ 新增：从 L3 markdown frontmatter 加载为 ToolDef
│
├── memory/                 ✶ 新增
│   ├── mod.rs              MemoryStore（全局 + 项目两路 IO）
│   ├── layers.rs           L0/L1/L2/L3/L4 类型 + 序列化
│   ├── store.rs            原子写入、分片、index 维护
│   ├── recall.rs           flash-model 批量召回 + BM25 fallback
│   └── crystallize.rs      task_done / start_long_term_update 写回
│
├── sandbox/                ✶ 新增
│   ├── policy.rs           SandboxPolicy + Bucket + Decision
│   ├── pixi.rs             pixi 探测、init、run 包装
│   └── net.rs              网络日志（按配置开关）
│
└── agent_loop/             ✶ 新增（替代 orchestrator/）
    ├── mod.rs              run_session 主循环
    ├── perceive.rs         project snapshot + run history + memory recall
    ├── reason.rs           provider 调用 + tool decision
    ├── execute.rs          风险桶分发 + 子进程
    └── record.rs           working checkpoint + 结晶触发
```

**删除**：`session/` 全量（旧 `ChatSession` 概念被 `AgentSession` 替代）、`orchestrator/` 全量、frontend `#chat` 视图、`chat_*` Tauri 命令。

> 全文中"session"统一指 `AgentSession`：一次研究任务的运行上下文，包含 messages、approved_buckets、working_checkpoint、project 引用。其归档（结束后写入 L4）即 `Archive`。

**保留**：provider 抽象、tool registry、module_derived、`run_qc` 等 adapter crate 全部不动、`Runner` 不动。

`rb-ai` 仍只依赖 `rb-core`，不引入 Tauri；新依赖：`bm25` (或自实现，体量很小)、`path-clean`、`fs2`（fsync）。

### 数据布局

```
~/.local/share/rust_brain/agent/         # 全局，跨项目
├── L0_meta.md                           # 元规则；出厂带默认，用户可改
├── L1_insights.jsonl                    # append-only 速查 insight
├── L2_facts.md                          # 长期事实（基因组路径、习惯参数等）按 section
└── L3_skills/
    ├── <slug>.md                        # frontmatter + body
    └── _index.json                      # {name, triggers[], scope:"global", path, hits, last_used}

<project>/agent/                         # 项目内
├── L3_local/
│   ├── <slug>.md
│   └── _index.json                      # scope:"project"
├── L4_archives/
│   ├── <id>.json (主文件，>5MB 切 .part2.json ...)
│   ├── <id>.net.log                     # 网络调用审计（full perm 关）
│   └── _index.json                      # {id, started_at, ended_at, summary, outcome, tags}
└── checkpoints/
    └── current.json                     # 单文件，崩溃恢复用
```

### 数据流

```
Frontend  invoke('agent_send', {session_id, text})
  ↓
rb-app    agent_send 命令
  ↓
rb-ai     agent_loop::run_session()
  ├─ session 启动时 load_or_resume(checkpoint)
  ├─ append UserMsg, fsync checkpoint
  ├─ loop:
  │    perceive():
  │      - project snapshot（沿用 orchestrator/snapshot.rs 思路）
  │      - 召回 L1/L3-index/L4-index → flash model 抽 top-K（无 LLM 时 BM25）
  │      - L0 全量注入；L2 按 section heading 关键词命中惰性注入
  │    reason():
  │      - provider.send(req, sink, cancel) 流式
  │      - 解析 tool_calls
  │    execute():
  │      for call in tool_calls {
  │        let (bucket, decision) = sandbox.classify(call);
  │        match decision {
  │          Allow                   → exec
  │          ApproveOnce(bucket) if !approved → wait_user, cache approval, exec
  │          ApproveOnce(_)         → exec
  │          AlwaysAsk              → wait_user, exec on approve
  │        }
  │        // full permission mode 旁路 ApproveOnce / AlwaysAsk → 直接 exec
  │        record(call, result, into=working_checkpoint, fsync)
  │      }
  │    if finish_reason ∈ {Stop, ToolCalls→empty, task_done} → break
  │
  └─ on break: crystallize() 写 L1（insight）+ L4（archive）
                按 agent 显式调用的 start_long_term_update 写 L2/L3
```

## 详细设计

### 工具表面

risk 枚举扩展：

```rust
pub enum RiskLevel {
    Read,            // 白名单，直接跑
    RunLow,          // sandbox 内写、code_run in sandbox：白名单，直接跑
    RunMid,          // module 工具、写到 results/runs：首次确认后 session 内免审
    Destructive,     // 删除、写到 ~、code_run --shell-out-of-sandbox：每次都问
}
```

Tool 表（首版）：

| 工具 | risk | 说明 |
|---|---|---|
| `file_read` | Read | 读项目内或常规系统配置文件 |
| `file_list` | Read | 列目录 |
| `read_results_table` | Read | 读 TSV/CSV/Parquet，分页返回 |
| `read_run_log` | Read | 读历史 run 的 stdout/stderr/Log.final.out |
| `recall_memory` | Read | 主动查 L1/L2/L3，返回 top-K |
| `project_state` | Read | dump 当前 project.json + runs 列表 |
| `file_write` | RunLow if path∈sandbox else RunMid | 写文本文件 |
| `file_patch` | RunLow if path∈sandbox else RunMid | unified diff |
| `code_run` | RunLow if cwd∈sandbox else RunMid | pixi/system python/Rscript/shell |
| `web_scan` | RunLow | HTTP GET，返回正文（截断到 N KB） |
| `web_execute_js` | RunMid | headless browser（第一版可桩，留接口） |
| `run_qc` / `run_trim` / `run_star_align` / `run_deseq2` / ... | RunMid | module-derived，自动 |
| `update_working_checkpoint` | Read | 更新当前 todo / 进度笔记 |
| `start_long_term_update` | Read | 显式声明 `{layer:"L2"|"L3", scope:"global"|"project", body}` 写入记忆 |
| `task_done` | Read | 标记任务结束，触发结晶 |
| `ask_user` | Read | 暂停等用户回复，无超时 |

`task_done` 与 `ask_user` 都是 Read 风险——它们对系统无副作用，仅改 agent 控制流。

**Skill as tool**：L3 markdown 例：

```markdown
---
name: human-rna-seq-de
description: Run a full RNA-seq differential expression pipeline for human samples
triggers: ["RNA-seq", "差异表达", "DE genes", "DESeq2"]
inputs_schema:
  type: object
  properties:
    samples_csv: {type: string, description: "path to sample sheet"}
    genome: {type: string, enum: ["hg38","hg19"], default: "hg38"}
  required: [samples_csv]
risk_tier: RunMid
crystallized_calls:
  - tool: run_qc
    args_template: {fastq_dir: "{{ project.raw_dir }}"}
  - tool: run_star_align
    args_template: {sjdb_overhang: 99, ...}
---

## SOP

1. 先 QC...
2. ...
```

加载时，每个 skill 注册为 `skill_<slug>` ToolDef；agent 调用时，agent_loop 把 body 作为子任务 prompt 注入对话栈，进入嵌套循环（共享同一 session_id 与 checkpoint）。`crystallized_calls` 作为 reasoning hint 注入而非强制执行。

### Memory 召回

```rust
pub trait Recaller {
    async fn recall(&self, query: &PerceiveCtx, budget_tokens: usize) -> Result<Recall, AiError>;
}

pub struct FlashRecaller { provider: Arc<dyn ChatProvider>, model: String }
pub struct BM25Recaller { /* in-memory index over _index.json files */ }

pub struct CompositeRecaller {
    primary: Arc<FlashRecaller>,
    fallback: Arc<BM25Recaller>,
    timeout: Duration,
}
```

`CompositeRecaller::recall` 先尝试 flash，超时/出错降级到 BM25。flash prompt 接收 `{candidates: [{id, summary, tags}]}`，要求返回 `{picked: [id...], rationale}`。BM25 直接打分排序。

预算：`L0_full + recalled_top_k + project_snapshot ≤ 8K tokens`（可配置）。

### 结晶（crystallize）

触发点：

1. `task_done` 工具调用：写 L1（insight 摘要）+ L4（完整归档）。
2. agent 显式 `start_long_term_update`：写 L2（追加 facts.md section）或 L3（新建/更新 skill md）。
3. session abort（cancel/panic/崩溃）：尽量写 L4 半成品归档（标 `outcome:"interrupted"`）。

写入流程：

```rust
pub async fn crystallize_on_done(
    session: &AgentSession,
    store: &MemoryStore,
) -> Result<(), AiError> {
    let archive = build_archive(session);
    store.append_l4(&session.project, archive).await?;
    let insight = summarize_via_flash(session).await?; // {tag, summary, evidence_ref:archive_id}
    store.append_l1_global(insight).await?;
    Ok(())
}
```

L4 大小控制：单 session 一文件，超过 5MB 切 `<id>.part2.json` 并在 index 关联。

`update_working_checkpoint` 每 step 写 `<project>/agent/checkpoints/current.json`（fsync），供恢复使用。

### Sandbox & 审批

```rust
pub enum Bucket {
    ReadFs,
    SandboxWrite,
    ProjectModule(ModuleId),    // run_qc / run_star_align / ...
    CodeRunSandbox,
    CodeRunOutOfSandbox,
    Web,
    MemoryWrite,
    DestructiveDelete,
}

pub enum Decision {
    Allow,
    ApproveOnce(Bucket),
    AlwaysAsk,
}

pub struct SandboxPolicy {
    pub mode: PolicyMode,                 // Normal | FullPermission
    pub project_root: PathBuf,
    pub sandbox_dir: PathBuf,             // <project_root>/sandbox
    pub net_whitelist: Option<Vec<String>>, // None = 全放行
    pub net_log: bool,                    // FullPermission 默认 false
    pub approved_buckets: Mutex<HashSet<Bucket>>, // 仅当前 AgentSession 内有效
}

impl SandboxPolicy {
    pub fn classify(&self, call: &ProviderToolCall) -> (Bucket, Decision) { ... }
}
```

路径校验：`canonicalize(path)?.starts_with(canonicalize(allowed_root)?)`，拒绝 `..` 逃逸与 symlink 出根。

`code_run` 实现：

```rust
pub struct CodeRunRequest {
    pub language: Lang,                   // Python | R | Shell
    pub code: String,                     // inline 代码或脚本文件
    pub cwd: PathBuf,                     // 必须 in sandbox（除非 Destructive 通过）
    pub timeout: Duration,                // 默认 600s，可配
    pub runtime: Runtime,                 // Pixi | System | Custom(cmd)
}
```

执行流：写代码到 `cwd/.tmp_<uuid>.<ext>`，构造命令（pixi: `pixi run -- python xxx.py`），`Command::new(...).current_dir(cwd)`，调 `rb_core::subprocess::harden_for_gui` 后 `.spawn()`。stdout/stderr 流式 push 为 `RunEvent::Log`。`tokio::select!` on `child.wait()` vs `cancel.cancelled()`。

pixi 探测：`which pixi`，找不到时返回结构化错误 `{error: "pixi_not_found", install_url: "https://pixi.sh"}`，agent 看到能用 `ask_user` 引导安装。

### UI

新视图 `#agent`，三栏：

- **左栏**：会话/归档列表（L4 index），"新研究" 按钮，"导入归档"。
- **中栏**：对话流。消息类型：
  - User text
  - Assistant text + reasoning（折叠）
  - ToolCall card（工具名、风险标签、参数 JSON 折叠、状态：running/awaiting_approval/done/failed）
  - ApproveCard（首次 RunMid bucket 或 AlwaysAsk）：bucket 描述、本 session 内勾选 "always allow this bucket"、approve / reject 按钮
  - Memory recall card（可关）：本回合召回了哪些条目
  - Crystallize card：写入哪一层、scope、文件路径链接
- **右栏**（可折叠）：working checkpoint todo（agent 维护）、当前 sandbox 文件树、网络日志 tail。

顶部工具条：模型选择、temperature、`Full Permission` toggle（点击弹确认对话框，明确告知风险）、Cancel button。

### Tauri 命令

| 命令 | 入参 | 出参 |
|---|---|---|
| `agent_start_session` | `{title?, model?}` | `{session_id}` |
| `agent_send` | `{session_id, text}` | `()`（流式事件） |
| `agent_approve` | `{session_id, call_id, edited_args?, bucket_always?:bool}` | `()` |
| `agent_reject` | `{session_id, call_id, reason?}` | `()` |
| `agent_cancel` | `{session_id}` | `()` |
| `agent_set_full_permission` | `{session_id, enabled}` | `()` |
| `agent_list_archives` | `{project_id}` | `[ArchiveSummary]` |
| `agent_load_archive` | `{archive_id}` | `Archive`（只读浏览） |
| `agent_list_skills` | `{}` | `{global:[...], project:[...]}` |
| `agent_edit_memory` | `{path, content}` | `()`（用户编辑 L0/L2/L3） |

事件流：单 channel `agent-stream`，扩展 `ChatStreamEvent` 增 `Memory{recalled}` / `Checkpoint{todo}` / `Crystallize{layer, scope, path}` 三 variant。

### 配置（AiConfig 扩展）

```toml
[agent]
default_model = "claude-sonnet-4-6"
flash_recall_model = "claude-haiku-4-5"     # 召回用
flash_recall_timeout_ms = 3000
recall_budget_tokens = 4096
working_token_budget = 8192

[agent.code_run]
runtime = "pixi"                            # pixi | system | custom
default_timeout_secs = 600
custom_command = ""                         # runtime=custom 时使用

[agent.sandbox]
sandbox_dirname = "sandbox"                 # <project>/<sandbox_dirname>/

[agent.network]
mode = "allow_all"                          # allow_all | whitelist | denied
whitelist = []                              # mode=whitelist 时
log_enabled = true                          # FullPermission 模式下强制 false
```

## 错误处理

- **Provider 失败**：reqwest 60s read timeout；流中断作 `Finish::Error`，agent 自决 retry / `ask_user`。
- **工具失败**：错误以 `Message::Tool {result:{error:..}}` 回填，agent 自我纠错；同一工具连续 N=5 次失败 → 强制 `ask_user`。
- **内存写入失败**：fsync 失败时 log warn，继续（不阻塞 agent）。
- **Cancel**：每 step 检查 token；运行中子进程 `child.kill()`；写 L4 归档 `outcome:"cancelled"`。
- **Panic safety**：crystallize 用 `tokio::spawn` + drop guard，尽量写 L4 半成品。
- **路径逃逸**：`canonicalize` 后比对，越界返回 `AiError::SandboxViolation`。

## 并发

同一 project 同一时刻只允许一个 active agent session。`Project` 上加 `Mutex<Option<AgentHandle>>`；新 `agent_start_session` 若已有 active session 则返回 `AlreadyRunning` 错误，frontend 提示用户先 Cancel 或继续旧会话。

memory 文件并发：

- L0/L2 用户编辑 + agent 写：写时 `fs2::lock_exclusive` + 临时文件原子 rename。
- L1 append-only：行级原子（单 write 调用），多 session 安全。
- L3 _index.json：写时全量重写 + lock。
- L4 _index.json：append session entry，写时 lock。

## 测试策略

- **单元**：
  - `sandbox::policy::classify` 全工具 × 多路径表驱动测。
  - `memory::store` 写、读、index 维护、5MB 切片。
  - `memory::recall` BM25 打分快照；FlashRecaller 用 wiremock。
  - `agent_loop::execute` 工具分发分支。
  - `crystallize_on_done` 输出文件 snapshot。
- **集成**（`crates/rb-ai/tests/`）：
  - `wiremock` mock provider，跑完整 5 步流程（write file, code_run, ask_user, run_qc 模拟, task_done）。
  - 取消测试：发出 cancel 后 ≤200ms 子进程被 kill，archive 标 cancelled。
  - 恢复测试：写 checkpoint，进程重启后能 resume。
- **e2e**（`crates/rb-app/tests/e2e/`）：mock provider 给 RNA-seq DE 工具调用序列，断言 archive + L3 skill 文件被写出。
- **回归**：保留 `module_derived` 现有 unit test。
- **不写**：embedding/向量库（不引入此依赖）。

## 迁移与发版

- **数据**：旧 `<project>/chats/` 目录在启动时检测到则 log warn"旧 chat 数据已弃用，可手动备份后删除"，不自动删。
- **API**：`chat_*` 命令删除；`#chat` 路由保留 30 天 alias 重定向到 `#agent`，提示"chat 模式已替换为 agent 模式"。
- **CHANGELOG**：标 BREAKING；列删除的命令、删除的视图。
- **版本**：本变更落 `v0.3.0`。

## 风险与开放问题

| 风险 | 缓解 |
|---|---|
| flash 召回 prompt 过长导致延迟 | 限制候选数（默认 ≤32 条 _index 摘要）；超时降级 BM25 |
| pixi 没装 | 结构化错误 + agent 主动 ask_user 引导，提供 system runtime fallback |
| 自由 `code_run` 写满磁盘 | sandbox 目录 GC（每 session 结束清理 .tmp_*.py）；可选磁盘配额（用 `du` 简单估算，超阈 ask_user） |
| 网络滥用 | 网络日志默认开；future 加白名单 |
| 长 session token 爆 | working_token_budget 软上限；超额时 agent 主动 `update_working_checkpoint` 总结后压缩历史（截断早期 tool result，留摘要） |
| memory 错归类（项目内容上浮全局） | agent 调 `start_long_term_update` 必须显式 scope；L0 prompt 给指引；用户可手动改文件 |
| 多 session 并发写 memory | fs2 lock + 原子 rename |
| 旧用户视图链接坏掉 | `#chat` alias 30 天 |

待 review 时拍板：

- 网络白名单 vs 全放行的最终默认值（当前定 `allow_all + 日志`）。
- L4 是否考虑 SQLite 替代 JSON 文件（当前否，5MB 切片够用）。
- skill body 在嵌套调用时的 token 预算策略（暂定共享 working budget，超额 LLM 自压缩）。

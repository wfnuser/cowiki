# Native Codex Agent Sidebar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Use superpowers:subagent-driven-development only if the user chooses delegated execution. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 CoWiki 现有右侧栏以原生对话、工具步骤和审批运行本机 Codex，不再通过 Terminal 交互。

**Architecture:** Rust/Tauri 拥有 app-server 进程、会话、权限和持久化；React 只消费类型化快照及增量事件。复用 LocalEngine 的 Draft / Agent Change / Review 语义，运行中 Change 的启动和关闭使用统一互斥门。Provider 适配器与 UI 分离，第一版只支持 Codex。

**Tech Stack:** Rust、Tauri 2、serde/serde_json、现有进程和文件设施；React 19、TypeScript、现有 Radix/shadcn 组件；Node test、Playwright、Rust unit tests。

**Spec:** [2026-11-05-native-codex-agent-design.md](../specs/2026-11-05-native-codex-agent-design.md)

## Global Constraints

- 代码基线 `dev@d469a21037d95705ed77bf48b703f56c3fe1943b`；工作分支 `feat/native-codex-agent`；设计文档见上方 Spec 链接。
- 首版基于已检查的 Codex CLI `0.163.0`；协议必须从该版本核对，不假定旧版或后续版完全兼容。
- 不增加全局顶栏按钮；新任务、历史、模型等次级功能在右栏内部收纳。
- 不引入 Node sidecar、不使用 PTY 或交互终端回退。
- Claude 等 Provider 配置保留，未接入入口明确不可用；不得静默替换 Provider。
- 本地优先：Cloud 浏览器不运行本机进程；本地 Shared Space 的修改仍需既有 Review/同步。
- 不自动 commit/push/merge Space 内容；不对真实团队 Space 做破坏性测试。
- 默认工作区写入、网络受限，不继承 full-access 或绕过审批配置；未知审批拒绝。
- 收起面板不终止任务；退出应用清理应用拥有的进程组；重启不自动重放任务。
- 测试先行：每项先写失败测试，再做最小实现，通过后提交；伪服务测试和真实模型验收分别记录。
- 2026-11-05 已实施，证据见 [验收记录](../../testing/native-codex-agent.md)；实施中按用户要求加入 Grok 与经 ACP 的 Claude（见 spec 第 0 节）。

## 任务总览与依赖

| ID | 交付目标 | 依赖 | 验收重点 | 状态 |
| --- | --- | --- | --- | --- |
| T1 | 固定协议、应用状态与 fake server | 无 | 乱序/重复/跨会话事件不污染状态 | 完成（47845fd、bf506a2） |
| T2 | Codex stdio 适配器 | T1 | 初始化、流式回合、中断、超时与退出 | 完成（47845fd；另加 ACP 适配器） |
| T3 | 运行注册表与 Change 互斥门 | T2 | 启动/停止/合并竞态；目录绑定 | 完成（47845fd） |
| T4 | readiness、登录、审批与权限 | T2、T3 | 无终端登录；默认权限不能被本机设置扩大 | 完成；应用内登录未真实重走 |
| T5 | 私有持久化与恢复 | T3、T4 | 重启不重放；恢复相同 thread；损坏文件保留 | 完成（47845fd） |
| T6 | 原生右栏 UI 与状态订阅 | T1、T4、T5 | 流式消息/工具/审批；关闭不杀任务；顶栏不增加 | 完成；模型选择未做 |
| T7 | 采集 / Review / 应用退出集成 | T3–T6 | 所有旧入口不绕过新运行管理器 | 完成；入口无单独浏览器测试 |
| T8 | 回归、macOS 实测与交付 | T1–T7 | 真实闭环、失败路径、证据和顶栏收纳建议 | 回归与真实 Codex 闭环通过；真实窗口演示、Grok/Claude 真实回合未验收 |

每项 Evidence 写入该项实现 commit/PR、执行命令与结果；T8 汇总到 `docs/testing/native-codex-agent.md`。计划本身不是测试通过证据。

## 文件边界

| 路径 | 职责 |
| --- | --- |
| `web/src/lib/agent-session.ts` | 应用状态、事件 reducer、快照/序号判断；不调用 Tauri |
| `web/src/agent-api.ts` | 类型化 invoke/listen 与订阅恢复；唯一前端传输入口 |
| `web/src-tauri/src/agent_runtime/{mod,contract,rpc,codex}.rs` | 应用契约、JSON 行 RPC、Codex 协议适配 |
| `web/src-tauri/src/agent_runtime/{registry,commands,permissions,auth,store}.rs` | 运行所有权、目录核验、审批、登录和本机记录 |
| `web/src-tauri/src/agent_execution_gate.rs` | 旧终端与新运行器共用的启动/关闭互斥门 |
| `web/src/components/agent/{AgentPanel,AgentTranscript,AgentApprovalCard,AgentComposer}.tsx` | 原生侧栏组件；使用现有设计系统 |
| `web/src/components/agent/useAgentSessions.ts` | 当前 Space 视图；不拥有进程生命周期 |
| `web/src/pages/MainLayout.tsx` | 替换旧侧栏入口及已有任务回调，不加入新的顶栏控件 |
| `web/src-tauri/src/{lib,terminal}.rs` | 命令注册、退出清理、终端互斥桥接；避免增加第二套关闭判断 |
| `web/tests/fixtures/fake-codex-app-server.mjs` | 受测协议、启动延迟、审批、乱序和崩溃场景 |
| `web/tests/{agent-session,agent-api,agent-native-integration}.test.ts` | 状态、传输与入口回归 |
| `web/tests/browser/native-agent.spec.ts` | 无真实账号的 UI 交互验收 |

新增 Rust 模块内部放 `#[cfg(test)] mod tests`；不为测试把任意进程执行接口暴露给 renderer。

## T1：协议与可重复测试基座

**Files:** 新建 `agent-session.ts`、`agent_runtime/contract.rs`、fake server、`agent-session.test.ts`；修改 `web/package.json` 纳入测试。

**Interfaces:** 后续 UI 和 RPC 归一化共同使用如下应用契约；不是直接暴露 Codex 原始对象。

```ts
export type AgentStatus = 'starting' | 'idle' | 'running' | 'waiting' | 'stopping' | 'stopped' | 'failed' | 'interrupted';
export type AgentScope = { spaceSlug: string; mode: 'live' | 'background'; changeId: string | null };
export type AgentMessage = { id: string; role: 'user' | 'assistant'; text: string; turnId: string | null };
export type AgentTool = { id: string; turnId: string; name: string; status: 'running' | 'completed' | 'failed'; output: string; truncated: boolean };
export type AgentApproval = { id: string; turnId: string; kind: 'command' | 'file'; description: string; paths: string[] };
export type AgentSnapshot = AgentScope & {
  sessionId: string; provider: 'codex'; threadId: string | null; seq: number;
  status: AgentStatus; messages: AgentMessage[]; tools: AgentTool[];
  approvals: AgentApproval[]; error: string | null;
};
export type AgentEvent = { sessionId: string; seq: number; snapshot: AgentSnapshot };
export function acceptSnapshot(current: AgentSnapshot | null, event: AgentEvent): AgentSnapshot;
```

先用带序号的完整快照建立正确性；T6 如改成文本 delta，必须保留 gap 检测与快照恢复，不能依赖事件永不丢失。后端聚合流式片段并限制通知频率，避免每个 token 复制整个 transcript。

- [ ] 用 `codex --version`、`codex app-server generate-json-schema --help` 核对目标版本，把必要的 initialize、thread/start/resume、turn/start/interrupt、审批、账户方法样例放进 fake server；记录协议来源。不运行真实任务来猜字段。
- [ ] 新建失败测试（完整样本构造函数写在同一测试文件）：

```ts
const initial = (id: string): AgentSnapshot => ({ sessionId: id, provider: 'codex', threadId: null, seq: 2, status: 'idle', spaceSlug: 'demo', mode: 'live', changeId: null, messages: [], tools: [], approvals: [], error: null });
test('rejects another session and older snapshots', () => {
  const a = initial('a');
  assert.equal(acceptSnapshot(a, { sessionId: 'b', seq: 3, snapshot: initial('b') }), a);
  assert.equal(acceptSnapshot(a, { sessionId: 'a', seq: 1, snapshot: { ...a, seq: 1 } }), a);
});
```

- [ ] 运行 `cd web && node --experimental-strip-types --test tests/agent-session.test.ts`，确认因缺少契约/函数失败；实现验证 envelope/sessionId/seq 一致、只接受更新快照的纯函数，Rust serde 采用同样 camelCase 字段。
- [ ] fake server 按 `process.argv[2]` 选择 `normal`、`early-complete`、`start-delay`、`approval`、`unknown-request`、`crash`、`oversize`、`timeout`；只写 JSON 行到 stdout，诊断走 stderr，不读取真实凭证。
- [ ] 测试通过后提交 `test(agent): define native session contract and fake app server`。

**Acceptance:** 重复、乱序、错误会话/空间的样例不会串消息；所有 fake 场景可无网络重放。

## T2：Codex app-server 适配器

**Files:** 新建 `agent_runtime/{mod,rpc,codex}.rs`；复用 T1 fixture。

**Interfaces:** `RpcClient::request(method: &str, params: serde_json::Value) -> Result<serde_json::Value, String>`；`CodexSession::start_turn(text: &str) -> Result<(), String>`、`interrupt() -> Result<(), String>`、`shutdown() -> Result<(), String>`。适配器通过内部 channel 发事件，不能直接操作 React 或创建 Change。

- [ ] 在 `rpc.rs` 先加序号关联测试：

```rust
#[test]
fn responses_follow_request_ids_not_arrival_order() {
    let mut pending = PendingRequests::default();
    pending.insert(1, "initialize");
    pending.insert(2, "thread/start");
    assert_eq!(pending.take(2).unwrap(), "thread/start");
    assert_eq!(pending.take(1).unwrap(), "initialize");
    assert!(pending.take(2).is_none());
}
```

`PendingRequests<T>` 在 `rpc.rs` 定义，使用 `HashMap<u64, T>`，提供 `insert(id, value)` 和 `take(id) -> Option<T>`；测试以操作名作为 value，生产以响应 channel/计时器作为 value，共用请求 ID 管理，不把结果按 FIFO 匹配。

- [ ] `cargo test --manifest-path web/src-tauri/Cargo.toml agent_runtime::rpc` 跑红；实现 `Command` 固定 argv、独立 stdin/stdout/stderr、initialize/initialized 握手及响应/通知分流，不使用 shell 拼接。
- [ ] `codex.rs` 保存 thread/turn 身份和 `stop_requested`；停止不能被下一轮继承，早到 completed 不能被迟到 start 响应覆盖。加入 fake `early-complete`、`start-delay`、`timeout` 回归；超时终止可能仍在执行的 Provider，不自动重发任务。
- [ ] 限制单行/工具输出/诊断缓冲区（分别 8 MiB / 1 MiB / 64 KiB），截断展示有标识；子进程退出拒绝所有 pending 请求；只清理当前 session 拥有的进程组。
- [ ] 定向测试转绿后提交 `feat(agent): add structured Codex app-server transport`。

**Acceptance:** 一条 fake 会话可发起并完成两轮任务；停止与崩溃不会留下悬挂请求或误停后续回合。

## T3：运行管理与统一 Change 互斥门

**Files:** 新建 `agent_runtime/{registry,commands}.rs`、`agent_execution_gate.rs`；修改 `terminal.rs`、`lib.rs`、必要的 `local_engine/agent_changes.rs` 查询接口。

**Interfaces:** renderer 仅传 `CreateSession { space_slug, mode, change_id }`；后端 `SessionRegistry::create(...)` 从 LocalEngine 找 cwd。统一 gate 提供 `reserve_start(scope) -> StartLease`、`begin_close(space_slug, change_id) -> CloseLease`，lease 生命周期覆盖启动、停止和文件合并。操作顺序固定为 gate → LocalEngine mutation lock。

- [ ] 先写互斥失败测试：

```rust
#[test]
fn starting_session_blocks_change_close() {
    let gate = AgentExecutionGate::default();
    let scope = ExecutionScope::background("space-a", "change-a");
    let lease = gate.reserve_start(scope).unwrap();
    assert!(gate.begin_close("space-a", "change-a").is_err());
    drop(lease);
    assert!(gate.begin_close("space-a", "change-a").is_ok());
}
```

`ExecutionScope` 定义在 gate 文件，表示 Space + 可选 Change；Live scope 阻止该 Space 的所有 Change 关闭。`StartLease`/`CloseLease` 使用 Drop 释放注册状态。

- [ ] 运行 `cargo test --manifest-path web/src-tauri/Cargo.toml agent_execution_gate`；实现两类 reservation 互斥，把现有 TerminalState 的关闭保护接入同一个 gate，删除重复的“先检查后操作”路径。
- [ ] registry 每次 start/resume 核验当前 Space 规范路径、Change 所属 Space 和开放状态；renderer 不可自定绝对目录。拒绝已合并/丢弃 Change，拒绝第二个并发 turn。
- [ ] 保持 lease 至“确认 Provider 与子进程结束”，不是收到 turn/completed 就释放；空闲 session 关闭 Change 时先撤销其可启动资格再收进程。若停止失败，保留阻塞状态。增加 Live、等待审批、停止中、close/start 并发和伪造 cwd 测试。
- [ ] 跑原有 terminal / Agent Change Rust 测试并提交 `feat(agent): guard native sessions and review mutations atomically`。

**Acceptance:** 不存在 Agent 写入与 merge/discard 同时成功的竞态；旧终端入口即使仍可被调用，也不能绕过互斥保护。

## T4：readiness、账户登录与审批

**Files:** 新建 `agent_runtime/{permissions,auth}.rs`，扩展 `commands.rs`；修改 `web/src/lib/agents.ts` 增加独立原生能力判断，不删除 AgentKind。

**Interfaces:** 命令 `agent_probe_native`、`agent_login_start`、`agent_login_cancel`、`agent_answer_approval({sessionId, approvalId, decision: 'allow' | 'deny'})`。登录返回公开展示状态和经验证的 URL，不返回 token；审批绑定服务端持有的原始 RPC request。

- [ ] 先写 fail-closed 测试：

```rust
#[test]
fn permission_escalation_is_not_a_file_approval() {
    assert!(classify_approval("item/permissions/requestApproval").is_err());
    assert!(classify_approval("item/commandExecution/requestApproval").is_ok());
    assert!(classify_approval("item/fileChange/requestApproval").is_ok());
}
```

`classify_approval(&str) -> Result<ApprovalKind, String>` 位于 permissions.rs。原生侧栏仅显示该映射内请求；越界写入、网络扩权、永久放行不因选择 allow 而获得授权。

- [ ] 用 `cargo test --manifest-path web/src-tauri/Cargo.toml agent_runtime::permissions` 跑红；根据固定协议生成显式 sandbox/approval 配置，并校验生效设置。无法覆盖本机 full-access 时拒绝启动。
- [ ] 按目标版本实现 account read/login start/cancel 和完成通知；系统浏览器只打开认证 URL 校验函数认可的 HTTPS issuer，要求无 userinfo，拒绝 javascript/file、任意域名和来自 assistant 消息的 URL。取消/失败/重复点击不产生第二个登录进程。
- [ ] 测试过期、重复、跨会话审批以及等待审批时停止；未知 user-input/扩权请求显式报不支持，不让 Provider 无限等待。配置不保留长期批准；credentials 不出现在快照、日志或 Space 文件。
- [ ] 跑 readiness 和原有 Agent contract 回归后提交 `feat(agent): add safe native login and approval flow`。

**Acceptance:** 未安装/未登录/不兼容分别可识别；无 Terminal 登录；恶意或陈旧审批不能授权执行。

## T5：持久化、快照订阅与显式恢复

**Files:** 新建 `agent_runtime/store.rs`、`web/src/agent-api.ts`、`web/tests/agent-api.test.ts`；扩展 registry/commands。

**Interfaces:** `agent_list_sessions({spaceSlug})`、`agent_snapshot({sessionId})`、`agent_resume({sessionId})`、`agent_send({sessionId,text})`、`agent_stop({sessionId})`；`agent:event` 使用 T1 envelope。`SessionStore::load/save` 输入带 `schema_version: 1` 的应用私有记录（包含规范 cwd，renderer 快照不需暴露该字段）。

- [ ] Rust store 失败测试：

```rust
#[test]
fn startup_never_replays_running_work() {
    assert_eq!(restore_status(AgentStatus::Running), AgentStatus::Interrupted);
    assert_eq!(restore_status(AgentStatus::Waiting), AgentStatus::Interrupted);
    assert_eq!(restore_status(AgentStatus::Stopped), AgentStatus::Stopped);
}
```

`restore_status` 为 store.rs 纯函数；恢复时 pending RPC 审批全部失效，用户必须显式继续。

- [ ] 跑红后实现应用私有目录、受限权限、临时文件+原子替换、schema 检查和损坏文件备份；拒绝 symlink 越界写。只存展示/身份状态，不存密钥或任意原始环境变量。
- [ ] 前端订阅先注册 listener、缓冲事件，再取 snapshot，按序号收敛；dispose 只取消订阅。加入快照与事件交错、重复事件、切换 Space 迟到响应测试；不得在订阅或组件 mount 时自动 send。
- [ ] resume 重校验 T3 目录/Change 后调用 thread/resume；线程不存在、目录变化、恢复失败均保留旧记录和明确错误，不创建新 thread 冒充继续。正常继续复用 thread ID，输入采用新的 turn ID。
- [ ] Rust store、前端传输测试转绿后提交 `feat(agent): persist sessions and recover without replay`。

**Acceptance:** 杀掉测试 app-server 并重开 UI 后仅显示中断记录；点击继续才恢复，不重复原任务或旧审批。

## T6：原生侧栏

**Files:** 新建四个 agent 组件和 `useAgentSessions.ts`；新建浏览器测试；修改 MainLayout 仅替换侧栏接入，不搬动其他功能。

**Interfaces:** `AgentPanel({spaceSlug, defaultAgent, openRequest, onOpenRequestHandled, onClose, onOpenChanges})`；`openRequest` 保留原有 new-change / existing-change 数据结构，后端重新解析目录。`useAgentSessions(spaceSlug)` 返回快照、active session 和 T5 操作，不拥有 process cleanup。

- [ ] 在 Playwright desktop mock 中写行为测试（测试页使用既有 MainLayout fixture 机制，不建立第二套产品 UI）：

```ts
test('collapsing the panel does not stop a running task', async ({ page }) => {
  await page.getByRole('button', { name: 'Agent', exact: true }).click();
  await page.getByRole('textbox', { name: 'Message agent' }).fill('Summarize the test note');
  await page.getByRole('button', { name: 'Send message' }).click();
  await page.getByRole('button', { name: 'Close agent panel' }).click();
  await page.getByRole('button', { name: 'Agent', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Stop task' })).toBeVisible();
});
```

fixture 的 invoke recorder 另断言 agent_send 恰好一次、agent_stop 零次，避免只验证按钮存在。

- [ ] `cd web && npx playwright test tests/browser/native-agent.spec.ts` 跑红；实现正常 Markdown 消息、默认折叠工具步骤、审批卡片和输入框。Markdown 禁止 raw HTML，长输出限制沿用 T2。
- [ ] 侧栏头部会话选择 + 状态 + 关闭；新任务/历史放 `⋯`，模型选项只在侧栏内部显示经服务端发现的可用项；单会话 busy 时禁用再次发送，停止完成后可继续输入。
- [ ] 追加 320px 宽侧栏、键盘焦点、审批卡片、切换 Space、迟到事件、组件卸载重挂载、失败重试测试；reducer 与订阅测试须覆盖真实操作而非仅源代码正则。
- [ ] `npm run lint`、`npm run build` 和 UI 测试通过后提交 `feat(agent): render native conversation sidebar`。

**Acceptance:** 对话/工具/审批可用，无 xterm；面板收起不终止任务，无跨 Space 内容；顶栏按钮数不变。

## T7：旧入口、Review 与应用退出

**Files:** 修改 `web/src/pages/MainLayout.tsx`、`web/src/components/AddSourceDialog.tsx`、`web/src/components/review/LocalReviewDetail.tsx`、`web/src/agent-api.ts`、`web/src-tauri/src/lib.rs`；新建 `web/tests/agent-native-integration.test.ts`。

**Interfaces:** 所有旧 AgentPanelOpenRequest 统一进入 AgentPanel，创建 Change 仍调用现有 `createLocalAgentChange`。registry 提供 `shutdown_all()`，只清理应用拥有的进程组；退出遵循既有保存和 checkpoint 生命周期。

- [ ] 用 T1 fake server/桌面 mock 添加测试：采集生成初始任务恰好发送一次，Review 继续恢复同 Change/thread；默认 Provider 为 Claude 时显示未支持，不能调用 terminal_create 或偷偷选 Codex。
- [ ] 跑红后接入 new-change / existing-change 回调；保留 source 页面、diff、人工 merge 和刷新逻辑。新增跨 Space request ID 去重，失败不能把请求永久标为成功处理。
- [ ] 在退出钩子中等待 native runtime 停稳并写入会话状态；超时保留可恢复中断状态。测试一个独立的非 CoWiki 子进程不会被误杀。
- [ ] 更新旧 terminal UI 结构断言到新侧栏语义；保留旧 PTY 后端安全测试，不靠删除测试掩盖回归。断言浏览器模式不会调用本机命令，合入 Draft 不显示为已上传。
- [ ] 完成 `npm run test:source-ingest`、`npm run test:agent-contract`、`npm run test:review-preview`、新增测试和 Rust 退出测试后提交 `feat(agent): connect native sessions to ingest and review`。

**Acceptance:** 用户原来的任务入口均可到达新侧栏，且任务模式、权限、Review 和退出行为一致。

## T8：完整验收与交付

**Files:** 新建 `docs/testing/native-codex-agent.md`；更新本计划的复选框和证据链接；不修改产品 Roadmap 的旧承诺。

- [ ] 全量回归：`cd web && npm test && npm run lint && npm run build && npm run test:browser`；Rust：`cargo fmt --manifest-path web/src-tauri/Cargo.toml --check` 和 `cargo test --manifest-path web/src-tauri/Cargo.toml`。记录实际命令、版本、结果和既有失败，不写未经运行的 pass。
- [ ] Mac mini 优先执行耗时构建，使用独立功能分支；临时本地 Space 演示“输入 → 工具审批 → 文件修改 → 停止 → Review → 合入 Draft”，核对 changed files；实际模型调用消耗账号额度，单独记录与 fake 测试的区别。
- [ ] 另测启动中停止、拒绝审批、断网、app-server 崩溃、Space 切换、应用重启、thread 消失和 Change 已关闭；截图/录屏不包含凭证与私人内容。
- [ ] 在验收页列出通过/失败/未测、commit、CLI/OS 版本、复现步骤与截图路径。附一段顶栏收纳建议，只讨论哪些低频按钮可进入 `⋯`，不擅自实施新的顶栏功能。
- [ ] 更新任务状态并提交 `docs(agent): record native sidebar verification`；仅在全部必需验收通过后标记完成。推 feature branch 不等于合并 dev/main 或部署，后者另需用户指示。

**Acceptance:** 设计 §6 的十项均有对应可复查证据；未完成真实 macOS/账户流程时明确列为未验收。

# CoWiki 原生 Agent 侧栏：Codex 第一版

日期：2026-11-05。基线：`dev@d469a21037d95705ed77bf48b703f56c3fe1943b`。

状态：用户已同意 Codex app-server 优先、独立 Provider 适配器和不增加顶栏按钮的方向，并要求推送和拆解任务；[实施计划](../plans/2026-11-05-native-codex-agent.md)已写入。2026-11-05 已在 `feat/native-codex-agent` 实施，验收记录见 [native-codex-agent 验收](../../testing/native-codex-agent.md)。

## 0. 实施时的调整（2026-11-05，用户在实施中提出）

- **Grok 一并支持**（“grok 這些可以也支持一下”）。本机 Grok Build 1.0.67 提供 `grok agent stdio`，讲 Agent Client Protocol（ACP v1）。因此新增一个通用 ACP 适配器，Grok 第一个使用；OpenCode（`opencode acp`）、Hermes（`hermes acp`）走同一适配器，是否可用由就绪探测在运行时的 ACP 握手确认，不预先声称支持。
- **Claude 直接驱动 `claude`，不需要另装适配器**（先是“claude 不能也接 acp 嗎？”，随后“最好是用户只装了 claude 我们的应用打开就可以用”）。Claude Code 本体没有 ACP 模式（2.1.318 已核对）；ACP 的 `claude-agent-acp` 与 Bramble 用的 Claude Agent SDK 都是在 stdio 上用 `claude -p --input-format stream-json --output-format stream-json --permission-prompt-tool stdio` 与 CLI 对话。CoWiki 在 Rust 里直接讲这套控制协议（按 `@anthropic-ai/claude-agent-sdk` 0.3.314 的类型定义和真实 2.1.318 核对），所以只需装好并登录 `claude`，不需要 Node 或任何适配器。这是 SDK 使用的协议，不是公开文档承诺的稳定接口：握手失败、权限模式不符时拒绝启动并提示 `claude update`，不回退到终端。
- Claude 的权限边界：
  - 以 `acceptEdits` 启动，初始化后核对 CLI 报告的 `current_permission_mode`，不符就拒绝启动。工作目录内的文件编辑与 Codex 的 workspace-write 一样直接生效。
  - 初始化时注册 PreToolUse hook，工作目录外的 Write/Edit/MultiEdit/NotebookEdit 一律拒绝，不受用户 allow 规则或 additionalDirectories 影响。
  - 其余需要许可的操作（多数 Bash 命令、WebFetch 等）走 `can_use_tool`，变成审批卡片，只放行一次，从不回写 `updatedPermissions`。
  - 只加载用户级设置（`--setting-sources user`，登录、代理、模型照常）；Space 自己的 `.claude/` 设置和 hook 不加载；`--strict-mcp-config` 只挂 CoWiki 自己的只读 MCP。
  - 禁用需要终端交互或会在 CoWiki 外排程的工具（AskUserQuestion、ExitPlanMode、Cron*、ScheduleWakeup、RemoteTrigger）。
- 就绪探测与登录：Claude 用 `claude auth status --json`，再做一次 stream-json 握手，不调用模型；登录由 `claude auth login` 打开浏览器完成。
- 因此第 1 节“第一版只接 Codex”、“不做 Claude SDK 接入”改为：Codex 用 app-server，Claude 用 CLI 自带的 stream-json 控制协议，其它 Agent 用 ACP；Antigravity CLI 没有结构化协议，仍明确标为暂不可用。
- ACP 的权限边界：只选 `allow_once` / `reject_once`，从不选 `allow_always`；会话若以绕过审批的模式启动（如 `bypassPermissions`、`acceptEdits`、`dontAsk`），先 `session/set_mode` 切回 `default`，切不回就拒绝启动；不向 Agent 提供 CoWiki 的文件系统或终端（`fs/*`、`terminal/*` 返回不支持）。Grok 额外以 `GROK_SANDBOX=workspace` 启动（进程级 Seatbelt：只写工作目录、/tmp、~/.grok），其子进程网络在 macOS 上是否被限制未验证。
- 应用契约里消息与工具步骤合并为一条按时间排序的 `items` 时间线（`{type: 'message' | 'tool'}`），其余字段与计划 T1 一致。

## 1. 用户结果与范围

用户在现有右侧栏输入任务，直接阅读流式回答、工具执行摘要和权限确认；不再启动交互终端，也不把 ANSI 输出伪装成聊天。使用 CoWiki 现有字体、颜色和组件实现应用内界面。

第一版只接本机 Codex app-server。保留现有 Live Draft / Background Agent Change 两种工作语义、采集任务入口及 Review 的继续操作。新建后台任务仍由 CoWiki 创建隔离 worktree，人工 Review 后才能合入 Draft；合入 Draft 不等于上传 Cloud。

不做 Claude SDK 接入、远程 Agent 服务、调度队列、自动多 Agent 编排、自动合并、运行中 steer 或新顶栏功能。Claude 和其他 Provider 的配置保留，但未接入原生协议的启动入口明确标为暂不可用，不静默改用 Codex 或终端。旧终端实现不作为运行失败时的回退，也不在这次顺手清理所有相关代码。

Cloud 浏览器保留现有阅读与 Review 能力，不假装可以访问本机进程；已克隆到桌面的 Shared Space 可以使用本地 Agent。共用的任务状态与 Review 概念不分叉。

## 2. 选择的架构

采用“统一应用事件 + 各 Provider 适配器”。Codex 通过 Tauri/Rust 启动本机 `codex app-server --listen stdio://`，使用结构化请求、响应、通知；前端不持有进程句柄，不接收任意命令执行接口。

不采用：把旧 PTY 隐藏后解析文字输出（审批、恢复和状态不可靠）；也不先强制所有 Provider 迁移到一个通用协议（扩大第一版适配范围）。未来 Claude 适配器如何打包 SDK 另行设计，本次不引入 Node sidecar。

边界按职责拆分：

- **协议适配器**：初始化、能力检测、请求关联、超时、通知归一化、取消和进程退出。首版基于本机已检查的 Codex CLI `0.163.0`，实施时生成/核对该版本协议类型，不声称任意旧版本兼容。
- **桌面运行管理器**：拥有会话、Space/Change 绑定、持久化、审批及任务生命周期。开始前从 LocalEngine 解析真实目录，不信任 renderer 提交的 cwd。
- **应用事件契约**：消息、工具步骤、状态、审批、错误，按 session / thread / turn / item 标识隔离；每个会话提供带递增序号的快照和增量事件，重连不重复追加内容。
- **侧栏组件**：会话选择、消息列表、折叠步骤、审批卡片和输入框。组件卸载不终止运行；重新打开从运行管理器恢复快照。

预期文件边界：新增 `web/src-tauri/src/agent_runtime/` 和 `web/src/components/agent/`；修改 `MainLayout.tsx`、Tauri 命令注册、Agent readiness/能力判断和 Change 关闭保护。现有 `AgentTerminalPanel` 不再是侧栏入口。避免继续扩大 `terminal.rs` 和 `MainLayout.tsx`。

## 3. 用户交互与顶栏约束

沿用已有 Agent 开关和可拖拽侧栏宽度，不增加全局顶栏按钮。侧栏头部只保留当前会话、状态和关闭；新任务、会话历史等次级操作放到侧栏 `⋯`，模型等配置在侧栏内部收纳，不占全局顶栏。

正文用正常文档排版显示消息；工具步骤默认显示名称和执行状态，可展开查看受限长度的结果。权限请求显示实际命令/路径、原因和明确的允许/拒绝操作，不能仅靠颜色区分状态。输入区包含发送和运行时的停止按钮；同一会话运行时不接受重复提交，停止完成后可继续追问。

关闭侧栏只是隐藏。切换 Space 后只显示当前 Space 会话，旧 Space 的后台任务继续但不会把消息或导航写入新 Space；切回可恢复查看。退出应用则结束所管理的进程，不将“后台运行”描述为应用退出后仍在运行。

顶栏按钮的整体收纳另交建议，不在这次改动阅读、评论、血缘、同步等入口。

## 4. 生命周期、恢复与 Review

运行状态包括启动中、空闲、执行中、等待用户、停止中、已停止和失败。停止请求与实际停止确认分开：启动阶段点击停止时记录取消意图，收到对应 turn 后立即中断；迟到事件不能让已结束的回合恢复为执行中。

关闭面板或 React 重挂载不重发用户任务。每个会话保存 Provider、thread ID、Space 的本机标识及规范路径、工作模式、Change ID、显示记录和最后状态；数据存于应用私有目录，不进入 Space/Git/Cloud，不保存账号密钥。写入采用原子替换，工具输出设上限并明确标记截断。

重启后未正常结束的任务显示“已中断”，不会自动重新执行。用户点击继续时核验目录、Space 和 Change 仍有效，再恢复 Codex thread；找不到历史线程时说明原因，不能静默新建线程冒充恢复。已合并或丢弃的 Change 不能恢复执行。

Live Draft 与后台 Change 的合并/丢弃保护必须同时覆盖旧终端和新运行管理器，启动与关闭共享同一原子互斥规则，不能先检查后启动产生竞态。进行中、等待审批、停止中都阻止相关 Change 关闭。空闲会话在关闭 Change 前撤销后续启动资格、关闭其 Provider 进程并确认不再有后台工具活动；无法确认时拒绝关闭，不把 turn 完成当作所有子进程均已退出。

后台结果继续走既有 diff 与人工合并逻辑；侧栏只提供查看 Changes 的入口，不自动提交 Git、上传或合并。

## 5. 权限与故障处理

- 使用现有本机 Codex 登录状态。未安装、未登录、版本/能力不兼容分别显示可行动的状态；不创建登录终端。未登录时使用该版本支持的结构化账户登录流程，以系统浏览器完成认证；只接受协议返回且校验过的认证 URL，不打开消息文本里的任意链接。
- 初始化、thread 和 turn 显式设置工作目录、sandbox 与审批策略，不继承会放大权限的 full-access/绕过审批设置。默认工作区写入、网络受限，不提供永久放行或任意目录扩权；无法落实该权限边界时拒绝启动。
- 可支持的审批由用户逐次处理；绑定原会话和请求 ID，防止跨会话审批、重复回复和过期审批。拒绝、取消、进程退出时解除等待；未知请求返回协议错误并显示不支持，不自动批准。
- 同一线程持续复用；响应与通知可乱序到达，必须按身份关联。请求超时不能盲目重试 `turn/start`；先终止可能仍运行的 Provider，显示不确定/失败状态。
- 进程崩溃、流中断、损坏事件、超长行和 stderr 有上限与明确错误，不卡住整个侧栏；未经验证的模型文本不能执行 HTML/脚本。
- 权限卡片中的命令只是展示和审批对象，不由前端另起 shell 执行。日志不记录凭证，退出/强制停止覆盖应用拥有的进程组，不能杀掉用户其他独立 Codex 会话。

## 6. 验收标准

使用专门的临时测试 Space 和 fake app-server 做确定性测试；真实模型测试单独注明账号、CLI 版本和结果，不拿团队知识库做破坏性验证。

1. 从已有 Agent 入口发送任务，无 PTY/Terminal 唤起，侧栏能流式显示回答与工具步骤。
2. Background 模式只修改已绑定 Change worktree；Live 模式仍明确写 Draft。跨 Space、伪造 cwd、已关闭 Change 的启动被拒绝。
3. 工具审批可允许/拒绝；未知、重复、过期或其他会话的审批不能执行。默认权限不因本机配置扩大。
4. 收起/重开、切页、切 Space 不重复运行、不丢失进行中状态、不显示其他 Space 的内容。
5. 正常执行、启动中和等待审批时都能停止；确认停稳前不能合并/丢弃；后台工具仍活动时同样受保护。
6. 应用重启显示中断状态，显式继续恢复同一个有效线程；目录移动、线程消失或 Change 已关闭时给出真实错误。
7. 现有采集任务入口、Review 的继续操作与人工合并可复用；合并到 Draft 不显示为已发布 Cloud。
8. 未登录/未安装/不兼容/超时/崩溃有明确反馈；没有终端回退，其他 Provider 不被偷偷替换。
9. 键盘可操作，窄侧栏不溢出；工具输出可折叠，审批不会被输入框遮挡；全局顶栏控件数量不增加。
10. 前端事件 reducer、会话恢复、组件交互测试，Rust 协议与生命周期测试，以及现有前端测试、lint、build、Rust fmt/test 通过；真实 macOS 客户端演示“发送 → 审批 → 修改 → 停止 → Review”。真实测试未完成不能写为已验收。

## 7. 协议验证边界

协议字段与能力须以本机目标 Codex 版本校验。初始化、请求关联、审批、停止竞态和会话恢复都需要 CoWiki 自身的确定性测试与真实客户端验收；未验证的版本或能力不得标记为已支持。

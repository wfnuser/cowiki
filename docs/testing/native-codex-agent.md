# 原生 Agent 侧栏验收记录

日期：2026-11-05。分支 `feat/native-codex-agent`，代码提交 `47845fd`（运行时）与 `bf506a2`（侧栏）。环境：Mac mini，macOS 26.3，codex-cli 0.166.0，Grok Build 1.0.67，Node 26.4.0，rustc 1.96.1。设计见 [spec](../superpowers/specs/2026-11-05-native-codex-agent-design.md)，任务见 [plan](../superpowers/plans/2026-11-05-native-codex-agent.md)。

## 结论

- 通过：确定性测试全部通过；真实 Codex 在临时 Space 中完成“写入 Agent Change → 网络越界触发审批并拒绝 → 停止长命令 → 合入 Draft”。
- 未验收：真实 macOS 窗口里的手动演示（mini 没有屏幕录制权限，界面只在无头浏览器中验证）；Grok 与 Claude 的真实回合（Grok 未登录，`claude-agent-acp` 未安装）；Codex 的应用内登录流程（本机已登录，未重走）。

## 自动化测试（实际运行）

| 命令（在 `web/` 下） | 结果 |
| --- | --- |
| `cargo test --lib`（`src-tauri`） | 195 通过，3 个 ignored（真实 CLI / 真实 vault） |
| `cargo clippy --all-targets -- -D warnings`、`cargo fmt --check` | 通过 |
| `npm test` | 32 组全部通过（含新增 `test:native-agent` 12 项） |
| `npm run lint`、`npm run build`（含 `tsc -b`） | 通过 |
| `npx playwright test tests/browser/native-agent.spec.ts` | Chromium 与 WebKit 共 8 项通过 |

Rust 运行时测试使用真实适配器连接确定性假进程（`web/tests/fixtures/fake-codex-app-server.mjs`、`fake-acp-agent.mjs`），覆盖：同一 thread 两轮回合；未登录；本机配置扩大权限（full-access、never）被拒；审批只答一次、伪造与重复审批被拒、从不 acceptForSession；扩权 / 用户输入 / 整目录写入请求失败关闭；turn/started 之前点停止、启动中停止不发送任务；早到的 completed 不被迟到响应复活；崩溃释放 Change 门；turn/start 超时停止进程且不重发；超长行丢弃；运行中会话阻止 merge/discard、空闲会话先结束进程再关闭、关闭后不能再发送或恢复；Live 会话阻止同 Space 的 Change 关闭；重启后显示中断且不重放，显式继续用同一 thread；thread 丢失时报错而不新建；跨 Space / 伪造目录绑定被拒；ACP 的消息、工具、计划、allow_once/reject_once、取消、未登录、旧协议、bypass 模式切回 default、fs/terminal 请求被拒；结束进程组不误杀组外进程。

浏览器测试用真实 `MainLayout` 与能推送 `agent:event` 的 IPC 夹具：收起再打开不停止任务（`agent_send` 恰好 1 次、`agent_stop` 0 次、无 `terminal_create`）；Agent 文本里的 HTML/脚本不执行；工具步骤默认折叠；键盘处理审批且只提交一次；320 px 侧栏无横向溢出、顶栏按钮数量不变；切换 Space 只显示本 Space 会话，切回后原任务仍在运行。

## 真实 Codex（消耗本机已登录账号的额度）

命令：`cargo test --lib real_codex_round_trip -- --ignored --nocapture`（临时目录中的一次性 Space，不触及任何真实知识库）。用时约 40 秒，结果通过：

1. 后台模式写 `hello.md`：在 workspace-write 沙箱内完成，无需审批；文件只出现在 Agent Change worktree，Draft 未变。
2. `curl -I https://example.com`：沙箱内网络被拦，Codex 请求在沙箱外重跑并说明原因；CoWiki 显示审批卡片，选择拒绝后该步骤为 Declined，Codex 如实报告失败。
3. 120 秒循环命令：点击停止后 0.21 秒进入 Stopped。
4. 关闭 Change：先结束空闲的 Codex 进程组，再取得关闭门并合并；`hello.md` 进入 Draft，会话标为已关闭、不能再运行。

真实运行中观察到并已修复：被停止回合里仍显示“运行中”的步骤现在标为 Stopped；步骤名不再显示 Codex 外层的 `/bin/zsh -lc '…'`。

就绪探测（`cargo test --lib real_cli_probes -- --ignored --nocapture`，只握手和读取账号状态，不调用模型）：Codex 可用并识别已登录；Grok 完成 ACP v1 握手（带 `GROK_SANDBOX=workspace` 与 `GROK_DEFAULT_PERMISSION_MODE=default` 启动正常），状态未登录；Claude 显示需安装 `claude-agent-acp`；OpenCode、Hermes 未安装；Antigravity 标为暂不支持。

## 未验证与已知限制

- Grok 未登录，未跑真实回合；Grok 在 ACP 下是否发出 `session/request_permission`、`GROK_SANDBOX` 对子进程网络的限制，均未验证。
- Claude 需要用户自行安装 `npm install -g @agentclientprotocol/claude-agent-acp`；未安装时侧栏给出该命令，不回退到终端。
- 侧栏内的模型选择（计划 T6）未实现，使用各 Agent 自己的默认模型。
- 旧的 `AgentTerminalPanel` 与 PTY 后端仍保留（不再是侧栏入口），按设计不在本次清理。
- Review 页的“继续”与采集入口通过同一 open request 进入新侧栏，代码路径已接通，但没有单独的浏览器测试。

## 顶栏收纳建议（未实施）

页面顶栏目前有：同步状态（Local only）、版本切换（Current Draft）、Agent、编辑、评论计数。本次没有增加按钮。建议把使用频率较低的同步状态和评论计数收进标题旁的 `⋯`，Agent、编辑和版本切换保留在顶栏；是否调整由团队决定。

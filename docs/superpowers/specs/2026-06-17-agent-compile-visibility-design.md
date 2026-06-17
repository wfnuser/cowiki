# Agent Compile Visibility — Design Spec

**Date**: 2026-06-17
**Status**: design-approved
**Topic**: Web 端实时展示 agent compile 过程 — SSE 驱动的侧边抽屉，显示工具调用日志流

## 1. Overview

当前 Compile 按钮触发后是黑盒体验：POST 阻塞等待 → 显示 "Compiled N page(s)"。用户看不到 agent 在做什么、调用了哪些工具、进度如何。

后端 SSE 基础设施已完成（`/api/agents/{ws}/events`），推送 `AgentEvent`（`TaskStarted`、`ToolStart`、`ToolEnd`、`TaskCompleted`、`AgentStopped`）。本设计关注前端消费层。

## 2. UI 设计

### 2.1 触发与展示

- 右上角 Compile 按钮点击 → 从右侧滑出固定宽度抽屉（400px）
- 抽屉实时展示 agent 工具调用日志流
- 主区域自适应变窄

### 2.2 日志格式

```
⬇ Started
  🔧 list sources
  ✓ list sources · 3 sources found
  🔧 read source
  ✓ read source · done
  🔧 write page
  ✓ write page · /page/slug written
✔ Done · 4 pages, 3 rounds
```

映射规则：

| AgentEvent | 显示 |
|---|---|
| `TaskStarted` | `⬇ Started` |
| `ToolStart { tool }` | `🔧 {tool}` — strip `wiki_` 前缀，`_` → ` ` |
| `ToolEnd { tool, summary }` | `✓ {tool} · {summary}` |
| `TaskCompleted { rounds }` | `✔ Done · N pages, M rounds` |
| `AgentStopped { reason }` | `✗ Agent stopped · {reason}` |

### 2.3 Header

```
COMPILING…                    [Pin]  [✕]
```

- 状态文字：`COMPILING…` / `DONE` / `ERROR`，使用 `--color-accent` / `--color-green` / `--color-red`
- Pin 按钮 + 关闭按钮

### 2.4 视觉设计

复用 CoWiki Design System（`web/DESIGN_SYSTEM.md` 和 `web/src/index.css`），不新增 token。

#### Color Tokens

| 元素 | Token | 值 |
|------|-------|-----|
| 抽屉背景 | `--color-panel` | `#FDFCFB` |
| 左边框 | `--color-border` | `#E8E6E1` |
| Header 文字 | `--color-text` | `#1D1C1A` |
| Log 正文 | `--color-text-secondary` | `#403E3A` |
| Tool name | `--color-text` | `#1D1C1A` |
| Summary | `--color-text-tertiary` | `#8C897F` |
| 进行中指示 | `--color-accent` | `#E2590B` |
| Done 指示 | `--color-green` | `#2F8A5B` |
| Error | `--color-red` | `#CF222E` |
| 左边框线-活跃 | `--color-accent` | `#E2590B` |
| 左边框线-完成 | `--color-green` | `#2F8A5B` |

#### Typography

| 角色 | Family | Size | Weight |
|------|--------|------|--------|
| Header 标题 | Inter | 14px | 600 |
| Log 条目 | Inter | 13px | 400 |
| Tool name | Inter | 13px | 500 |
| Summary | Inter | 13px | 400 |
| 状态文字 | Inter | 12px | 500 |
| 时间戳 | Inter | 11px | 400 |

等宽字体不适用 — 这是 editorial log，不是 terminal。

#### Component

直接复用已有 `Sheet` 组件（`web/src/components/ui/sheet.tsx`，基于 radix-ui Dialog），已有右侧滑入动画。不新增自定义 CSS transition。

#### Signature — 左边框线指示器

Log 条目左侧加一条 2px 竖线，作为当前进行中步骤的视觉锚点：
- 进行中：`--color-accent`（rust-red）
- 已完成：`--color-green`
- 未开始：`--color-border`（subtle）

```
│ ⬇ Started
│ 🔧 list sources
│ ✓ list sources · 3 sources found
│ 🔧 read source          ← 左边框线 rust-red，表示进行中
  🔧 write page           ← 尚未执行，左边框线 subtle
```

#### 视觉稿

```
┌─ Compile Drawer (400px, bg=panel, border-l) ─────┐
│                                                    │
│  COMPILING…                     [Pin] [✕]         │
│  ──────────────────────────────────────────────    │
│                                                    │
│  │ ⬇ Started                            12:01    │
│  │ ✓ list sources · 3 sources found               │
│  │ ✓ read source · done                           │
│  │ 🔧 write page                         12:02    │
│  │ ✓ write page · /getting-started written        │
│  ✔ Done · 4 pages, 3 rounds                       │
│                                                    │
└────────────────────────────────────────────────────┘
```

- Tool name 和 summary 之间用 `·`（middle dot）分隔
- Summary 用 `--color-text-tertiary` 压低视觉层级
- 每条 log 间距 4px
- 时间戳用 `--color-text-faint`，hover 时显示

## 3. Auto-close 与 Pin

状态机：

```
Idle → [Compile clicked] → Streaming
Streaming → [TaskCompleted] → Countdown(3s)
Countdown → [timeout] → Idle (close drawer)
Countdown → [pin] → Pinned (stays open)
Countdown → [new compile] → Streaming
Pinned → [unpin] → Idle (close drawer)
Pinned → [new compile] → Streaming
```

- 倒计时默认 3 秒，可配置
- 倒计时中 pin 按钮显示 `📌 3s`
- Pin 后显示 `📌 Pinned`
- 用户点 ✕ 直接关闭，忽略 pin 状态

## 4. 数据流

```
Compile 按钮 onClick
  → useCompileStream.startCompile()
    → POST /api/workspaces/{ws}/compile   (fire-and-forget，不阻塞 UI)
    → new EventSource(/api/agents/{ws}/events)
    → 每收到 SSE event → push 到 events[]
    → 收到 TaskCompleted/AgentStopped → 更新状态，启动 auto-close
```

- POST 用 fire-and-forget 模式，不解析 body（`compile()` 返回值仅用于 fallback 错误消息）
- SSE 和 POST 是独立 HTTP 连接，后端 AgentManager 在 `connect()` 执行期间实时广播事件

## 5. 错误处理

| 场景 | 行为 |
|---|---|
| SSE 连接失败 | `⚠ Connection lost · reconnecting...`，自动重连（backoff 1s/2s/4s，最多 3 次） |
| Compile POST 失败 | `✗ Failed to start compile · {error}`，不打开 SSE |
| AgentStopped (error) | `✗ Agent stopped · {reason}`，抽屉保持打开，不自动关闭 |
| 抽屉关闭时 compile 进行中 | SSE 不断开，重开抽屉可见累计 events |
| 切换 workspace | 关闭当前 SSE + 重置 events |

## 6. 组件结构

| 文件 | 改动 |
|------|------|
| **新增** `web/src/components/CompileDrawer.tsx` | 基于 `Sheet` 组件的抽屉壳 + log 列表 + header + auto-scroll |
| **新增** `web/src/hooks/useCompileStream.ts` | SSE 连接管理，events 累积，compile 触发，auto-close 倒计时 |
| **修改** `MainLayout.tsx` | 引入 drawer + hook，传 `open`/`onClose`/`ws`；`handleCompile` 改为调用 hook |
| **修改** `api.ts` | 加 `compileAsync()`（fire-and-forget 模式） |

### useCompileStream hook

```ts
function useCompileStream(wsSlug: string): {
  events: AgentEvent[];
  isCompiling: boolean;
  status: 'idle' | 'streaming' | 'done' | 'error';
  startCompile: (branch: string) => Promise<void>;
  isPinned: boolean;
  setPinned: (v: boolean) => void;
}
```

- `startCompile` 内部 POST compile + 建立 EventSource
- SSE 解析 `AgentEvent` JSON，push 到 `events` 数组
- `status` 由收到的事件自动推进
- auto-close 倒计时在 hook 内管理

### CompileDrawer 组件

- Props: `open`, `onClose`, `events`, `isCompiling`, `status`, `isPinned`, `onTogglePin`
- 基于已有 `Sheet` 组件（radix-ui），`side="right"`，宽度 400px
- 不引入自定义 CSS transition，复用 Sheet 内置动画
- Log 区域 `overflow-y: auto`，新事件到来自动 scroll 到底部

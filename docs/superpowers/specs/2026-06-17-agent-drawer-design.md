# Agent Drawer — 双模抽屉设计规范

**Date**: 2026-06-17
**Status**: design-approved
**Topic**: 将现有 CompileDrawer 重构为双模 AgentDrawer — function 模式展示结构化日志，stream 模式展示双向聊天界面

## 1. Overview

当前 `CompileDrawer.tsx` 只支持 function agent 的结构化工具日志。需要扩展为统一抽屉组件 `AgentDrawer`，根据后端 SSE 首事件的 `mode` 字段切换两种视图：

| mode | agent 类型 | 视图 | 交互 |
|------|-----------|------|------|
| `function` | OneShot / Pipeline | `AgentLogView` | 只读日志 |
| `stream` | Stream / AgentPool | `AgentChatView` | 双向聊天 |

Mode 由后端 `Auto` 路由决定，前端不预设。

## 2. 触发流程

```
用户点击 Compile
  → POST /api/workspaces/{ws}/compile { branch, mode: "auto" }
  → 后端 complexity gate：
      Pipeline → SSE connected 事件 { type: "connected", mode: "function" }
      Agent    → SSE connected 事件 { type: "connected", mode: "stream" }
  → useCompileStream 解析 connected，setMode(event.mode)
  → AgentDrawer 根据 mode 渲染 AgentLogView 或 AgentChatView
```

前端不需要任何手动切换逻辑。

## 3. 组件架构

```
AgentDrawer (壳 — Sheet, header, pin, close)
├── AgentLogView    ← mode === "function"
│   └── 结构化日志列表（左边框线 + 工具调用条目）
│
└── AgentChatView   ← mode === "stream"
    ├── MessageList
    │   ├── AgentBubble      ← agent 消息（左，浅灰底）
    │   ├── UserBubble       ← 用户消息（右，accent 色底）
    │   └── SystemCard       ← 工具调用/状态（居中细条卡片）
    ├── TypingIndicator      ← 思考中动画（3 跳动圆点）
    └── ChatInput            ← 输入框 + 发送按钮
```

## 4. 视觉设计

### 4.1 Color Tokens（复用现有 C 对象）

| 元素 | Token | 值 |
|------|-------|-----|
| 抽屉背景 | `C.panel` | `#fdfcfb` |
| 左边框 | `C.line` | `#e8e6e1` |
| Agent 气泡背景 | `C.sidebar` | `#f5f4f1` |
| Agent 气泡边框 | `C.lineSoft` | `#eeece8` |
| 用户气泡背景 | `C.accent` | `#e2590b` |
| 用户气泡文字 | white | — |
| 系统卡片背景 | `C.sidebar` | `#f5f4f1` |
| 系统卡片边框 | `C.line` | `#e8e6e1` |
| Header 状态文字 | 动态 | streaming→C.accent, done→C.green, error→C.red |
| Log 正文 | `C.ink` | `#1d1c1a` |
| Summary | `C.muted` | `#8c897f` |
| 时间戳 | `C.faint` | `#a8a59b` |
| 进行中指示 | `C.accent` | `#e2590b` |
| Done 指示 | `C.green` | `#2f8a5b` |

### 4.2 Typography

| 角色 | Family | Size | Weight |
|------|--------|------|--------|
| Header 标题 | Inter / system sans | 14px | 600 |
| 消息气泡正文 | Inter / system sans | 13px | 400 |
| 发送者标签 | Inter / system sans | 12px | 500 |
| 系统卡片 | Inter / system sans | 12px | 400 |
| 时间戳 | Inter / system sans | 11px | 400 |
| 代码/路径 | var(--font-mono) | 12px | 400 |

### 4.3 Function Agent — Log View

结构化日志列表，复用现有 `CompileDrawer` 的日志格式，提取为 `AgentLogView.tsx`：

```
│ ⬇ Started                            14:01
│ 🔧 list sources
│ ✓ list sources · 3 个源文件
│ 🔧 read source
│ ✓ read source · 完成
│ 🔧 write page                        14:02
│ ✓ write page · /getting-started 写入
✔ Done · 4 页, 3 轮
```

- 左边框线指示器：进行中=C.accent, 完成=C.green, 等待=C.line
- Tool name 和 summary 之间用 `·` 分隔
- 新事件自动 scroll 到底部

### 4.4 Stream Agent — Chat View

对话气泡布局：

**Agent 消息**：
```
🤖 compiler                          14:01
┌──────────────────────────────────────┐
│ 我来编译这 3 个源文件。              │
│ 先读取内容再生成页面。               │
└──────────────────────────────────────┘
```
- 气泡：`C.sidebar` 背景，`C.lineSoft` 边框，`C.ink2` 文字
- 发送者标签 `C.muted`，时间戳 `C.faint`

**用户消息**：
```
                          你  14:03
┌──────────────────────────────────────┐
│ 把 introduction 的语气改得更正式一些 │
└──────────────────────────────────────┘
```
- 气泡：`C.accent` 背景，白色文字
- 右对齐

**工具调用卡片（系统消息，居中）**：
```
        ┌─────────────────────────────┐
        │ ✓ 已读取 sources/intro.md  │
        └─────────────────────────────┘
```
- `C.sidebar` 背景，`C.line` 边框，`C.ink2` 文字
- 路径用 mono 字体

**输入区域（底部固定）**：
```
┌──────────────────────────────────────┐
│ ┌──────────────────────────────┐ [→] │
│ │ 输入消息…                    │     │
│ └──────────────────────────────┘     │
```
- 24px padding，`border-top: 1px C.line`
- 输入框 `C.sidebar` 背景，12px 圆角
- 发送按钮 accent 色圆形按钮
- Stream 消息 endpoint 本次只搭前端骨架，后端后续实现

### 4.5 动画

- 新消息：`translateY(8px)` + `opacity: 0` → 渐入，250ms ease-out
- Agent 思考中：3 个圆点跳动（`C.muted`），循环 1.4s，间隔 0.2s
- 自动滚动：消息列表平滑滚到底部

## 5. Auto-close 与 Pin

状态机保持不变，与现有 `useCompileStream` 一致：

```
Streaming → TaskCompleted → Countdown(3s) → 关闭
Countdown → Pin → 保持打开
```

Stream 模式下 auto-close 逻辑待定（对话结束后是否需要倒计时）。

## 6. 数据流

### 6.1 SSE 首事件

```json
{"type": "connected", "mode": "function"}
```

`AgentEvent::connected` 增加 `mode: Option<String>` 字段。后端在 `agent_events` handler 或 AgentManager 广播时注入。

### 6.2 useCompileStream 变更

```ts
function useCompileStream(wsSlug: string): {
  // ... 现有返回值 +
  mode: 'function' | 'stream' | null;
}
```

connected 事件解析时 `setMode(event.mode)`。

### 6.3 Stream 消息（骨架）

前端本地维护 `chatMessages: ChatMessage[]`，Enter 发送时 push 到本地数组。后端 message endpoint 后续实现。

## 7. 文件变更

| 文件 | 变更 |
|------|------|
| **重命名** `CompileDrawer.tsx` → `AgentDrawer.tsx` | 增加 mode 判断，渲染 AgentLogView 或 AgentChatView |
| **新增** `AgentLogView.tsx` | 从现有 CompileDrawer 抽取日志列表组件 |
| **新增** `AgentChatView.tsx` | 聊天界面（MessageList + AgentBubble + UserBubble + SystemCard + TypingIndicator + ChatInput） |
| **修改** `useCompileStream.ts` | 增加 `mode` 状态，connected 事件解析 mode |
| **修改** `MainLayout.tsx` | `CompileDrawer` → `AgentDrawer` |
| **修改** `crates/agents/src/stream/mod.rs` | `AgentEvent::connected` 增加 `mode` 字段 |
| **修改** `crates/server/src/main.rs` | connected 事件写入 mode |

## 8. Out of Scope

- Stream agent 后端 message endpoint（本次只搭前端 UI 骨架）
- Stream agent 对话历史持久化
- AgentPool 的 stream agent 注册/调度（后端已有框架）

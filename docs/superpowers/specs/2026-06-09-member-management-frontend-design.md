# Cowiki Member Management 前端设计

> 参考后端: PR #41 Role-Management System | Design System: web/DESIGN_SYSTEM.md | 2026-06-09

---

## 总图

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Cowiki Member Management Frontend                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─ 角色体系 (4 级, PR #41 已完成) ──────────────────────────────────────┐  │
│  │                                                                        │  │
│  │   Owner ──── 完全控制，唯一，可删除/转让                               │  │
│  │    │                                                                    │  │
│  │   Manager ── 管理成员+设置+邀请，不可删除/转让                         │  │
│  │    │                                                                    │  │
│  │   Editor ─── 创建/编辑/删除页面，提交/审核                             │  │
│  │    │                                                                    │  │
│  │   Viewer ─── 只读                                                      │  │
│  │                                                                        │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  ┌─ Members 页面 ────────────────────────────────────────────────────────┐  │
│  │                                                                        │  │
│  │   Header: "Members" + count badge + [Invite people]                    │  │
│  │                                                                        │  │
│  │   ┌──────┬──────────────────┬──────────────┬──────────┬──────────┐    │  │
│  │   │      │ Member           │ Role         │ Active   │ Actions  │    │  │
│  │   ├──────┼──────────────────┼──────────────┼──────────┼──────────┤    │  │
│  │   │ [A]  │ Alice Chen       │ owner  (r/o) │ 2h ago   │   ---    │    │  │
│  │   │      │ alice@...        │              │          │          │    │  │
│  │   ├──────┼──────────────────┼──────────────┼──────────┼──────────┤    │  │
│  │   │ [B]  │ Bob Wang         │ manager  ▾   │ 1d ago   │   ...    │    │  │
│  │   │      │ bob@...          │              │          │          │    │  │
│  │   ├──────┼──────────────────┼──────────────┼──────────┼──────────┤    │  │
│  │   │ [C]  │ Carol Liu        │ editor  (r/o)│ 3d ago   │   ...    │    │  │
│  │   │      │ (no email)       │              │          │          │    │  │
│  │   └──────┴──────────────────┴──────────────┴──────────┴──────────┘    │  │
│  │                                                                        │  │
│  │   头像: 用户名首字母 + 哈希取色 (spaceTileColors 8 色)                  │  │
│  │   角色: Owner/Manager 可改他人角色, 其他只读                            │  │
│  │                                                                        │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  ┌─ 邀请弹窗 (shadcn Dialog + Select) ───────────────────────────────────┐  │
│  │                                                                        │  │
│  │   Invite Member — MyWorkspace                                         │  │
│  │   ┌──────────────────────────────────────────────────────────────┐    │  │
│  │   │  Email / Username                                           │    │  │
│  │   │  [____________________________]                              │    │  │
│  │   │                                                              │    │  │
│  │   │  Role                     Expires                           │    │  │
│  │   │  [Viewer  ▾]              [7 days  ▾]                       │    │  │
│  │   │                                                              │    │  │
│  │   │                    [Cancel]  [Send Invitation]              │    │  │
│  │   └──────────────────────────────────────────────────────────────┘    │  │
│  │                                                                        │  │
│  │   Role: shadcn Select (owner / manager / editor / viewer)             │  │
│  │   Expires: shadcn Select (1 day / 3 days / 7 days / 30 days)         │  │
│  │                                                                        │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 目录

1. [设计决策](#1-设计决策)
2. [后端补充](#2-后端补充)
3. [前端架构](#3-前端架构)
4. [MembersView 详细设计](#4-membersview-详细设计)
5. [邀请弹窗设计](#5-邀请弹窗设计)
6. [MainLayout 清理](#6-mainlayout-清理)
7. [状态处理](#7-状态处理)
8. [实施计划](#8-实施计划)

---

## 1. 设计决策

| 决策项 | 决定 | 理由 |
|--------|------|------|
| Member 管理入口 | 全页面 MembersView (Tab "Members") | 与 PR #41 后端 `GET /members` 对齐 |
| 嵌入式弹窗 | 删除 | 功能重复, 单一入口 |
| 头像设计 | 首字母 + 哈希取色 | 无需头像上传, 即时可识别 |
| 角色选择器 (Dialog) | shadcn `<Select>` dropdown | DESIGN_SYSTEM.md 要求使用 shadcn/ui; Select 比自定义 pill 更规范 |
| 角色选择器 (Table) | 行内 dropdown (shadcn Select) | 一致的设计语言, Owner/Manager 可交互 |
| 邀请过期时间 | 预设选项: 1 / 3 / 7 / 30 天 | PR #41 设计有 expires_at, 默认 7 天 |
| last_active_at | 后端 `WorkspaceMember` 已有, 补充到 API 响应 | 零 DB 迁移, guard.rs 已自动更新 |
| 颜色体系 | 遵循 DESIGN_SYSTEM.md tokens | `--color-accent`: #C8442A, `--color-text`: #1F1B16 |
| 圆角 | `0.375rem` | DESIGN_SYSTEM.md 规范 |

---

## 2. 后端补充

### 2.1 MemberResponse 补充字段

`WorkspaceMember` 已有 `last_active_at` 和 `joined_via`, 只需在 API 响应中暴露:

```rust
// crates/server/src/routes/workspace.rs

#[derive(Serialize)]
pub struct MemberResponse {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub role: String,
    pub last_active_at: Option<String>,  // ← 新增
    pub joined_via: String,              // ← 新增
}

// list_members — 补充映射:
for m in members {
    if let Some(u) = cowiki_db::users::find_by_id(&state.db, m.user_id).await? {
        result.push(MemberResponse {
            // ... existing fields ...
            last_active_at: m.last_active_at.map(|t| t.to_rfc3339()),
            joined_via: m.joined_via,
        });
    }
}
```

### 2.2 邀请接口增加过期时间

```rust
#[derive(Deserialize)]
pub struct InviteWithRoleRequest {
    pub user: String,
    pub role: Option<String>,             // defaults to "viewer"
    pub expires_in_days: Option<i32>,     // ← 新增: 默认 7
}

// invite handler — 传给 create_invitation:
cowiki_db::workspaces::create_invitation(
    &state.db, guard.workspace.id, &item.user, role, guard.user.id, invited_user,
    item.expires_in_days.unwrap_or(7),    // ← 新增参数
).await
```

### 2.3 DB 层: create_invitation 接受自定义过期时间

```rust
// crates/db/src/workspaces.rs
pub async fn create_invitation(
    pool: &PgPool,
    workspace_id: Uuid,
    email: &str,
    role: &str,
    invited_by: Uuid,
    invited_user_id: Uuid,
    expires_in_days: i32,                 // ← 新增参数
) -> sqlx::Result<Invitation> {
    // SQL 中: expires_at = now() + $7 * INTERVAL '1 day'
}
```

### 2.4 前端 API 适配批量邀请

```typescript
// web/src/api.ts
export async function inviteToWorkspace(
  workspaceSlug: string,
  user: string,
  role = 'viewer',
  expiresInDays = 7,
) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/invite`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({
      invitations: [{ user, role, expires_in_days: expiresInDays }],
    }),
  });
  // ...
}
```

### 2.5 前端类型同步

```typescript
export interface MemberInfo {
  id: string;
  name: string;
  email: string | null;
  role: string;
  last_active_at: string | null;  // ← 新增
  joined_via: string;             // ← 新增
}
```

---

## 3. 前端架构

### 3.1 组件树

```
MainLayout
├── SpaceRail              (左侧导航栏)
├── SpacePanel             (页面树 + Tab 切换)
│   └── Tab: Members       → setActiveView({ kind: 'members' })
├── MembersView            ← 本次重写
│   ├── Header
│   ├── TableBody
│   │   └── MemberRow
│   │       ├── AvatarCell
│   │       ├── RoleCell (shadcn Select if canManage)
│   │       ├── LastActiveCell
│   │       └── ActionsCell (remove via DropdownMenu)
│   └── EmptyState
└── InviteDialog           ← 本次增强
    ├── Input (email/username)
    ├── Select (role)
    └── Select (expiration)
```

### 3.2 数据流

```
MembersView
  ├─ props: workspaceSlug, isOwner (role >= Manager), currentUserId
  ├─ state: members[], loading, error
  ├─ onMount: listMembers(workspaceSlug) → setMembers
  ├─ onRoleChange: changeMemberRole(slug, userId, newRole) → optimistic update
  ├─ onRemove: removeMember(slug, userId) → filter from list
  └─ onInvite: callback → MainLayout opens showInviteDialog → API inviteToWorkspace
```

### 3.3 权限模型

```
               Viewer   Editor   Manager   Owner
查看 Members     ✅       ✅       ✅        ✅
修改角色         ❌       ❌       ✅¹       ✅¹
移除成员         ❌       ❌       ✅¹       ✅¹
邀请成员         ❌       ❌       ✅        ✅
删除 workspace   ❌       ❌       ❌        ✅

¹ 不能管理同级或上级 (can_manage_role: strict >)
  UI 侧: 对 Owner 行隐藏操作, 对 Manager 行(当前用户为 Manager 时)隐藏
```

---

## 4. MembersView 详细设计

### 4.1 Table 布局

遵循 DESIGN_SYSTEM.md: `borderRadius: 0.375rem`, border via `--color-border`, no heavy shadows.

```
Grid: 1fr 130px 130px 48px
Gap: 0 (行内用 padding 做间距)
Row padding: 10px 16px
Border: 1px solid var(--color-border), borderRadius: 0.375rem
Background: var(--color-bg) (#FDFCFA)
Header background: var(--color-bg-secondary) (#F8F6F2)
```

### 4.2 头像 (Avatar)

```typescript
const AVATAR_COLORS = spaceTileColors; // 项目已有 8 色

function avatarColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = ((hash << 5) - hash) + name.charCodeAt(i);
    hash |= 0;
  }
  return AVATAR_COLORS[Math.abs(hash) % AVATAR_COLORS.length];
}
```

规格:
- 36px 圆形, flex-shrink: 0
- 背景: `avatarColor(m.name)`
- 文字: 首字母大写, #fff, font-weight 600, font-size 14px

### 4.3 角色列 (RoleCell) — shadcn Select

**Owner/Manager 视角 — 可交互:**

使用 shadcn `<Select>` 组件 (与 DESIGN_SYSTEM.md 一致):

```tsx
<Select value={m.role} onValueChange={(v) => onRoleChange(m.id, v)}>
  <SelectTrigger className="h-7 w-28 text-xs">
    <SelectValue />
  </SelectTrigger>
  <SelectContent>
    <SelectItem value="owner">Owner</SelectItem>
    <SelectItem value="manager">Manager</SelectItem>
    <SelectItem value="editor">Editor</SelectItem>
    <SelectItem value="viewer">Viewer</SelectItem>
  </SelectContent>
</Select>
```

- 交互条件: `canManage && m.id !== currentUserId && (currentUserRole === 'owner' || m.role < currentUserRole)`
- 不能更改: 自己的行 / Owner 的行 / 同级的行

**Editor/Viewer 视角 — 只读:**

```tsx
<span className="text-xs capitalize text-[var(--color-text-secondary)]">
  {m.role}
</span>
```

### 4.4 最后活跃列 (LastActiveCell)

```typescript
function relativeTime(iso: string | null): string {
  if (!iso) return '--';
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  return `${months}mo ago`;
}
```

样式: fontSize: 12, color: `var(--color-text-tertiary)` (#8B8275)

### 4.5 操作列 (ActionsCell)

使用 shadcn `<DropdownMenu>`:

```tsx
<DropdownMenu>
  <DropdownMenuTrigger>...</DropdownMenuTrigger>
  <DropdownMenuContent>
    <DropdownMenuItem
      className="text-[var(--color-red)]"
      onClick={() => setConfirmRemove(m.id)}
    >
      Remove member
    </DropdownMenuItem>
  </DropdownMenuContent>
</DropdownMenu>
```

点击 "Remove member" → 行内确认:
```
"Remove Alice Chen?"  [Cancel]  [Remove]
```

### 4.6 Header

```
Members  3              [👤+ Invite people]
```

- Invite button: accent 背景 (`--color-accent`: #C8442A), 白色文字, `borderRadius: 0.375rem`
- 仅 Owner/Manager 可见

### 4.7 边界情况

| 场景 | 处理 |
|------|------|
| 加载中 | "Loading members..." centered |
| 加载失败 | 红色错误消息, 可切换 Tab 重试 |
| 空列表 | Users icon + "No members found." |
| 超长用户名 | text-overflow: ellipsis, 头像固定 36px |
| 无 email | 显示 "No email", color: --color-text-tertiary |
| Owner 自己 | 角色只读, 无操作按钮 |
| Manager 自己 | 角色只读, 无操作按钮 |
| Manager 看 Manager | 角色只读, 无操作按钮 |
| Editor/Viewer | 全表只读, 无邀请按钮 |

---

## 5. 邀请弹窗设计

### 5.1 使用 shadcn 组件

遵循 DESIGN_SYSTEM.md: 使用 `<Dialog>`, `<Input>`, `<Select>`, `<Button>`。

### 5.2 布局

```
┌─────────────────────────────────────────┐
│  Invite Member — MyWorkspace            │
├─────────────────────────────────────────┤
│                                         │
│  Email / Username                       │
│  ┌─────────────────────────────────────┐│
│  │ colleague@example.com               ││
│  └─────────────────────────────────────┘│
│                                         │
│  Role              Expires              │
│  ┌──────────────┐  ┌──────────────┐    │
│  │ Viewer     ▾ │  │ 7 days     ▾ │    │
│  └──────────────┘  └──────────────┘    │
│                                         │
│              [Cancel]  [Send Invitation]│
└─────────────────────────────────────────┘
```

### 5.3 Role Select

shadcn `<Select>`, 选项:

| Value | Label |
|-------|-------|
| `viewer` | Viewer |
| `editor` | Editor |
| `manager` | Manager |
| `owner` | Owner |

默认: `viewer` (最低权限原则)

### 5.4 Expires Select

shadcn `<Select>`, 选项:

| Value | Label |
|-------|-------|
| `1` | 1 day |
| `3` | 3 days |
| `7` | 7 days (default) |
| `30` | 30 days |

默认: `7`

### 5.5 按钮状态

| 状态 | 表现 |
|------|------|
| 正常 | "Send Invitation", accent 背景 |
| email 为空 | 按钮可点击 (由 Input required 属性阻止提交) |
| submitting | "Sending...", 按钮 disabled |
| 成功 | 关闭弹窗, toast "Invitation sent." |
| 失败 | 显示错误消息, 弹窗保持打开 |

### 5.6 Send 按钮禁用规则

- `email` 为空 → disabled
- `role === 'owner'` → disabled, tooltip: "Cannot invite as Owner"
- `submitting` → disabled

---

## 6. MainLayout 清理

### 6.1 删除项

| 删除 | 内容 |
|------|------|
| `showMembersPanel` state | members dialog 状态 |
| `membersList`, `membersLoading`, `membersError` | dialog 相关状态 |
| `openMembersPanel()` | dialog 打开函数 |
| `handleRemoveMember()` | dialog 内 remove handler |
| `handleChangeRole()` | dialog 内 role change handler |
| `<Dialog>`: Members management | 整个 JSX block |

### 6.2 保留项

| 保留 | 说明 |
|------|------|
| `showInviteDialog` + 相关 state | 邀请弹窗 |
| `handleInvite()` | 邀请提交 (改为调用更新后的 `inviteToWorkspace`) |
| `<Dialog>`: Invite member | JSX (替换为 Select 版本, 增加 expires) |
| `pendingInvites` badge | 顶部待处理邀请通知 |

### 6.3 其他适配

- `inviteToWorkspace` 的函数签名从 `(slug, email, role)` 改为 `(slug, user, role, expiresInDays)`
- SpacePanel `onSettings` 不再调用 `openMembersPanel` — 改为保留设置入口或暂时移除

---

## 7. 状态处理

### 7.1 MembersView 状态机

```
         ┌──────────┐
         │  LOADING │ ← 初始
         └────┬─────┘
              │
     ┌───────┼───────┐
     ▼                ▼
┌─────────┐     ┌─────────┐
│  ERROR  │     │  EMPTY  │
└─────────┘     └─────────┘
                         │
                         ▼
                    ┌──────────┐
                    │  LOADED  │
                    └──────────┘
```

无 retry button — 用户切换 Tab 再切回即可重新加载。

### 7.2 Optimistic Update (角色修改)

```typescript
const handleChangeRole = async (userId: string, newRole: string) => {
  const old = members;
  setMembers(prev => prev.map(m => m.id === userId ? { ...m, role: newRole } : m));
  try {
    await changeMemberRole(workspaceSlug, userId, newRole);
  } catch (err) {
    setMembers(old);  // 回滚
    setError(err.message);
  }
};
```

---

## 8. 实施计划

### Phase 1: 后端补充

| 文件 | 改动 |
|------|------|
| `crates/db/src/workspaces.rs` | `create_invitation` 加 `expires_in_days` 参数 |
| `crates/server/src/routes/workspace.rs` | `MemberResponse` 加 `last_active_at`/`joined_via`; `InviteWithRoleRequest` 加 `expires_in_days`; `list_members` 补字段; `invite` 传过期时间 |

### Phase 2: 前端 API 层

| 文件 | 改动 |
|------|------|
| `web/src/api.ts` | `MemberInfo` 加字段; `inviteToWorkspace` 改为批量格式 + expires |

### Phase 3: MembersView 重写

| 文件 | 改动 |
|------|------|
| `web/src/components/views/MembersView.tsx` | 完整重写: 彩色头像 + shadcn Select 角色 + relative time + DropdownMenu 操作 |

### Phase 4: 邀请弹窗 + MainLayout 适配

| 文件 | 改动 |
|------|------|
| `web/src/pages/MainLayout.tsx` | 邀请弹窗使用 shadcn Select; 删除 members dialog; 适配新 API |

---

## 9. 文件总览

| File | Action | Est. Lines |
|------|--------|------------|
| `crates/db/src/workspaces.rs` | Modify | ±5 |
| `crates/server/src/routes/workspace.rs` | Modify | +15 |
| `web/src/api.ts` | Modify | +10 / -5 |
| `web/src/components/views/MembersView.tsx` | Rewrite | ~220 |
| `web/src/pages/MainLayout.tsx` | Modify | -60 / +40 |

---

> **下一步**: 确认此设计后, 进入 implementation plan 阶段。

# Cowiki Role-Management System — Implementation Plan

> 基于设计 spec: `docs/plans/2026-06-05-role-management-refactor.md`
> 执行顺序: Phase 1 → 6 顺序执行，每个 Phase 完成后测试通过再进入下一 Phase

---

## Phase 1: 数据库 Migration 009 + Role Enum 重构

**目标**: 新建增量 migration，更新 Rust 代码适配 4 级角色，保留现有 001–008

### 1.1 创建 migration 009
| 操作 | 文件 |
|------|------|
| 创建 | `crates/db/src/migrations/009_role_management.sql` |

内容: 完整 migration SQL（见 spec §3.2），包括：
- workspace_members: 角色扩展 + joined_via + share_link_id + last_active_at
- invitations: 角色扩展 + message + expires_at + resent_count + last_resent_at
- 新表: share_links, share_link_joins, ownership_transfers
- 向后兼容: writer→editor, reader→viewer

### 1.2 更新 migration runner
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/db/src/lib.rs` | `run_migrations()` 增加 `009_role_management.sql` |

### 1.3 重构 Role enum
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/db/src/workspaces.rs` | `Role` enum: `Viewer=1, Editor=2, Manager=3, Owner=4`，实现 `PartialOrd`/`Ord`，添加 `can_manage()`/`can_edit()`/`can_view()`/`can_delete_workspace()`/`can_transfer_ownership()`/`is_shareable()`/`can_manage_role()` |
| 修改 | `crates/db/src/workspaces.rs` | `Role::ALL` 从 `&[&str]` 改为 `&[Role]` |
| 修改 | `crates/db/src/workspaces.rs` | `FromStr for Role` 支持 4 个新变体 |
| 修改 | `crates/db/src/workspaces.rs` | `Display for Role` 输出 lowercase |

### 1.4 更新 DB 函数签名
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/db/src/workspaces.rs` | `add_member()` 增加 `joined_via` 和 `share_link_id` 参数 |
| 修改 | `crates/db/src/workspaces.rs` | `WorkspaceMember` struct 增加 `invited_by`/`joined_via`/`share_link_id`/`last_active_at` 字段 |
| 修改 | `crates/db/src/workspaces.rs` | 新增 `touch_last_active()` — 更新 workspace_members.last_active_at |
| 修改 | `crates/db/src/workspaces.rs` | `Invitation` struct 增加 `message`/`expires_at`/`resent_count`/`last_resent_at` 字段 |
| 修改 | `crates/db/src/workspaces.rs` | `remove_member()` 不再硬编码 `role != 'owner'`，改为接受调用者 role，用 `can_manage_role()` 判断 |
| 修改 | `crates/db/src/workspaces.rs` | 新增 `resend_invitation()` / `revoke_invitation()` / `expire_invitations()` |
| 修改 | `crates/db/src/workspaces.rs` | 新增 `add_member_direct()` — 直接添加已有用户 |
| 修改 | `crates/db/src/workspaces.rs` | `create_invitation()` 支持可选 `message` 参数 |

### 1.5 更新测试
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/db/src/workspaces.rs` | 测试: `test_pool()` 只跑新 `001_init.sql` |
| 修改 | `crates/db/src/workspaces.rs` | 测试: 更新所有使用 `writer`/`reader` 的测试用例为 `editor`/`viewer` |
| 修改 | `crates/db/src/workspaces.rs` | 测试: 更新 `test_role_all_*` 测试用新的 Role enum 值 |
| 修改 | `crates/db/src/workspaces.rs` | 新增: Manager 权限测试（不能管理 Owner、不能提升为 Owner） |

---

## Phase 2: 权限中间件 (PermissionGuard)

**目标**: 实现声明式 extractor，将所有路由中的硬编码 role check 替换掉

### 2.1 创建 PermissionGuard
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/server/src/routes/guard.rs` | `Permission` enum (ViewContent/EditContent/ManageMembers/ManageWorkspace/DeleteWorkspace/TransferOwnership) |
| 创建 | `crates/server/src/routes/guard.rs` | `PermissionGuard` — axum `FromRequestParts` extractor。从 Path 提取 workspace slug，从 Header 提取用户，查 DB 获取 role，根据所需 Permission 校验。校验通过后将 `(Workspace, Role, User)` 注入 request extensions |

### 2.2 重构路由 handler
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/src/routes/workspace.rs` | `invite()` — 移除 `current_role != "owner"`，改用 `guard.require(Permission::ManageMembers)` |
| 修改 | `crates/server/src/routes/workspace.rs` | `remove_member()` — 同上 |
| 修改 | `crates/server/src/routes/workspace.rs` | `change_member_role()` — 同上 + 增加 Manager 不能管理 Owner 的逻辑 |
| 修改 | `crates/server/src/routes/workspace.rs` | `delete_workspace()` — 改用 `Permission::DeleteWorkspace` |
| 修改 | `crates/server/src/routes/workspace.rs` | `rename_workspace()` — 改用 `Permission::ManageWorkspace`（Manager+ 均可） |
| 修改 | `crates/server/src/routes/workspace.rs` | `join_workspace()` — 改用 `Permission::ViewContent` |
| 修改 | `crates/server/src/routes/workspace.rs` | `list_members()` — 移除 hardcoded owner-only check |
| 修改 | `crates/server/src/routes/review.rs` | `review_action()` — 移除 `role != "owner" && role != "writer"`，改用 `Permission::EditContent` |
| 修改 | `crates/server/src/routes/pages.rs` | content 写路由 — 加 `Permission::EditContent` |
| 修改 | `crates/server/src/routes/mod.rs` | 注册 `pub mod guard;` |

### 2.3 更新路由注册
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/src/main.rs` | 所有需要权限的路由增加 `PermissionLayer` 或调整 handler 签名 |

### 2.4 测试
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/server/tests/guard_tests.rs` | 权限矩阵全覆盖测试：每个 Permission × 每个 Role，验证允许/拒绝的正确性 |

---

## Phase 3: 邀请系统增强

**目标**: 批量邀请、撤回、重发、过期管理

### 3.1 DB 层
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/db/src/workspaces.rs` | `create_invitation()` 改为接受 `Option<&str>` message |
| 创建 | `crates/db/src/workspaces.rs` | `create_invitations_batch()` — 事务内批量创建多条 invitation |
| 创建 | `crates/db/src/workspaces.rs` | `find_invitations_by_workspace()` — 列出 workspace 所有 invitation（含非 pending） |
| 创建 | `crates/db/src/workspaces.rs` | `resend_invitation()` — 更新 `resent_count` + `last_resent_at` + 重置 `expires_at` |
| 创建 | `crates/db/src/workspaces.rs` | `revoke_invitation()` — 设置 `status = 'expired'`（区别于 accepted/rejected） |
| 创建 | `crates/db/src/workspaces.rs` | `expire_stale_invitations()` — 批量标记过期 |

### 3.2 API 路由
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/src/routes/workspace.rs` | `invite()` — 支持批量 body（`invitations: [{email, role, message}]`），逐个创建，返回 `{sent, failed, results}` |
| 创建 | `crates/server/src/routes/workspace.rs` | `list_invitations()` — `GET /workspaces/{slug}/invitations`，返回所有 invitation 列表 |
| 创建 | `crates/server/src/routes/workspace.rs` | `resend_invitation()` — `POST /workspaces/{slug}/invitations/{id}/resend` |
| 创建 | `crates/server/src/routes/workspace.rs` | `revoke_invitation()` — `DELETE /workspaces/{slug}/invitations/{id}` |

### 3.3 路由注册
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/src/main.rs` | 注册新路由: `GET /invitations`、`POST resend`、`DELETE revoke` |

### 3.4 后台任务
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/src/main.rs` | 启动时 spawn 一个 tokio task，每 1 小时调用 `expire_stale_invitations()` |

### 3.5 测试
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/tests/permission_api_tests.rs` | 新增批量邀请测试、撤回测试、重发测试、过期测试 |

---

## Phase 4: 分享链接系统

**目标**: ShareLink CRUD + Join via link + 密码/过期验证

### 4.1 DB 层
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/db/src/share_links.rs` | `ShareLink` struct (sqlx::FromRow) |
| 创建 | `crates/db/src/share_links.rs` | `create_share_link()` — 生成 64-char random token (URL-safe base64)，bcrypt password hash |
| 创建 | `crates/db/src/share_links.rs` | `find_by_token()` — 按 token 查找活跃链接 |
| 创建 | `crates/db/src/share_links.rs` | `list_by_workspace()` — workspace 所有链接 |
| 创建 | `crates/db/src/share_links.rs` | `update_share_link()` — 更新 label/password/expires_at/is_active |
| 创建 | `crates/db/src/share_links.rs` | `deactivate_share_link()` — 设置 is_active=false |
| 创建 | `crates/db/src/share_links.rs` | `record_join()` — 写入 share_link_joins |
| 创建 | `crates/db/src/share_links.rs` | `get_join_count()` — 统计链接使用人数 |
| 修改 | `crates/db/src/lib.rs` | 添加 `pub mod share_links;` |

### 4.2 API 路由
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/server/src/routes/share_links.rs` | `create_share_link()` — `POST /workspaces/{slug}/share-links`，validate role ≤ Editor |
| 创建 | `crates/server/src/routes/share_links.rs` | `list_share_links()` — `GET /workspaces/{slug}/share-links` |
| 创建 | `crates/server/src/routes/share_links.rs` | `update_share_link()` — `PATCH /workspaces/{slug}/share-links/{id}` |
| 创建 | `crates/server/src/routes/share_links.rs` | `delete_share_link()` — `DELETE /workspaces/{slug}/share-links/{id}` → 设置 is_active=false |
| 创建 | `crates/server/src/routes/share_links.rs` | `join_via_link()` — `POST /share/{token}/join`。验证 token 有效性 + 过期 + 密码 + 速率限制，事务内 insert member + record join |
| 创建 | `crates/server/src/routes/share_links.rs` | `get_link_info()` — `GET /share/{token}/info`，无需认证，返回 workspace 基本信息 + role + 是否需要密码 |

### 4.3 路由注册
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/src/routes/mod.rs` | 添加 `pub mod share_links;` |
| 修改 | `crates/server/src/main.rs` | 注册 share_links 路由 + share 公开路由 |

### 4.4 安全措施
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/server/src/middleware/rate_limiter.rs` | 简单 IP-based 速率限制中间件，join 端点 10 req/hour |
| 修改 | `crates/server/src/routes/share_links.rs` | join 路由集成速率限制 |

### 4.5 测试
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/server/tests/share_link_tests.rs` | CRUD 测试、过期测试、密码测试、角色天花板测试（不能创建 Manager/Owner 链接）、速率限制测试 |

---

## Phase 5: 转让 Ownership

**目标**: 发起转让 → 新 Owner 确认 → 事务执行

### 5.1 DB 层
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/db/src/transfers.rs` | `OwnershipTransfer` struct (sqlx::FromRow) |
| 创建 | `crates/db/src/transfers.rs` | `create_transfer()` — 检查 to_user 是 member 且不是当前 Owner |
| 创建 | `crates/db/src/transfers.rs` | `find_pending_transfers_for_user()` — 某人作为 to_user 的 pending 转让 |
| 创建 | `crates/db/src/transfers.rs` | `find_transfer_by_id()` |
| 创建 | `crates/db/src/transfers.rs` | `accept_transfer()` — 事务: UPDATE old_owner → previous_owner_new_role, UPDATE new_owner → owner, UPDATE transfer → accepted |
| 创建 | `crates/db/src/transfers.rs` | `reject_transfer()` / `cancel_transfer()` — 更新 status |
| 修改 | `crates/db/src/lib.rs` | 添加 `pub mod transfers;` |

### 5.2 API 路由
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/server/src/routes/transfers.rs` | `initiate_transfer()` — `POST /workspaces/{slug}/transfer-ownership` |
| 创建 | `crates/server/src/routes/transfers.rs` | `list_pending_transfers()` — `GET /transfers/pending` |
| 创建 | `crates/server/src/routes/transfers.rs` | `accept_transfer()` — `POST /transfers/{id}/accept` |
| 创建 | `crates/server/src/routes/transfers.rs` | `reject_transfer()` — `POST /transfers/{id}/reject` |
| 创建 | `crates/server/src/routes/transfers.rs` | `cancel_transfer()` — `DELETE /transfers/{id}` |

### 5.3 路由注册
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/src/routes/mod.rs` | 添加 `pub mod transfers;` |
| 修改 | `crates/server/src/main.rs` | 注册 transfer 路由 |

### 5.4 测试
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/server/tests/transfer_tests.rs` | 正常转让流程、非 member 不能接收、已不在 workspace 时转让驳回、Manager 不能发起转让 |

---

## Phase 6: 前端重构

**目标**: ShareDialog 三 Tab、JoinViaLinkPage、侧边栏 badge+popover、权限驱动 UI

### 6.1 新增 API 函数
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `web/src/api.ts` | 新增 TypeScript 类型: `ShareLink`, `OwnershipTransfer` |
| 修改 | `web/src/api.ts` | 新增函数: `createShareLink()`, `listShareLinks()`, `updateShareLink()`, `deleteShareLink()`, `joinViaLink()`, `getLinkInfo()` |
| 修改 | `web/src/api.ts` | 新增函数: `initiateTransfer()`, `listPendingTransfers()`, `acceptTransfer()`, `rejectTransfer()`, `cancelTransfer()` |
| 修改 | `web/src/api.ts` | 更新函数: `inviteToWorkspace()` 改为批量接口 |
| 修改 | `web/src/api.ts` | 新增函数: `resendInvitation()`, `revokeInvitation()`, `listInvitations()` |
| 修改 | `web/src/api.ts` | 新增函数: `addMemberDirect()` — `POST /workspaces/{slug}/members` |

### 6.2 创建新组件
| 操作 | 文件 | 用途 |
|------|------|------|
| 创建 | `web/src/components/share/ShareDialog.tsx` | 三 Tab 容器弹窗 |
| 创建 | `web/src/components/share/InviteMembersTab.tsx` | 批量邮箱输入 + 角色下拉 + message + 发送按钮 + 待处理邀请列表（撤回/重发） |
| 创建 | `web/src/components/share/ShareLinksTab.tsx` | 创建链接按钮 + 活跃链接卡片列表 |
| 创建 | `web/src/components/share/CreateShareLinkDialog.tsx` | 链接创建表单（label/role/password/expires_at） |
| 创建 | `web/src/components/share/ShareLinkCard.tsx` | 单个链接卡片（label/role/已用人数/过期状态/复制/设置/失效按钮） |
| 创建 | `web/src/components/share/MembersTab.tsx` | 搜索/筛选 + 成员列表 + 角色修改 + 移除 + 转让按钮 |
| 创建 | `web/src/components/workspace/JoinViaLinkPage.tsx` | `/join/:slug?token=xxx` 路由页面 |
| 创建 | `web/src/components/workspace/PendingInvitationsPopover.tsx` | 侧边栏 badge 点击弹窗 |
| 创建 | `web/src/components/workspace/TransferOwnershipDialog.tsx` | 转让确认弹窗（选新 Owner + 选降级角色） |

### 6.3 创建 hooks
| 操作 | 文件 | 用途 |
|------|------|------|
| 创建 | `web/src/hooks/useShareLinks.ts` | 分享链接 CRUD + 加载状态 |
| 创建 | `web/src/hooks/useInvitations.ts` | 邀请管理（批量发送/撤回/重发）|
| 创建 | `web/src/hooks/useMembers.ts` | 成员管理（列表/角色修改/移除/转让）|

### 6.4 修改 MainLayout
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `web/src/pages/MainLayout.tsx` | 替换旧的 invite dialog 为 `<ShareDialog>`（三 Tab） |
| 修改 | `web/src/pages/MainLayout.tsx` | 替换旧成员管理 dialog 为 ShareDialog 的 MembersTab |
| 修改 | `web/src/pages/MainLayout.tsx` | 侧边栏 pending invitations badge → 改用 `<PendingInvitationsPopover>` |
| 修改 | `web/src/pages/MainLayout.tsx` | `SpaceTreeItem` 右键菜单: `role === 'owner' \|\| role === 'manager'` 时显示管理选项 |
| 修改 | `web/src/pages/MainLayout.tsx` | 角色下拉选项: Owner/Manager/Editor/Viewer（替换旧 writer/reader） |
| 修改 | `web/src/pages/MainLayout.tsx` | Manager 角色不显示 Delete 选项 |
| 修改 | `web/src/pages/MainLayout.tsx` | Owner 角色增加 "Transfer Ownership" 选项 |
| 修改 | `web/src/pages/MainLayout.tsx` | Workspace 设置中增加 visibility toggle (private ↔ public) — 仅 Owner/Manager 可见 |

### 6.5 MemberResponse 增加 last_active_at
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/src/routes/workspace.rs` | `MemberResponse` struct 增加 `last_active_at: Option<String>` 字段 |
| 修改 | `web/src/api.ts` | `MemberInfo` interface 增加 `last_active_at?: string` |
| 修改 | `web/src/components/share/MembersTab.tsx` | 成员列表展示最后活跃时间 |

### 6.6 路由
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `web/src/App.tsx` | 添加 `/join/:slug` 路由 → `<JoinViaLinkPage>` |

### 6.7 E2E 测试
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `web/e2e/role-management.spec.ts` | 完整流程: Manager+Owner 创建链接 → 另一用户通过链接加入 → 验证角色 → 邀请流程 → 角色修改 → 管理选项显隐 |

---

## 文件变更总览

```
创建 (22):
  crates/db/src/migrations/009_role_management.sql
  crates/server/src/routes/guard.rs
  crates/server/src/routes/share_links.rs
  crates/server/src/routes/transfers.rs
  crates/server/src/middleware/rate_limiter.rs
  crates/db/src/share_links.rs
  crates/db/src/transfers.rs
  crates/server/tests/guard_tests.rs
  crates/server/tests/share_link_tests.rs
  crates/server/tests/transfer_tests.rs
  web/src/components/share/ShareDialog.tsx
  web/src/components/share/InviteMembersTab.tsx
  web/src/components/share/ShareLinksTab.tsx
  web/src/components/share/CreateShareLinkDialog.tsx
  web/src/components/share/ShareLinkCard.tsx
  web/src/components/share/MembersTab.tsx
  web/src/components/workspace/JoinViaLinkPage.tsx
  web/src/components/workspace/PendingInvitationsPopover.tsx
  web/src/components/workspace/TransferOwnershipDialog.tsx
  web/src/hooks/useShareLinks.ts
  web/src/hooks/useInvitations.ts
  web/src/hooks/useMembers.ts
  web/e2e/role-management.spec.ts

修改 (14):
  crates/db/src/lib.rs
  crates/db/src/workspaces.rs
  crates/server/src/routes/mod.rs
  crates/server/src/routes/workspace.rs
  crates/server/src/routes/pages.rs
  crates/server/src/routes/review.rs
  crates/server/src/main.rs
  crates/server/tests/permission_api_tests.rs
  web/src/api.ts
  web/src/App.tsx
  web/src/pages/MainLayout.tsx
```

> **下一步**: 确认此计划后开始 Phase 1 实现。

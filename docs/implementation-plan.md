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
| 修改 | `crates/db/src/workspaces.rs` | `add_member()` 增加 `joined_via` 参数 |
| 修改 | `crates/db/src/workspaces.rs` | `WorkspaceMember` struct 增加 `invited_by`/`joined_via`/`last_active_at` 字段 |
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

**目标**: User Account 批量邀请 (id/email/username)、撤回、重发、过期管理

### 3.1 DB 层
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/db/src/workspaces.rs` | `create_invitation()` 改为接受 `invited_user_id: Uuid` + 可选 `message` |
| 创建 | `crates/db/src/workspaces.rs` | `resolve_user_identifier()` — 按 UUID/email/username 查找用户 |
| 创建 | `crates/db/src/workspaces.rs` | `create_invitations_batch()` — 事务内批量 resolve + 创建多条 invitation |
| 创建 | `crates/db/src/workspaces.rs` | `find_invitations_by_workspace()` — 列出 workspace 所有 invitation |
| 创建 | `crates/db/src/workspaces.rs` | `find_pending_invitations_for_user()` — 改用 `invited_user_id` 匹配，不再用 email |
| 创建 | `crates/db/src/workspaces.rs` | `resend_invitation()` / `revoke_invitation()` / `expire_stale_invitations()` |

### 3.2 API 路由
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/src/routes/workspace.rs` | `invite()` — 支持批量 body（`invitations: [{user: "id|email|name", role, message}]`），逐个 resolve + 创建 |
| 创建 | `crates/server/src/routes/workspace.rs` | `list_invitations()` / `resend_invitation()` / `revoke_invitation()` |
| 修改 | `crates/server/src/routes/workspace.rs` | `accept_invitation()` — 改用 `invited_user_id` 匹配，移除 email 比对 |
| 修改 | `crates/server/src/routes/workspace.rs` | `list_pending_invitations()` — 按 `invited_user_id` 查询 |

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

## Phase 4: 转让 Ownership

**目标**: 发起转让 → 新 Owner 确认 → 事务执行

### 4.1 DB 层
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/db/src/transfers.rs` | `OwnershipTransfer` struct (sqlx::FromRow) |
| 创建 | `crates/db/src/transfers.rs` | `create_transfer()` — 检查 to_user 是 member 且不是当前 Owner |
| 创建 | `crates/db/src/transfers.rs` | `find_pending_transfers_for_user()` — 某人作为 to_user 的 pending 转让 |
| 创建 | `crates/db/src/transfers.rs` | `find_transfer_by_id()` |
| 创建 | `crates/db/src/transfers.rs` | `accept_transfer()` — 事务: UPDATE old_owner → previous_owner_new_role, UPDATE new_owner → owner, UPDATE transfer → accepted |
| 创建 | `crates/db/src/transfers.rs` | `reject_transfer()` / `cancel_transfer()` — 更新 status |
| 修改 | `crates/db/src/lib.rs` | 添加 `pub mod transfers;` |

### 4.2 API 路由
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/server/src/routes/transfers.rs` | `initiate_transfer()` — `POST /workspaces/{slug}/transfer-ownership` |
| 创建 | `crates/server/src/routes/transfers.rs` | `list_pending_transfers()` — `GET /transfers/pending` |
| 创建 | `crates/server/src/routes/transfers.rs` | `accept_transfer()` — `POST /transfers/{id}/accept` |
| 创建 | `crates/server/src/routes/transfers.rs` | `reject_transfer()` — `POST /transfers/{id}/reject` |
| 创建 | `crates/server/src/routes/transfers.rs` | `cancel_transfer()` — `DELETE /transfers/{id}` |

### 4.3 路由注册
| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/src/routes/mod.rs` | 添加 `pub mod transfers;` |
| 修改 | `crates/server/src/main.rs` | 注册 transfer 路由 |

### 4.4 测试
| 操作 | 文件 | 改动 |
|------|------|------|
| 创建 | `crates/server/tests/transfer_tests.rs` | 正常转让流程、非 member 不能接收、已不在 workspace 时转让驳回、Manager 不能发起转让 |

---

## Phase 6: MemberResponse 增加 last_active_at

**目标**: API 返回成员最后活跃时间

| 操作 | 文件 | 改动 |
|------|------|------|
| 修改 | `crates/server/src/routes/workspace.rs` | `MemberResponse` struct 增加 `last_active_at: Option<String>` 字段 |
| 修改 | `crates/server/src/routes/workspace.rs` | `PermissionGuard` 中间件在每次请求时调用 `touch_last_active()` |

> **前端重构** 将在后端完成后的后续设计文档中统一规划。

---

## 文件变更总览

```
创建 (7):
  crates/db/src/migrations/009_role_management.sql
  crates/db/src/transfers.rs
  crates/server/src/routes/guard.rs
  crates/server/src/routes/transfers.rs
  crates/server/tests/guard_tests.rs
  crates/server/tests/transfer_tests.rs

修改 (8):
  crates/db/src/lib.rs
  crates/db/src/workspaces.rs
  crates/server/src/routes/mod.rs
  crates/server/src/routes/workspace.rs
  crates/server/src/routes/pages.rs
  crates/server/src/routes/review.rs
  crates/server/src/main.rs
  crates/server/tests/permission_api_tests.rs
```

> **前端文件** (web/src/*) 将在后续前端设计文档中统一规划。

> **下一步**: 确认此计划后开始 Phase 1 实现。

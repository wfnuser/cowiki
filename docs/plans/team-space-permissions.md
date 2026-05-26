# Team Space 邀请+权限系统 — 实现计划

> Ralplan 共识终版 | 2026-05-26 | 迭代: 3 (Planner → Architect → Critic → 终版)
>
> 规格来源: `.omg/specs/deep-interview-team-space-permissions.md`

---

## ADR (Architecture Decision Record)

**Decision**: 在现有 workspace 体系上增量添加角色权限系统，通过 Rust enum + DB CHECK 约束实现可扩展三层角色模型，邀请流程改为显式接受/拒绝，管理操作写入审计日志。

**Drivers**:
1. 向后兼容 — 现有 API 不 breaking change
2. 可扩展 — 添加新角色只需改 CHECK 约束 + enum + 权限矩阵
3. 轻量 — 审计日志用简单 JSONB 表，不做重型审计系统

**Alternatives considered**:
- 完整 RBAC (角色模板/自定义角色) → 过度工程，推迟到未来迭代
- 邀请自动接受 → 被 Critic 否决，改为显式 accept/reject

**Consequences**:
- `workspace_members.role` 保持 String，通过 Rust enum 验证
- 现有 `invite` 端点行为变更：不再自动添加成员
- 权限守卫需要插入所有管理端点

---

## 文件变更清单

### 🔴 新建文件

| # | 文件 | 说明 |
|---|------|------|
| F1 | `crates/db/src/migrations/007_team_permissions.sql` | 迁移：invitation.role, audit_log 表 |
| F2 | `crates/db/src/audit.rs` | 审计日志写入模块 |
| F3 | `crates/server/src/routes/workspace/permissions.rs` | 权限守卫辅助函数 |

### 🟡 修改文件

| # | 文件 | 变更类型 |
|---|------|---------|
| M1 | `crates/db/src/lib.rs` | 注册 007 迁移 + 注册 audit 模块 |
| M2 | `crates/db/src/workspaces.rs` | 添加 Role enum + 新 DB 函数 |
| M3 | `crates/server/src/error.rs` | 添加 `AppError::Forbidden` |
| M4 | `crates/server/src/routes/workspace.rs` | 重写 invite + 新增 6 个端点 |
| M5 | `crates/server/src/routes/mod.rs` | 注册 permissions 子模块 (如需要) |
| M6 | `crates/server/src/main.rs` | 注册新路由 |
| M7 | `web/src/api.ts` | 新增前端 API 函数 + 类型 |
| M8 | `web/src/pages/MainLayout.tsx` | 角色感知 UI |

---

## 详细实现

### Phase 1: 基础设施 (DB + 核心类型)

#### M1: `crates/db/src/lib.rs`

**变更**:
```rust
// 在 run_migrations 中添加 (after 006):
let sql7 = include_str!("migrations/007_team_permissions.sql");
sqlx::raw_sql(sql7).execute(pool).await?;

// 在模块声明中添加:
pub mod audit;
```

#### F1: `crates/db/src/migrations/007_team_permissions.sql`

```sql
-- 007_team_permissions: Team Space invitation + permissions + audit log
-- Adds: invitation.role, audit_log table

-- 1. Add role column to invitations
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS role VARCHAR(20) NOT NULL DEFAULT 'writer';
ALTER TABLE invitations ADD CONSTRAINT invitations_role_check
    CHECK (role IN ('owner', 'writer', 'reader'));

-- 2. Audit log table for management operations
CREATE TABLE IF NOT EXISTS audit_log (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    actor_id UUID NOT NULL REFERENCES users(id),
    action VARCHAR(50) NOT NULL,
    target_type VARCHAR(50),
    target_id UUID,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_audit_log_workspace
    ON audit_log(workspace_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_audit_log_actor
    ON audit_log(actor_id);
```

#### F2: `crates/db/src/audit.rs`

**新建模块**，提供独立函数写入审计日志：

```rust
use sqlx::PgPool;
use uuid::Uuid;
use serde::Serialize;

/// Insert an audit log entry.
/// Called by route handlers after management actions.
pub async fn log(
    pool: &PgPool,
    workspace_id: Uuid,
    actor_id: Uuid,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<Uuid>,
    metadata: Option<serde_json::Value>,
) -> sqlx::Result<()> {
    let meta = metadata.unwrap_or(serde_json::Value::Object(Default::default()));
    sqlx::query(
        "INSERT INTO audit_log (workspace_id, actor_id, action, target_type, target_id, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(workspace_id)
    .bind(actor_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(meta)
    .execute(pool)
    .await?;
    Ok(())
}
```

#### M2: `crates/db/src/workspaces.rs`

**在文件顶部添加 Role enum**（在现有 struct 定义之前）：

```rust
use std::str::FromStr;

/// Workspace member role with GitHub-style three-tier permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Owner,
    Writer,
    Reader,
}

impl Role {
    /// All valid role strings (for validation).
    pub const ALL: &[&str] = &["owner", "writer", "reader"];

    /// Check if this role has management privileges (invite/remove/change_role/delete).
    pub fn can_manage(&self) -> bool {
        matches!(self, Role::Owner)
    }

    /// Check if this role can edit content.
    pub fn can_write(&self) -> bool {
        matches!(self, Role::Owner | Role::Writer)
    }
}

impl FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "owner" => Ok(Role::Owner),
            "writer" => Ok(Role::Writer),
            "reader" => Ok(Role::Reader),
            _ => Err(format!("invalid role: {s}")),
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Owner => write!(f, "owner"),
            Role::Writer => write!(f, "writer"),
            Role::Reader => write!(f, "reader"),
        }
    }
}
```

**新增 DB 函数**（追加到文件末尾）：

```rust
/// Get the role of a user in a workspace. Returns None if not a member.
pub async fn get_member_role(pool: &PgPool, workspace_id: Uuid, user_id: Uuid) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2"
    )
    .bind(workspace_id).bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| { tracing::error!("DB get_member_role failed: {e}"); e })
}

/// Remove a member from a workspace. Returns true if a row was deleted.
pub async fn remove_member(pool: &PgPool, workspace_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    let rows = sqlx::query(
        "DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2 AND role != 'owner'"
    )
    .bind(workspace_id).bind(user_id)
    .execute(pool)
    .await
    .map_err(|e| { tracing::error!("DB remove_member failed: {e}"); e })?;
    Ok(rows.rows_affected() > 0)
}

/// Change a member's role. Returns the new role string.
pub async fn change_member_role(
    pool: &PgPool, workspace_id: Uuid, user_id: Uuid, new_role: &str
) -> sqlx::Result<Option<String>> {
    let row = sqlx::query_scalar::<_, String>(
        "UPDATE workspace_members SET role = $3
         WHERE workspace_id = $1 AND user_id = $2 AND role != 'owner'
         RETURNING role"
    )
    .bind(workspace_id).bind(user_id).bind(new_role)
    .fetch_optional(pool)
    .await
    .map_err(|e| { tracing::error!("DB change_member_role failed: {e}"); e })?;
    Ok(row)
}

/// Find all pending invitations for a user by email.
pub async fn find_pending_invitations_for_user(
    pool: &PgPool, user_id: Uuid
) -> sqlx::Result<Vec<Invitation>> {
    // Join with users to get email, then find pending invitations
    sqlx::query_as::<_, Invitation>(
        "SELECT i.* FROM invitations i
         JOIN users u ON i.email = u.email
         WHERE u.id = $1 AND i.status = 'pending'"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| { tracing::error!("DB find_pending_invitations_for_user failed: {e}"); e })
}

/// Find an invitation by ID.
pub async fn find_invitation_by_id(pool: &PgPool, invitation_id: Uuid) -> sqlx::Result<Option<Invitation>> {
    sqlx::query_as::<_, Invitation>("SELECT * FROM invitations WHERE id = $1")
        .bind(invitation_id)
        .fetch_optional(pool)
        .await
}

/// Reject an invitation (set status to 'rejected').
pub async fn reject_invitation(pool: &PgPool, invitation_id: Uuid) -> sqlx::Result<Invitation> {
    sqlx::query_as::<_, Invitation>(
        "UPDATE invitations SET status = 'rejected' WHERE id = $1 RETURNING *"
    )
    .bind(invitation_id)
    .fetch_one(pool)
    .await
}

/// Delete a workspace (soft — cascade handled by DB ON DELETE CASCADE).
pub async fn delete_workspace(pool: &PgPool, workspace_id: Uuid) -> sqlx::Result<bool> {
    let rows = sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(pool)
        .await
        .map_err(|e| { tracing::error!("DB delete_workspace failed: {e}"); e })?;
    Ok(rows.rows_affected() > 0)
}
```

**修改 `create_invitation`** — 增加 `role` 参数：

```rust
// 旧签名:
pub async fn create_invitation(pool: &PgPool, workspace_id: Uuid, email: &str, invited_by: Uuid) -> sqlx::Result<Invitation>

// 新签名 (增加 role 参数):
pub async fn create_invitation(pool: &PgPool, workspace_id: Uuid, email: &str, role: &str, invited_by: Uuid) -> sqlx::Result<Invitation> {
    sqlx::query_as::<_, Invitation>(
        "INSERT INTO invitations (workspace_id, email, role, invited_by) VALUES ($1, $2, $3, $4) RETURNING *"
    )
    .bind(workspace_id).bind(email).bind(role).bind(invited_by)
    .fetch_one(pool)
    .await
    .map_err(|e| { tracing::error!("DB create invitation failed: {e}"); e })
}
```

---

### Phase 2: Server 端

#### M3: `crates/server/src/error.rs`

**添加 `Forbidden` variant**：

```rust
#[derive(Debug)]
pub enum AppError {
    Internal(String),
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),   // <-- NEW
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),  // <-- NEW
        };
        (status, msg).into_response()
    }
}
```

#### F3: `crates/server/src/routes/workspace/permissions.rs` (新文件)

注意：由于现有 `workspace.rs` 是单文件而非目录模块，需决定架构。**推荐**: 将权限守卫函数直接放在 `workspace.rs` 文件末尾作为私有辅助函数，避免重构路由模块结构。这样 F3 实际不需要单独文件。

**权限守卫函数**（添加到 `workspace.rs` 末尾）：

```rust
// ── Permission Guards ──

/// Require that the user is an owner of the workspace.
/// Returns the workspace on success.
async fn require_owner(
    db: &PgPool,
    workspace_slug: &str,
    user_id: Uuid,
) -> Result<(cowiki_db::workspaces::Workspace,)> {
    let ws = cowiki_db::workspaces::find_by_slug(db, workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    let role = cowiki_db::workspaces::get_member_role(db, ws.id, user_id)
        .await?
        .unwrap_or_default();

    if role != "owner" {
        return Err(AppError::Forbidden("only the workspace owner can perform this action".into()));
    }
    Ok((ws,))
}

/// Require that the user is at least a writer in the workspace.
/// Returns the workspace and role on success.
async fn require_writer(
    db: &PgPool,
    workspace_slug: &str,
    user_id: Uuid,
) -> Result<(cowiki_db::workspaces::Workspace, String)> {
    let ws = cowiki_db::workspaces::find_by_slug(db, workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    let role = cowiki_db::workspaces::get_member_role(db, ws.id, user_id)
        .await?
        .unwrap_or_default();

    if role != "owner" && role != "writer" {
        return Err(AppError::Forbidden("you do not have write access to this workspace".into()));
    }
    Ok((ws, role))
}

/// Require membership (any role). Returns workspace and role.
async fn require_member(
    db: &PgPool,
    workspace_slug: &str,
    user_id: Uuid,
) -> Result<(cowiki_db::workspaces::Workspace, String)> {
    let ws = cowiki_db::workspaces::find_by_slug(db, workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    let role = cowiki_db::workspaces::get_member_role(db, ws.id, user_id)
        .await?
        .ok_or_else(|| AppError::Forbidden("you are not a member of this workspace".into()))?;

    Ok((ws, role))
}
```

#### M4: `crates/server/src/routes/workspace.rs`

##### 4a. 新增 Request/Response 类型

在文件类型定义区域添加：

```rust
// ── Invitation with role ──

#[derive(Deserialize)]
pub struct InviteWithRoleRequest {
    pub email: String,
    pub role: Option<String>,  // defaults to "writer" if not specified
}

#[derive(Serialize)]
pub struct PendingInvitationResponse {
    pub id: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub workspace_slug: String,
    pub role: String,
    pub invited_by_name: String,
    pub created_at: String,
}

// ── Management request types ──

#[derive(Deserialize)]
pub struct ChangeRoleRequest {
    pub user_id: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct RemoveMemberRequest {
    pub user_id: String,
}
```

##### 4b. 重写 `invite` 端点 (行 ~127-159)

**旧行为**: 自动添加被邀者为成员 + 自动接受邀请
**新行为**: 仅创建邀请记录（含 role），不再自动添加成员

```rust
/// Invite someone to a workspace by email (with optional role).
/// Owner-only. Creates a pending invitation — the invited user must accept/reject.
pub async fn invite(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<InviteWithRoleRequest>,
) -> Result<Json<InviteResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    // Validate role
    let role = input.role.as_deref().unwrap_or("writer");
    if !cowiki_db::workspaces::Role::ALL.contains(&role) {
        return Err(AppError::BadRequest(
            format!("invalid role '{}': must be one of: owner, writer, reader", role)
        ));
    }

    // Require owner
    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    let current_role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
        .await?
        .unwrap_or_default();
    if current_role != "owner" {
        return Err(AppError::Forbidden("only the workspace owner can invite members".into()));
    }

    // Create invitation (no auto-add)
    let invitation = cowiki_db::workspaces::create_invitation(
        &state.db, ws.id, &input.email, role, user.id,
    ).await?;

    // Audit log
    cowiki_db::audit::log(
        &state.db, ws.id, user.id,
        "invite_member", Some("invitation"), Some(invitation.id),
        Some(serde_json::json!({"email": input.email, "role": role})),
    ).await?;

    Ok(Json(InviteResponse {
        invitation_id: invitation.id.to_string(),
        email: input.email,
        workspace: ws.slug,
    }))
}
```

##### 4c. 新增端点

以下端点全部追加到 `workspace.rs` 中：

**`POST /api/invitations/{id}/accept`** — 接受邀请：

```rust
/// Accept a pending invitation. Adds the user as a member with the invited role.
pub async fn accept_invitation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(invitation_id): Path<Uuid>,
) -> Result<Json<WorkspaceResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    let invitation = cowiki_db::workspaces::find_invitation_by_id(&state.db, invitation_id)
        .await?
        .ok_or_else(|| AppError::NotFound("invitation not found".into()))?;

    if invitation.status != "pending" {
        return Err(AppError::BadRequest("invitation is no longer pending".into()));
    }

    // Verify the invitation is for this user (by email match)
    let current_user = cowiki_db::users::find_by_id(&state.db, user.id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("user not found".into()))?;

    if current_user.email.as_deref() != Some(&invitation.email) {
        return Err(AppError::Forbidden("this invitation is for a different email address".into()));
    }

    // Add as member with the invited role
    cowiki_db::workspaces::add_member(
        &state.db, invitation.workspace_id, user.id,
        &invitation.role, invitation.invited_by,
    ).await?;

    // Accept the invitation
    cowiki_db::workspaces::accept_invitation(&state.db, invitation.id).await?;

    // Audit log
    cowiki_db::audit::log(
        &state.db, invitation.workspace_id, user.id,
        "accept_invitation", Some("invitation"), Some(invitation.id),
        Some(serde_json::json!({"role": invitation.role})),
    ).await?;

    // Create user branch in git repo
    state.wiki_repo
        .ensure_user_branch(&user.id.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &invitation.workspace_id.to_string())
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    Ok(Json(ws_response(&ws, &invitation.role)))
}
```

**`POST /api/invitations/{id}/reject`** — 拒绝邀请：

```rust
/// Reject a pending invitation.
pub async fn reject_invitation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(invitation_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let user = extract_user(&state.db, &headers).await?;

    let invitation = cowiki_db::workspaces::find_invitation_by_id(&state.db, invitation_id)
        .await?
        .ok_or_else(|| AppError::NotFound("invitation not found".into()))?;

    if invitation.status != "pending" {
        return Err(AppError::BadRequest("invitation is no longer pending".into()));
    }

    // Verify email match
    let current_user = cowiki_db::users::find_by_id(&state.db, user.id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("user not found".into()))?;

    if current_user.email.as_deref() != Some(&invitation.email) {
        return Err(AppError::Forbidden("this invitation is for a different email address".into()));
    }

    cowiki_db::workspaces::reject_invitation(&state.db, invitation.id).await?;

    // Audit log
    cowiki_db::audit::log(
        &state.db, invitation.workspace_id, user.id,
        "reject_invitation", Some("invitation"), Some(invitation.id),
        None,
    ).await?;

    Ok(Json(serde_json::json!({"status": "rejected"})))
}
```

**`GET /api/invitations/pending`** — 列出待处理邀请：

```rust
/// List pending invitations for the current user.
pub async fn list_pending_invitations(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<PendingInvitationResponse>>> {
    let user = extract_user(&state.db, &headers).await?;

    let invitations = cowiki_db::workspaces::find_pending_invitations_for_user(
        &state.db, user.id,
    ).await?;

    let mut result = Vec::new();
    for inv in invitations {
        let ws = cowiki_db::workspaces::find_by_slug(&state.db, &inv.workspace_id.to_string())
            .await?
            .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;
        let inviter = cowiki_db::users::find_by_id(&state.db, inv.invited_by)
            .await?
            .map(|u| u.name)
            .unwrap_or_default();

        result.push(PendingInvitationResponse {
            id: inv.id.to_string(),
            workspace_id: ws.id.to_string(),
            workspace_name: ws.name.clone(),
            workspace_slug: ws.slug.clone(),
            role: inv.role.clone(),
            invited_by_name: inviter,
            created_at: inv.created_at.to_rfc3339(),
        });
    }
    Ok(Json(result))
}
```

**`POST /api/workspaces/{slug}/members/remove`** — 移除成员 (owner only)：

```rust
/// Remove a member from a workspace (owner only). Cannot remove the owner.
pub async fn remove_member(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<RemoveMemberRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = extract_user(&state.db, &headers).await?;

    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    // Require owner
    let current_role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
        .await?
        .unwrap_or_default();
    if current_role != "owner" {
        return Err(AppError::Forbidden("only the owner can remove members".into()));
    }

    let target_id = Uuid::parse_str(&input.user_id)
        .map_err(|_| AppError::BadRequest("invalid user_id".into()))?;

    if target_id == user.id {
        return Err(AppError::BadRequest("cannot remove yourself as owner".into()));
    }

    let removed = cowiki_db::workspaces::remove_member(&state.db, ws.id, target_id).await?;
    if !removed {
        return Err(AppError::NotFound("member not found or is owner".into()));
    }

    // Audit log
    cowiki_db::audit::log(
        &state.db, ws.id, user.id,
        "remove_member", Some("user"), Some(target_id),
        None,
    ).await?;

    Ok(Json(serde_json::json!({"status": "removed"})))
}
```

**`POST /api/workspaces/{slug}/members/role`** — 修改成员角色 (owner only)：

```rust
/// Change a member's role (owner only). Cannot change the owner's role.
pub async fn change_member_role(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
    Json(input): Json<ChangeRoleRequest>,
) -> Result<Json<MemberResponse>> {
    let user = extract_user(&state.db, &headers).await?;

    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    // Require owner
    let current_role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
        .await?
        .unwrap_or_default();
    if current_role != "owner" {
        return Err(AppError::Forbidden("only the owner can change member roles".into()));
    }

    // Validate target role
    if !cowiki_db::workspaces::Role::ALL.contains(&input.role.as_str()) {
        return Err(AppError::BadRequest(
            format!("invalid role '{}': must be owner, writer, or reader", input.role)
        ));
    }

    let target_id = Uuid::parse_str(&input.user_id)
        .map_err(|_| AppError::BadRequest("invalid user_id".into()))?;

    let new_role = cowiki_db::workspaces::change_member_role(
        &state.db, ws.id, target_id, &input.role,
    ).await?
    .ok_or_else(|| AppError::NotFound("member not found or is owner".into()))?;

    // Audit log
    cowiki_db::audit::log(
        &state.db, ws.id, user.id,
        "change_member_role", Some("user"), Some(target_id),
        Some(serde_json::json!({"new_role": new_role})),
    ).await?;

    let member_user = cowiki_db::users::find_by_id(&state.db, target_id)
        .await?
        .ok_or_else(|| AppError::NotFound("user not found".into()))?;

    Ok(Json(MemberResponse {
        id: member_user.id.to_string(),
        name: member_user.name,
        email: member_user.email,
        role: new_role,
    }))
}
```

**`DELETE /api/workspaces/{slug}`** — 删除 workspace (owner only)：

```rust
/// Delete a workspace (owner only). Cascade deletes members and content.
pub async fn delete_workspace(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(workspace_slug): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = extract_user(&state.db, &headers).await?;

    let ws = cowiki_db::workspaces::find_by_slug(&state.db, &workspace_slug)
        .await?
        .ok_or_else(|| AppError::NotFound("workspace not found".into()))?;

    // Require owner
    let current_role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
        .await?
        .unwrap_or_default();
    if current_role != "owner" {
        return Err(AppError::Forbidden("only the owner can delete a workspace".into()));
    }

    // Audit log (before delete so we capture it)
    cowiki_db::audit::log(
        &state.db, ws.id, user.id,
        "delete_workspace", Some("workspace"), Some(ws.id),
        Some(serde_json::json!({"name": ws.name, "slug": ws.slug})),
    ).await?;

    cowiki_db::workspaces::delete_workspace(&state.db, ws.id).await?;

    Ok(Json(serde_json::json!({"status": "deleted"})))
}
```

##### 4d. 更新现有 `list_members` — 增加角色感知

在 `list_members` 中将 caller 的 role 与成员一并返回（已在返回 MemberResponse 中包含 role，无需改动）。可选地，添加 caller role 到头信息中：

不需要修改结构，当前 `MemberResponse` 已包含 `role: String`。

##### 4e. 更新 `list_workspaces` — 返回正确角色

当前 `list_workspaces` 硬编码 `if ws.created_by == user.id { "owner" } else { "writer" }`。需要改为从 DB 查询真实角色：

```rust
pub async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<WorkspaceResponse>>> {
    let user = extract_user(&state.db, &headers).await?;
    let workspaces = cowiki_db::workspaces::list_for_user(&state.db, user.id).await?;

    let mut result = Vec::new();
    for ws in workspaces {
        let role = cowiki_db::workspaces::get_member_role(&state.db, ws.id, user.id)
            .await?
            .unwrap_or_else(|| if ws.created_by == user.id { "owner".into() } else { "writer".into() });
        result.push(ws_response(&ws, &role));
    }
    Ok(Json(result))
}
```

#### M5: `crates/server/src/routes/mod.rs`

**不需要修改**。权限守卫函数放在 `workspace.rs` 内部，不创建子模块。

#### M6: `crates/server/src/main.rs`

**新增路由**（在现有 workspace 路由之后）：

```rust
// 在 .route("/api/workspaces/{slug}/members", ...) 之后添加:

        // Invitations (accept/reject)
        .route("/api/invitations/pending", get(routes::workspace::list_pending_invitations))
        .route("/api/invitations/{id}/accept", post(routes::workspace::accept_invitation))
        .route("/api/invitations/{id}/reject", post(routes::workspace::reject_invitation))
        // Member management (owner only)
        .route("/api/workspaces/{slug}/members/remove", post(routes::workspace::remove_member))
        .route("/api/workspaces/{slug}/members/role", post(routes::workspace::change_member_role))
        // Workspace deletion (owner only)
        .route("/api/workspaces/{slug}", delete(routes::workspace::delete_workspace))
```

---

### Phase 3: 前端

#### M7: `web/src/api.ts`

**新增类型**：

```typescript
export interface PendingInvitation {
  id: string;
  workspace_id: string;
  workspace_name: string;
  workspace_slug: string;
  role: string;
  invited_by_name: string;
  created_at: string;
}

// 扩展 Workspace 类型 (添加 visibility):
export interface Workspace {
  id: string;
  name: string;
  slug: string;
  role: string;
  visibility?: string;  // "public" | "private"
}
```

**新增 API 函数**（追加到 `// ── Workspaces ──` 区域）：

```typescript
export async function inviteToWorkspace(workspaceSlug: string, email: string, role = 'writer') {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/invite`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ email, role }),
  });
  return res.json();
}

export async function listPendingInvitations(): Promise<PendingInvitation[]> {
  const res = await fetch(`${BASE}/invitations/pending`, { headers: h() });
  return res.json();
}

export async function acceptInvitation(invitationId: string): Promise<Workspace> {
  const res = await fetch(`${BASE}/invitations/${invitationId}/accept`, {
    method: 'POST',
    headers: h(),
  });
  return res.json();
}

export async function rejectInvitation(invitationId: string) {
  const res = await fetch(`${BASE}/invitations/${invitationId}/reject`, {
    method: 'POST',
    headers: h(),
  });
  return res.json();
}

export async function removeMember(workspaceSlug: string, userId: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/members/remove`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ user_id: userId }),
  });
  return res.json();
}

export async function changeMemberRole(workspaceSlug: string, userId: string, role: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}/members/role`, {
    method: 'POST',
    headers: h({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ user_id: userId, role }),
  });
  return res.json();
}

export async function deleteWorkspace(workspaceSlug: string) {
  const res = await fetch(`${BASE}/workspaces/${workspaceSlug}`, {
    method: 'DELETE',
    headers: h(),
  });
  return res.json();
}
```

#### M8: `web/src/pages/MainLayout.tsx`

##### 8a. 管理按钮的条件渲染

在 workspace sidebar 中，基于 `workspace.role === 'owner'` 条件渲染管理按钮。需要添加以下 UI：

1. **Workspace 操作菜单** — 在 workspace 名称旁边添加 `Settings`/`...` 按钮 (仅 owner 可见):
   - 邀请成员 (打开 dialog)
   - 管理成员 (打开 panel/dialog)
   - 删除 workspace (确认 dialog)

2. **邀请 Dialog** — 包含 email + role 下拉框

3. **成员管理 Panel** — 显示成员列表，每行有角色下拉 + 移除按钮 (owner only)

##### 8b. 待处理邀请通知

在 sidebar 顶部/bell icon 显示 pending invitations 数量，点击展开列表，每个邀请有 Accept/Reject 按钮。

具体实现概要：

```tsx
// 新增 state
const [pendingInvites, setPendingInvites] = useState<PendingInvitation[]>([]);
const [showInviteDialog, setShowInviteDialog] = useState<Workspace | null>(null);
const [showMembersPanel, setShowMembersPanel] = useState<Workspace | null>(null);

// 新增 import
import { listPendingInvitations, acceptInvitation, rejectInvitation, ... } from '../api';

// 在 loadWorkspaces 后加载 pending invitations
useEffect(() => {
  if (auth) {
    listPendingInvitations().then(setPendingInvites).catch(() => {});
  }
}, [auth?.id]);

// Workspace sidebar item 增加右键/...菜单
// 仅当 workspace.role === 'owner' 且 workspace.visibility === 'public' (team space):
//   - "Invite Members" → 打开 invite dialog
//   - "Manage Members" → 打开 members panel
//   - "Delete Workspace" → 确认后调用 deleteWorkspace

// 邀请 Dialog:
//   - Email input
//   - Role select (owner/writer/reader, default writer)
//   - Submit → inviteToWorkspace(ws.slug, email, role)

// 成员管理 Panel:
//   - 加载 listMembers(ws.slug)
//   - 每行: name, email, role dropdown, remove button (×)
//   - role change → changeMemberRole
//   - remove → removeMember (确认)

// Pending Invitations badge:
//   - 在 sidebar 顶部显示 count badge
//   - 点击展开 list，每个: workspace name, role, accept/reject按钮
```

---

## 任务顺序与依赖

```
Phase 1 (基础设施 — 并行):
├── Task 1.1: F1 — 创建 007 migration SQL         [无依赖]
├── Task 1.2: M2 — workspaces.rs 添加 Role enum   [无依赖]
└── Task 1.3: F2 — 创建 audit.rs                  [无依赖]

Phase 2 (DB 层 — 串行，依赖 Phase 1):
├── Task 2.1: M1 — lib.rs 注册 007 迁移 + audit   [依赖 1.1, 1.3]
└── Task 2.2: M2 — workspaces.rs 添加 DB 函数     [依赖 1.2]

Phase 3 (Server 层 — 并行):
├── Task 3.1: M3 — error.rs 添加 Forbidden        [无依赖]
├── Task 3.2: M4 — workspace.rs 重写 invite       [依赖 2.2]
├── Task 3.3: M4 — workspace.rs 新增 accept/reject [依赖 2.2]
├── Task 3.4: M4 — workspace.rs 新增管理端点       [依赖 2.2]
├── Task 3.5: M4 — workspace.rs 权限守卫函数       [依赖 2.2, 3.1]
└── Task 3.6: M6 — main.rs 注册新路由             [依赖 3.2-3.5]

Phase 4 (前端 — 并行):
├── Task 4.1: M7 — api.ts 新增 API 函数           [无依赖]
├── Task 4.2: M8 — MainLayout.tsx 邀请 Dialog     [依赖 4.1]
├── Task 4.3: M8 — MainLayout.tsx 成员管理 Panel  [依赖 4.1]
├── Task 4.4: M8 — MainLayout.tsx 待处理邀请      [依赖 4.1]
└── Task 4.5: M8 — MainLayout.tsx 角色感知按钮    [依赖 4.1]

Phase 5 (验证):
├── Task 5.1: cargo build --workspace              [依赖 Phase 2-3]
├── Task 5.2: cargo test --workspace              [依赖 5.1]
├── Task 5.3: 手动测试邀请 accept/reject 流程      [依赖 5.1]
├── Task 5.4: 手动测试权限矩阵 (owner/writer/reader) [依赖 5.1]
└── Task 5.5: npm run build (前端)                [依赖 Phase 4]
```

---

## API 端点总览

| Method | Path | 权限 | 说明 |
|--------|------|------|------|
| GET | `/api/workspaces` | 登录 | 列出我的 workspaces (含正确角色) |
| POST | `/api/workspaces` | 登录 | 创建 workspace |
| GET | `/api/workspaces/public` | 登录 | 列出公开 workspace |
| POST | `/api/workspaces/{slug}/join` | 登录 | 加入公开 workspace |
| POST | `/api/workspaces/{slug}/invite` | **owner** | 邀请成员(含 role) |
| GET | `/api/workspaces/{slug}/members` | 成员 | 查看成员列表 |
| POST | `/api/workspaces/{slug}/members/remove` | **owner** | 移除成员 |
| POST | `/api/workspaces/{slug}/members/role` | **owner** | 修改成员角色 |
| POST | `/api/workspaces/{slug}/rename` | **owner** | 重命名 |
| DELETE | `/api/workspaces/{slug}` | **owner** | 删除 workspace |
| GET | `/api/invitations/pending` | 登录 | 待处理邀请列表 |
| POST | `/api/invitations/{id}/accept` | 登录(email匹配) | 接受邀请 |
| POST | `/api/invitations/{id}/reject` | 登录(email匹配) | 拒绝邀请 |

---

## 审计日志 Action 常量

| Action | Target Type | 触发操作 |
|--------|-------------|---------|
| `invite_member` | `invitation` | 邀请成员 |
| `accept_invitation` | `invitation` | 接受邀请 |
| `reject_invitation` | `invitation` | 拒绝邀请 |
| `remove_member` | `user` | 移除成员 |
| `change_member_role` | `user` | 修改角色 |
| `delete_workspace` | `workspace` | 删除 workspace |

---

## 验证步骤

### 自动测试
1. `cargo build --workspace` — 编译通过
2. `cargo test --workspace` — 现有测试不回归
3. `npm run build` (in `web/`) — 前端编译通过

### 手动测试矩阵
1. **Owner 邀请 writer/reader**:
   - POST `/api/workspaces/{slug}/invite` with `{"email":"...", "role":"writer"}`
   - 验证返回 invitation_id，状态 pending
2. **被邀者查看待处理邀请**:
   - GET `/api/invitations/pending`
   - 验证列表中包含该邀请
3. **被邀者接受邀请**:
   - POST `/api/invitations/{id}/accept`
   - 验证返回 workspace 信息，role=writer
   - 验证 `workspace_members` 表中有该用户
4. **被邀者拒绝邀请**:
   - POST `/api/invitations/{id}/reject`
   - 验证 invitation status=rejected
5. **权限守卫 — writer 不能邀请**:
   - writer 调用 invite → 403 Forbidden
6. **权限守卫 — writer 不能删除**:
   - writer 调用 DELETE workspace → 403 Forbidden
7. **权限守卫 — reader 不能编辑**:
   - (现有写操作端点，如有守卫)
8. **Owner 修改角色**:
   - POST `/api/workspaces/{slug}/members/role` → role 更新
9. **Owner 移除成员**:
   - POST `/api/workspaces/{slug}/members/remove` → 成员被移除
10. **审计日志验证**:
    - 每个管理操作后查询 `audit_log` 表，确认记录存在

### 前端验证
11. owner 可见: "Invite", "Manage Members", "Delete" 按钮
12. writer 不可见管理按钮
13. reader 不可见管理按钮和编辑按钮
14. 待处理邀请 badge 显示正确数量
15. Accept/Reject 按钮功能正常

---

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| `create_invitation` 签名变更影响调用方 | 仅 workspace.rs 中 invite 端点调用，一次性改完 |
| `invite` 行为变更(不再自动接受) | 前端同步更新，添加 accept/reject 流程 |
| `list_workspaces` role 查询 N+1 | 当前 workspace 数量少(<100)，可接受。未来可 JOIN 优化 |
| 审计日志表增长 | 定期清理策略未来迭代，当前量级可控 |
| ON DELETE CASCADE 删除 wiki 数据 | delete_workspace 前弹出二次确认对话框 |

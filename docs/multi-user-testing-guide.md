# 多人场景调试与测试指南

> 本文档记录 cowiki 多人权限场景（邀请/角色/成员管理）的调试方法论、常见陷阱和快速测试脚本。

---

## 快速开始：搭建测试场景

### 1. 注册测试用户

注册不依赖 GitHub OAuth，直接用 API 创建：

```bash
# 注册 Alice (owner)
curl -s -X POST http://localhost:3000/api/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"name":"Alice"}' | python3 -m json.tool

# 注册 Bob (被邀请者)
curl -s -X POST http://localhost:3000/api/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"name":"Bob"}' | python3 -m json.tool
```

返回格式：`{"user": {"id": "uuid", "name": "...", "email": null}, "api_key": "cw_xxx"}`

> ⚠️ **注意**: 注册时不设 email，email 为 `null`。需要用 SQL 手动设置（见下文）。

### 2. 给用户设置 email（邀请匹配必需）

邀请系统通过 email 匹配用户。email 为 null 时 JOIN 永远失败。

```bash
sudo docker exec cowiki-db-1 psql -U cowiki -d cowiki <<'SQL'
UPDATE users SET email = 'alice@test.com' WHERE name LIKE 'Alice%';
UPDATE users SET email = 'bob@test.com' WHERE name LIKE 'Bob%';
SQL
```

### 3. 创建 workspace 并发送邀请

```bash
ALICE_KEY="cw_xxx"
SLUG="my-team-$(date +%s)"

# 创建 public workspace
curl -s -X POST http://localhost:3000/api/workspaces \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ALICE_KEY" \
  -d "{\"name\":\"My Team\",\"slug\":\"$SLUG\",\"visibility\":\"public\"}"

# 邀请 Bob
curl -s -X POST "http://localhost:3000/api/workspaces/$SLUG/invite" \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ALICE_KEY" \
  -d '{"email":"bob@test.com","role":"writer"}'
```

### 4. 验证待处理邀请

```bash
BOB_KEY="cw_xxx"
curl -s http://localhost:3000/api/invitations/pending \
  -H "Authorization: Bearer $BOB_KEY" | python3 -m json.tool
```

---

## 浏览器端切换用户

前端认证完全依赖 `localStorage`，无需 GitHub 登录即可模拟任意用户：

```js
// 获取真实 user ID
curl -s http://localhost:3000/api/auth/me -H "Authorization: Bearer $API_KEY"

// Alice 窗口 (F12 Console)
localStorage.setItem('cowiki_api_key', 'cw_b9c46eea9f064b30a2582415259fbc20')
localStorage.setItem('cowiki_user', JSON.stringify({id:'f240de2e-61d2-4d01-b623-ef8056140e70', name:'Alice'}))
location.reload()

// Bob 无痕窗口 (Ctrl+Shift+N → F12 Console)
localStorage.setItem('cowiki_api_key', 'cw_eee904b18b4d4e58aa7fee3d348b5af3')
localStorage.setItem('cowiki_user', JSON.stringify({id:'9af7c817-3737-4dc0-9059-bb6e6919453c', name:'Bob'}))
location.reload()
```

> ⚠️ **关键**: `cowiki_user` 中的 `id` 必须是真实 UUID（从 `/api/auth/me` 获取），不能是 `'alice'`/`'bob'` 这样的假 ID。否则 `userBranch` = `user/bob` 会导致 Git 分支找不到。

> 💡 **两窗口策略**: Alice 在正常浏览器窗口，Bob 在无痕窗口 — 两个独立 localStorage 互不干扰。

---

## 常见陷阱与解决方案

### 1. 邀请列表为空（email NULL 问题）

**现象**: API 返回 `[]`，前端无铃铛按钮。

**根因**: `find_pending_invitations_for_user` 通过 email JOIN 匹配。`users.email = NULL` 时 SQL `NULL = NULL` 不成立。

**修复**: 
- 短期：给用户手动设 email
- 长期：代码改为两步查询（先取 email，再用 email 匹配 invitation），避免 JOIN 的 NULL 问题

```rust
// crates/db/src/workspaces.rs — 修复后
pub async fn find_pending_invitations_for_user(pool, user_id) {
    let email = query_scalar("SELECT email FROM users WHERE id = $1").fetch_optional()?.flatten();
    match email {
        Some(ref e) if !e.is_empty() => {
            query_as("SELECT i.* FROM invitations i WHERE i.email = $1 AND i.status = 'pending'")
        }
        _ => Ok(vec![]) // 无 email → 无匹配
    }
}
```

### 2. Team Space 页面 500（user branch 不存在）

**现象**: `GET /api/workspaces/{slug}/pages?branch=user/{uuid}` 返回 500。

**根因**: `join_workspace` / `accept_invitation` 中 `ensure_user_branch` 只对**默认 repo** 调用，但 workspace 页面使用 `repo_manager.get(ws_slug)` 获取 **workspace 专属 repo**，两个 repo 不同。

**修复**: 
1. 在 `join_workspace` 和 `accept_invitation` 中也调用 `repo_manager.get(slug).ensure_user_branch(user_id)`
2. 在 pages handler 中加入懒创建逻辑：

```rust
// crates/server/src/routes/pages.rs
fn ensure_user_branch_if_needed(repo: &WikiRepo, branch: &str) -> Result<()> {
    if let Some(user_id) = branch.strip_prefix("user/") {
        repo.ensure_user_branch(user_id)?;
    }
    Ok(())
}

// 在每个 workspace handler 中：
let repo = state.repo_manager.get(&ws_slug)?;
ensure_user_branch_if_needed(&repo, &branch)?;
```

> ⚠️ `ensure_user_branch` 接受的是**纯 UUID**（如 `f240de2e-...`），不是完整分支名 `user/f240de2e-...`。

### 3. Server 启动 panic: constraint already exists

**现象**: `cargo run` 时报 `constraint "invitations_role_check" already exists`。

**根因**: 迁移脚本中 `ALTER TABLE ADD CONSTRAINT` 不是幂等的。重复运行迁移会报错。

**修复**: 用 `DO $$` 块包裹，检查约束是否存在：

```sql
-- crates/db/src/migrations/007_team_permissions.sql
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'invitations_role_check'
    ) THEN
        ALTER TABLE invitations ADD CONSTRAINT invitations_role_check
            CHECK (role IN ('owner', 'writer', 'reader'));
    END IF;
END $$;
```

### 4. Port 3000 被占用

**现象**: `cargo run` 报 `AddrInUse`。

```bash
# 强杀占用端口的进程
sudo kill -9 $(lsof -t -i:3000)
cargo run
```

### 5. API 中 find_by_slug 误用 workspace UUID

**现象**: `list_pending_invitations` 返回 `"workspace not found"`。

**根因**: 代码中用 `find_by_slug(workspace_uuid)` 把 UUID 当 slug 查。

**修复**: 新增 `find_by_id()` 函数，根据场景选择正确的查询方法：

```rust
// ID 查询
pub async fn find_by_id(pool, id: Uuid) -> Option<Workspace>;
// slug 查询
pub async fn find_by_slug(pool, slug: &str) -> Option<Workspace>;
```

### 6. Vite 代理 404 — server 未用最新代码

**现象**: 前端 `/api/invitations/pending` 返回 404，但后端路由已注册。

**根因**: server 二进制文件是旧版本，缺少新路由。Vite proxy 配置正确（`/api` → `localhost:3000`），但后端没有该路由。

**修复**: 每次改后端代码后必须 `cargo build && 重启 server`。

---

## 一键测试脚本

```bash
#!/bin/bash
# scripts/setup-multi-user-test.sh — 搭建完整多人测试场景

set -e
API="http://localhost:3000/api"
TIMESTAMP=$(date +%s)

echo "=== 1. 注册用户 ==="
ALICE=$(curl -s -X POST $API/auth/register -H 'Content-Type: application/json' -d "{\"name\":\"Alice-$TIMESTAMP\"}")
BOB=$(curl -s -X POST $API/auth/register -H 'Content-Type: application/json' -d "{\"name\":\"Bob-$TIMESTAMP\"}")
ALICE_KEY=$(echo "$ALICE" | python3 -c "import sys,json;print(json.load(sys.stdin)['api_key'])")
BOB_KEY=$(echo "$BOB" | python3 -c "import sys,json;print(json.load(sys.stdin)['api_key'])")
ALICE_ID=$(echo "$ALICE" | python3 -c "import sys,json;print(json.load(sys.stdin)['user']['id'])")
BOB_ID=$(echo "$BOB" | python3 -c "import sys,json;print(json.load(sys.stdin)['user']['id'])")

echo "=== 2. 设置 email ==="
sudo docker exec cowiki-db-1 psql -U cowiki -d cowiki -c \
  "UPDATE users SET email='alice@t.com' WHERE id='$ALICE_ID'; UPDATE users SET email='bob@t.com' WHERE id='$BOB_ID';"

echo "=== 3. 创建 workspace + 邀请 ==="
SLUG="team-$TIMESTAMP"
curl -s -X POST $API/workspaces -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ALICE_KEY" \
  -d "{\"name\":\"Test Team\",\"slug\":\"$SLUG\",\"visibility\":\"public\"}" > /dev/null

curl -s -X POST "$API/workspaces/$SLUG/invite" -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ALICE_KEY" \
  -d '{"email":"bob@t.com","role":"writer"}' > /dev/null

echo "=== 4. 验证 ==="
PENDING=$(curl -s $API/invitations/pending -H "Authorization: Bearer $BOB_KEY")
COUNT=$(echo "$PENDING" | python3 -c "import sys,json;print(len(json.load(sys.stdin)))")
echo "Bob 有 $COUNT 条待处理邀请"

echo ""
echo "========================"
echo "Alice: localStorage.setItem('cowiki_api_key','$ALICE_KEY'); localStorage.setItem('cowiki_user',JSON.stringify({id:'$ALICE_ID',name:'Alice'})); location.reload()"
echo "Bob:   localStorage.setItem('cowiki_api_key','$BOB_KEY'); localStorage.setItem('cowiki_user',JSON.stringify({id:'$BOB_ID',name:'Bob'})); location.reload()"
```

---

## 调试检查清单

遇到多人场景问题时，按以下顺序排查：

- [ ] Server 是否用最新代码重启？（`cargo build && pkill cowiki-server && cargo run`）
- [ ] 端口是否被占用？（`lsof -i:3000`）
- [ ] 迁移是否幂等？（`docker exec cowiki-db-1 psql -U cowiki -d cowiki -c "\d invitations"`）
- [ ] 测试用户 email 是否为 null？（`GET /api/auth/me`）
- [ ] API 是否用 curl 直接验证通过？（绕过前端 proxy）
- [ ] localStorage 中的 id 是否是真实 UUID？
- [ ] `find_by_slug` vs `find_by_id` 是否用对？
- [ ] workspace repo 中是否创建了 user branch？

---

## 相关文件

| 文件 | 说明 |
|------|------|
| `crates/db/src/workspaces.rs` | Role enum, DB 函数, 邀请匹配 |
| `crates/db/src/migrations/007_team_permissions.sql` | 迁移脚本 |
| `crates/db/src/audit.rs` | 审计日志 |
| `crates/server/src/routes/workspace.rs` | 权限守卫, 邀请/成员管理端点 |
| `crates/server/src/routes/pages.rs` | 懒创建 user branch |
| `crates/server/src/error.rs` | AppError::Forbidden(403) |
| `crates/server/src/main.rs` | 路由注册 |
| `web/src/api.ts` | 前端 API 函数 |
| `web/src/pages/MainLayout.tsx` | 邀请通知, 角色感知 UI |
| `crates/server/tests/permission_api_tests.rs` | API 集成测试 |

# Team Space 邀请+权限系统 — 规格文档

> 通过 Deep Interview 生成 | 歧义度: 8% | 2026-05-26

---

## 目标 (Goal)

为 cowiki 实现 GitHub 风格的 Team Space 邀请 + 基于角色的权限控制系统，支持可扩展的角色模型和轻量审计日志。

## 约束 (Constraints)

1. **角色模型**: GitHub 风格三层 — `owner`(管理+读写), `writer`(读写), `reader`(只读)
2. **可扩展**: 角色通过数据库 CHECK 约束 + Rust enum 管理，添加新角色只需改约束和代码
3. **邀请流程**: 邀请时可指定角色 → 被邀者接受/拒绝 → 自动加入并赋予对应角色
4. **权限粒度**: owner 独享管理权（邀请/移除成员/改角色/改设置/删 workspace），writer 可编辑内容，reader 只读
5. **审计日志**: 轻量 `audit_log` 表，记录 (actor, action, target_type, target_id, metadata JSON, timestamp)
6. **向后兼容**: 现有 API 不 breaking change，role 为 String 的字段暂时保留

## 非目标 (Non-Goals)

- 自定义角色 / 角色模板（RBAC 完整实现）→ 未来迭代
- 审计日志高级查询 UI → 仅提供 API
- Team 嵌套 / 子团队 → 未来迭代
- 仓库级细粒度权限 → 当前 workspace 级别即可

## 成功标准 (Acceptance Criteria)

1. ✅ 邀请 API 支持 `role` 参数，被邀者接受后自动获得指定角色
2. ✅ 权限中间件/守卫: 非 owner 调用管理 API 返回 `403 Forbidden`
3. ✅ 权限矩阵测试: 覆盖 owner/writer/reader 对所有 API 的访问控制
4. ✅ `audit_log` 表记录所有管理操作（邀请/移除/改角色/改设置）
5. ✅ 前端根据当前用户角色正确显示/隐藏管理按钮
6. ✅ 现有测试不回归

## 权限矩阵

| 操作 | owner | writer | reader | 匿名/非成员 |
|------|-------|--------|--------|------------|
| 查看 workspace/wiki | ✅ | ✅ | ✅ | public only |
| 编辑 wiki 内容 | ✅ | ✅ | ❌ | ❌ |
| 提交审核 | ✅ | ✅ | ❌ | ❌ |
| 邀请成员 | ✅ | ❌ | ❌ | ❌ |
| 移除成员 | ✅ | ❌ | ❌ | ❌ |
| 修改成员角色 | ✅ | ❌ | ❌ | ❌ |
| 修改 workspace 设置 | ✅ | ❌ | ❌ | ❌ |
| 删除 workspace | ✅ | ❌ | ❌ | ❌ |
| 查看成员列表 | ✅ | ✅ | ✅ | ❌ |

## 本体论 (Key Entities)

| 实体 | 稳定性 | 说明 |
|------|--------|------|
| WorkspaceMember | 稳定 | 现有，增加 `invited_by` 已有 |
| Role (enum) | 稳定 | 现有 CHECK 约束: owner/writer/reader，需改为可扩展设计 |
| Invitation | 稳定 | 现有，需增加 `role` 字段 |
| AuditLog | 新增 | (id, workspace_id, actor_id, action, target_type, target_id, metadata JSONB, created_at) |
| PermissionGuard | 新增 | 中间件/辅助函数，基于角色+操作做鉴权 |

## 数据模型变更

### migrations/005 — 新增
```sql
-- invitation 增加 role
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS role VARCHAR(20) NOT NULL DEFAULT 'writer';
ALTER TABLE invitations ADD CONSTRAINT invitations_role_check
    CHECK (role IN ('owner', 'writer', 'reader'));

-- 审计日志表
CREATE TABLE IF NOT EXISTS audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    actor_id UUID NOT NULL REFERENCES users(id),
    action VARCHAR(50) NOT NULL,
    target_type VARCHAR(50),
    target_id UUID,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_audit_log_workspace ON audit_log(workspace_id, created_at DESC);
```

### 角色扩展设计
- Rust 侧: 定义 `Role` enum + `FromStr`/`Display` + 权限判断方法
- DB 侧: CHECK 约束保持最新角色列表
- 添加新角色: 1) 更新 CHECK 约束 (migration) 2) 添加 enum variant 3) 更新权限矩阵

## 访谈记录

| Round | 问题 | 回答 | 歧义度变化 |
|-------|------|------|-----------|
| 1 | 核心目标是什么？ | 完整实现邀请流程 + 基于角色的权限控制 | 100% → 23% |
| 2 | 角色权限边界如何定义？ | GitHub 风格: owner 管理+读写, writer 读写, reader 只读 | 23% → 19% |
| 3 | owner 管理权范围 + 成功标准？ | 全部管理 + 审计日志 | 19% → 12% |
| 4 (Contrarian) | 审计日志 MVP 必需？ | 坚持但要轻量, 简单 audit_log 表 | 12% → 8% |

## 假设暴露与解决

| 假设 | 挑战 | 解决 |
|------|------|------|
| "审计日志要完整" | Contrarian: 太重 | 轻量 audit_log 表, 仅记录管理操作 |
| "角色固定三层" | 用户明确要求可扩展 | CHECK 约束 + enum 设计预留扩展点 |
| "邀请默认 writer" | 由本轮访谈暴露 | 邀请 API 增加 role 参数 |

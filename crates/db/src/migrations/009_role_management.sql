-- ============================================================
-- Migration 009: Enhanced Role System + User Account Invitations
-- Preserves 001–008, adds role expansion + new tables
-- ============================================================

-- 1. 更新 workspace_members 角色约束
ALTER TABLE workspace_members
    DROP CONSTRAINT IF EXISTS workspace_members_role_check;

ALTER TABLE workspace_members
    ADD CONSTRAINT workspace_members_role_check
    CHECK (role IN ('owner', 'manager', 'editor', 'reviewer', 'viewer'));

-- 2. 新增列：来源追踪 + 最后活跃时间
ALTER TABLE workspace_members
    ADD COLUMN IF NOT EXISTS joined_via TEXT NOT NULL DEFAULT 'direct',
    ADD COLUMN IF NOT EXISTS last_active_at TIMESTAMPTZ NOT NULL DEFAULT now();

ALTER TABLE workspace_members
    ADD CONSTRAINT workspace_members_joined_via_check
    CHECK (joined_via IN ('direct', 'invitation', 'public_join'));

-- 3. 更新 invitations: 角色约束 + User Account 邀请 + 新字段
ALTER TABLE invitations
    DROP CONSTRAINT IF EXISTS invitations_role_check;

ALTER TABLE invitations
    ADD CONSTRAINT invitations_role_check
    CHECK (role IN ('owner', 'manager', 'editor', 'reviewer', 'viewer'));

ALTER TABLE invitations
    ADD COLUMN IF NOT EXISTS invited_user_id UUID REFERENCES users(id),
    ADD COLUMN IF NOT EXISTS message TEXT,
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ
        DEFAULT (now() + INTERVAL '7 days'),
    ADD COLUMN IF NOT EXISTS resent_count INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_resent_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_invitations_user ON invitations(invited_user_id, status);
CREATE INDEX IF NOT EXISTS idx_invitations_status ON invitations(status);

-- 4. Ownership 转让表
CREATE TABLE IF NOT EXISTS ownership_transfers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    from_user_id UUID NOT NULL REFERENCES users(id),
    to_user_id UUID NOT NULL REFERENCES users(id),
    previous_owner_new_role TEXT NOT NULL DEFAULT 'manager'
        CHECK (previous_owner_new_role IN ('manager', 'editor', 'viewer')),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'rejected', 'cancelled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_transfers_workspace
    ON ownership_transfers(workspace_id);
CREATE INDEX IF NOT EXISTS idx_transfers_to_user
    ON ownership_transfers(to_user_id, status);

-- 5. 旧角色兼容转换 (向后兼容)
UPDATE workspace_members SET role = 'editor' WHERE role = 'writer';
UPDATE workspace_members SET role = 'viewer' WHERE role = 'reader';
UPDATE invitations SET role = 'editor' WHERE role = 'writer';
UPDATE invitations SET role = 'viewer' WHERE role = 'reader';

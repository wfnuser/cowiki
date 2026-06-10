-- Ensure 4-tier role constraint on workspace_members (idempotent)
ALTER TABLE workspace_members DROP CONSTRAINT IF EXISTS workspace_members_role_check;
ALTER TABLE workspace_members ADD CONSTRAINT workspace_members_role_check
    CHECK (role IN ('owner', 'manager', 'editor', 'viewer'));

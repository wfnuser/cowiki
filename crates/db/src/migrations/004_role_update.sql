-- Update role check constraint to 4-tier system
ALTER TABLE workspace_members DROP CONSTRAINT IF EXISTS workspace_members_role_check;
ALTER TABLE workspace_members ADD CONSTRAINT workspace_members_role_check
    CHECK (role IN ('owner', 'manager', 'editor', 'viewer'));

-- Migrate old roles
UPDATE workspace_members SET role = 'editor' WHERE role = 'member';
UPDATE workspace_members SET role = 'editor' WHERE role = 'admin';
UPDATE workspace_members SET role = 'editor' WHERE role = 'writer';
UPDATE workspace_members SET role = 'viewer' WHERE role = 'reader';

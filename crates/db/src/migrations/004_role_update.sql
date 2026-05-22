-- Update role check constraint to new values
ALTER TABLE workspace_members DROP CONSTRAINT IF EXISTS workspace_members_role_check;
ALTER TABLE workspace_members ADD CONSTRAINT workspace_members_role_check
    CHECK (role IN ('owner', 'writer', 'reader'));

-- Migrate old roles
UPDATE workspace_members SET role = 'writer' WHERE role = 'member';
UPDATE workspace_members SET role = 'writer' WHERE role = 'admin';

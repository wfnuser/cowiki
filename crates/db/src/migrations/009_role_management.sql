-- Migration 009: Enhanced Role System + User Account Invitations (idempotent)

-- Step 1: Drop old constraints first, so UPDATEs to new role values aren't blocked
ALTER TABLE workspace_members DROP CONSTRAINT IF EXISTS workspace_members_role_check;
ALTER TABLE invitations DROP CONSTRAINT IF EXISTS invitations_role_check;

-- Step 2: Migrate known old roles to new values
UPDATE workspace_members SET role = 'editor' WHERE role IN ('admin', 'member', 'writer');
UPDATE workspace_members SET role = 'viewer' WHERE role IN ('reader');
UPDATE invitations SET role = 'editor' WHERE role IN ('writer');
UPDATE invitations SET role = 'viewer' WHERE role IN ('reader');

-- Step 3: Safety net — map any remaining unrecognized/NULL roles to 'editor'
UPDATE workspace_members SET role = 'editor' WHERE role IS NULL OR role NOT IN ('owner', 'manager', 'editor', 'reviewer', 'viewer');
UPDATE invitations SET role = 'editor' WHERE role IS NULL OR role NOT IN ('owner', 'manager', 'editor', 'reviewer', 'viewer');

-- Step 4: Add new constraints on cleaned data
ALTER TABLE workspace_members ADD CONSTRAINT workspace_members_role_check CHECK (role IN ('owner', 'manager', 'editor', 'reviewer', 'viewer'));
ALTER TABLE invitations ADD CONSTRAINT invitations_role_check CHECK (role IN ('owner', 'manager', 'editor', 'reviewer', 'viewer'));

ALTER TABLE workspace_members ADD COLUMN IF NOT EXISTS joined_via TEXT NOT NULL DEFAULT 'direct';
ALTER TABLE workspace_members ADD COLUMN IF NOT EXISTS last_active_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE workspace_members DROP CONSTRAINT IF EXISTS workspace_members_joined_via_check;
ALTER TABLE workspace_members ADD CONSTRAINT workspace_members_joined_via_check CHECK (joined_via IN ('direct', 'invitation', 'public_join'));

ALTER TABLE invitations ADD COLUMN IF NOT EXISTS invited_user_id UUID REFERENCES users(id);
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS message TEXT;
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ DEFAULT (now() + INTERVAL '7 days');
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS resent_count INT NOT NULL DEFAULT 0;
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS last_resent_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_invitations_user ON invitations(invited_user_id, status);
CREATE INDEX IF NOT EXISTS idx_invitations_status ON invitations(status);

CREATE TABLE IF NOT EXISTS ownership_transfers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    from_user_id UUID NOT NULL REFERENCES users(id),
    to_user_id UUID NOT NULL REFERENCES users(id),
    previous_owner_new_role TEXT NOT NULL DEFAULT 'manager' CHECK (previous_owner_new_role IN ('manager', 'editor', 'viewer')),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'rejected', 'cancelled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_transfers_workspace ON ownership_transfers(workspace_id);
CREATE INDEX IF NOT EXISTS idx_transfers_to_user ON ownership_transfers(to_user_id, status);

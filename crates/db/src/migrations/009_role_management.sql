-- Migration 009: Enhanced Role System — new columns + tables only
-- Role constraints already fixed in 004 and 007 (no migration needed here)

-- New columns for workspace_members
ALTER TABLE workspace_members ADD COLUMN IF NOT EXISTS joined_via TEXT NOT NULL DEFAULT 'direct';
ALTER TABLE workspace_members ADD COLUMN IF NOT EXISTS last_active_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE workspace_members DROP CONSTRAINT IF EXISTS workspace_members_joined_via_check;
ALTER TABLE workspace_members ADD CONSTRAINT workspace_members_joined_via_check CHECK (joined_via IN ('direct', 'invitation', 'public_join'));

-- New columns for invitations
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS invited_user_id UUID REFERENCES users(id);
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS message TEXT;
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ DEFAULT (now() + INTERVAL '7 days');
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS resent_count INT NOT NULL DEFAULT 0;
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS last_resent_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_invitations_user ON invitations(invited_user_id, status);
CREATE INDEX IF NOT EXISTS idx_invitations_status ON invitations(status);

-- Ownership transfers table
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

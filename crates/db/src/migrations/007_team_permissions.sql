-- 007_team_permissions: Team Space invitation + permissions + audit log
-- Adds: invitation.role, audit_log table

-- 1. Add role column to invitations
ALTER TABLE invitations ADD COLUMN IF NOT EXISTS role VARCHAR(20) NOT NULL DEFAULT 'writer';

-- Add role check constraint (idempotent)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'invitations_role_check'
    ) THEN
        ALTER TABLE invitations ADD CONSTRAINT invitations_role_check
            CHECK (role IN ('owner', 'writer', 'reader'));
    END IF;
END $$;

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

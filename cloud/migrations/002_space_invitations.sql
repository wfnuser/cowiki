CREATE TABLE space_invitations (
    id UUID PRIMARY KEY,
    space_id UUID NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id),
    role member_role NOT NULL CHECK (role <> 'owner'),
    token_hash BYTEA NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    accepted_count INTEGER NOT NULL DEFAULT 0 CHECK (accepted_count >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX space_invitations_active_space_idx
    ON space_invitations(space_id, created_at DESC)
    WHERE revoked_at IS NULL;

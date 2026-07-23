CREATE TYPE member_role AS ENUM ('owner', 'manager', 'editor', 'viewer');
CREATE TYPE pull_request_status AS ENUM ('open', 'merged', 'closed');

CREATE TABLE users (
    id UUID PRIMARY KEY,
    github_id BIGINT NOT NULL UNIQUE,
    handle TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    avatar_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE oauth_states (
    state_hash BYTEA PRIMARY KEY,
    desktop_callback TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE desktop_exchange_codes (
    code_hash BYTEA PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE api_keys (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL UNIQUE,
    label TEXT NOT NULL,
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX api_keys_user_id_idx ON api_keys(user_id);

CREATE TABLE spaces (
    id UUID PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,62}$'),
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 120),
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE space_members (
    space_id UUID NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role member_role NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (space_id, user_id)
);

CREATE INDEX space_members_user_id_idx ON space_members(user_id);

CREATE TABLE pull_requests (
    id UUID PRIMARY KEY,
    space_id UUID NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    number BIGINT NOT NULL,
    author_id UUID NOT NULL REFERENCES users(id),
    title TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 240),
    body TEXT NOT NULL DEFAULT '',
    base_ref TEXT NOT NULL DEFAULT 'main' CHECK (base_ref = 'main'),
    head_ref TEXT NOT NULL,
    base_oid TEXT NOT NULL CHECK (base_oid ~ '^[0-9a-f]{40,64}$'),
    head_oid TEXT NOT NULL CHECK (head_oid ~ '^[0-9a-f]{40,64}$'),
    status pull_request_status NOT NULL DEFAULT 'open',
    merged_by UUID REFERENCES users(id),
    merged_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (space_id, number)
);

CREATE UNIQUE INDEX one_open_pr_per_head
    ON pull_requests(space_id, head_ref)
    WHERE status = 'open';

CREATE TABLE pull_request_approvals (
    pull_request_id UUID NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    head_oid TEXT NOT NULL CHECK (head_oid ~ '^[0-9a-f]{40,64}$'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (pull_request_id, user_id)
);

CREATE TABLE audit_events (
    id BIGSERIAL PRIMARY KEY,
    space_id UUID REFERENCES spaces(id) ON DELETE SET NULL,
    actor_id UUID REFERENCES users(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX audit_events_space_created_idx ON audit_events(space_id, created_at DESC);

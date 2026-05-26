-- API Keys: multi-key support with SHA-256 hashing
CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_prefix TEXT NOT NULL,         -- first 8 chars of raw key (for display)
    key_hash TEXT NOT NULL UNIQUE,    -- SHA-256 hash of full key
    last_used_at TIMESTAMPTZ,         -- NULL = never used
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ            -- NULL = active
);

CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash ON api_keys(key_hash);

-- #59: store the primary API key (users.api_key) as a SHA-256 hex digest instead
-- of plaintext. Idempotent: cw_-prefixed (or any non-64-hex) values are legacy
-- plaintext and get hashed; already-hashed values match ^[0-9a-f]{64}$ and are
-- left alone. Existing keys keep working — auth hashes the incoming bearer token.
CREATE EXTENSION IF NOT EXISTS pgcrypto;
UPDATE users
SET api_key = encode(digest(api_key, 'sha256'), 'hex')
WHERE api_key !~ '^[0-9a-f]{64}$';

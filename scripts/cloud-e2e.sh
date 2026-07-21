#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/cowiki-cloud-e2e.XXXXXX")"
postgres_name="cowiki-cloud-e2e-$$"
cloud_pid=""

cleanup() {
  if [ -n "$cloud_pid" ]; then
    kill "$cloud_pid" 2>/dev/null || true
    wait "$cloud_pid" 2>/dev/null || true
  fi
  docker rm -f "$postgres_name" >/dev/null 2>&1 || true
  rm -rf "$tmp"
}
trap cleanup EXIT HUP INT TERM

for command in cargo curl docker git jq python3; do
  command -v "$command" >/dev/null || { echo "$command is required" >&2; exit 1; }
done

docker run --detach --rm --name "$postgres_name" \
  -e POSTGRES_PASSWORD=cowiki \
  -p 127.0.0.1::5432 \
  postgres:17-alpine >/dev/null

for _ in $(seq 1 60); do
  if docker exec "$postgres_name" pg_isready -U postgres >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
docker exec "$postgres_name" pg_isready -U postgres >/dev/null
postgres_port="$(docker port "$postgres_name" 5432/tcp | awk -F: 'NR == 1 { print $NF }')"
cloud_port="$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"

pepper="0123456789abcdef0123456789abcdef"
owner_id="11111111-1111-4111-8111-111111111111"
manager_id="22222222-2222-4222-8222-222222222222"
owner_token="cw_key_e2e_owner"
manager_token="cw_key_e2e_manager"
origin="http://127.0.0.1:$cloud_port"
repos="$tmp/repos"
mkdir -p "$repos"

DATABASE_URL="postgres://postgres:cowiki@127.0.0.1:$postgres_port/postgres" \
COWIKI_REPO_ROOT="$repos" \
COWIKI_PUBLIC_ORIGIN="$origin" \
COWIKI_BIND_ADDR="127.0.0.1:$cloud_port" \
COWIKI_TOKEN_PEPPER="$pepper" \
GITHUB_CLIENT_ID=e2e \
GITHUB_CLIENT_SECRET=e2e \
RUST_LOG=warn \
  cargo run --quiet --manifest-path "$root/cloud/Cargo.toml" >"$tmp/cloud.log" 2>&1 &
cloud_pid=$!

for _ in $(seq 1 120); do
  if curl --fail --silent "$origin/healthz" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$cloud_pid" 2>/dev/null; then
    cat "$tmp/cloud.log" >&2
    exit 1
  fi
  sleep 1
done
curl --fail --silent "$origin/healthz" >/dev/null

hash_token() {
  python3 - "$pepper" "$1" <<'PY'
import hashlib
import sys
print(hashlib.sha256(sys.argv[1].encode() + b"\0" + sys.argv[2].encode()).hexdigest())
PY
}

owner_hash="$(hash_token "$owner_token")"
manager_hash="$(hash_token "$manager_token")"
docker exec -i "$postgres_name" psql -v ON_ERROR_STOP=1 -U postgres -d postgres >/dev/null <<SQL
INSERT INTO users (id, github_id, handle, display_name) VALUES
  ('$owner_id', 900001, 'e2e-owner', 'E2E Owner'),
  ('$manager_id', 900002, 'e2e-manager', 'E2E Manager');
INSERT INTO api_keys (id, user_id, token_hash, label) VALUES
  ('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '$owner_id', decode('$owner_hash', 'hex'), 'e2e'),
  ('bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', '$manager_id', decode('$manager_hash', 'hex'), 'e2e');
SQL

space_json="$(curl --fail --silent \
  -H "Authorization: Bearer $owner_token" \
  -H 'Content-Type: application/json' \
  -d '{"name":"E2E Space","slug":"e2e-space"}' \
  "$origin/api/spaces")"
space_id="$(jq -er '.id' <<<"$space_json")"
git_url="$(jq -er '.gitUrl' <<<"$space_json")"

docker exec -i "$postgres_name" psql -v ON_ERROR_STOP=1 -U postgres -d postgres >/dev/null <<SQL
INSERT INTO space_members (space_id, user_id, role)
VALUES ('$space_id', '$manager_id', 'manager');
SQL

git_with_token() {
  local token="$1"
  shift
  git -c "http.extraHeader=Authorization: Bearer $token" "$@"
}

local_repo="$tmp/local"
mkdir -p "$local_repo"
git -C "$local_repo" init -b main >/dev/null
git -C "$local_repo" config user.name 'E2E Owner'
git -C "$local_repo" config user.email 'owner@e2e.cowiki'
printf '%s\n' '---' 'okf_version: "0.1"' '---' '' '# Initial' >"$local_repo/index.md"
git -C "$local_repo" add index.md
git -C "$local_repo" commit -m 'initial' >/dev/null
git -C "$local_repo" remote add cowiki "$git_url"
git_with_token "$owner_token" -C "$local_repo" push --atomic cowiki \
  main:refs/heads/main \
  "main:refs/heads/user/$owner_id" >/dev/null

printf '\nShared through CoWiki Cloud.\n' >>"$local_repo/index.md"
git -C "$local_repo" commit -am 'share Cloud update' >/dev/null
git_with_token "$owner_token" -C "$local_repo" push cowiki \
  "main:refs/heads/user/$owner_id" >/dev/null
head_oid="$(git -C "$local_repo" rev-parse HEAD)"
base_oid="$(git -C "$local_repo" rev-parse HEAD^)"

pr_json="$(curl --fail --silent \
  -H "Authorization: Bearer $owner_token" \
  -H 'Content-Type: application/json' \
  -d '{"title":"Share Cloud update","body":"End-to-end verification"}' \
  "$origin/api/spaces/$space_id/pull-requests")"
pr_id="$(jq -er '.id' <<<"$pr_json")"
test "$(jq -er '.headOid' <<<"$pr_json")" = "$head_oid"

approval_json="$(curl --fail --silent -X POST \
  -H "Authorization: Bearer $manager_token" \
  "$origin/api/spaces/$space_id/pull-requests/$pr_id/approve")"
test "$(jq -er '.approvalCount' <<<"$approval_json")" = "1"

stale_status="$(curl --silent --output "$tmp/stale.json" --write-out '%{http_code}' \
  -H "Authorization: Bearer $manager_token" \
  -H 'Content-Type: application/json' \
  -d "{\"expectedHeadOid\":\"$base_oid\"}" \
  "$origin/api/spaces/$space_id/pull-requests/$pr_id/merge")"
test "$stale_status" = "409"

merged_json="$(curl --fail --silent \
  -H "Authorization: Bearer $manager_token" \
  -H 'Content-Type: application/json' \
  -d "{\"expectedHeadOid\":\"$head_oid\"}" \
  "$origin/api/spaces/$space_id/pull-requests/$pr_id/merge")"
test "$(jq -er '.status' <<<"$merged_json")" = "merged"

second_repo="$tmp/second"
git_with_token "$manager_token" clone --quiet "$git_url" "$second_repo"
test "$(git -C "$second_repo" rev-parse HEAD)" = "$head_oid"
grep -Fq 'Shared through CoWiki Cloud.' "$second_repo/index.md"

echo "Cloud PostgreSQL + Smart HTTP + pull-request E2E passed"

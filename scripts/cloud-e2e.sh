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
editor_id="33333333-3333-4333-8333-333333333333"
viewer_id="44444444-4444-4444-8444-444444444444"
outsider_id="55555555-5555-4555-8555-555555555555"
owner_token="cw_key_e2e_owner"
manager_token="cw_key_e2e_manager"
editor_token="cw_key_e2e_editor"
viewer_token="cw_key_e2e_viewer"
outsider_token="cw_key_e2e_outsider"
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
editor_hash="$(hash_token "$editor_token")"
viewer_hash="$(hash_token "$viewer_token")"
outsider_hash="$(hash_token "$outsider_token")"
docker exec -i "$postgres_name" psql -v ON_ERROR_STOP=1 -U postgres -d postgres >/dev/null <<SQL
INSERT INTO users (id, github_id, handle, display_name) VALUES
  ('$owner_id', 900001, 'e2e-owner', 'E2E Owner'),
  ('$manager_id', 900002, 'e2e-manager', 'E2E Manager'),
  ('$editor_id', 900003, 'e2e-editor', 'E2E Editor'),
  ('$viewer_id', 900004, 'e2e-viewer', 'E2E Viewer'),
  ('$outsider_id', 900005, 'e2e-outsider', 'E2E Outsider');
INSERT INTO api_keys (id, user_id, token_hash, label) VALUES
  ('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '$owner_id', decode('$owner_hash', 'hex'), 'e2e'),
  ('bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', '$manager_id', decode('$manager_hash', 'hex'), 'e2e'),
  ('cccccccc-cccc-4ccc-8ccc-cccccccccccc', '$editor_id', decode('$editor_hash', 'hex'), 'e2e'),
  ('dddddddd-dddd-4ddd-8ddd-dddddddddddd', '$viewer_id', decode('$viewer_hash', 'hex'), 'e2e'),
  ('eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee', '$outsider_id', decode('$outsider_hash', 'hex'), 'e2e');
SQL

api_json() {
  local token="$1"
  local method="$2"
  local path="$3"
  local body="${4:-}"
  if [ -n "$body" ]; then
    curl --fail --silent -X "$method" \
      -H "Authorization: Bearer $token" \
      -H 'Content-Type: application/json' \
      -d "$body" "$origin$path"
  else
    curl --fail --silent -X "$method" \
      -H "Authorization: Bearer $token" "$origin$path"
  fi
}

api_status() {
  local token="$1"
  local method="$2"
  local path="$3"
  local body="${4:-}"
  local args=(--silent --output /dev/null --write-out '%{http_code}' -X "$method" -H "Authorization: Bearer $token")
  if [ -n "$body" ]; then
    args+=(-H 'Content-Type: application/json' -d "$body")
  fi
  curl "${args[@]}" "$origin$path"
}

git_with_token() {
  local token="$1"
  shift
  git -c "http.extraHeader=Authorization: Bearer $token" "$@"
}

expect_git_failure() {
  local label="$1"
  shift
  if "$@" >"$tmp/forbidden-git.out" 2>&1; then
    echo "expected Git failure: $label" >&2
    exit 1
  fi
}

create_space() {
  local name="$1"
  local slug="$2"
  api_json "$owner_token" POST /api/spaces "{\"name\":\"$name\",\"slug\":\"$slug\"}"
}

space_json="$(create_space 'E2E Space' 'e2e-space')"
space_id="$(jq -er '.id' <<<"$space_json")"
git_url="$(jq -er '.gitUrl' <<<"$space_json")"

# Preserve the existing role model: Owner/Manager manage members, Editor pushes,
# and Viewer reads only.
test "$(jq -er '.role' <<<"$(api_json "$owner_token" POST "/api/spaces/$space_id/members" '{"handle":"e2e-manager","role":"manager"}')")" = "manager"
test "$(jq -er '.role' <<<"$(api_json "$manager_token" POST "/api/spaces/$space_id/members" '{"handle":"e2e-editor","role":"editor"}')")" = "editor"
test "$(jq -er '.role' <<<"$(api_json "$manager_token" POST "/api/spaces/$space_id/members" '{"handle":"e2e-viewer","role":"viewer"}')")" = "viewer"
test "$(api_status "$editor_token" POST "/api/spaces/$space_id/members" '{"handle":"e2e-viewer","role":"editor"}')" = "403"

owner_repo="$tmp/owner"
mkdir -p "$owner_repo"
git -C "$owner_repo" init -b main >/dev/null
git -C "$owner_repo" config user.name 'E2E Owner'
git -C "$owner_repo" config user.email 'owner@e2e.cowiki'
printf '%s\n' '---' 'okf_version: "0.1"' '---' '' '# Initial' >"$owner_repo/index.md"
git -C "$owner_repo" add index.md
git -C "$owner_repo" commit -m 'initial' >/dev/null
git -C "$owner_repo" remote add cowiki "$git_url"
git_with_token "$owner_token" -C "$owner_repo" push --atomic cowiki \
  main:refs/heads/main \
  "main:refs/heads/user/$owner_id" >/dev/null
initial_oid="$(git -C "$owner_repo" rev-parse HEAD)"

editor_repo="$tmp/editor"
git_with_token "$editor_token" clone --quiet "$git_url" "$editor_repo"
git -C "$editor_repo" config user.name 'E2E Editor'
git -C "$editor_repo" config user.email 'editor@e2e.cowiki'
mkdir -p "$editor_repo/guide"
printf '%s\n' '# Shared workflow' '' 'Published by an Editor through a Cloud pull request.' >"$editor_repo/guide/e2e.md"
printf '\377\376\375' >"$editor_repo/image.bin"
git -C "$editor_repo" add guide/e2e.md image.bin
git -C "$editor_repo" commit -m 'share editor knowledge' >/dev/null

# The live Smart HTTP + pre-receive boundary must reject every forbidden ref,
# not merely pass the pure Rust validator tests.
expect_git_failure 'Editor direct push to main' \
  git_with_token "$editor_token" -C "$editor_repo" push origin HEAD:refs/heads/main
test "$(git_with_token "$owner_token" ls-remote "$git_url" refs/heads/main | awk '{print $1}')" = "$initial_oid"
expect_git_failure 'Editor push to another user branch' \
  git_with_token "$editor_token" -C "$editor_repo" push origin "HEAD:refs/heads/user/$owner_id"
git_with_token "$editor_token" -C "$editor_repo" push origin "HEAD:refs/heads/user/$editor_id" >/dev/null
expect_git_failure 'Editor deletion of own user branch' \
  git_with_token "$editor_token" -C "$editor_repo" push origin ":refs/heads/user/$editor_id"

viewer_repo="$tmp/viewer"
git_with_token "$viewer_token" clone --quiet "$git_url" "$viewer_repo"
git -C "$viewer_repo" config user.name 'E2E Viewer'
git -C "$viewer_repo" config user.email 'viewer@e2e.cowiki'
printf '%s\n' '# Viewer write' >"$viewer_repo/viewer.md"
git -C "$viewer_repo" add viewer.md
git -C "$viewer_repo" commit -m 'viewer write' >/dev/null
expect_git_failure 'Viewer push' \
  git_with_token "$viewer_token" -C "$viewer_repo" push origin "HEAD:refs/heads/user/$viewer_id"

# Non-owner bootstrap and mismatched owner bootstrap both leave the new bare
# repository without refs.
non_owner_space="$(create_space 'Non-owner Bootstrap' 'non-owner-bootstrap')"
non_owner_space_id="$(jq -er '.id' <<<"$non_owner_space")"
non_owner_git_url="$(jq -er '.gitUrl' <<<"$non_owner_space")"
api_json "$owner_token" POST "/api/spaces/$non_owner_space_id/members" '{"handle":"e2e-editor","role":"editor"}' >/dev/null
expect_git_failure 'Non-owner bootstrap' \
  git_with_token "$editor_token" -C "$editor_repo" push --atomic "$non_owner_git_url" \
    HEAD:refs/heads/main "HEAD:refs/heads/user/$editor_id"
test -z "$(git_with_token "$owner_token" ls-remote --heads "$non_owner_git_url")"

mismatch_space="$(create_space 'Mismatched Bootstrap' 'mismatched-bootstrap')"
mismatch_git_url="$(jq -er '.gitUrl' <<<"$mismatch_space")"
expect_git_failure 'Bootstrap refs with unequal OIDs' \
  git_with_token "$owner_token" -C "$editor_repo" push --atomic "$mismatch_git_url" \
    HEAD~1:refs/heads/main "HEAD:refs/heads/user/$owner_id"
test -z "$(git_with_token "$owner_token" ls-remote --heads "$mismatch_git_url")"

head_oid="$(git -C "$editor_repo" rev-parse HEAD)"
pr_json="$(api_json "$editor_token" POST "/api/spaces/$space_id/pull-requests" '{"title":"Share editor knowledge","body":"End-to-end verification"}')"
pr_id="$(jq -er '.id' <<<"$pr_json")"
test "$(jq -er '.headOid' <<<"$pr_json")" = "$head_oid"
test "$(api_status "$viewer_token" POST "/api/spaces/$space_id/pull-requests" '{"title":"Forbidden"}')" = "403"

approval_json="$(api_json "$manager_token" POST "/api/spaces/$space_id/pull-requests/$pr_id/approve")"
test "$(jq -er '.approvalCount' <<<"$approval_json")" = "1"
test "$(api_status "$editor_token" POST "/api/spaces/$space_id/pull-requests/$pr_id/merge" "{\"expectedHeadOid\":\"$head_oid\"}")" = "403"

stale_status="$(api_status "$manager_token" POST "/api/spaces/$space_id/pull-requests/$pr_id/merge" "{\"expectedHeadOid\":\"$initial_oid\"}")"
test "$stale_status" = "409"
merged_json="$(api_json "$manager_token" POST "/api/spaces/$space_id/pull-requests/$pr_id/merge" "{\"expectedHeadOid\":\"$head_oid\"}")"
test "$(jq -er '.status' <<<"$merged_json")" = "merged"

# Browser Cloud content is read directly from the merged bare Git main and is
# hidden from non-members.
tree_json="$(api_json "$viewer_token" GET "/api/spaces/$space_id/tree?ref=main")"
test "$(jq -er '.oid' <<<"$tree_json")" = "$head_oid"
jq -e '.entries[] | select(.path == "guide/e2e.md" and .kind == "page")' <<<"$tree_json" >/dev/null
content_json="$(api_json "$viewer_token" GET "/api/spaces/$space_id/content?ref=main&path=guide%2Fe2e.md")"
grep -Fq 'Published by an Editor' <<<"$(jq -er '.content' <<<"$content_json")"
test "$(api_status "$outsider_token" GET "/api/spaces/$space_id/tree?ref=main")" = "404"
test "$(api_status "$viewer_token" GET "/api/spaces/$space_id/content?ref=main&path=image.bin")" = "415"

fresh_repo="$tmp/fresh"
git_with_token "$manager_token" clone --quiet "$git_url" "$fresh_repo"
test "$(git -C "$fresh_repo" rev-parse HEAD)" = "$head_oid"
grep -Fq 'Published by an Editor' "$fresh_repo/guide/e2e.md"

echo "Cloud publish, permissions, content, and pull-request E2E passed"

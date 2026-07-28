#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
env_file="$root/.env.cloud.local"
base_compose="$root/docker-compose.cloud.yml"
dev_compose="$root/docker-compose.dev.yml"

if [[ ! -f "$env_file" ]]; then
  echo "Missing $env_file. Copy .env.cloud.local.example and add the local GitHub OAuth credentials." >&2
  exit 1
fi

for name in GITHUB_CLIENT_ID GITHUB_CLIENT_SECRET COWIKI_TOKEN_PEPPER; do
  value="$(sed -n "s/^${name}=//p" "$env_file" | tail -n 1)"
  if [[ -z "$value" || "$value" == replace-* || "$value" == change-* ]]; then
    echo "$name is missing or still a placeholder in $env_file" >&2
    exit 1
  fi
done

docker compose \
  --env-file "$env_file" \
  -f "$base_compose" \
  -f "$dev_compose" \
  up -d --build

if [[ ! -d "$root/web/node_modules" ]]; then
  npm --prefix "$root/web" ci
fi

for _attempt in {1..60}; do
  if curl --silent --fail http://127.0.0.1:8787/healthz >/dev/null; then
    break
  fi
  sleep 1
done
curl --silent --fail http://127.0.0.1:8787/healthz >/dev/null

npm --prefix "$root/web" run dev -- --host 127.0.0.1 &
vite_pid=$!
trap 'kill "$vite_pid" 2>/dev/null || true' EXIT INT TERM

for _attempt in {1..30}; do
  if curl --silent --fail http://127.0.0.1:5173/healthz >/dev/null; then
    echo "CoWiki Cloud browser is ready at http://localhost:5173/cloud"
    wait "$vite_pid"
    exit $?
  fi
  sleep 1
done

echo "Vite did not become ready on http://127.0.0.1:5173" >&2
exit 1

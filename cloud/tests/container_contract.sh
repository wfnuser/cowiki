#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
dockerfile="$root/cloud/Dockerfile"
compose="$root/docker-compose.cloud.yml"
example_env="$root/.env.cloud.example"
entrypoint="$root/cloud/entrypoint.sh"
dev_compose="$root/docker-compose.dev.yml"
local_env="$root/.env.cloud.local.example"
dev_script="$root/scripts/dev-cloud.sh"
vite_config="$root/web/vite.config.ts"

for file in "$dockerfile" "$compose" "$example_env" "$entrypoint" "$dev_compose" "$local_env" "$dev_script" "$vite_config"; do
  test -f "$file" || { echo "missing deployment file: $file" >&2; exit 1; }
done

grep -Eq '^USER [^0]' "$dockerfile"
grep -Fq 'VOLUME ["/var/lib/cowiki/repos"]' "$dockerfile"
grep -Fq 'git' "$dockerfile"
grep -Fq 'tini' "$dockerfile"
grep -Fq 'DATABASE_URL' "$example_env"
grep -Fq 'COWIKI_TOKEN_PEPPER' "$example_env"
grep -Fq 'healthcheck:' "$compose"
grep -Fq 'condition: service_healthy' "$compose"
grep -Fq '/var/lib/cowiki/repos' "$compose"
grep -Fq 'exec /usr/bin/tini -- /usr/local/bin/cowiki-cloud' "$entrypoint"
grep -Fq '55432:5432' "$dev_compose"
grep -Fq 'COWIKI_PUBLIC_ORIGIN=http://localhost:5173' "$local_env"
grep -Fq 'docker-compose.dev.yml' "$dev_script"
grep -Fq "'/api': cloudTarget" "$vite_config"
grep -Fq "'/git': cloudTarget" "$vite_config"
grep -Fq "'/healthz': cloudTarget" "$vite_config"

echo "Cloud container contract passed"

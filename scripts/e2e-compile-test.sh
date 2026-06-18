#!/usr/bin/env bash
# e2e-compile-test.sh — End-to-end test: source ingest → compile → verify
#
# Uses cowiki HTTP API for all wiki operations. No manual git manipulation.
#
# Prerequisites:
#   1. PostgreSQL running (docker compose up -d postgres)
#   2. API key in .env (OPENAI_API_KEY or COWIKI_LLM_API_KEY)
#   3. cargo build succeeded
#
# Usage:
#   ./scripts/e2e-compile-test.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

SERVER_PORT="${SERVER_PORT:-3000}"
MCP_PORT="${MCP_PORT:-9380}"
COMPILE_MODE="${COMPILE_MODE:-pipeline}"  # pipeline | agent | auto
WORKSPACE="${WORKSPACE:-}"  # derived from user_id after registration
BRANCH="${BRANCH:-}"     # user/<user_id>
TIMEOUT="${TIMEOUT:-300}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

pass() { echo -e "${GREEN}✅ $1${NC}"; }
fail() { echo -e "${RED}❌ $1${NC}"; exit 1; }
info() { echo -e "${YELLOW}→ $1${NC}"; }
warn() { echo -e "${YELLOW}⚠ $1${NC}"; }

cleanup() {
    # Kill by name first (catches orphaned children that escaped PID tracking)
    pkill -9 -f "cowiki-mcp" 2>/dev/null || true
    pkill -9 -f "cowiki-server" 2>/dev/null || true
    pkill -9 -f "^pi " 2>/dev/null || true
    # Then kill tracked PIDs (belt and suspenders)
    if [ -n "${MCP_PID:-}" ]; then
        kill -9 "$MCP_PID" 2>/dev/null || true
    fi
    if [ -n "${SERVER_PID:-}" ]; then
        kill -9 "$SERVER_PID" 2>/dev/null || true
    fi
}

# Double-cleanup on INT/TERM — same aggressive kill, just guarantee it runs early
force_cleanup() {
    cleanup
}

trap cleanup EXIT
trap force_cleanup INT TERM

API="http://localhost:$SERVER_PORT/api"

# ── Step 0: Prerequisites ────────────────────────────────────────

info "Step 0: checking prerequisites..."

# PostgreSQL
if ! pg_isready -h localhost -p 5432 >/dev/null 2>&1; then
    info "PostgreSQL not running, starting via docker compose..."
    docker compose up -d postgres
    sleep 3
fi
pg_isready -h localhost -p 5432 >/dev/null 2>&1 || fail "PostgreSQL not reachable"
pass "PostgreSQL ok"

info "API key: will use .env config"

# ── Step 1: Build & start server ─────────────────────────────────

pkill -9 cowiki-mcp 2>/dev/null || true
pkill -9 cowiki-server 2>/dev/null || true

info "Step 1: building..."
cargo build -p cowiki-mcp 2>&1 | tail -1
cargo build -p cowiki-server 2>&1 | tail -1
pass "Binaries built"

info "Step 2: starting MCP server on :$MCP_PORT..."
NO_COLOR=1 RUST_LOG=info cargo run -p cowiki-mcp -- --port "$MCP_PORT" | tee "$ROOT"/tmp/cowiki-e2e-mcp.log 2>&1 &
MCP_PID=$!
sleep 1
if kill -0 "$MCP_PID" 2>/dev/null; then
    pass "MCP server started (pid=$MCP_PID, port $MCP_PORT)"
else
    echo "=== MCP log ==="
    tail -20 "$ROOT"/tmp/cowiki-e2e-mcp.log
    fail "MCP server failed to start"
fi

info "Step 3: starting cowiki server on :$SERVER_PORT..."
COWIKI_MCP_PORT="$MCP_PORT" NO_COLOR=1 RUST_LOG=info cargo run -p cowiki-server | tee "$ROOT"/tmp/cowiki-e2e-server.log 2>&1 &
SERVER_PID=$!

for i in $(seq 1 20); do
    if curl -s "$API/health" >/dev/null 2>&1; then
        break
    fi
    if [ "$i" -eq 20 ]; then
        echo "=== Server log ==="
        tail -50 "$ROOT"/tmp/cowiki-e2e-server.log
        fail "Server failed to start within 20s"
    fi
    sleep 1
done
pass "Server started (pid=$SERVER_PID, HTTP :$SERVER_PORT)"

# ── Step 4: Health check ─────────────────────────────────────────

info "Step 4: health check..."
HEALTH=$(curl -s "$API/health")
[ "$HEALTH" = "ok" ] || fail "Health check failed: $HEALTH"
pass "Health check: $HEALTH"

# ── Step 3.5: Register user & resolve branch via API ─────────────

info "Step 3.5: registering test user via API..."
info "  POST /api/auth/register"

REGISTER_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST \
    "$API/auth/register" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"e2e-test-user-$(date +%s)\"}" \
    --max-time 10)
REGISTER_HTTP=$(echo "$REGISTER_RESPONSE" | tail -1)
REGISTER_BODY=$(echo "$REGISTER_RESPONSE" | sed '$d')

if [ "$REGISTER_HTTP" = "200" ] || [ "$REGISTER_HTTP" = "201" ]; then
    USER_ID=$(echo "$REGISTER_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['user']['id'])" 2>/dev/null || echo "")
    API_KEY=$(echo "$REGISTER_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['api_key'])" 2>/dev/null || echo "")
    if [ -n "$USER_ID" ]; then
        BRANCH="user/$USER_ID"
        WORKSPACE="personal-${USER_ID:0:8}"
        pass "User registered: id=$USER_ID, branch=$BRANCH, ws=$WORKSPACE"
    else
        fail "Could not extract user_id from response: $REGISTER_BODY"
    fi
else
    echo "  Response: $REGISTER_BODY"
    fail "User registration returned HTTP $REGISTER_HTTP"
fi

# ── Step 4: Ingest test source via API ───────────────────────────

SOURCE_FILENAME="e2e-test-docker.md"

info "Step 4: ingesting test source via API..."
info "  POST /api/workspaces/$WORKSPACE/ingest"

INGEST_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST \
    "$API/workspaces/$WORKSPACE/ingest" \
    -H "Content-Type: application/json" \
    -d "{
        \"source_type\": \"text\",
        \"content\": \"# Docker: Containerization Platform\\n\\nDocker is a platform for developing, shipping, and running applications\\nin containers. It was created by Solomon Hykes and first released in 2013\\nas an open-source project by dotCloud, a platform-as-a-service company.\\n\\n## Core Concepts\\n\\n### Containerization\\n\\nContainers package an application with all its dependencies — libraries,\\nconfiguration files, and runtime — into a single portable unit. Unlike\\nvirtual machines, containers share the host OS kernel, making them\\nlightweight and fast to start.\\n\\n### Image Layering\\n\\nDocker images are built in layers using a Dockerfile. Each instruction\\n(RUN, COPY, ADD) creates a new layer. Layers are cached, so rebuilds are\\nincremental — only changed layers are rebuilt. This pattern is known as\\nlayered filesystem optimization and is critical for CI/CD pipeline efficiency.\\n\\n### Declarative Infrastructure\\n\\nDockerfiles use a declarative syntax to describe the desired environment.\\nThis follows the infrastructure-as-code pattern: the environment is\\ndefined in version-controlled text files, making it reproducible and\\nauditable.\\n\\n## Key People\\n\\n- **Solomon Hykes** — Founder and original creator of Docker. Previously\\n  CTO of dotCloud, later founded Docker Inc.\\n- **Kamal Jain** — Early engineer who contributed to Docker's networking stack\\n- **Jessie Frazelle** — Prominent Docker contributor, known for running\\n  everything in containers (\\\"containerized desktop\\\")\\n\\n## Related Projects\\n\\n- **Kubernetes** — Container orchestration platform initially developed by\\n  Google. Often used alongside Docker to manage container clusters.\\n- **Podman** — Daemonless container engine developed by Red Hat as an\\n  alternative to Docker\\n- **BuildKit** — Next-generation Docker build engine with parallel build\\n  support and better caching\\n\\n## Common Use Cases\\n\\n1. **Development environments** — Reproducible dev setups (\\\"works on my\\n   machine\\\" problem)\\n2. **CI/CD pipelines** — Consistent build and test environments\\n3. **Microservices** — Each service runs in its own container\\n4. **Legacy application migration** — Package old apps with their exact\\n   dependencies for cloud deployment\\n\\n## References\\n\\n- Docker Documentation: https://docs.docker.com\\n- \\\"Docker: The Complete Guide\\\" by Solomon Hykes, DockerCon 2014\",
        \"filename\": \"$SOURCE_FILENAME\",
        \"branch\": \"$BRANCH\"
    }" \
    --max-time 30)
INGEST_HTTP=$(echo "$INGEST_RESPONSE" | tail -1)
INGEST_BODY=$(echo "$INGEST_RESPONSE" | sed '$d')

if [ "$INGEST_HTTP" != "200" ]; then
    echo "  Response: $INGEST_BODY"
    fail "Source ingest returned HTTP $INGEST_HTTP"
fi
INGEST_FILENAME=$(echo "$INGEST_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['filename'])" 2>/dev/null || echo "")
INGEST_HASH=$(echo "$INGEST_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['content_hash'])" 2>/dev/null || echo "")
pass "Source ingested: $INGEST_FILENAME (hash=$INGEST_HASH)"

# ── Step 5: Verify source listed via API ─────────────────────────

info "Step 5: verifying source via API..."
info "  GET /api/workspaces/$WORKSPACE/sources?branch=$BRANCH"

SOURCES_RESPONSE=$(curl -s "$API/workspaces/$WORKSPACE/sources?branch=$BRANCH")
SOURCE_FOUND=$(echo "$SOURCES_RESPONSE" | python3 -c "
import sys, json
d = json.load(sys.stdin)
sources = d if isinstance(d, list) else d.get('sources', d.get('entries', []))
if isinstance(sources, list):
    found = any('$SOURCE_FILENAME' in str(s) for s in sources)
else:
    found = '$SOURCE_FILENAME' in str(sources)
print('yes' if found else 'no')
" 2>/dev/null || echo "no")

if [ "$SOURCE_FOUND" = "yes" ]; then
    pass "Source '$SOURCE_FILENAME' listed via API"
else
    warn "Source listing returned: $(echo "$SOURCES_RESPONSE" | head -c 200)"
    fail "Source '$SOURCE_FILENAME' not found via API"
fi

# ── Step 6: Compile via API ──────────────────────────────────────

info "Step 6: triggering compile via API (mode=$COMPILE_MODE)..."
info "  POST /api/workspaces/$WORKSPACE/compile"
info "  (mode: $COMPILE_MODE)"

START=$(date +%s)
COMPILE_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST \
    "$API/workspaces/$WORKSPACE/compile" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $API_KEY" \
    -d "{\"branch\":\"$BRANCH\"}" \
    --max-time "$TIMEOUT")
COMPILE_HTTP=$(echo "$COMPILE_RESPONSE" | tail -1)
COMPILE_BODY=$(echo "$COMPILE_RESPONSE" | sed '$d')
ELAPSED=$(($(date +%s) - START))

# ── Step 7: Verify compiled pages ────────────────────────────────

info "Step 7: verifying compiled pages..."
echo "  HTTP status: $COMPILE_HTTP"
echo "  Time: ${ELAPSED}s"

if [ "$COMPILE_HTTP" != "200" ]; then
    echo "=== Server log (last 30 lines) ==="
    tail -30 "$ROOT"/tmp/cowiki-e2e-server.log
    fail "Compile returned HTTP $COMPILE_HTTP"
fi

PAGE_COUNT=$(echo "$COMPILE_BODY" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('pages',[])))" 2>/dev/null || echo "0")
SKIPPED=$(echo "$COMPILE_BODY" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('skipped',0))" 2>/dev/null || echo "0")

if [ "$PAGE_COUNT" -gt 0 ]; then
    pass "Compile produced $PAGE_COUNT page(s) (${ELAPSED}s, skipped=$SKIPPED)"
            echo ""
            echo -e "  ${YELLOW}Workspace:${NC} $WORKSPACE"
            echo -e "  ${YELLOW}Branch:${NC}   $BRANCH"

    # Show compiled page info from API response
    echo ""
    info "Compiled pages:"
    echo "$COMPILE_BODY" | python3 -c "
import sys, json
d = json.load(sys.stdin)
for p in d.get('pages', []):
    print(f\"  - {p['slug']}: {p['title']}\")
    print(f\"    summary: {p['summary']}\")
" 2>/dev/null

    # Verify pages via API (GET /api/workspaces/{ws}/pages?branch=...&dir=wiki)
    echo ""
    info "Verifying pages via API..."
    PAGES_RESPONSE=$(curl -s "$API/workspaces/$WORKSPACE/pages?branch=$BRANCH&dir=wiki")
    API_PAGE_COUNT=$(echo "$PAGES_RESPONSE" | python3 -c "
import sys, json
d = json.load(sys.stdin)
pages = d if isinstance(d, list) else d.get('pages', d.get('entries', []))
print(len(pages))
" 2>/dev/null || echo "0")
    pass "  API reports $API_PAGE_COUNT page(s) in wiki/"

    # Verify pages exist on git branch (filesystem persistence)
    echo ""
    info "Verifying pages on git branch..."
    COWIKI_DATA_DIR="$(grep -oP 'COWIKI_DATA_DIR=\K.*' "$ROOT/.env" 2>/dev/null || echo "$ROOT/data")"
    DATA_DIR="$COWIKI_DATA_DIR/$WORKSPACE/repo"
    for slug in $(echo "$COMPILE_BODY" | python3 -c "import sys,json; d=json.load(sys.stdin); [print(p['slug']) for p in d.get('pages',[])]" 2>/dev/null); do
        found=false
        for dir in wiki entities concepts; do
            if git -C "$DATA_DIR" show "$BRANCH:${dir}/${slug}.md" >/dev/null 2>&1; then
                pass "  ${dir}/${slug}.md → git branch $BRANCH"
                found=true
                break
            fi
        done
        if [ "$found" = false ]; then
            fail "  ${slug}.md NOT FOUND in wiki|entities|concepts on branch $BRANCH"
        fi
    done
else
    echo ""
    if [ "$SKIPPED" -gt 0 ]; then
        pass "All $SKIPPED source(s) already compiled (no new pages needed)"
    else
        echo "=== Server log (last 30 lines) ==="
        tail -30 "$ROOT"/tmp/cowiki-e2e-server.log
        fail "Compile produced 0 pages"
    fi
fi

# ── Step 8: Checkout branch for inspection ───────────────────────

info "Step 8: checking out branch for inspection..."

# Load COWIKI_DATA_DIR from .env (same as server)
COWIKI_DATA_DIR="$(grep -oP 'COWIKI_DATA_DIR=\K.*' "$ROOT/.env" 2>/dev/null || echo "$ROOT/data")"
REPO_DIR="$COWIKI_DATA_DIR/$WORKSPACE/repo"

if [ -d "$REPO_DIR/.git" ]; then
    git -C "$REPO_DIR" checkout "$BRANCH" 2>/dev/null && \
        pass "Checked out $BRANCH in $REPO_DIR" || \
        warn "Could not checkout $BRANCH in $REPO_DIR"
    echo ""
    echo -e "  ${YELLOW}Repo path:${NC}   $REPO_DIR"
    echo -e "  ${YELLOW}Branch:${NC}     $BRANCH"
    echo -e "  ${YELLOW}Workspace:${NC}  $WORKSPACE"
    echo ""
    echo -e "  ${YELLOW}Pages on branch:${NC}"
    git -C "$REPO_DIR" ls-tree --name-only "$BRANCH" -- 'wiki/' 'entities/' 'concepts/' 2>/dev/null | while read f; do
        echo "    $f"
    done
else
    warn "Repo not found at $REPO_DIR"
fi
# ── Done ─────────────────────────────────────────────────────────

echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}  E2E Compile Test PASSED${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "Server log: "$ROOT"/tmp/cowiki-e2e-server.log"

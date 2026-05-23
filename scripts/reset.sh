#!/usr/bin/env bash
#
# cowiki reset script
# Resets both the PostgreSQL database and the data directory (git repo/wiki files).
#
# Usage:
#   ./scripts/reset.sh              # reset both DB and data
#   ./scripts/reset.sh --db-only    # reset only the database
#   ./scripts/reset.sh --data-only  # reset only the data directory
#   ./scripts/reset.sh --confirm    # skip confirmation prompt
#
# Configuration is read from .env first, then cowiki.conf as fallback.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# ---- parse flags ----
RESET_DB=true
RESET_DATA=true
SKIP_CONFIRM=false

for arg in "$@"; do
    case "$arg" in
        --db-only)   RESET_DATA=false ;;
        --data-only) RESET_DB=false ;;
        --confirm)   SKIP_CONFIRM=true ;;
        --help|-h)
            echo "Usage: $0 [--db-only|--data-only|--confirm]"
            echo "  --db-only    Reset only the PostgreSQL database"
            echo "  --data-only  Reset only the data directory (repo/wiki)"
            echo "  --confirm    Skip confirmation prompt"
            exit 0
            ;;
        *)
            echo "Unknown flag: $arg"
            exit 1
            ;;
    esac
done

# ---- load configuration ----
# Prefer .env, fall back to extracting from cowiki.conf
DB_URL=""
DATA_DIR=""

if [ -f "$PROJECT_DIR/.env" ]; then
    echo "[config] Loading from .env"
    set -a
    # shellcheck source=/dev/null
    source "$PROJECT_DIR/.env"
    set +a
    DB_URL="${DATABASE_URL:-}"
    DATA_DIR="${COWIKI_DATA_DIR:-}"
fi

# Fallback: parse cowiki.conf for any keys still missing
if [ -f "$PROJECT_DIR/cowiki.conf" ]; then
    echo "[config] Reading cowiki.conf for missing values"
    # Extract [database] url
    if [ -z "$DB_URL" ]; then
        DB_URL=$(awk -F ' *= *' '/^\[database\]/{f=1; next} /^\[/{f=0} f && /^url/{gsub(/"/,"",$2); print $2}' "$PROJECT_DIR/cowiki.conf")
    fi
    # Extract [server] data_dir
    if [ -z "$DATA_DIR" ]; then
        DATA_DIR=$(awk -F ' *= *' '/^\[server\]/{f=1; next} /^\[/{f=0} f && /^data_dir/{gsub(/"/,"",$2); print $2}' "$PROJECT_DIR/cowiki.conf")
    fi
fi

# Apply defaults
DB_URL="${DB_URL:-postgres://cowiki:cowiki@localhost:5432/cowiki}"
DATA_DIR="${DATA_DIR:-./data}"

# Resolve relative data_dir against PROJECT_DIR
if [[ "$DATA_DIR" == ./* ]] || [[ "$DATA_DIR" == ../* ]]; then
    DATA_DIR="$PROJECT_DIR/${DATA_DIR#./}"
elif [[ "$DATA_DIR" != /* ]]; then
    DATA_DIR="$PROJECT_DIR/$DATA_DIR"
fi

# Parse DB URL into components
# Format: postgres://user:password@host:port/dbname
DB_USER=$(echo "$DB_URL" | sed -n 's|.*://\([^:]*\):.*@.*|\1|p')
DB_PASS=$(echo "$DB_URL" | sed -n 's|.*://[^:]*:\([^@]*\)@.*|\1|p')
DB_HOST=$(echo "$DB_URL" | sed -n 's|.*@\([^:/]*\).*|\1|p')
DB_PORT=$(echo "$DB_URL" | sed -n 's|.*:\([0-9]\+\)/.*|\1|p')
DB_NAME=$(echo "$DB_URL" | sed -n 's|.*/\([^/?]*\).*|\1|p')

DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-cowiki}"
DB_USER="${DB_USER:-cowiki}"

echo ""
echo "============================================"
echo "  cowiki reset"
echo "============================================"
echo "  Database : postgres://$DB_USER:***@$DB_HOST:$DB_PORT/$DB_NAME"
echo "  Data dir : $DATA_DIR"
echo "  Reset DB : $RESET_DB"
echo "  Reset data: $RESET_DATA"
echo "============================================"
echo ""

# ---- confirmation ----
if [ "$SKIP_CONFIRM" = false ]; then
    read -r -p "This will DELETE all cowiki data. Continue? [y/N] " CONFIRM
    if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
fi

# ---- reset database ----
if [ "$RESET_DB" = true ]; then
    echo ""
    echo "[db] Resetting PostgreSQL database..."

    # Check if psql is available
    if ! command -v psql &>/dev/null; then
        echo "[db] ERROR: psql not found. Please install PostgreSQL client."
        exit 1
    fi

    export PGPASSWORD="$DB_PASS"

    # Destroy all tables by dropping and recreating the public schema
    echo "[db] Destroying all tables..."
    if psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -q <<'SQL' 2>&1; then
DROP SCHEMA public CASCADE;
CREATE SCHEMA public;
GRANT ALL ON SCHEMA public TO "$DB_USER";
GRANT ALL ON SCHEMA public TO public;
SQL
        echo "[db] Tables destroyed. Will be recreated on next server start."
    else
        echo "[db] WARNING: Could not drop tables (DB may not exist or be unreachable)."
        echo "[db] Attempting to create database..."
        psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -c "CREATE DATABASE $DB_NAME;" 2>/dev/null || true
    fi


    unset PGPASSWORD
fi

# ---- reset data directory ----
if [ "$RESET_DATA" = true ]; then
    echo ""
    echo "[data] Clearing data directory: $DATA_DIR"

    if [ -d "$DATA_DIR" ]; then
        # Remove repo content (sources/ and wiki/) but keep the data dir itself
        if [ -d "$DATA_DIR/repo" ]; then
            rm -rf "$DATA_DIR/repo"
            echo "[data] Removed $DATA_DIR/repo"
        fi
        # Remove any other generated content
        if [ -d "$DATA_DIR/repo" ]; then
            :
        fi
        echo "[data] Done."
    else
        echo "[data] Directory does not exist, nothing to clear."
    fi
fi

echo ""
echo "============================================"
echo "  Reset complete."
echo "============================================"
echo ""
echo "  Start the server to re-run migrations:"
echo "    cargo run --bin cowiki-server"
echo "  or with docker:"
echo "    docker compose up -d"
echo ""

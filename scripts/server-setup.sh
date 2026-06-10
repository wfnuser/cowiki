#!/usr/bin/env bash
#
# One-shot server bootstrap for cowiki (test or prod machine).
# Installs Docker, PostgreSQL 17 + pgvector, creates the DB, and prepares
# the deploy directory at /opt/cowiki owned by the current (non-root) user.
#
# Usage:
#   COWIKI_DB_PASSWORD='xxxx' ./server-setup.sh <branch>
#     <branch>  git branch to check out for compose/Caddyfile (dev | main)
#
# Run as a NORMAL user that has sudo. Do NOT run as root.
#
set -euo pipefail

BRANCH="${1:-main}"
REPO_URL="https://github.com/wfnuser/cowiki.git"
DEPLOY_DIR="/opt/cowiki"
PG_VERSION="17"
DB_NAME="cowiki"
DB_USER="cowiki"
DOCKER_SUBNET="172.16.0.0/12"   # default Docker bridge range

if [[ "$(id -u)" -eq 0 ]]; then
  echo "ERROR: run as a normal user with sudo, not as root." >&2
  exit 1
fi
if [[ -z "${COWIKI_DB_PASSWORD:-}" ]]; then
  echo "ERROR: set COWIKI_DB_PASSWORD env var before running." >&2
  exit 1
fi

DEPLOY_USER="$(id -un)"
echo "==> Bootstrapping cowiki server (branch=$BRANCH, user=$DEPLOY_USER)"

# ---------------------------------------------------------------------------
# 1. Base packages
# ---------------------------------------------------------------------------
echo "==> Installing base packages"
sudo apt-get update -y
sudo apt-get install -y ca-certificates curl gnupg lsb-release ufw

# ---------------------------------------------------------------------------
# 2. Docker + compose plugin
# ---------------------------------------------------------------------------
# The snap build of Docker is confined and cannot bind-mount paths outside
# /home (e.g. /opt). Remove it so we can install the apt docker-ce build.
if snap list docker >/dev/null 2>&1; then
  echo "==> Removing snap docker (confinement blocks /opt bind mounts)"
  sudo snap stop docker 2>/dev/null || true
  sudo snap remove --purge docker
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "==> Installing Docker"
  sudo install -m 0755 -d /etc/apt/keyrings
  curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
    | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
  sudo chmod a+r /etc/apt/keyrings/docker.gpg
  echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable" \
    | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
  sudo apt-get update -y
  sudo apt-get install -y docker-ce docker-ce-cli containerd.io \
    docker-buildx-plugin docker-compose-plugin
else
  echo "==> Docker already installed"
fi

echo "==> Adding $DEPLOY_USER to docker group"
sudo usermod -aG docker "$DEPLOY_USER"

# ---------------------------------------------------------------------------
# 3. PostgreSQL 17 + pgvector
# ---------------------------------------------------------------------------
if ! command -v psql >/dev/null 2>&1; then
  echo "==> Installing PostgreSQL $PG_VERSION + pgvector"
  sudo install -d /usr/share/postgresql-common/pgdg
  sudo curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
    -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc
  echo "deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] \
https://apt.postgresql.org/pub/repos/apt $(lsb_release -cs)-pgdg main" \
    | sudo tee /etc/apt/sources.list.d/pgdg.list > /dev/null
  sudo apt-get update -y
  sudo apt-get install -y "postgresql-$PG_VERSION" "postgresql-$PG_VERSION-pgvector"
else
  echo "==> PostgreSQL already installed"
fi

# ---------------------------------------------------------------------------
# 4. Create DB + user, enable pgvector
# ---------------------------------------------------------------------------
echo "==> Creating database and user (idempotent)"
sudo -u postgres psql -v ON_ERROR_STOP=1 <<SQL
DO \$\$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '$DB_USER') THEN
    CREATE ROLE $DB_USER LOGIN PASSWORD '$COWIKI_DB_PASSWORD';
  ELSE
    ALTER ROLE $DB_USER WITH PASSWORD '$COWIKI_DB_PASSWORD';
  END IF;
END
\$\$;
SQL
# CREATE DATABASE can't run inside DO block; guard separately
if ! sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='$DB_NAME'" | grep -q 1; then
  sudo -u postgres createdb -O "$DB_USER" "$DB_NAME"
fi
sudo -u postgres psql -d "$DB_NAME" -c "CREATE EXTENSION IF NOT EXISTS vector;"

# ---------------------------------------------------------------------------
# 5. Let containers reach PostgreSQL over the Docker bridge
# ---------------------------------------------------------------------------
echo "==> Configuring PostgreSQL to accept connections from Docker bridge"
PG_CONF_DIR="/etc/postgresql/$PG_VERSION/main"
# Listen on all interfaces (firewall restricts external access below)
sudo sed -i "s/^#\?listen_addresses.*/listen_addresses = '*'/" "$PG_CONF_DIR/postgresql.conf"
# Allow the Docker subnet with scram-sha-256 auth
HBA_LINE="host    $DB_NAME    $DB_USER    $DOCKER_SUBNET    scram-sha-256"
if ! sudo grep -qF "$DOCKER_SUBNET" "$PG_CONF_DIR/pg_hba.conf"; then
  echo "$HBA_LINE" | sudo tee -a "$PG_CONF_DIR/pg_hba.conf" > /dev/null
fi
sudo systemctl restart postgresql

# ---------------------------------------------------------------------------
# 6. Firewall: expose only 22/80/443, block public Postgres (5432)
# ---------------------------------------------------------------------------
echo "==> Configuring firewall (ufw)"
sudo ufw allow 22/tcp
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
# Allow containers on the Docker bridge to reach host PostgreSQL.
# (ufw otherwise drops bridge->host traffic, so the backend can't connect.)
sudo ufw allow from "$DOCKER_SUBNET" to any port 5432 proto tcp
# 5432 stays closed to the public; only the Docker subnet is allowed above.
sudo ufw --force enable

# ---------------------------------------------------------------------------
# 7. Deploy directory
# ---------------------------------------------------------------------------
echo "==> Preparing $DEPLOY_DIR"
sudo mkdir -p "$DEPLOY_DIR"
sudo chown -R "$DEPLOY_USER:$DEPLOY_USER" "$DEPLOY_DIR"
if [[ ! -d "$DEPLOY_DIR/.git" ]]; then
  git clone --branch "$BRANCH" "$REPO_URL" "$DEPLOY_DIR"
else
  git -C "$DEPLOY_DIR" fetch origin "$BRANCH" && git -C "$DEPLOY_DIR" checkout "$BRANCH" && git -C "$DEPLOY_DIR" pull
fi
mkdir -p "$DEPLOY_DIR/data"

echo ""
echo "============================================================"
echo " Server bootstrap complete."
echo "------------------------------------------------------------"
echo " Next steps:"
echo "  1. Log out and back in (so docker group membership applies)."
echo "  2. Copy your env file to the server:"
echo "       scp .env.test  $DEPLOY_USER@<host>:$DEPLOY_DIR/.env   # test"
echo "       scp .env.prod  $DEPLOY_USER@<host>:$DEPLOY_DIR/.env   # prod"
echo "  3. Add this server's SSH deploy key (see DEPLOY.md)."
echo "  4. Push to '$BRANCH' — GitHub Actions deploys automatically."
echo "     Or first-run manually:"
echo "       cd $DEPLOY_DIR && docker compose -f docker-compose.prod.yml up -d --build"
echo "============================================================"

# Deployment Guide

cowiki runs as a split deployment:

- **Frontend** → Netlify (`cowiki.app`, `test.cowiki.app`)
- **Backend** → your own servers behind Caddy (`api.cowiki.app`, `api-test.cowiki.app`)
- **PostgreSQL** → installed on each server's host (not in Docker)

Two environments, one per machine:

| Env  | Branch | Backend domain         | Frontend domain    | Deploy dir     |
|------|--------|------------------------|--------------------|----------------|
| Test | `dev`  | `api-test.cowiki.app`  | `test.cowiki.app`  | `/opt/cowiki`  |
| Prod | `main` | `api.cowiki.app`       | `cowiki.app`       | `/opt/cowiki`  |

Push to `dev` → auto-deploy to test. Push to `main` → auto-deploy to prod.

---

## 1. DNS

| Record                 | Type  | Points to            |
|------------------------|-------|----------------------|
| `cowiki.app`           | —     | Netlify (prod)       |
| `test.cowiki.app`      | —     | Netlify (test)       |
| `api.cowiki.app`       | A     | Prod server IP       |
| `api-test.cowiki.app`  | A     | Test server IP       |

## 2. GitHub OAuth Apps

Register **two** apps at <https://github.com/settings/developers>:

| App   | Homepage URL          | Authorization callback URL                          |
|-------|-----------------------|-----------------------------------------------------|
| prod  | `https://cowiki.app`  | `https://api.cowiki.app/api/auth/github/callback`   |
| test  | `https://test.cowiki.app` | `https://api-test.cowiki.app/api/auth/github/callback` |

Put the Client ID / Secret into `.env.prod` / `.env.test`.

## 3. Bootstrap each server

SSH in as your normal (non-root, sudo-capable) user and run:

```bash
# Test server
curl -fsSL https://raw.githubusercontent.com/wfnuser/cowiki/dev/scripts/server-setup.sh -o server-setup.sh
COWIKI_DB_PASSWORD='<test-db-password>' bash server-setup.sh dev

# Prod server
COWIKI_DB_PASSWORD='<prod-db-password>' bash server-setup.sh main
```

This installs Docker + PostgreSQL 17 + pgvector, creates the DB, configures the
firewall, and clones the repo into `/opt/cowiki` (owned by you). Log out and back
in once so docker-group membership takes effect.

## 4. Push env file to each server

From your laptop (these files are gitignored and hold the real secrets):

```bash
scp .env.test  <user>@<test-host>:/opt/cowiki/.env
scp .env.prod  <user>@<prod-host>:/opt/cowiki/.env
```

## 5. SSH deploy key (for GitHub Actions → server)

Generate a **dedicated** keypair per server (no passphrase, CI can't type one):

```bash
# Test
ssh-keygen -t ed25519 -C "gha-deploy-test" -f ~/.ssh/cowiki_deploy_test -N ""
# Prod
ssh-keygen -t ed25519 -C "gha-deploy-prod" -f ~/.ssh/cowiki_deploy_prod -N ""
```

Install the **public** key on the matching server:

```bash
ssh-copy-id -i ~/.ssh/cowiki_deploy_test.pub  <user>@<test-host>
ssh-copy-id -i ~/.ssh/cowiki_deploy_prod.pub  <user>@<prod-host>
# or append the .pub line manually to ~/.ssh/authorized_keys on the server
```

Add the **private** keys + host info as GitHub repo secrets
(Settings → Secrets and variables → Actions):

| Secret                 | Value                                  |
|------------------------|----------------------------------------|
| `TEST_SERVER_HOST`     | test server IP / hostname              |
| `TEST_SERVER_USER`     | your SSH username                      |
| `TEST_SERVER_SSH_KEY`  | contents of `~/.ssh/cowiki_deploy_test` (private) |
| `PROD_SERVER_HOST`     | prod server IP / hostname              |
| `PROD_SERVER_USER`     | your SSH username                      |
| `PROD_SERVER_SSH_KEY`  | contents of `~/.ssh/cowiki_deploy_prod` (private) |

```bash
# Copy a private key to clipboard (macOS) to paste into GitHub:
pbcopy < ~/.ssh/cowiki_deploy_test
```

## 6. Frontend on Netlify

Two Netlify sites (prod + test), both pointing at this repo:

- **Base directory**: `web`
- **Build command**: `npm install && npm run build`
- **Publish directory**: `web/dist`
- **Production branch**: `main` (prod site) / `dev` (test site)
- **Environment variable**:
  - prod site: `VITE_API_BASE=https://api.cowiki.app`
  - test site: `VITE_API_BASE=https://api-test.cowiki.app`

## 7. First deploy

```bash
# Test
git push origin dev
# Prod (after verifying test)
git push origin main
```

GitHub Actions builds the image, ships it over SSH, and runs
`docker compose -f docker-compose.prod.yml up -d --no-build backend`.
Caddy obtains TLS certificates automatically on first request.

---

## Notes & operations

- **Updating compose/Caddyfile**: these live in the git checkout on the server.
  After changing them, `git -C /opt/cowiki pull` then re-run `docker compose up -d`.
  The deployed **image** is updated by CI; only infra files need a manual pull.
- **Backups** (cron on each server):
  ```bash
  0 3 * * * pg_dump -U cowiki -Fc cowiki > /opt/cowiki/backups/db-$(date +\%Y\%m\%d).dump
  0 3 * * * tar czf /opt/cowiki/backups/data-$(date +\%Y\%m\%d).tar.gz /opt/cowiki/data
  0 4 * * * find /opt/cowiki/backups -mtime +7 -delete
  ```
- **Data-dir permissions / container `user`**: the backend writes per-user git
  repos into the bind-mounted `/opt/cowiki/data`, so the container must run as a
  uid/gid that owns that directory. `docker-compose.prod.yml` pins
  `user: "1000:1001"` to match the deploy user (`ubuntu`) on our servers. Before
  deploying to a new machine, check `id <deploy-user>` — if its uid/gid is not
  `1000:1001`, update the `user:` line (or `chown` the data dir to match), or the
  auth route returns 500 with `mkdir ... Permission denied`.
- **MCP server** (port 8080) is **not** deployed publicly — no auth yet. Keep internal.
- **Migrate PostgreSQL to managed later**: change `DATABASE_URL` in `/opt/cowiki/.env`,
  `pg_dump | pg_restore` the data, restart. Nothing else changes.

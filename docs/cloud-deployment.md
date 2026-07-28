# CoWiki Cloud deployment

CoWiki Cloud is one PostgreSQL control plane plus one persistent bare-Git repository volume. PostgreSQL contains identity, membership, pull-request, approval, and audit metadata. The Git volume contains the authoritative Markdown content and history. Do not replace either side with SQLite or an object-store snapshot.

## First deployment

1. Copy `.env.cloud.example` to `.env.cloud` and replace every `change-me` value. Keep `POSTGRES_PASSWORD` and the password embedded in `DATABASE_URL` in sync; URL-encode reserved URL characters in `DATABASE_URL`. Generate `COWIKI_TOKEN_PEPPER` with at least 32 random bytes and keep it stable; changing it revokes every API key and outstanding OAuth code.
2. Register a GitHub OAuth application. Set its callback to `${COWIKI_PUBLIC_ORIGIN}/api/auth/github/callback`.
3. Put an HTTPS reverse proxy in front of port 8787 and preserve request bodies, `Authorization`, `Content-Type`, and Git's query string. The public origin must be the externally visible HTTPS origin.
4. Start with one application replica:

   ```bash
   docker compose --env-file .env.cloud -f docker-compose.cloud.yml up -d --build
   curl --fail https://cloud.example.com/healthz
   ```

The service applies embedded SQL migrations before listening. Startup fails if PostgreSQL is unavailable or `/var/lib/cowiki/repos` is not writable. Logs are structured JSON on stdout.

The browser application and API are expected to share `COWIKI_PUBLIC_ORIGIN`. API CORS responses allow that exact origin, REST request bodies are limited to 1 MiB, REST requests time out after 30 seconds, and Git service children are terminated after 120 seconds. Keep a reverse-proxy request limit at least as large as the Cloud Git limit (256 MiB) on `/git/*`, while using a smaller limit for `/api/*`.

## Local Cloud and browser

Register a development GitHub OAuth app with callback
`http://localhost:5173/api/auth/github/callback`. Then:

```bash
cp .env.cloud.local.example .env.cloud.local
# Fill GITHUB_CLIENT_ID, GITHUB_CLIENT_SECRET, and a stable random
# COWIKI_TOKEN_PEPPER in .env.cloud.local.
scripts/dev-cloud.sh
```

The script starts PostgreSQL on `127.0.0.1:55432`, Cloud on
`127.0.0.1:8787`, and Vite on `127.0.0.1:5173`. Vite proxies `/api`, `/git`,
and `/healthz` so OAuth callbacks, browser requests, and Git URLs use the same
public origin. Open `http://localhost:5173/cloud`.

## Publish and review workflow

The desktop app exposes one Space-scoped Cloud action. It never publishes a local Space in the background:

1. **Publish Space** creates the PostgreSQL control-plane record and bare Git repository. The local `main` commit initializes both Cloud `main` and `user/<owner-id>` at the same OID.
2. **Sync** fetches Cloud `main`. A clean local `main` is rebased automatically; a conflict stops without silently choosing either side.
3. **Submit** commits dirty local Markdown after the user confirms a commit message, syncs Cloud `main`, pushes local `main` to `user/<user-id>` with `--force-with-lease`, and creates or updates the open pull request for that live user branch.
4. An Owner or Manager merges the exact reviewed `head_oid` into Cloud `main`. The browser Wiki reads only that merged bare-Git main; local drafts and unmerged user branches are not shown as published knowledge.

Pull requests follow subsequent pushes to `user/<user-id>`. An approval is counted only for the PR's current head. A changed head therefore requires a fresh approval in the UI, even if a stale approval row still exists temporarily.

### Role matrix

| Capability | Owner | Manager | Editor | Viewer |
| --- | --- | --- | --- | --- |
| Read Cloud Wiki, members, and PRs | Yes | Yes | Yes | Yes |
| Push `user/<own-id>` and create/approve PRs | Yes | Yes | Yes | No |
| Merge PRs | Yes | Yes | No | No |
| Add, change, or remove non-owner members | Yes | Yes | No | No |
| Bootstrap a new Space | Yes | No | No | No |

All members authenticate for Git Smart HTTP with the same bearer credential used by the REST API. The live pre-receive hook rejects direct pushes to `main`, pushes to another user's branch, user-branch deletion, unauthorized bootstrap, and unequal bootstrap refs.

## Development session injection

OAuth is the production session source. Tests and local previews may inject an already-issued development session without adding an API-key form to the product UI. In browser DevTools on the preview origin:

```js
localStorage.setItem('cowiki_api_key', '<development API key>');
localStorage.setItem('cowiki_user', JSON.stringify({
  id: '<user UUID>',
  name: 'Development User',
  mode: 'remote',
}));
localStorage.setItem('cowiki.apiOrigin', 'http://127.0.0.1:8787');
location.assign('/cloud');
```

The React `CloudApp` also accepts an explicit `CloudSession` prop for component tests and preview harnesses. Never ship a hard-coded API key or expose the injection seam as an end-user credential field.

## Storage and backup

Back up the PostgreSQL database and `cloud-repositories` volume as one coordinated recovery point. Pause writes (or stop the Cloud container), record the checkpoint time, run `pg_dump --format=custom`, and archive the repository volume before resuming writes. Encrypt both backups and test restores regularly.

A valid restore always restores both artifacts from the same checkpoint:

1. Stop Cloud so no Git receive or PR merge can run.
2. Restore PostgreSQL into an empty database.
3. Restore every `<space-uuid>.git` directory into the repository volume with ownership UID 10001.
4. Start Cloud and confirm `/healthz`, Space listing, Git fetch, and a non-mutating PR read.
5. For each open PR, compare PostgreSQL's `head_oid` with `refs/heads/user/<id>`. Normal PR reads reconcile a changed live head and invalidate stale approvals.

Never restore only PostgreSQL or only Git. If the two backups cannot be matched, keep the service offline and recover to the latest earlier coordinated checkpoint.

## Operations and scaling

- Rotate GitHub OAuth secrets in the deployment environment. Rotating `COWIKI_TOKEN_PEPPER` is a full session revocation event.
- Alert on failed migrations, PostgreSQL errors, an unwritable repository root, Git hook rejection spikes, and health-check failures.
- Keep the first production version at one Cloud application replica. PostgreSQL row locks and Git compare-and-swap protect merge correctness, but bootstrap/merge serialization is process-local; horizontal replicas require a distributed per-Space lock or stable per-Space routing.
- The container runs as UID 10001. Bind-mounted repository directories must be writable by that UID.
- Members must sign in once before an Owner or Manager can add their GitHub handle. Ownership transfer is intentionally unavailable in this version; Owners cannot be demoted or removed through the member API.

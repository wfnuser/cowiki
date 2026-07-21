# CoWiki Cloud deployment

CoWiki Cloud is one PostgreSQL control plane plus one persistent bare-Git repository volume. PostgreSQL contains identity, membership, pull-request, approval, and audit metadata. The Git volume contains the authoritative Markdown content and history. Do not replace either side with SQLite or an object-store snapshot.

## First deployment

1. Copy `.env.cloud.example` to `.env.cloud` and replace every `change-me` value. Generate `COWIKI_TOKEN_PEPPER` with at least 32 random bytes and keep it stable; changing it revokes every API key and outstanding OAuth code.
2. Register a GitHub OAuth application. Set its callback to `${COWIKI_PUBLIC_ORIGIN}/api/auth/github/callback`.
3. Put an HTTPS reverse proxy in front of port 8787 and preserve request bodies, `Authorization`, `Content-Type`, and Git's query string. The public origin must be the externally visible HTTPS origin.
4. Start with one application replica:

   ```bash
   docker compose --env-file .env.cloud -f docker-compose.cloud.yml up -d --build
   curl --fail https://cloud.example.com/healthz
   ```

The service applies embedded SQL migrations before listening. Startup fails if PostgreSQL is unavailable or `/var/lib/cowiki/repos` is not writable. Logs are structured JSON on stdout.

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

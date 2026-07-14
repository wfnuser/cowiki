# CoWiki Client

CoWiki is moving to a split architecture:

- `wfnuser/cowiki` — client surfaces: Web, desktop, CLI, and MCP client/server tooling.
- `wfnuser/cowiki-backend` — centralized Rust API service, database migrations, git-backed workspace storage, and deployment files.

The product direction is local-first for individual work and cloud-backed for
collaboration. The desktop client runs its local engine in-process; local
coding agents and the open editor operate on the same versioned document.
Shared team spaces use the hosted backend only when cloud capabilities are
enabled.

## Web Client

```bash
cd web
npm install
npm run dev
```

Open <http://localhost:5173>. During Vite development, `/api` proxies to `http://localhost:3000`.

For hosted deployments, set `VITE_API_BASE`:

```bash
VITE_API_BASE=https://api-test.cowiki.app npm run build
```

## Desktop Client

The desktop app uses Tauri 2 and owns its complete local runtime. It starts a
private loopback API on an OS-assigned port, stores metadata in SQLite at
`~/cowiki/.cowiki/metadata.db`, and keeps each Space as a local Git repository
under `~/cowiki`. It does not require a separately running server or Postgres.

```bash
cd web
npm install
npm run desktop:dev
```

The desktop window receives the private local origin directly from Tauri. It
never probes port 3000 and never silently reuses a running `cowiki-backend`.
The Web build continues to use `VITE_API_BASE` for cloud spaces.

## CLI

```bash
cd cli
npm install
npm run build
npm link
```

## MCP

```bash
cd cowiki-mcp-server
cargo run
```

The MCP package is intentionally independent from backend internals and talks to CoWiki over HTTP.

## Backend

Run the backend from the sibling repository:

```bash
cd ../cowiki-backend
cargo run
```

See `../cowiki-backend/DEPLOY.md` for server deployment.

# CoWiki Client

CoWiki is moving to a split architecture:

- `wfnuser/cowiki` — client surfaces: Web, desktop, CLI, and MCP client/server tooling.
- `wfnuser/cowiki-backend` — centralized Rust API service, database migrations, git-backed workspace storage, and deployment files.

The product direction is local-first for individual work and cloud-backed for collaboration. A user can run the client against a local backend while using local coding agents to organize documents; shared team spaces use the hosted backend as the coordination layer.

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

The desktop app uses Tauri 2.

```bash
cd web
npm install
npm run desktop:dev
```

Desktop builds default to `http://localhost:3000/api` when `VITE_API_BASE` is not set. Users can override the backend origin at runtime by setting `localStorage["cowiki.apiOrigin"]`, or at build time with `VITE_API_BASE`.

```bash
cd web
VITE_API_BASE=https://api-test.cowiki.app npm run desktop:build
```

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

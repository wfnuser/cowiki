# cowiki CLI — Troubleshooting

## "cowiki: command not found"

**Cause:** The CLI is not installed or not in PATH.

**Fix:**
```bash
npm install -g @cowiki/cli
```

If using npx instead:
```bash
npx @cowiki/cli <command>
```

## "Cannot connect to server"

**Cause:** The server is not running, or the URL is wrong.

**Fix:**
1. Check `COWIKI_BASE_URL` in `.env` or `--server` flag
2. Verify the server is reachable: `curl $COWIKI_BASE_URL/api/health` (default `https://api.cowiki.app/api/health`)
3. Check network connectivity

## "API error (HTTP 401): Unauthorized"

**Cause:** Missing or invalid API key.

**Fix:**
1. Open http://localhost:5173/login in your browser and sign in with GitHub
2. Copy the API key from the dialog
3. Run `cowiki setup --api-key <key>` to save it
4. Or run `cowiki setup` to manually enter an API key
5. Check that `COWIKI_API_KEY` is set in `~/.cowiki-cli/config`

## "Workspace required. Use -w <slug>."

**Cause:** Workspace-scoped commands need `-w <slug>`.

**Fix:**
```bash
cowiki workspaces          # list available workspaces
cowiki list -w my-wiki     # use the slug from the list
```

## "node --version < 18"

**Cause:** Node.js version too old.

**Fix:** Install Node.js 18+ from https://nodejs.org

## "npm install -g fails with EACCES"

**Cause:** Permission error on global install.

**Fix:**
```bash
# Option 1: Use npx instead (no install needed)
npx @cowiki/cli <command>

# Option 2: Fix npm permissions
# See https://docs.npmjs.com/resolving-eacces-permissions-errors
```

## "npm install -g fails (package not found or other errors)"

**Cause:** The package may not be published to the npm registry, or there may be dependency resolution issues.

**Fix:**
```bash
# Option 1: Clone the repo and link locally
npm run cli:dev-install

# Verify installation
cowiki --version

# Option 2: If npm install fails within the local directory
cd cli
rm -rf node_modules package-lock.json
cd ..
npm run cli:dev-install
```

`npm link` creates a global symlink to the local build, so `cowiki` will be available system-wide. This is also the recommended approach for development.

## "WARNING: Server URL is not HTTPS"

**Cause:** Using HTTP (not HTTPS) with a remote server. API keys sent in cleartext.

**Fix:** Use `https://` for the `COWIKI_BASE_URL`, or accept the risk for local development.

## TypeScript / Build Issues

If you're developing the CLI itself:

```bash
# Type check
npm --prefix cli run typecheck

# Build from the repo root
npm run cli:build

# Clean install
rm -rf cli/node_modules cli/package-lock.json && npm run cli:dev-install
```

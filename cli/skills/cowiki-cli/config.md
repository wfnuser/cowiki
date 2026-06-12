# cowiki CLI — Configuration Guide

## Config File

The CLI reads configuration from `~/.cowiki-cli/.env` by default.

```env
COWIKI_BASE_URL=https://cowiki.example.com
COWIKI_API_KEY=cw_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

> **Note:** `http://localhost:3000` is only appropriate for local development against a locally-running cowiki server. For production use, set `COWIKI_BASE_URL` to your actual server URL.

## Configuration Priority

1. CLI flag (`--server <url>`)
2. Environment variable (`COWIKI_BASE_URL`, `COWIKI_API_KEY`)
3. `~/.cowiki-cli/.env` file
4. No defaults — you must configure a server and API key

## Setting Up

### Primary: `cowiki setup`

Interactive wizard that guides you through configuration:

```bash
cowiki setup
```

This is the recommended way to set up the CLI. It validates your credentials and saves them to `~/.cowiki-cli/.env`.

### Alternative: Environment Variables

```bash
export COWIKI_BASE_URL=https://cowiki.example.com
export COWIKI_API_KEY=cw_xxx
```

### Alternative: Manual

Create `~/.cowiki-cli/.env` manually:

```bash
mkdir -p ~/.cowiki-cli
echo "COWIKI_BASE_URL=https://cowiki.example.com" > ~/.cowiki-cli/.env
echo "COWIKI_API_KEY=cw_xxx" >> ~/.cowiki-cli/.env
```

## Workspace Selection

Workspace-scoped commands require `-w <slug>`:

```bash
cowiki list -w my-wiki
```

List available workspaces first:

```bash
cowiki workspaces
```

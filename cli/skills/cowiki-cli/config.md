# cowiki CLI — Configuration Guide

## Config File

The CLI reads configuration from `~/.cowiki-cli/.env` by default.

```env
COWIKI_BASE_URL=http://localhost:3000
COWIKI_API_KEY=cw_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

## Configuration Priority

1. CLI flag (`--server <url>`)
2. Environment variable (`COWIKI_BASE_URL`, `COWIKI_API_KEY`)
3. `~/.cowiki-cli/.env` file
4. Defaults (`http://localhost:3000`, no API key)

## Setting Up

### Option A: `cowiki setup`
Interactive wizard that creates the config file:

```bash
cowiki setup
```

### Option B: `cowiki setup --api-key`
Non-interactive setup with a pre-obtained API key:

```bash
cowiki setup --api-key cw_xxx --server http://localhost:3000
```

### Option C: Manual
Create `~/.cowiki-cli/.env` manually:

```bash
mkdir -p ~/.cowiki-cli
echo "COWIKI_BASE_URL=http://localhost:3000" > ~/.cowiki-cli/.env
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

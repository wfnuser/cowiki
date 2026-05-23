# cowiki CLI Configuration Reference

## Config File

Location: `~/.config/cowiki/config.toml`

```toml
server_url = "http://localhost:3000"
api_key = "your-api-key"
```

### Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `server_url` | string | `http://localhost:3000` | cowiki server base URL |
| `api_key` | string | (none) | API key for authenticated operations |

## Environment Variables

Environment variables override config file values.

| Variable | Config Field | Description |
|----------|-------------|-------------|
| `COWIKI_BASE_URL` | `server_url` | Server base URL |
| `COWIKI_API_KEY` | `api_key` | API key |

## Priority Order

Highest to lowest:

1. **CLI flag** `--server <URL>` (per-invocation only)
2. **Environment variable** `COWIKI_BASE_URL` / `COWIKI_API_KEY`
3. **`.env` file** in working directory (loaded via `dotenvy`)
4. **Config file** `~/.config/cowiki/config.toml`
5. **Defaults** (`http://localhost:3000`, no API key)

## Security

- Config file is created with `0600` permissions on Unix (owner read/write only)
- API key is sent as `Authorization: Bearer <key>` header
- A warning is emitted if the API key would be sent over non-HTTPS to a remote server
- Localhost (`127.0.0.1`, `::1`) is exempt from the HTTPS warning

## Branch Defaults

When `--branch` is not specified:

- **Authenticated users**: defaults to `user/<user_id>` (personal workspace)
- **Unauthenticated (no API key)**: defaults to `main`

## .env File Support

A `.env` file in the working directory is automatically loaded. Example:

```env
COWIKI_BASE_URL=http://localhost:3000
COWIKI_API_KEY=cw_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

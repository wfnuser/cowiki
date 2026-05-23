# cowiki CLI Troubleshooting

## Error Categories

The CLI has 4 error categories:

| Error Type | Pattern | Meaning |
|------------|---------|---------|
| `Network` | "Cannot connect to server" | Server unreachable |
| `Network` | "Request timed out" | Server not responding in time |
| `Api` | "API error (HTTP XXX)" | Server returned an error |
| `Config` | "Config error:" | Configuration file issue |
| `Unexpected` | "Unexpected error:" | Unhandled error |

## Common Issues

### Cannot connect to server

```
Cannot connect to server. Is cowiki running? (...)
```

**Likely causes:**
- cowiki server is not running
- Wrong server URL
- Firewall/network blocking the connection

**Fix:**
```bash
# Check if server is running
curl http://localhost:3000/health

# Verify config
cat ~/.config/cowiki/config.toml

# Override for this command
cowiki list --server http://localhost:3000
```

### Request timed out

```
Request timed out. Try again or check your connection.
```

**Likely causes:**
- Server is overloaded (especially `cowiki compile`)
- Network latency
- Timeout too short for compile operations

**Fix:**
```bash
# For compile, increase timeout
cowiki compile --timeout 300
```

### API error (HTTP 401)

```
API error (HTTP 401): Unauthorized
```

**Likely causes:**
- Missing or invalid API key
- API key expired

**Fix:**
```bash
# Check API key is set
grep api_key ~/.config/cowiki/config.toml

# Or set via env
export COWIKI_API_KEY=<new-key>
```

### API error (HTTP 404)

```
API error (HTTP 404): page not found
```

**Likely causes:**
- Wrong page slug
- Page exists on a different branch

**Fix:**
```bash
# List pages to find the correct slug
cowiki list

# Try with branch flag
cowiki read my-page --branch user/123
```

### Review not found

```
review not found: "abc123"
```

**Likely causes:**
- Wrong review ID
- Review was already processed

**Fix:**
```bash
# List current reviews
cowiki review list
```

### Config error

```
Config error: cannot read /home/user/.config/cowiki/config.toml: ...
```

**Likely causes:**
- Config file missing or corrupted
- Permission issue

**Fix:**
```bash
# Check config exists and is valid
cat ~/.config/cowiki/config.toml

# Recreate with valid content
mkdir -p ~/.config/cowiki
echo 'server_url = "http://localhost:3000"' > ~/.config/cowiki/config.toml

# Or use env vars instead
export COWIKI_BASE_URL=http://localhost:3000
```

### HTTPS Warning

```
⚠️  WARNING: Server URL 'http://remote.example.com' is not HTTPS.
    Your API key will be sent in cleartext.
```

This is a security warning — not an error. The CLI continues to work.

**Fix:**
- Use `https://` for remote servers
- Use `http://localhost` for local development (warning suppressed)
- Set up an HTTPS reverse proxy in front of cowiki-server

## Debugging Tips

### Use --json for debugging

```bash
cowiki search "test" --json | jq .
```

JSON output includes error details not visible in table mode.

### Check config resolution

```bash
# See what config values are being used
cowiki list --server http://debug.example.com

# Check env vars
env | grep COWIKI
```

### Verify server connectivity

```bash
# Basic health check
curl -v http://localhost:3000/

# With auth
curl -H "Authorization: Bearer <key>" http://localhost:3000/api/pages
```

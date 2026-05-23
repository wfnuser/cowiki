# cowiki Configuration Reference

## Configuration File

cowiki uses a TOML configuration file named `cowiki.conf`. All settings can also be set via environment variables, which take precedence over file values.

### Discovery Order

The configuration file is discovered in the following priority:

1. **CLI argument**: `--config <path>` or `-c <path>`
2. **Environment variable**: `COWIKI_CONFIG`
3. **Current directory**: `./cowiki.conf`
4. **User config directory**: `~/.cowiki/cowiki.conf`
5. **Fallback**: environment variables only (no config file)

Example:
```bash
cowiki-server --config /etc/cowiki/cowiki.conf
# or
COWIKI_CONFIG=/etc/cowiki/cowiki.conf cowiki-server
# or
cowiki-server  # looks for ./cowiki.conf, then ~/.cowiki/cowiki.conf
```

---

## Sections

### `[database]`

PostgreSQL connection settings.

| Field | Type | Default | Env Var | Description |
|-------|------|---------|---------|-------------|
| `url` | string | *required* | `DATABASE_URL` | PostgreSQL connection URL |
| `embedding_dimension` | integer | *(embedder.dimension)* | `COWIKI_DATABASE_EMBEDDING_DIMENSION` | pgvector column dimension. Defaults to `[embedder].dimension` if not set |

Example:
```toml
[database]
url = "postgres://cowiki:cowiki@localhost:5432/cowiki"
```

---

### `[server]`

Server runtime settings (shared between cowiki-server and cowiki-rmcp-server via `cowiki-utils` crate).

| Field | Type | Default | Env Var | Description |
|-------|------|---------|---------|-------------|
| `port` | integer | `3000` | `COWIKI_PORT` | HTTP server port (cowiki-server) |
| `data_dir` | string | `"./data"` | `COWIKI_DATA_DIR` | Data directory for git repo and wiki files |

For the MCP server (`cowiki-rmcp-server`), the port is resolved as follows:

| Priority | Source | Default |
|----------|--------|---------|
| 1 | `COWIKI_MCP_PORT` env var | — |
| 2 | `COWIKI_PORT` env var | — |
| 3 | `[server].port` in cowiki.conf | — |
| 4 | Hardcoded default | `8080` |

Example:
```toml
[server]
port = 3000
data_dir = "./data"
```

```bash
# Start REST API on port 3000, MCP on port 8080
cowiki-server &
COWIKI_MCP_PORT=9090 cowiki-rmcp-server &
```

---

### `[llm]`

Language Model (LLM) settings. Used for compilation, summary generation, and text generation.

| Field | Type | Default | Env Var | Description |
|-------|------|---------|---------|-------------|
| `provider` | string | `"openai"` | `COWIKI_LLM_PROVIDER` | Provider name (currently only `"openai"`) |
| `model` | string | `"gpt-4o-mini"` | `COWIKI_LLM_MODEL` | Model name |
| `api_key` | string | *required* | `OPENAI_API_KEY` | API key for the provider |
| `api_base` | string | `"https://api.openai.com/v1"` | `OPENAI_BASE_URL` | API base URL (change for proxies) |
| `temperature` | float | `0.3` | `COWIKI_LLM_TEMPERATURE` | Generation temperature (0.0–2.0) |
| `max_tokens` | integer | `4096` | `COWIKI_LLM_MAX_TOKENS` | Max tokens per completion |

Example:
```toml
[llm]
provider = "openai"
model = "gpt-4o-mini"
api_key = "sk-..."
api_base = "https://api.openai.com/v1"
temperature = 0.3
max_tokens = 4096
```

#### Using OpenAI-Compatible Proxies

Set `api_base` to your proxy URL:

```toml
[llm]
provider = "openai"
model = "gpt-4o-mini"
api_key = "sk-..."
api_base = "https://litellm.example.com/v1"
```

---

### `[embedder]`

Embedding model settings. Used for semantic search and deduplication.

| Field | Type | Default | Env Var | Description |
|-------|------|---------|---------|-------------|
| `provider` | string | `"openai"` | `COWIKI_EMBEDDER_PROVIDER` | Provider name (currently only `"openai"`) |
| `model` | string | `"text-embedding-3-small"` | `COWIKI_EMBEDDER_MODEL` | Embedding model name |
| `api_key` | string | *(falls back to llm.api_key)* | `COWIKI_EMBEDDER_API_KEY`, `OPENAI_API_KEY` | API key. If empty, uses `[llm].api_key` |
| `api_base` | string | *(falls back to llm.api_base)* | `COWIKI_EMBEDDER_BASE_URL`, `OPENAI_BASE_URL` | API base URL. If empty, uses `[llm].api_base` |
| `dimension` | integer | `1536` | `COWIKI_EMBEDDER_DIMENSION` | Output vector dimension. `0` = model default |

Example:
```toml
[embedder]
provider = "openai"
model = "text-embedding-3-small"
api_key = "sk-..."
api_base = "https://api.openai.com/v1"
dimension = 1536
```

#### Using Different Providers for LLM and Embedder

You can use separate API keys and endpoints:

```toml
[llm]
api_key = "sk-llm-key"
api_base = "https://api.openai.com/v1"

[embedder]
api_key = "sk-embedder-key"
api_base = "https://other-provider.example.com/v1"
model = "bge-m3"
dimension = 1024
```

---

### `[mcp-server]`

MCP server settings (used by `cowiki-rmcp-server`).

| Field | Type | Default | Env Var | Description |
|-------|------|---------|---------|-------------|
| `port` | integer | `8080` | `COWIKI_MCP_PORT` | MCP server port |
| `api_url` | string | `"http://localhost:3000/"` | `COWIKI_API_URL` | cowiki-server REST API base URL. **不要带尾部斜杠**（代码会自动去除，但建议配置文件中也保持一致） |

```toml
[mcp-server]
port = 8080
api_url = "http://localhost:3000"
```

> MCP server 是独立进程，通过 HTTP 代理到 `api_url` 指定的 cowiki-server REST API。确保 cowiki-server 已在该地址运行。

---

### `[github]`

GitHub OAuth app credentials for social login.

| Field | Type | Default | Env Var | Description |
|-------|------|---------|---------|-------------|
| `client_id` | string | `""` | `GITHUB_CLIENT_ID` | GitHub OAuth app client ID |
| `client_secret` | string | `""` | `GITHUB_CLIENT_SECRET` | GitHub OAuth app client secret |
| `redirect_uri` | string | `"http://localhost:3000/api/auth/github/callback"` | `GITHUB_REDIRECT_URI` | OAuth callback URL |

Example:
```toml
[github]
client_id = "Iv23li..."
client_secret = "abc123..."
redirect_uri = "http://localhost:3000/api/auth/github/callback"
```

---

### `[frontend]`

Frontend URL for CORS and redirects.

| Field | Type | Default | Env Var | Description |
|-------|------|---------|---------|-------------|
| `url` | string | `"http://localhost:5173"` | `FRONTEND_URL` | Frontend URL |

Example:
```toml
[frontend]
url = "http://localhost:5173"
```

---

## Environment Variable Override

Every field in `cowiki.conf` can be overridden by an environment variable. The env var always takes priority over the file value.

| Config Path | Env Var |
|-------------|---------|
| `database.url` | `DATABASE_URL` |
| `database.embedding_dimension` | `COWIKI_DATABASE_EMBEDDING_DIMENSION` |
| `server.port` | `COWIKI_PORT` |
| `server.data_dir` | `COWIKI_DATA_DIR` |
| `llm.provider` | `COWIKI_LLM_PROVIDER` |
| `llm.model` | `COWIKI_LLM_MODEL` |
| `llm.api_key` | `OPENAI_API_KEY` |
| `llm.api_base` | `OPENAI_BASE_URL` |
| `llm.temperature` | `COWIKI_LLM_TEMPERATURE` |
| `llm.max_tokens` | `COWIKI_LLM_MAX_TOKENS` |
| `embedder.provider` | `COWIKI_EMBEDDER_PROVIDER` |
| `embedder.model` | `COWIKI_EMBEDDER_MODEL` |
| `embedder.api_key` | `COWIKI_EMBEDDER_API_KEY` |
| `embedder.api_base` | `COWIKI_EMBEDDER_BASE_URL` |
| `embedder.dimension` | `COWIKI_EMBEDDER_DIMENSION` |
| `mcp-server.port` | `COWIKI_MCP_PORT` |
| `mcp-server.api_url` | `COWIKI_API_URL` |
| `github.client_id` | `GITHUB_CLIENT_ID` |
| `github.client_secret` | `GITHUB_CLIENT_SECRET` |
| `github.redirect_uri` | `GITHUB_REDIRECT_URI` |
| `frontend.url` | `FRONTEND_URL` |

---

## Backward Compatibility

The old `.env`-only approach still works. If no `cowiki.conf` is found, the server falls back to reading all values from environment variables (same env vars as above), preserving compatibility with existing deployments.

---

## Token Usage Tracking

cowiki tracks token usage for both LLM and embedding calls:

- **Endpoint**: `GET /api/usage`
- **Response format**:
  ```json
  {
    "llm": {
      "total": {
        "prompt_tokens": 15234,
        "completion_tokens": 4567,
        "total_tokens": 19801,
        "call_count": 42,
        "last_updated": "2026-05-22T10:30:00Z"
      },
      "gpt-4o-mini": { "..." }
    },
    "embedder": {
      "total": { "..." },
      "text-embedding-3-small": { "..." }
    }
  }
  ```

- Token usage is tracked per-model, with cumulative counts across all API calls
- Usage resets on server restart (in-memory tracking)

---

## Full Example

See `cowiki.conf.example` in the repository root for a complete annotated example.

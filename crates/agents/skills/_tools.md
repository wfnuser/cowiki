# Cowiki MCP Tool Reference — Complete Parameter Specs

You have 5 MCP tools. All are accessed via `cowiki_` prefix (pi-mcp-adapter directTools mode).
Using wrong tool names (e.g., `list` instead of `cowiki_list`) will fail.

---

## Tool Catalog

| Tool | Required | Optional | Context (always pass) |
|------|----------|----------|------------------------|
| `cowiki_list` | `dir` | `recursive` | `_workspace`, `_branch`, `_execution_id` |
| `cowiki_read` | `path` | — | `_workspace`, `_branch`, `_execution_id` |
| `cowiki_write` | `path`, `body` | — | `_workspace`, `_branch`, `_execution_id` |
| `cowiki_remove` | `path` | — | `_workspace`, `_branch`, `_execution_id` |
| `cowiki_search` | `query` | `top_k` (default 10) | `_workspace`, `_branch`, `_execution_id` |

---

## Context Parameters (MANDATORY for EVERY call)

Every tool call MUST include these three parameters. They are provided in the Session Context section of your prompt.

```json
{
  "_workspace": "personal-xxxxxxxx",
  "_branch": "user/xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
  "_execution_id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**These are NOT optional.** Omitting any of them will cause the call to fail.

---

## `cowiki_list`

**Parameter**: `dir` (string, required)
**Optional**: `recursive` (boolean)

### Calling Convention

```json
{
  "dir": "wiki",
  "_workspace": "personal-xxxxxxxx",
  "_branch": "user/xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
  "_execution_id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

### Valid `dir` values

- `"wiki"`, `"entities"`, `"concepts"`, `"sources"`
- Subdirectories: `"entities/people"`, `"concepts/patterns/rust/async"`
- Any depth is OK — no fixed subdirectory structure

### WRONG vs RIGHT

| ❌ WRONG | ✅ RIGHT |
|----------|----------|
| `cowiki_list({"dir":"."})` | `cowiki_list({"dir":"wiki", ...context})` |
| `cowiki_list({"dir":"/absolute"})` | `cowiki_list({"dir":"entities", ...context})` |
| `cowiki_list({"dir":""})` | `cowiki_list({"dir":"entities/people", ...context})` |
| `cowiki_list({"dir":"wiki"})` (no context) | Always include `_workspace`, `_branch`, `_execution_id` |

---

## `cowiki_read`

**Parameter**: `path` (string, required)

### Calling Convention

```json
{
  "path": "wiki/docker-overview.md",
  "_workspace": "personal-xxxxxxxx",
  "_branch": "user/xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
  "_execution_id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

### WRONG vs RIGHT

| ❌ WRONG | ✅ RIGHT |
|----------|----------|
| `cowiki_read({"path":"SKILL.md"})` | `cowiki_read({"path":"wiki/docker.md", ...context})` |
| `cowiki_read({"path":"./sources/test.md"})` | `cowiki_read({"path":"sources/test-docker.md", ...context})` |
| `cowiki_read({"path":"/absolute/path.md"})` | `cowiki_read({"path":"entities/people/alice.md", ...context})` |
| `cowiki_read({"path":"wiki/test"})` | Must end with `.md` or `.json` |
| Missing context params | Always include `_workspace`, `_branch`, `_execution_id` |

---

## `cowiki_write`

**Parameters**: `path` (string, required) + `body` (string, required)

### Minimal Valid Call

```json
{
  "path": "wiki/hello.md",
  "body": "---\ntitle: Hello\nsummary: Test\n---\n\n# Hello",
  "_workspace": "personal-xxxxxxxx",
  "_branch": "user/xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
  "_execution_id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

### WRONG vs RIGHT

| ❌ WRONG | ✅ RIGHT |
|----------|----------|
| `cowiki_write({"path":"sources/new.md",...})` | Never write to `sources/` — it's read-only |
| `cowiki_write({"path":"wiki/test",...})` | Must end with `.md` |
| `cowiki_write({"path":"wiki/test.md"})` (no body) | `body` is required |
| `cowiki_write({"path":"...","content":"..."})` | Parameter is `body`, not `content` |
| Missing context params | Always include `_workspace`, `_branch`, `_execution_id` |

---

## `cowiki_remove`

**Parameter**: `path` (string, required)

```json
{
  "path": "wiki/obsolete-page.md",
  "_workspace": "personal-xxxxxxxx",
  "_branch": "user/xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
  "_execution_id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

- Only works on `wiki/`, `entities/`, `concepts/` (NOT `sources/`)
- Path must end with `.md`

---

## `cowiki_search`

**Parameters**: `query` (string, required) + `top_k` (number, optional, default 10)

```json
{
  "query": "container orchestration",
  "top_k": 5,
  "_workspace": "personal-xxxxxxxx",
  "_branch": "user/xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
  "_execution_id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| ❌ WRONG | ✅ RIGHT |
|----------|----------|
| `cowiki_search({"query":""})` | Query must be ≥ 2 chars |
| `cowiki_search({"q":"docker"})` | Parameter is `query`, not `q` |
| Missing context params | Always include `_workspace`, `_branch`, `_execution_id` |

---

## Golden Rules

1. **Always `cowiki_` prefix** — never call bare `list()`, `read()`, etc.
2. **Always pass context** — `_workspace`, `_branch`, `_execution_id` on EVERY call
3. **Every path starts with `wiki/`, `entities/`, `concepts/`, or `sources/`**
4. **Every path ends with `.md` or `.json`**
5. **Never use `.`, `..`, or absolute paths**
6. **Never write to `sources/`**
7. **Body parameter is `body`, not `content`**
8. **Search parameter is `query`, not `q`**

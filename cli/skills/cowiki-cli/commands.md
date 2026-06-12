# cowiki CLI — Command Reference

## Global Flags

| Flag | Description |
|------|-------------|
| `--server <URL>` | Override server URL (default: `http://localhost:3000`) |
| `-w, --workspace <slug>` | Target workspace (required for page/ingest/compile/submit/review) |
| `--json` | Machine-readable JSON output |

## Commands

### `cowiki setup`
Interactive configuration wizard. Prompts for server URL and API key.

```bash
cowiki setup
cowiki setup --api-key cw_xxx --server http://localhost:3000   # non-interactive
cowiki setup --env-path /custom/path/.env                        # custom path
```

### `cowiki key`
Manage API keys.

```bash
cowiki key generate --name "my-agent"   # Create new key
cowiki key list                          # List all keys
cowiki key revoke <id>                   # Revoke a key
```

### `cowiki ingest`
Add a source document to the wiki. Use with `cowiki compile` for the cloud compile workflow (Path 1). See SKILL.md for the dual-path workflow.

```bash
cowiki ingest -w <ws> --type url --content "https://example.com/doc"
cowiki ingest -w <ws> --type text --content "# My Document"
cowiki ingest -w <ws> --type file --content ./local-file.md
cat file.md | cowiki ingest -w <ws> --type text   # from stdin
```

### `cowiki compile`
Compile sources into wiki pages. Triggers cloud-side agent (Path 1). For local agent compile, use `cowiki write` (Path 2).

```bash
cowiki compile -w <ws>
cowiki compile -w <ws> --timeout 300
```

### `cowiki write`
Create or edit a wiki page. Primary tool for local agent output (Path 2). Use `--path` to target entities/, concepts/, or wiki/. Use `--title` to set the page title (server prepends YAML frontmatter). For large external sources, prefer `cowiki ingest` → `cowiki compile`.

```bash
cowiki write -w <ws> my-page --body "# Hello"
cowiki write -w <ws> my-page --title "My Page" --body "content here"
cowiki write -w <ws> my-page --title "My Page"  # opens $EDITOR
echo "# content" | cowiki write -w <ws> my-page  # from stdin
cowiki write -w <ws> my-page --path entities --title "Entity" --body "..."
cowiki write -w <ws> my-page --path concepts --title "Concept" --body "..."
```

### `cowiki search`
Search wiki pages.

```bash
cowiki search "quantum computing"
cowiki search "quantum computing" --limit 20
```

### `cowiki read`
Read a wiki page.

```bash
cowiki read -w <ws> my-page
cowiki read -w <ws> my-page --no-pager
cowiki read -w <ws> my-page --json
cowiki read -w <ws> my-entity --dir entities    # read from entities/
cowiki read -w <ws> my-concept --dir concepts   # read from concepts/
```

### `cowiki list`
List wiki pages in a workspace.

```bash
cowiki list -w <ws>
cowiki list -w <ws> --dir entities               # list entities/ only
cowiki list -w <ws> --dir concepts               # list concepts/ only
cowiki list -w <ws> --dir all                    # list all directories
```

### `cowiki workspaces`
List available workspaces.

```bash
cowiki workspaces
```

### `cowiki submit`
Submit pages for review.

```bash
cowiki submit -w <ws> page1 page2
cowiki submit -w <ws> --all --yes
```

### `cowiki review`
Review submissions.

```bash
cowiki review list -w <ws>
cowiki review list -w <ws> --status pending
cowiki review show -w <ws> <submission-id>
cowiki review approve -w <ws> <submission-id>
cowiki review reject -w <ws> <submission-id>
```

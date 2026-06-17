# Cowiki Conventions — Directory Architecture & Rules

## Root Directories

Cowiki organizes knowledge across four root directories:

| Directory | Purpose | Write? | Read? |
|-----------|---------|--------|-------|
| `wiki/` | Synthesized knowledge pages | ✅ | ✅ |
| `entities/` | Named things (people, projects, orgs, events, tools) | ✅ | ✅ |
| `concepts/` | Ideas, patterns, decisions, mental models | ✅ | ✅ |
| `sources/` | Input documents (ingested) | ❌ NEVER | ✅ (given paths only) |

There is no mandatory subdirectory structure. Create nested dirs when helpful, skip when not.

---

## Path Format Rules

- **Format**: `<root>/path/to/slug.md`
- **No `.` or `..`** — these are rejected
- **No absolute paths** — path traversal rejected
- **Extension**: `.md` for pages, `.json` for metadata
- Examples:
  - `wiki/docker.md` — simple flat page
  - `wiki/containers/docker/networking.md` — nested hierarchy
  - `entities/people/alice-chen.md` — entity page
  - `concepts/patterns/retry-with-backoff.md` — concept page

---

## Slug Naming

- **Lowercase, hyphens**: `llm-training-survey`
- **No special characters** (no underscores, spaces, unicode)
- **Entities**: `<category>/<full-name>` → `people/alice-chen`
- **Concepts**: `<category>/<descriptive-name>` → `patterns/error-handling`
- **Wiki**: just the page name → `docker-overview`

---

## YAML Frontmatter

Every page MUST start with:

```yaml
---
title: "Page Title"
summary: "One-line description for search and preview cards"
---
```

- `title` and `summary` are both **required**
- Missing frontmatter will cause lint failures

---

## Cross-References

Use `[[dir/slug]]` syntax within page bodies:

```markdown
## Related
- [[entities/people/alice-chen]] — pioneered this approach
- [[concepts/patterns/retry-with-backoff]] — complementary pattern
- [[wiki/distributed-systems]] — broader context
```

### Bidirectional Linking

Links should be bidirectional — when you link from A to B, ensure B links back to A. When creating a new page that links to existing pages, read those existing pages and add back-links.

---

## Efficiency Principles

| Goal | Use | Why |
|------|-----|-----|
| Check if page exists | `cowiki_list("wiki")` | Filenames tell you everything |
| Find related pages | `cowiki_list + cowiki_search` | Search returns slugs/summaries |
| Need cross-references | `cowiki_read("wiki/related.md")` | Content is necessary here |
| Check entity before creating | `cowiki_list("entities/people")` | Filenames = entity names |

### Anti-Patterns

- ❌ `cowiki_list("entities")` then `cowiki_read` every file — only read what you need
- ❌ `cowiki_list("sources")` to explore — use the source paths given to you
- ❌ Reading the same source twice — read once, work from memory

---

## Page Size Guidelines

- Wiki pages: 200–800 words
- Entity pages: 100–400 words
- Concept pages: 150–500 words

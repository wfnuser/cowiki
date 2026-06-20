# Sidebar: 4-Section Directory Tree

**Date:** 2026-06-16
**Status:** design approved

## Summary

Replace the flat "Wiki Space" tree in the sidebar with 4 independent collapsible sections — Sources, Wiki, Entities, Concepts — each grouping pages by their slug prefix. Within each section, folders display above files, both sorted alphabetically.

## Motivation

The backend (`wiki_fs.rs`) already organizes pages under content directories: `wiki/`, `entities/`, `concepts/`. The frontend ignores this structure and renders a flat merged list. Surfacing the content directories in the UI gives users a clear mental model of where pages live and makes navigation faster as pages grow.

## Design

### Section Layout

Four sections, displayed top-to-bottom in the sidebar tree area, replacing the current "Wiki Space" header section:

| Order | Section | Content Source | Editable | Sort |
|---|---|---|---|---|
| 1 | **Sources** | `SourceItem[]` from API | Yes — add source, compile | Existing behavior |
| 2 | **Wiki** | Pages with `wiki/` prefix | Yes — CRUD | Folders first, A→Z |
| 3 | **Entities** | Pages with `entities/` prefix | Yes — CRUD | Folders first, A→Z |
| 4 | **Concepts** | Pages with `concepts/` prefix | Yes — CRUD | Folders first, A→Z |

All four sections are editable. Conflict resolution between sections is handled later by lint tooling.

### Section Component

Each section:
- **Header row**: section name, chevron for expand/collapse (default: expanded), + button with dropdown (New Page, New Folder) scoped to that section's directory prefix
- **Tree area**: folders-first, alphabetical sort. Folders render with the existing `PageTreeItem` recursive component with expand/collapse. Files render as existing leaf items.
- **Empty state**: "No pages yet" italic placeholder when the section has zero items
- **Collapse state**: local `useState`, not persisted across sessions

### Data Flow

```
API: listPages() → PageMeta[] (flat)
        ↓
Group by slug prefix:
  "wiki/" prefix     → Wiki section
  "entities/" prefix → Entities section
  "concepts/" prefix → Concepts section
        ↓
Per section: build tree from path segments, sort folders-first A→Z
        ↓
Render <Section> → <PageTreeItem> (existing, unchanged)
```

### Sort Rule

At every tree level:
1. Folders come before files
2. Within folders: alphabetical by name
3. Within files: alphabetical by title (fallback to slug)

### Page Creation

When creating a page or folder from a section's + menu, the slug automatically prepends the section's directory prefix:

- Wiki section → `wiki/<slug>.md`
- Entities section → `entities/<slug>.md`
- Concepts section → `concepts/<slug>.md`

The existing `handleCreatePage` / `handleCreateFolder` in `MainLayout.tsx` are modified to accept a `dir` parameter.

### Sources Section

Unchanged from current implementation. Controlled by `SourceItem[]` from the API, collapsible, with "Add Source" and "Compile" actions in the context menu.

### What Goes Away

- The "Wiki Space" label above the tree
- The global + button (now per-section)
- The flat page rendering loop — replaced by the 4-section grouped render

## Files Touched

| File | Change |
|---|---|
| `web/src/components/layout/SpacePanel.tsx` | Replace flat tree with 4 sections; add grouping + sorting logic; remove "Wiki Space" header |
| `web/src/pages/MainLayout.tsx` | Pass `dir` prefix to page/folder creation handlers |
| `web/src/api.ts` | No changes (API already supports slug prefixes) |

## Backend

No backend changes required. `wiki_fs.rs` already supports `wiki/`, `entities/`, `concepts/` as content directories. The `listPages` API already returns slugs with full paths.

## Non-Goals

- Pinning/sticky items (deferred)
- Persisted collapse state (default: all expanded)
- Custom sort within sections (folders-first-alphabetical only)
- Lint/conflict resolution between sections (deferred to separate tooling)

## Open Questions

None.

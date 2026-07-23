# Local Review and Source Flow

## Goal

Make the desktop workflow read like a local-first product rather than an
expanded Git diff utility:

- Reviews is an inbox of Current Draft and independent Agent Changes.
- Opening an inbox item navigates to a dedicated detail view.
- Adding a Source clearly separates deterministic local import from optional
  Agent synthesis.
- Long Source paths never steal space from header actions or overflow the
  reading surface.

## Local review model

`Current Draft` is the Space working tree. Human edits and Live Agent edits
remain ordinary local files until the user creates a checkpoint.

Each Background Agent task is a managed branch and worktree:

```text
Current Draft snapshot
  └── cowiki/agent/<change-id>
```

The Reviews list contains:

1. Current Draft, always first.
2. Agent Changes, newest first.

Rows are navigation targets, not accordions. No diff is expanded in the list.

### Current Draft detail

The detail compares the working tree with the latest checkpoint (`HEAD`) and
offers `Create Checkpoint`. A checkpoint commits the exact reviewed snapshot.

### Agent Change detail

The detail compares the Agent branch with its dispatch base and identifies the
relationship as:

```text
agent/<change-id> → Current Draft
```

Open changes offer:

- Continue with Agent
- Merge into Draft
- Discard

Merge applies a three-way merge to the latest working tree. It does not create
a Draft checkpoint. A conflict keeps the Draft unchanged and moves the change
to `Needs resolution`.

Cloud submissions remain a separate future boundary:

```text
Current Draft → Cloud main
```

## Add Source model

Add Source has two explicit phases.

### 1. Import

CoWiki performs deterministic local work:

- write pasted text or URL as an OKF Source;
- extract supported local files to Markdown;
- refresh OKF progressive indexes;
- refresh the rebuildable SQLite search index.

This phase does not call an LLM.

### 2. Organize with Agent

After import, the dialog keeps a success state instead of disappearing. It
names the Agent selected in client Settings and offers an explicit action to
organize the newly imported Source. Starting that action creates an isolated
Agent Change so generated knowledge remains reviewable.

The UI must never imply that local extraction is AI synthesis. It uses
`Importing…` for deterministic work and `Organize with <Agent>` for model work.
The user may close the dialog after import and leave the Source untouched.

## Source reading layout

Source filenames are repository-relative identities and may be very long.

- The top breadcrumb owns all remaining flexible width and ellipsizes.
- The action group is `flex-shrink: 0`; action labels do not wrap.
- The document heading wraps long unbroken path segments and cannot widen the
  main content column.
- YAML frontmatter is system metadata and is stripped before Markdown render.

## Non-goals

- Automatically committing the Draft after an Agent Change merge.
- Silently spending model tokens immediately after every import.
- Treating local Agent Changes as cloud pull requests.
- Adding a second source-processing database or server.

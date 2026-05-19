# cowiki

A collaborative knowledge base where humans and AI agents co-maintain a shared wiki, with version control, review workflows, and semantic deduplication.

## Language

**Source**:
Raw input material — a URL, file, document, or session export that hasn't been compiled into a wiki page yet.
_Avoid_: resource, input, data

**Page**:
A compiled wiki document in Markdown with frontmatter, living in `wiki/`. One concept per page, interlinked via `[[wikilinks]]`.
_Avoid_: article, entry, note, document (too generic)

**Personal Space**:
A user's private Git branch (`user/{user_id}`) where they freely ingest sources and compile pages. Only visible to the owning user and their agents.
_Avoid_: workspace, local, draft

**Shared Space**:
The `main` branch of the cowiki repo. Contains reviewed, approved pages that all team members and agents can read.
_Avoid_: public, common, global

**Submit**:
The act of proposing pages from a Personal Space to the Shared Space. Triggers lightweight compilation (formatting, dedup check, summary generation) and creates a review request.
_Avoid_: push, publish, commit (these are Git internals the user doesn't see)

**Review**:
The process of examining a submission. The reviewer sees an LLM-generated summary plus full diff, and can approve, reject, or request changes.
_Avoid_: merge, PR, pull request (Git internals)

**Compile**:
LLM-powered transformation of sources into structured wiki pages — extracting concepts, generating summaries, resolving wikilinks.
_Avoid_: build, generate, process

**Ingest**:
Adding a source (URL, file, session export) into a Personal Space for later compilation.
_Avoid_: import, upload, fetch

## Relationships

- A **Source** is ingested into a **Personal Space**
- **Compile** transforms **Sources** into **Pages**
- A **Submit** moves **Pages** from **Personal Space** to **Shared Space** via **Review**
- A **Page** lives in exactly one space at a time (personal or shared)
- Multiple **Sources** can contribute to a single **Page**

## Example dialogue

> **Dev:** "I found a good article about retry patterns. How do I get it into the team wiki?"
> **Domain expert:** "First **ingest** it — that puts it in your **Personal Space**. Then **compile** to turn it into a **Page**. When you're happy with it, **submit** to the **Shared Space**. A teammate will **review** it before it lands."

## Flagged ambiguities

- "document" was used loosely to mean both Source and Page — resolved: Source is raw input, Page is compiled output.
- "branch" and "repo" are implementation details (Git); users see Personal Space and Shared Space.

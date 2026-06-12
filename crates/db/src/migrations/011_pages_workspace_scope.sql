-- Scope pages by workspace. Previously the unique key was (slug, branch), so two
-- workspaces that shared a slug+branch (e.g. both have "main"/"retry-patterns")
-- overwrote each other via ON CONFLICT, and semantic search/dedup (find_similar)
-- leaked pages across all workspaces on that branch. workspace_slug already exists
-- (005_fts) but was never written.
ALTER TABLE pages DROP CONSTRAINT IF EXISTS pages_slug_branch_key;
CREATE UNIQUE INDEX IF NOT EXISTS pages_slug_branch_workspace_key
    ON pages (slug, branch, workspace_slug);

-- ANN index for vector similarity. find_similar's inner query is shaped
-- `ORDER BY embedding <=> $1 LIMIT n` (the form pgvector can serve from HNSW);
-- the threshold/workspace filters are applied around it. Previously every
-- search/submit was an exact full scan.
CREATE INDEX IF NOT EXISTS pages_embedding_hnsw
    ON pages USING hnsw (embedding vector_cosine_ops);

-- NOTE: existing rows keep workspace_slug = '' (legacy/unscoped); they are not
-- backfilled here because branch alone does not identify a workspace. They simply
-- won't match workspace-scoped reads. Search route scoping is tracked in #44.

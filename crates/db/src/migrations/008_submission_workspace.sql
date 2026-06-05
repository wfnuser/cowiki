-- Scope submissions to a workspace so review can resolve the correct per-workspace git repo.
-- Existing rows referenced the now-removed global repo; they default to '' and become
-- inaccessible through the new per-workspace review listing (acceptable: internal, early-stage).
ALTER TABLE submissions ADD COLUMN IF NOT EXISTS workspace_slug TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_submissions_workspace_pending
    ON submissions(workspace_slug, status, created_at DESC);

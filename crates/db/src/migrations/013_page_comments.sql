-- Document comments: Feishu-style inline comments on a wiki page, anchored to a
-- source *line range* of the page markdown. Comments live entirely outside the
-- markdown blob (so page diffs stay clean); a content snapshot is kept per
-- (page, content_hash) so the anchor can be re-mapped onto whatever branch
-- version a viewer is currently looking at (git-diff line mapping on the client).

-- Snapshot of the page markdown as the author saw it when anchoring a comment.
-- Deduplicated by content_hash so many comments on the same version share one row.
CREATE TABLE IF NOT EXISTS page_comment_snapshots (
    workspace_slug TEXT NOT NULL,
    page_slug      TEXT NOT NULL,
    content_hash   TEXT NOT NULL,
    source         TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_slug, page_slug, content_hash)
);

CREATE TABLE IF NOT EXISTS page_comments (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_slug TEXT NOT NULL,
    page_slug      TEXT NOT NULL,
    user_id        UUID NOT NULL REFERENCES users(id),
    -- Anchor; NULL for replies, which inherit their thread root's anchor.
    content_hash   TEXT,
    start_line     INTEGER,
    end_line       INTEGER,
    body           TEXT NOT NULL,
    parent_id      UUID REFERENCES page_comments(id) ON DELETE CASCADE,
    resolved       BOOLEAN NOT NULL DEFAULT FALSE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- A top-level comment carries a full line-range anchor; a reply carries none.
    CONSTRAINT page_comments_anchor_shape CHECK (
        (parent_id IS NULL
            AND content_hash IS NOT NULL AND start_line IS NOT NULL AND end_line IS NOT NULL)
        OR (parent_id IS NOT NULL
            AND content_hash IS NULL AND start_line IS NULL AND end_line IS NULL)
    ),
    CONSTRAINT page_comments_line_range CHECK (
        start_line IS NULL OR (start_line >= 1 AND start_line <= end_line)
    )
);

CREATE INDEX IF NOT EXISTS idx_page_comments_page ON page_comments(workspace_slug, page_slug);
CREATE INDEX IF NOT EXISTS idx_page_comments_parent ON page_comments(parent_id);

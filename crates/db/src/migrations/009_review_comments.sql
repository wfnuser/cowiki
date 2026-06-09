-- Review comments: inline comments on specific lines of a submission diff
CREATE TABLE IF NOT EXISTS review_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    submission_id UUID NOT NULL REFERENCES submissions(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id),
    file_path TEXT NOT NULL,
    line_number INTEGER,
    body TEXT NOT NULL,
    parent_id UUID REFERENCES review_comments(id) ON DELETE CASCADE,
    resolved BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_review_comments_submission ON review_comments(submission_id);
CREATE INDEX IF NOT EXISTS idx_review_comments_parent ON review_comments(parent_id);

ALTER TABLE submissions DROP CONSTRAINT IF EXISTS submissions_status_check;
ALTER TABLE submissions ADD CONSTRAINT submissions_status_check
    CHECK (status IN ('pending', 'approved', 'rejected', 'changes_requested', 'merged'));

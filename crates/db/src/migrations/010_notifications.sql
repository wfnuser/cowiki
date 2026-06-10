-- ============================================================
-- Migration 010: Notification System + User Search
-- ============================================================

-- Notifications table
CREATE TABLE IF NOT EXISTS notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,                      -- 'invitation' | 'review_request' | 'review_decision' | 'member_joined' | 'compile_done'
    title TEXT NOT NULL,
    body TEXT,
    workspace_id UUID REFERENCES workspaces(id) ON DELETE CASCADE,
    link TEXT,                              -- optional deep link
    read BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_notifications_user ON notifications(user_id, read, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_unread ON notifications(user_id) WHERE read = FALSE;


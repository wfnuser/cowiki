-- Migration 012: Rename page_slugs → paths in submissions table
-- Submit Path Awareness: replaces bare slugs with repo paths (dir-prefixed)
-- Idempotent: skip if column already renamed (fresh DBs get paths from init.sql).
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'submissions' AND column_name = 'page_slugs'
    ) THEN
        ALTER TABLE submissions RENAME COLUMN page_slugs TO paths;
    END IF;
END $$;

-- Migration 012: Rename page_slugs → paths in submissions table
-- Submit Path Awareness: replaces bare slugs with repo paths (dir-prefixed)
--
-- Idempotent: run_migrations() re-executes every migration on each boot, so a
-- bare `ALTER TABLE ... RENAME COLUMN` would succeed once and then panic on the
-- next boot ("column page_slugs does not exist"), crash-looping the server.
-- Guard the rename so it only fires when page_slugs still exists and paths does not.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'submissions' AND column_name = 'page_slugs'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'submissions' AND column_name = 'paths'
    ) THEN
        ALTER TABLE submissions RENAME COLUMN page_slugs TO paths;
    END IF;
END $$;

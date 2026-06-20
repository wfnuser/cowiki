-- Migration 012: Rename page_slugs → paths in submissions table
-- Submit Path Awareness: replaces bare slugs with repo paths (dir-prefixed)
ALTER TABLE submissions RENAME COLUMN page_slugs TO paths;

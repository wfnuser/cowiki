-- Full-text search column + GIN index
ALTER TABLE pages ADD COLUMN IF NOT EXISTS body_text TEXT NOT NULL DEFAULT '';
ALTER TABLE pages ADD COLUMN IF NOT EXISTS tsv tsvector;

CREATE INDEX IF NOT EXISTS pages_tsv_idx ON pages USING GIN (tsv);

-- Auto-update tsv on insert/update
CREATE OR REPLACE FUNCTION pages_tsv_update() RETURNS trigger AS $$
BEGIN
  NEW.tsv := to_tsvector('english', coalesce(NEW.title, '') || ' ' || coalesce(NEW.summary, '') || ' ' || coalesce(NEW.body_text, ''));
  RETURN NEW;
END
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS pages_tsv_trigger ON pages;
CREATE TRIGGER pages_tsv_trigger BEFORE INSERT OR UPDATE ON pages
  FOR EACH ROW EXECUTE FUNCTION pages_tsv_update();

-- Add workspace_slug to pages for scoping
ALTER TABLE pages ADD COLUMN IF NOT EXISTS workspace_slug TEXT NOT NULL DEFAULT '';

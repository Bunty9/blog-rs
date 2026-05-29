-- Phase 1d: soft-delete column for posts. Admin "delete" sets `deleted_at`
-- rather than removing the row, preserving referential integrity for
-- newsletter_outbox rows that point at the post.
ALTER TABLE posts ADD COLUMN deleted_at INTEGER;

-- Rebuild the published-list partial index so it excludes soft-deleted rows
-- without touching it (SQLite ignores rows where any indexed expression is NULL,
-- but `deleted_at IS NULL` filter has to be expressed in queries explicitly).
CREATE INDEX IF NOT EXISTS posts_deleted_idx ON posts(deleted_at);

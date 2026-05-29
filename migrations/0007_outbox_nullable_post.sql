-- Allow NULL post_id on `newsletter_outbox` so confirm-purpose rows
-- (which have no associated post) no longer rely on a synthetic post_id=0
-- sentinel that would FK-fail in production.
--
-- SQLite cannot ALTER a column's NOT NULL constraint or change a FK in place,
-- so we rebuild the table preserving data, indexes, and the UNIQUE constraint.

PRAGMA foreign_keys = OFF;

CREATE TABLE newsletter_outbox_new (
    id         INTEGER PRIMARY KEY,
    post_id    INTEGER REFERENCES posts(id),
    member_id  INTEGER NOT NULL REFERENCES members(id),
    status     TEXT NOT NULL CHECK (status IN ('pending','sending','sent','failed','dead')),
    attempts   INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    sent_at    INTEGER,
    created_at INTEGER NOT NULL,
    UNIQUE(post_id, member_id)
);

-- Copy existing data. Rows previously stamped with the post_id=0 sentinel are
-- rewritten to NULL so the new schema reflects intent: NULL means "confirm".
INSERT INTO newsletter_outbox_new
    (id, post_id, member_id, status, attempts, last_error, sent_at, created_at)
SELECT
    id,
    CASE WHEN post_id = 0 THEN NULL ELSE post_id END,
    member_id,
    status,
    attempts,
    last_error,
    sent_at,
    created_at
FROM newsletter_outbox;

DROP TABLE newsletter_outbox;
ALTER TABLE newsletter_outbox_new RENAME TO newsletter_outbox;

CREATE INDEX outbox_pending_idx ON newsletter_outbox(status, attempts);

-- The composite UNIQUE(post_id, member_id) does not enforce uniqueness when
-- post_id is NULL (SQLite treats every NULL as distinct in a UNIQUE index).
-- A partial unique index on (member_id) WHERE post_id IS NULL keeps the
-- confirm-purpose enqueue idempotent: a member can only have one outstanding
-- confirm row at a time.
CREATE UNIQUE INDEX outbox_confirm_unique_idx
    ON newsletter_outbox(member_id)
    WHERE post_id IS NULL;

PRAGMA foreign_keys = ON;

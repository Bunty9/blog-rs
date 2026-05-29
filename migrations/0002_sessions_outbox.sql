PRAGMA foreign_keys = ON;

CREATE TABLE sessions (
    token       TEXT PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    csrf_token  TEXT NOT NULL,
    expires_at  INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX sessions_user_idx    ON sessions(user_id);
CREATE INDEX sessions_expires_idx ON sessions(expires_at);

CREATE TABLE newsletter_outbox (
    id         INTEGER PRIMARY KEY,
    post_id    INTEGER NOT NULL REFERENCES posts(id),
    member_id  INTEGER NOT NULL REFERENCES members(id),
    status     TEXT NOT NULL CHECK (status IN ('pending','sending','sent','failed','dead')),
    attempts   INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    sent_at    INTEGER,
    created_at INTEGER NOT NULL,
    UNIQUE(post_id, member_id)
);
CREATE INDEX outbox_pending_idx ON newsletter_outbox(status, attempts);

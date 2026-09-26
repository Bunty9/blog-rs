-- Post revisions: a snapshot of the author-editable content fields, taken
-- before a save overwrites them (subject to a throttling policy — see
-- db::revisions::should_snapshot) and always right before publish.
-- Restoring a revision writes these same fields back through the normal
-- save path, so only what a save can change is captured here: title,
-- subtitle, body_md, and the meta_json blob (SEO fields + series).
CREATE TABLE post_revisions (
    id         INTEGER PRIMARY KEY,
    post_id    INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    subtitle   TEXT,
    body_md    TEXT NOT NULL,
    meta_json  TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX post_revisions_post_created_idx ON post_revisions(post_id, created_at DESC);

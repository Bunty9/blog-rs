-- Full-text search mirror kept in sync via INSERT/UPDATE/DELETE triggers on posts.
-- `tokenize='porter unicode61'` matches spec §4.1.

CREATE VIRTUAL TABLE posts_fts USING fts5(
    title, excerpt, body_md,
    content='posts', content_rowid='id', tokenize='porter unicode61'
);

CREATE TRIGGER posts_ai AFTER INSERT ON posts BEGIN
    INSERT INTO posts_fts(rowid, title, excerpt, body_md)
    VALUES (new.id, new.title, COALESCE(new.excerpt, ''), new.body_md);
END;

CREATE TRIGGER posts_ad AFTER DELETE ON posts BEGIN
    INSERT INTO posts_fts(posts_fts, rowid, title, excerpt, body_md)
    VALUES ('delete', old.id, old.title, COALESCE(old.excerpt, ''), old.body_md);
END;

CREATE TRIGGER posts_au AFTER UPDATE ON posts BEGIN
    INSERT INTO posts_fts(posts_fts, rowid, title, excerpt, body_md)
    VALUES ('delete', old.id, old.title, COALESCE(old.excerpt, ''), old.body_md);
    INSERT INTO posts_fts(rowid, title, excerpt, body_md)
    VALUES (new.id, new.title, COALESCE(new.excerpt, ''), new.body_md);
END;

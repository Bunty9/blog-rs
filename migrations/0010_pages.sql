CREATE TABLE pages (
    id                INTEGER PRIMARY KEY,
    slug              TEXT NOT NULL UNIQUE,
    title             TEXT NOT NULL,
    body_md           TEXT NOT NULL,
    body_html         TEXT NOT NULL,
    body_html_version INTEGER NOT NULL DEFAULT 0,
    toc_json          TEXT NOT NULL DEFAULT '[]',
    meta_json         TEXT,
    status            TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','published')),
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);
CREATE INDEX pages_status_idx ON pages(status);

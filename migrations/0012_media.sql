-- Media library: uploaded images stored on local disk and served from
-- /media/<filename>. filename is content-addressed (sha256 prefix plus an
-- extension derived from the sniffed content type), so sha256's UNIQUE
-- constraint gives free dedupe on repeat uploads of the same bytes.
CREATE TABLE media (
    id            INTEGER PRIMARY KEY,
    filename      TEXT NOT NULL UNIQUE,
    original_name TEXT NOT NULL,
    content_type  TEXT NOT NULL,
    size_bytes    INTEGER NOT NULL,
    sha256        TEXT NOT NULL UNIQUE,
    width         INTEGER,
    height        INTEGER,
    alt           TEXT NOT NULL DEFAULT '',
    created_at    INTEGER NOT NULL
);
CREATE INDEX media_created_at_idx ON media(created_at DESC);

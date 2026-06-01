-- Persist the table-of-contents JSON alongside body_html so readers can
-- render a TOC without re-rendering the whole document.
-- Default '[]' matches the sentinel value used by empty/stale rows.
ALTER TABLE posts ADD COLUMN toc_json TEXT NOT NULL DEFAULT '[]';

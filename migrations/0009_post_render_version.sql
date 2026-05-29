-- Track which RENDER_VERSION produced the cached body_html column.
-- When the shortcode registry or markdown pipeline changes in a way
-- that affects output, bump content::RENDER_VERSION; rows whose value
-- diverges become candidates for lazy or batch regeneration.
ALTER TABLE posts ADD COLUMN body_html_version INTEGER NOT NULL DEFAULT 0;

-- Cache the per-post AssetManifest as JSON alongside body_html.
-- Populated by the admin save path (Plan 1d) and read by the reader (Plan 1c).
ALTER TABLE posts ADD COLUMN assets_json TEXT NOT NULL DEFAULT '[]';

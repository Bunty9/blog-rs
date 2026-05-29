-- Phase 1d: site/SMTP key/value settings backing the admin Settings UI.
CREATE TABLE IF NOT EXISTS settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Seed default keys with empty values so the settings UI always has rows to render.
INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES
    ('site_title',           '',  strftime('%s','now')),
    ('site_subtitle',        '',  strftime('%s','now')),
    ('site_url',             '',  strftime('%s','now')),
    ('default_author_email', '',  strftime('%s','now')),
    ('smtp_host',            '',  strftime('%s','now')),
    ('smtp_port',            '587', strftime('%s','now')),
    ('smtp_user',            '',  strftime('%s','now')),
    ('smtp_password',        '',  strftime('%s','now')),
    ('smtp_from',            '',  strftime('%s','now'));

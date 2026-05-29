-- Phase 1b initial schema: users, posts, tags, post_tags, members.
-- Sessions and outbox live in 0002 because they reference users and posts.

PRAGMA foreign_keys = ON;

CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'admin' CHECK (role IN ('admin')),
    created_at    INTEGER NOT NULL
);

CREATE TABLE posts (
    id              INTEGER PRIMARY KEY,
    slug            TEXT NOT NULL UNIQUE,
    title           TEXT NOT NULL,
    subtitle        TEXT,
    status          TEXT NOT NULL CHECK (status IN ('draft','published','scheduled')),
    author_id       INTEGER NOT NULL REFERENCES users(id),
    published_at    INTEGER,
    scheduled_for   INTEGER,
    updated_at      INTEGER NOT NULL,
    created_at      INTEGER NOT NULL,
    excerpt         TEXT,
    cover_image     TEXT,
    reading_minutes INTEGER,
    body_md         TEXT NOT NULL,
    body_html       TEXT NOT NULL,
    meta_json       TEXT
);
CREATE INDEX posts_published_idx ON posts(status, published_at DESC);
CREATE INDEX posts_author_idx    ON posts(author_id);

CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL
);

CREATE TABLE post_tags (
    post_id INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    tag_id  INTEGER NOT NULL REFERENCES tags(id)  ON DELETE CASCADE,
    PRIMARY KEY (post_id, tag_id)
);
CREATE INDEX post_tags_tag_idx ON post_tags(tag_id);

CREATE TABLE members (
    id              INTEGER PRIMARY KEY,
    email           TEXT NOT NULL UNIQUE,
    confirmed_at    INTEGER,
    unsubscribed_at INTEGER,
    created_at      INTEGER NOT NULL
);
CREATE INDEX members_confirmed_idx ON members(confirmed_at) WHERE confirmed_at IS NOT NULL;

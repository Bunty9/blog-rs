-- Privacy-friendly page view analytics: aggregate daily counters only.
-- No cookies, no IP addresses, no user agents, no per-visitor identifiers
-- are ever stored here -- each row is just a (day, path)/(day, host) counter.
CREATE TABLE page_views_daily (
    day   TEXT NOT NULL, -- YYYY-MM-DD, UTC
    path  TEXT NOT NULL,
    views INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, path)
);

CREATE TABLE referrers_daily (
    day   TEXT NOT NULL, -- YYYY-MM-DD, UTC
    host  TEXT NOT NULL,
    views INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, host)
);

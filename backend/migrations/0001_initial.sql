CREATE TABLE IF NOT EXISTS admins (
    username TEXT PRIMARY KEY,
    password_hash TEXT NOT NULL,
    created_at INTEGER DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS users (
    username TEXT PRIMARY KEY,
    links TEXT NOT NULL DEFAULT '[]',
    rank INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_users_rank ON users(rank);

CREATE TABLE IF NOT EXISTS fetch_diagnostics (
    username TEXT NOT NULL,
    source_url TEXT NOT NULL,
    status TEXT NOT NULL,
    detail TEXT,
    http_status INTEGER,
    content_type TEXT,
    body_bytes INTEGER,
    redirect_count INTEGER NOT NULL DEFAULT 0,
    is_html INTEGER NOT NULL DEFAULT 0,
    fetched_at INTEGER DEFAULT (strftime('%s', 'now')),
    PRIMARY KEY (username, source_url)
);

CREATE INDEX IF NOT EXISTS idx_fetch_diagnostics_username
    ON fetch_diagnostics(username);

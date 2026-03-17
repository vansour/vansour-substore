CREATE TABLE IF NOT EXISTS user_cache_snapshots (
    username TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    line_count INTEGER NOT NULL DEFAULT 0,
    body_bytes INTEGER NOT NULL DEFAULT 0,
    generated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_user_cache_snapshots_expires_at
    ON user_cache_snapshots(expires_at);

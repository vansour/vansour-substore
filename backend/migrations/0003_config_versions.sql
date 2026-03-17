ALTER TABLE users
    ADD COLUMN config_version INTEGER NOT NULL DEFAULT 1;

ALTER TABLE user_cache_snapshots
    ADD COLUMN source_config_version INTEGER NOT NULL DEFAULT 1;

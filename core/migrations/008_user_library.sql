-- User-scoped library state (playback, manual lists, watch later)

CREATE TABLE user_profile (
    id         TEXT PRIMARY KEY NOT NULL,
    label      TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE user_playback_state (
    user_id          TEXT NOT NULL REFERENCES user_profile(id),
    entity_id        TEXT NOT NULL REFERENCES entity(id),
    asset_id         TEXT NOT NULL,
    position_seconds REAL NOT NULL DEFAULT 0,
    duration_seconds REAL,
    completed_at     TEXT,
    updated_at       TEXT NOT NULL,
    dismissed_at     TEXT,
    PRIMARY KEY (user_id, entity_id)
);

CREATE INDEX idx_playback_user_updated ON user_playback_state(user_id, updated_at DESC);

CREATE TABLE user_list (
    id         TEXT PRIMARY KEY NOT NULL,
    user_id    TEXT NOT NULL REFERENCES user_profile(id),
    list_type  TEXT NOT NULL,
    title      TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_user_list_user_type ON user_list(user_id, list_type);

CREATE TABLE user_list_member (
    list_id    TEXT NOT NULL REFERENCES user_list(id),
    entity_id  TEXT NOT NULL REFERENCES entity(id),
    position   INTEGER NOT NULL,
    added_at   TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'active',
    PRIMARY KEY (list_id, entity_id)
);

CREATE INDEX idx_user_list_member_list ON user_list_member(list_id, position ASC);

-- Default single-user profile for single-vault MVP
INSERT INTO user_profile (id, label, created_at)
VALUES ('00000000-0000-0000-0000-000000000001', 'default', datetime('now'));

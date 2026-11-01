CREATE TABLE activity_presence (
    client_id TEXT PRIMARY KEY,
    sequence BIGINT NOT NULL CHECK(sequence>0),
    mode TEXT NOT NULL CHECK(mode IN ('online','engine','same_device')),
    game_id TEXT,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    playing BOOLEAN NOT NULL,
    visible BOOLEAN NOT NULL,
    seen_ms BIGINT NOT NULL
);
CREATE INDEX activity_presence_seen ON activity_presence(seen_ms);

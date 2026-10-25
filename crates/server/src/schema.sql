PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    handle TEXT NOT NULL UNIQUE COLLATE NOCASE,
    guest INTEGER NOT NULL CHECK(guest IN (0,1)),
    created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS identities (
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id),
    PRIMARY KEY(issuer, subject)
);
CREATE TABLE IF NOT EXISTS sessions (
    hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    expires_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS auth_flows (
    hash TEXT PRIMARY KEY,
    browser_hash TEXT NOT NULL,
    provider TEXT NOT NULL,
    verifier TEXT NOT NULL,
    nonce TEXT NOT NULL,
    user_id TEXT REFERENCES users(id),
    linking INTEGER NOT NULL,
    expires_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS ratings (
    user_id TEXT NOT NULL REFERENCES users(id),
    pool TEXT NOT NULL,
    rating REAL NOT NULL DEFAULT 1500,
    games INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(user_id, pool)
);
CREATE TABLE IF NOT EXISTS games (
    id TEXT PRIMARY KEY,
    initial_save TEXT NOT NULL,
    white_id TEXT REFERENCES users(id),
    black_id TEXT REFERENCES users(id),
    seq INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL CHECK(status IN ('waiting','active','paused','finished','cancelled')),
    size INTEGER NOT NULL,
    layout TEXT NOT NULL,
    rated INTEGER NOT NULL,
    result TEXT CHECK(result IN ('white','black','draw')),
    reason TEXT,
    white_ms INTEGER,
    black_ms INTEGER,
    increment_ms INTEGER NOT NULL,
    running_since_ms INTEGER,
    turn TEXT NOT NULL DEFAULT 'white',
    ready_white INTEGER NOT NULL DEFAULT 0,
    ready_black INTEGER NOT NULL DEFAULT 0,
    created_ms INTEGER NOT NULL,
    CHECK(white_id IS NULL OR black_id IS NULL OR white_id != black_id)
);
CREATE INDEX IF NOT EXISTS games_white ON games(white_id, created_ms);
CREATE INDEX IF NOT EXISTS games_black ON games(black_id, created_ms);
CREATE TABLE IF NOT EXISTS events (
    game_id TEXT NOT NULL REFERENCES games(id),
    seq INTEGER NOT NULL,
    actor_id TEXT REFERENCES users(id),
    request_id TEXT,
    request_json TEXT,
    event_json TEXT NOT NULL,
    PRIMARY KEY(game_id, seq),
    UNIQUE(game_id, request_id)
);
CREATE TABLE IF NOT EXISTS rating_changes (
    game_id TEXT NOT NULL REFERENCES games(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    before REAL NOT NULL,
    after REAL NOT NULL,
    PRIMARY KEY(game_id, user_id)
);
CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS mobile_auth (
    hash TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    challenge TEXT NOT NULL,
    client_state TEXT NOT NULL,
    user_id TEXT REFERENCES users(id),
    session_hash TEXT,
    linking INTEGER NOT NULL,
    state_hash TEXT UNIQUE,
    code_hash TEXT UNIQUE,
    result_user TEXT REFERENCES users(id),
    error TEXT,
    expires_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS stream_tickets (
    hash TEXT PRIMARY KEY,
    game_id TEXT NOT NULL REFERENCES games(id),
    session_hash TEXT NOT NULL,
    expires_ms INTEGER NOT NULL
);
PRAGMA user_version = 2;

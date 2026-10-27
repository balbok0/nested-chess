CREATE TABLE invitation_requests (
    user_id TEXT NOT NULL REFERENCES users(id),
    request_id TEXT NOT NULL,
    spec_json TEXT NOT NULL,
    game_id TEXT NOT NULL REFERENCES games(id),
    PRIMARY KEY(user_id,request_id)
);

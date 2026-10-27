ALTER TABLE games ADD COLUMN queue_started_ms BIGINT;
ALTER TABLE games ADD COLUMN matched_id TEXT REFERENCES games(id);
DROP INDEX games_queue;
CREATE INDEX games_queue ON games(queue_started_ms,id)
WHERE status='waiting' AND queue_started_ms IS NOT NULL;

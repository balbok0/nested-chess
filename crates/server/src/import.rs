use crate::{
    model::{Error, EventAction, MatchEvent, Result},
    store::{Clock, Store},
};
use nested_chess::Game;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Legacy {
    version: u8,
    tables: BTreeMap<String, Vec<Value>>,
}

impl Store {
    /// Import a SQLite export into an empty PostgreSQL database.
    pub async fn import_sqlite(&self, input: &str) -> Result<()> {
        let mut legacy: Legacy = serde_json::from_str(input)?;
        let spec: BTreeMap<String, String> =
            serde_json::from_str(include_str!("../legacy-tables.json"))?;
        if legacy.version != 1 || legacy.tables.keys().ne(spec.keys()) {
            return Err(Error::Invalid("Unsupported SQLite export.".into()));
        }
        let mut games = BTreeMap::new();
        for row in &legacy.tables["games"] {
            let id = row["id"]
                .as_str()
                .ok_or_else(|| Error::Invalid("Missing game ID.".into()))?;
            let initial = row["initial_save"]
                .as_str()
                .ok_or_else(|| Error::Invalid("Missing game save.".into()))?;
            games.insert(id.to_owned(), (Game::load_json(initial)?, 0_u32));
        }
        legacy
            .tables
            .get_mut("events")
            .unwrap()
            .sort_by_key(|e| e["seq"].as_u64());
        for row in &legacy.tables["events"] {
            let event: MatchEvent = serde_json::from_str(
                row["event_json"]
                    .as_str()
                    .ok_or_else(|| Error::Invalid("Missing event JSON.".into()))?,
            )?;
            let id = row["game_id"]
                .as_str()
                .ok_or_else(|| Error::Invalid("Missing event game.".into()))?;
            let (game, seq) = games
                .get_mut(id)
                .ok_or_else(|| Error::Invalid("Unknown event game.".into()))?;
            if event.info.id != id
                || event.seq != *seq + 1
                || row["seq"].as_u64() != Some(u64::from(event.seq))
                || event.info.seq != event.seq
            {
                return Err(Error::Invalid("Non-contiguous event log.".into()));
            }
            if let EventAction::Move { board, uci } = &event.action {
                game.play(*board, uci)?;
            }
            *seq = event.seq;
        }
        for row in legacy.tables.get_mut("games").unwrap() {
            let (game, seq) = games.get(row["id"].as_str().unwrap()).unwrap();
            if row["seq"].as_u64() != Some(u64::from(*seq)) {
                return Err(Error::Invalid(
                    "Game sequence differs from its event log.".into(),
                ));
            }
            row["checkpoint"] = json!(game.checkpoint_json()?);
            row["deadline_ms"] = Value::Null;
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("LOCK TABLE users,games IN ACCESS EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await?;
        let occupied: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users) OR EXISTS(SELECT 1 FROM games)")
                .fetch_one(&mut *tx)
                .await?;
        if occupied {
            return Err(Error::Conflict("Import requires an empty database.".into()));
        }
        for table in [
            "users",
            "identities",
            "sessions",
            "auth_flows",
            "ratings",
            "games",
            "events",
            "rating_changes",
            "mobile_auth",
            "stream_tickets",
        ] {
            let columns = &spec[table];
            let query = format!(
                "INSERT INTO {table} ({columns}) SELECT {columns} FROM json_populate_recordset(NULL::{table},$1::json)"
            );
            sqlx::query(&query)
                .bind(serde_json::to_string(&legacy.tables[table])?)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("SELECT setval(pg_get_serial_sequence('games','ordinal'),COALESCE(MAX(ordinal),1),MAX(ordinal) IS NOT NULL) FROM games").execute(&mut *tx).await?;
        let now = Clock::Database.resolve(&mut tx).await?;
        for id in games.keys() {
            let mut info = Self::info_on(&mut tx, id, now).await?;
            if info.status == "active" && info.white_ms.is_some() {
                info.status = "paused".into();
                info.running_since_ms = None;
                info.ready_white = false;
                info.ready_black = false;
                Self::persist(&mut tx, info, EventAction::Paused, None, None, None).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }
}

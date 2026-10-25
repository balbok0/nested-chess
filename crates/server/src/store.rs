use crate::model::*;
use nested_chess::{Color, Game, Outcome};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions, TryLockError},
    path::Path,
    time::Duration,
};
use tokio::sync::broadcast;
use uuid::Uuid;

pub struct Store {
    pub(crate) db: Connection,
    games: HashMap<String, Game>,
    topics: HashMap<String, broadcast::Sender<MatchEvent>>,
    _lease: File,
}

pub struct Subscription {
    pub snapshot: Snapshot,
    pub updates: broadcast::Receiver<MatchEvent>,
}

pub fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
pub fn hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
fn color(value: &str) -> rusqlite::Result<Color> {
    match value {
        "white" => Ok(Color::White),
        "black" => Ok(Color::Black),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
fn color_name(value: Color) -> &'static str {
    match value {
        Color::White => "white",
        Color::Black => "black",
    }
}

impl Store {
    pub fn open(path: &Path, now: i64) -> Result<Self> {
        let db = Connection::open(path)?;
        let mut lock_path = path
            .canonicalize()
            .map_err(|error| Error::Internal(format!("Could not resolve database path: {error}")))?
            .into_os_string();
        lock_path.push(".lock");
        let lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|error| Error::Internal(format!("Could not open database lease: {error}")))?;
        lease.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => {
                Error::Conflict("This database is already open by another server.".into())
            }
            TryLockError::Error(error) => {
                Error::Internal(format!("Could not lock database lease: {error}"))
            }
        })?;
        db.busy_timeout(Duration::from_secs(5))?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > 2 {
            return Err(Error::Internal(
                "Database schema is newer than this server.".into(),
            ));
        }
        db.execute_batch(include_str!("schema.sql"))?;
        let mut store = Self {
            db,
            games: HashMap::new(),
            topics: HashMap::new(),
            _lease: lease,
        };
        let heartbeat: Option<i64> = store
            .db
            .query_row(
                "SELECT value FROM metadata WHERE key='heartbeat'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let ids = store
            .ids("SELECT id FROM games WHERE status='active' AND running_since_ms IS NOT NULL")?;
        for id in ids {
            let mut info = store.info(&id, now)?;
            let charged_until = heartbeat.unwrap_or(info.running_since_ms.unwrap()).min(now);
            store.charge_clock(&mut info, charged_until);
            info.status = "paused".into();
            info.running_since_ms = None;
            info.ready_white = false;
            info.ready_black = false;
            store.persist(info, EventAction::Paused, None, None, None)?;
        }
        store
            .db
            .execute("DELETE FROM sessions WHERE expires_ms <= ?1", [now])?;
        store
            .db
            .execute("DELETE FROM auth_flows WHERE expires_ms <= ?1", [now])?;
        Ok(store)
    }

    fn ids(&self, sql: &str) -> Result<Vec<String>> {
        Ok(self
            .db
            .prepare(sql)?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn user(&self, id: &str) -> Result<User> {
        self.db.query_row("SELECT u.id,u.handle,u.guest,r.rating,r.games FROM users u JOIN ratings r ON r.user_id=u.id AND r.pool=?2 WHERE u.id=?1", params![id, RATING_POOL], |row| Ok(User { id: row.get(0)?, handle: row.get(1)?, guest: row.get(2)?, rating: row.get(3)?, rated_games: row.get(4)? })).optional()?.ok_or(Error::Unauthorized)
    }

    pub fn session(&self, token: &str, now: i64) -> Result<User> {
        self.session_hash(&hash(token), now)
    }

    pub(crate) fn session_hash(&self, token_hash: &str, now: i64) -> Result<User> {
        let id: Option<String> = self
            .db
            .query_row(
                "SELECT user_id FROM sessions WHERE hash=?1 AND expires_ms>?2",
                params![token_hash, now],
                |row| row.get(0),
            )
            .optional()?;
        self.user(&id.ok_or(Error::Unauthorized)?)
    }

    pub fn new_session(&mut self, user: &str, now: i64) -> Result<String> {
        let token = secret();
        self.db.execute(
            "INSERT INTO sessions(hash,user_id,expires_ms) VALUES(?1,?2,?3)",
            params![hash(&token), user, now + SESSION_SECONDS * 1000],
        )?;
        Ok(token)
    }

    pub fn logout(&mut self, token: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM sessions WHERE hash=?1", [hash(token)])?;
        Ok(())
    }

    pub fn guest(&mut self, now: i64) -> Result<(User, String)> {
        let id = Uuid::new_v4().to_string();
        self.insert_user(
            &id,
            &format!("Guest_{}", &Uuid::new_v4().simple().to_string()[..12]),
            true,
            now,
        )?;
        Ok((self.user(&id)?, self.new_session(&id, now)?))
    }

    pub(crate) fn insert_user(
        &mut self,
        id: &str,
        handle: &str,
        guest: bool,
        now: i64,
    ) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO users(id,handle,guest,created_ms) VALUES(?1,?2,?3,?4)",
            params![id, handle, guest, now],
        )?;
        tx.execute(
            "INSERT INTO ratings(user_id,pool) VALUES(?1,?2)",
            params![id, RATING_POOL],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn rename(&mut self, user: &str, handle: &str) -> Result<User> {
        if !(3..=24).contains(&handle.len())
            || !handle
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(Error::Invalid(
                "Use 3–24 letters, numbers, or underscores.".into(),
            ));
        }
        if self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE handle=?1 COLLATE NOCASE AND id!=?2)",
            params![handle, user],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(Error::Conflict("That username is taken.".into()));
        }
        self.db.execute(
            "UPDATE users SET handle=?1 WHERE id=?2",
            params![handle, user],
        )?;
        self.user(user)
    }

    pub fn create(&mut self, user: &str, spec: CreateGame, now: i64) -> Result<Snapshot> {
        let player = self.user(user)?;
        if !matches!(spec.size, 2 | 4 | 8) {
            return Err(Error::Invalid("Choose a 2×2, 4×4, or 8×8 grid.".into()));
        }
        if spec.rated
            && (player.guest
                || spec.size != 2
                || spec.layout != "classical"
                || spec.clock != Some(TimeControl::default()))
        {
            return Err(Error::Invalid(
                "Rated games require sign-in, classical 2×2, and 15+10.".into(),
            ));
        }
        if let Some(clock) = spec.clock {
            if !(60..=7200).contains(&clock.seconds) || clock.increment > 60 {
                return Err(Error::Invalid(
                    "Clock must be 1–120 minutes, with 0–60 seconds increment.".into(),
                ));
            }
        }
        let open: i64 = self.db.query_row("SELECT count(*) FROM games WHERE (white_id=?1 OR black_id=?1) AND status IN ('waiting','active','paused')", [user], |row| row.get(0))?;
        if open >= 3 {
            return Err(Error::Conflict(
                "Finish or cancel an open match first.".into(),
            ));
        }
        if spec.rated && self.has_rated_match(user)? {
            return Err(Error::Conflict(
                "You already have an open rated match.".into(),
            ));
        }
        let engine = match spec.layout.as_str() {
            "classical" => Game::new(spec.size)?,
            "chess960" => Game::chess960(spec.size, (Uuid::new_v4().as_u128() % 960) as u16)?,
            "independent960" => Game::chess960_boards(
                spec.size,
                &(0..usize::from(spec.size).pow(2))
                    .map(|_| (Uuid::new_v4().as_u128() % 960) as u16)
                    .collect::<Vec<_>>(),
            )?,
            _ => return Err(Error::Invalid("Unknown starting layout.".into())),
        };
        let id = Uuid::new_v4().to_string();
        let white = Uuid::new_v4().as_u128() & 1 == 0;
        self.db.execute("INSERT INTO games(id,initial_save,white_id,black_id,status,size,layout,rated,white_ms,black_ms,increment_ms,created_ms) VALUES(?1,?2,?3,?4,'waiting',?5,?6,?7,?8,?8,?9,?10)", params![id,engine.save_json()?,if white {Some(user)} else {None},if white {None} else {Some(user)},spec.size,spec.layout,spec.rated,spec.clock.map(|c|i64::from(c.seconds)*1000),spec.clock.map_or(0,|c|i64::from(c.increment)*1000),now])?;
        self.games.insert(id.clone(), engine);
        self.snapshot(&id, user, now)
    }

    fn has_rated_match(&self, user: &str) -> Result<bool> {
        Ok(self.db.query_row("SELECT EXISTS(SELECT 1 FROM games WHERE rated=1 AND (white_id=?1 OR black_id=?1) AND status IN ('waiting','active','paused'))", [user], |row| row.get(0))?)
    }

    pub fn queue(&mut self, user: &str, now: i64) -> Result<Snapshot> {
        if self.user(user)?.guest {
            return Err(Error::Forbidden("Sign in to play rated matches.".into()));
        }
        if let Some(id) = self.db.query_row("SELECT id FROM games WHERE rated=1 AND (white_id=?1 OR black_id=?1) AND status IN ('waiting','active','paused') LIMIT 1", [user], |row| row.get::<_,String>(0)).optional()? { return self.snapshot(&id,user,now); }
        let id: Option<String> = self.db.query_row("SELECT id FROM games WHERE rated=1 AND status='waiting' AND COALESCE(white_id,black_id)!=?1 ORDER BY created_ms LIMIT 1", [user], |row| row.get(0)).optional()?;
        match id {
            Some(id) => self.join(&id, user, now),
            None => self.create(
                user,
                CreateGame {
                    rated: true,
                    clock: Some(TimeControl::default()),
                    ..Default::default()
                },
                now,
            ),
        }
    }

    pub fn join(&mut self, id: &str, user: &str, now: i64) -> Result<Snapshot> {
        let mut info = self.info(id, now)?;
        if info.seat(user).is_some() {
            return self.snapshot(id, user, now);
        }
        if info.status != "waiting" {
            return Err(Error::Conflict("Both seats are occupied.".into()));
        }
        let player = self.user(user)?;
        if info.rated && (player.guest || self.has_rated_match(user)?) {
            return Err(Error::Forbidden(
                "Sign in and finish any open rated match before joining.".into(),
            ));
        }
        let open:i64=self.db.query_row("SELECT count(*) FROM games WHERE (white_id=?1 OR black_id=?1) AND status IN ('waiting','active','paused')",[user],|row|row.get(0))?;
        if open >= 3 {
            return Err(Error::Conflict(
                "Finish or cancel an open match first.".into(),
            ));
        }
        if info.white.is_none() {
            info.white = Some(player);
        } else {
            info.black = Some(player);
        }
        info.status = "active".into();
        info.running_since_ms = info.white_ms.map(|_| now);
        self.persist(info, EventAction::Joined, None, None, None)?;
        self.snapshot(id, user, now)
    }

    pub fn info(&self, id: &str, now: i64) -> Result<MatchInfo> {
        let row = self.db.query_row("SELECT id,seq,status,white_id,black_id,rated,size,layout,result,reason,white_ms,black_ms,increment_ms,running_since_ms,turn,ready_white,ready_black,created_ms FROM games WHERE id=?1", [id], |row| Ok((MatchInfo {id:row.get(0)?,seq:row.get(1)?,status:row.get(2)?,white:None,black:None,rated:row.get(5)?,size:row.get(6)?,layout:row.get(7)?,result:row.get(8)?,reason:row.get(9)?,white_ms:row.get(10)?,black_ms:row.get(11)?,increment_ms:row.get(12)?,running_since_ms:row.get(13)?,turn:color(&row.get::<_,String>(14)?)?,ready_white:row.get(15)?,ready_black:row.get(16)?,created_ms:row.get(17)?,server_ms:now}, row.get::<_,Option<String>>(3)?,row.get::<_,Option<String>>(4)?))).optional()?.ok_or(Error::NotFound)?;
        let (mut info, white, black) = row;
        info.white = white.map(|id| self.user(&id)).transpose()?;
        info.black = black.map(|id| self.user(&id)).transpose()?;
        Ok(info)
    }

    fn load(&mut self, id: &str) -> Result<()> {
        if self.games.contains_key(id) {
            return Ok(());
        }
        let save: String = self
            .db
            .query_row("SELECT initial_save FROM games WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .optional()?
            .ok_or(Error::NotFound)?;
        let mut game = Game::load_json(&save)?;
        let events = self
            .db
            .prepare("SELECT event_json FROM events WHERE game_id=?1 ORDER BY seq")?
            .query_map([id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for event in events {
            if let EventAction::Move { board, uci } =
                serde_json::from_str::<MatchEvent>(&event)?.action
            {
                game.play(board, &uci)?;
            }
        }
        self.games.insert(id.into(), game);
        Ok(())
    }

    pub fn snapshot(&mut self, id: &str, user: &str, now: i64) -> Result<Snapshot> {
        let info = self.info(id, now)?;
        let seat = info
            .seat(user)
            .ok_or_else(|| Error::Forbidden("Join this match before viewing it.".into()))?;
        self.load(id)?;
        let game = &self.games[id];
        let closed = matches!(info.status.as_str(), "finished" | "cancelled");
        let snapshot = Snapshot {
            info,
            seat,
            save: game.save_json()?,
            state: game.state(),
        };
        if closed {
            self.games.remove(id);
        }
        Ok(snapshot)
    }

    pub fn subscribe(&mut self, id: &str, user: &str, now: i64) -> Result<Subscription> {
        let snapshot = self.snapshot(id, user, now)?;
        let updates = if matches!(snapshot.info.status.as_str(), "finished" | "cancelled") {
            broadcast::channel(1).1
        } else {
            self.topics
                .entry(id.into())
                .or_insert_with(|| broadcast::channel(128).0)
                .subscribe()
        };
        Ok(Subscription { snapshot, updates })
    }

    fn charge_clock(&self, info: &mut MatchInfo, now: i64) {
        if info.running_since_ms.is_some() {
            let remaining = info.remaining_ms(info.turn, now);
            match info.turn {
                Color::White => info.white_ms = remaining,
                Color::Black => info.black_ms = remaining,
            };
            info.running_since_ms = Some(now);
        }
    }

    fn finish(info: &mut MatchInfo, result: &str, reason: &str) {
        info.status = "finished".into();
        info.result = Some(result.into());
        info.reason = Some(reason.into());
        info.running_since_ms = None;
    }

    pub fn command(
        &mut self,
        id: &str,
        user: &str,
        command: Command,
        now: i64,
    ) -> Result<MatchEvent> {
        Uuid::parse_str(&command.request_id)
            .map_err(|_| Error::Invalid("Invalid request ID.".into()))?;
        let mut info = self.info(id, now)?;
        let seat = info
            .seat(user)
            .ok_or_else(|| Error::Forbidden("You are not a player in this match.".into()))?;
        let request = serde_json::to_string(&command)?;
        if let Some((actor,stored,event))=self.db.query_row("SELECT actor_id,request_json,event_json FROM events WHERE game_id=?1 AND request_id=?2",params![id,command.request_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?))).optional()? {
            if actor != user || stored != request {return Err(Error::Conflict("That request ID was already used.".into()));}
            return Ok(serde_json::from_str(&event)?);
        }
        if info.seq != command.expected_seq {
            return Err(Error::Conflict(
                "Position changed. Reconnect to refresh it.".into(),
            ));
        }
        if info.status == "active" && info.remaining_ms(info.turn, now) == Some(0) {
            self.expire(info, now)?;
            return Err(Error::Conflict("The match ended on time.".into()));
        }
        let action = match &command.action {
            Action::Move { board, uci } => {
                if info.status != "active" {
                    return Err(Error::Conflict("The match is not active.".into()));
                }
                if info.turn != seat {
                    return Err(Error::Forbidden("It is your opponent's turn.".into()));
                }
                self.load(id)?;
                let mut candidate = self.games[id].clone();
                candidate.play(*board, uci)?;
                self.charge_clock(&mut info, now);
                match seat {
                    Color::White => info.white_ms = info.white_ms.map(|v| v + info.increment_ms),
                    Color::Black => info.black_ms = info.black_ms.map(|v| v + info.increment_ms),
                };
                info.turn = candidate.turn();
                match candidate.outcome() {
                    Outcome::White => Self::finish(&mut info, "white", "rules"),
                    Outcome::Black => Self::finish(&mut info, "black", "rules"),
                    Outcome::Draw => Self::finish(&mut info, "draw", "rules"),
                    Outcome::Ongoing => {}
                }
                let event = self.persist(
                    info,
                    EventAction::Move {
                        board: *board,
                        uci: uci.clone(),
                    },
                    Some(user),
                    Some((&command.request_id, &request)),
                    Some(candidate),
                )?;
                return Ok(event);
            }
            Action::Resign => match info.status.as_str() {
                "waiting" => {
                    info.status = "cancelled".into();
                    EventAction::Cancelled
                }
                "active" | "paused" => {
                    self.charge_clock(&mut info, now);
                    Self::finish(&mut info, color_name(seat.other()), "resignation");
                    EventAction::Resigned { color: seat }
                }
                _ => return Err(Error::Conflict("The match has ended.".into())),
            },
            Action::Resume => {
                if info.status != "paused" {
                    return Err(Error::Conflict("The match is not paused.".into()));
                }
                if match seat {
                    Color::White => info.ready_white,
                    Color::Black => info.ready_black,
                } {
                    return Err(Error::Conflict("You are already ready.".into()));
                }
                match seat {
                    Color::White => info.ready_white = true,
                    Color::Black => info.ready_black = true,
                };
                if info.ready_white && info.ready_black {
                    info.status = "active".into();
                    info.running_since_ms = Some(now);
                }
                EventAction::Ready { color: seat }
            }
        };
        self.persist(
            info,
            action,
            Some(user),
            Some((&command.request_id, &request)),
            None,
        )
    }

    fn persist(
        &mut self,
        mut info: MatchInfo,
        action: EventAction,
        actor: Option<&str>,
        request: Option<(&str, &str)>,
        candidate: Option<Game>,
    ) -> Result<MatchEvent> {
        info.seq += 1;
        let mut event = MatchEvent {
            seq: info.seq,
            action,
            info,
        };
        let tx = self.db.transaction()?;
        if event.info.status == "finished" && event.info.rated {
            let white = event
                .info
                .white
                .as_ref()
                .ok_or_else(|| Error::Internal("Missing White player.".into()))?;
            let black = event
                .info
                .black
                .as_ref()
                .ok_or_else(|| Error::Internal("Missing Black player.".into()))?;
            let score = match event.info.result.as_deref() {
                Some("white") => 1.0,
                Some("black") => 0.0,
                Some("draw") => 0.5,
                _ => return Err(Error::Internal("Missing rated result.".into())),
            };
            let expected = 1.0 / (1.0 + 10_f64.powf((black.rating - white.rating) / 400.0));
            let change = 32.0 * (score - expected);
            for (player, delta) in [(white, change), (black, -change)] {
                tx.execute(
                    "INSERT INTO rating_changes(game_id,user_id,before,after) VALUES(?1,?2,?3,?4)",
                    params![
                        event.info.id,
                        player.id,
                        player.rating,
                        player.rating + delta
                    ],
                )?;
                tx.execute(
                    "UPDATE ratings SET rating=?1,games=games+1 WHERE user_id=?2 AND pool=?3",
                    params![player.rating + delta, player.id, RATING_POOL],
                )?;
            }
            if let Some(player) = &mut event.info.white {
                player.rating += change;
                player.rated_games += 1;
            }
            if let Some(player) = &mut event.info.black {
                player.rating -= change;
                player.rated_games += 1;
            }
        }
        let i = &event.info;
        tx.execute("UPDATE games SET seq=?2,status=?3,white_id=?4,black_id=?5,result=?6,reason=?7,white_ms=?8,black_ms=?9,running_since_ms=?10,turn=?11,ready_white=?12,ready_black=?13 WHERE id=?1",params![i.id,i.seq,i.status,i.white.as_ref().map(|u|&u.id),i.black.as_ref().map(|u|&u.id),i.result,i.reason,i.white_ms,i.black_ms,i.running_since_ms,color_name(i.turn),i.ready_white,i.ready_black])?;
        tx.execute("INSERT INTO events(game_id,seq,actor_id,request_id,request_json,event_json) VALUES(?1,?2,?3,?4,?5,?6)",params![i.id,i.seq,actor,request.map(|r|r.0),request.map(|r|r.1),serde_json::to_string(&event)?])?;
        tx.commit()?;
        if let Some(game) = candidate {
            self.games.insert(i.id.clone(), game);
        }
        if let Some(topic) = self.topics.get(&i.id) {
            let _ = topic.send(event.clone());
        }
        if matches!(i.status.as_str(), "finished" | "cancelled") {
            self.games.remove(&i.id);
            self.topics.remove(&i.id);
        }
        Ok(event)
    }

    fn expire(&mut self, mut info: MatchInfo, now: i64) -> Result<()> {
        let loser = info.turn;
        self.charge_clock(&mut info, now);
        Self::finish(&mut info, color_name(loser.other()), "timeout");
        self.persist(
            info,
            EventAction::Timeout { color: loser },
            None,
            None,
            None,
        )?;
        Ok(())
    }

    pub fn tick(&mut self, now: i64) -> Result<()> {
        let ids = self
            .ids("SELECT id FROM games WHERE status='active' AND running_since_ms IS NOT NULL")?;
        for id in &ids {
            let info = self.info(id, now)?;
            if info.remaining_ms(info.turn, now) == Some(0) {
                self.expire(info, now)?;
            }
        }
        if !ids.is_empty() {
            self.db.execute("INSERT INTO metadata(key,value) VALUES('heartbeat',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[now])?;
        }
        Ok(())
    }

    pub fn open_games(&self, user: &str, now: i64) -> Result<Vec<MatchInfo>> {
        let ids=self.db.prepare("SELECT id FROM games WHERE (white_id=?1 OR black_id=?1) AND status IN ('waiting','active','paused') ORDER BY created_ms DESC")?.query_map([user],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().map(|id| self.info(id, now)).collect()
    }

    pub fn history(&self, user: &str, before: Option<i64>, now: i64) -> Result<Vec<HistoryEntry>> {
        let entries=self.db.prepare("SELECT g.rowid,g.id,r.before,r.after FROM games g LEFT JOIN rating_changes r ON r.game_id=g.id AND r.user_id=?1 WHERE (g.white_id=?1 OR g.black_id=?1) AND g.status='finished' AND g.rowid<?2 ORDER BY g.rowid DESC LIMIT 20")?.query_map(params![user,before.unwrap_or(i64::MAX)],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<f64>>(2)?,row.get::<_,Option<f64>>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        entries
            .into_iter()
            .map(|(cursor, id, rating_before, rating_after)| {
                Ok(HistoryEntry {
                    cursor,
                    info: self.info(&id, now)?,
                    rating_before,
                    rating_after,
                })
            })
            .collect()
    }
}

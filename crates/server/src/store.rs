use crate::model::*;
use nested_chess::{Color, Game, Outcome};
use sha2::{Digest, Sha256};
use sqlx::{
    PgConnection, PgPool, Row,
    postgres::{PgPoolOptions, PgRow},
};
use std::time::Duration;
use uuid::Uuid;

pub const MATCH_CHANNEL: &str = "nested_matches";
const CLOCK_BATCH: i64 = 64;
const QUEUE_LOCK: i64 = 74102501;
const QUEUE_INITIAL_GAP: i64 = 150;
const QUEUE_GAP_STEP: i64 = 50;
const QUEUE_STEP_MS: i64 = 15_000;
const QUEUE_MAX_GAP: i64 = 600;
const QUEUE_BATCH: i64 = 64;

#[derive(Clone)]
pub struct Store {
    pub(crate) pool: PgPool,
}

#[derive(Clone, Copy)]
pub enum Clock {
    Database,
    At(i64),
}
impl From<i64> for Clock {
    fn from(value: i64) -> Self {
        Self::At(value)
    }
}
impl From<i32> for Clock {
    fn from(value: i32) -> Self {
        Self::At(i64::from(value))
    }
}
impl Clock {
    pub(crate) async fn resolve(self, db: &mut PgConnection) -> Result<i64> {
        match self {
            Self::At(value) => Ok(value),
            Self::Database => Ok(sqlx::query_scalar(
                "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            )
            .fetch_one(db)
            .await?),
        }
    }
}
pub fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
pub fn hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
fn color(value: &str) -> Result<Color> {
    match value {
        "white" => Ok(Color::White),
        "black" => Ok(Color::Black),
        _ => Err(Error::Internal("Invalid stored color.".into())),
    }
}
fn color_name(value: Color) -> &'static str {
    match value {
        Color::White => "white",
        Color::Black => "black",
    }
}

impl Store {
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self { pool })
    }
    pub async fn ready(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
    pub(crate) async fn user_on(db: &mut PgConnection, id: &str) -> Result<User> {
        let row=sqlx::query("SELECT u.id,u.handle,u.guest,r.rating,r.games FROM users u JOIN ratings r ON r.user_id=u.id AND r.pool=$2 WHERE u.id=$1")
            .bind(id).bind(RATING_POOL).fetch_optional(db).await?.ok_or(Error::Unauthorized)?;
        Ok(User {
            id: row.try_get("id")?,
            handle: row.try_get("handle")?,
            guest: row.try_get("guest")?,
            rating: row.try_get("rating")?,
            rated_games: row
                .try_get::<i32, _>("games")?
                .try_into()
                .map_err(|_| Error::Internal("Invalid rating count.".into()))?,
        })
    }
    pub async fn user(&self, id: &str) -> Result<User> {
        let mut db = self.pool.acquire().await?;
        Self::user_on(&mut db, id).await
    }
    pub(crate) async fn session_on(
        db: &mut PgConnection,
        token_hash: &str,
        now: i64,
    ) -> Result<User> {
        let id: String =
            sqlx::query_scalar("SELECT user_id FROM sessions WHERE hash=$1 AND expires_ms>$2")
                .bind(token_hash)
                .bind(now)
                .fetch_optional(&mut *db)
                .await?
                .ok_or(Error::Unauthorized)?;
        Self::user_on(db, &id).await
    }
    pub async fn session(&self, token: &str, clock: impl Into<Clock>) -> Result<User> {
        self.session_hash(&hash(token), clock).await
    }
    pub(crate) async fn session_hash(
        &self,
        token_hash: &str,
        clock: impl Into<Clock>,
    ) -> Result<User> {
        let mut db = self.pool.acquire().await?;
        let now = clock.into().resolve(&mut db).await?;
        Self::session_on(&mut db, token_hash, now).await
    }
    pub(crate) async fn new_session_on(
        db: &mut PgConnection,
        user: &str,
        now: i64,
    ) -> Result<String> {
        let token = secret();
        sqlx::query("INSERT INTO sessions(hash,user_id,expires_ms) VALUES($1,$2,$3)")
            .bind(hash(&token))
            .bind(user)
            .bind(now + SESSION_SECONDS * 1000)
            .execute(db)
            .await?;
        Ok(token)
    }
    pub async fn new_session(&self, user: &str, clock: impl Into<Clock>) -> Result<String> {
        let mut db = self.pool.acquire().await?;
        let now = clock.into().resolve(&mut db).await?;
        Self::new_session_on(&mut db, user, now).await
    }
    pub async fn logout(&self, token: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE hash=$1")
            .bind(hash(token))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn guest(&self, clock: impl Into<Clock>) -> Result<(User, String)> {
        let mut tx = self.pool.begin().await?;
        let now = clock.into().resolve(&mut tx).await?;
        let id = Uuid::new_v4().to_string();
        Self::insert_user_on(
            &mut tx,
            &id,
            &format!("Guest_{}", &Uuid::new_v4().simple().to_string()[..12]),
            true,
            now,
        )
        .await?;
        let token = Self::new_session_on(&mut tx, &id, now).await?;
        let user = Self::user_on(&mut tx, &id).await?;
        tx.commit().await?;
        Ok((user, token))
    }
    pub(crate) async fn insert_user_on(
        db: &mut PgConnection,
        id: &str,
        handle: &str,
        guest: bool,
        now: i64,
    ) -> Result<()> {
        sqlx::query("INSERT INTO users(id,handle,guest,created_ms) VALUES($1,$2,$3,$4)")
            .bind(id)
            .bind(handle)
            .bind(guest)
            .bind(now)
            .execute(&mut *db)
            .await?;
        sqlx::query("INSERT INTO ratings(user_id,pool) VALUES($1,$2)")
            .bind(id)
            .bind(RATING_POOL)
            .execute(db)
            .await?;
        Ok(())
    }
    pub async fn rename(&self, user: &str, handle: &str) -> Result<User> {
        if !(3..=24).contains(&handle.len())
            || !handle
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(Error::Invalid(
                "Use 3–24 letters, numbers, or underscores.".into(),
            ));
        }
        let result = sqlx::query("UPDATE users SET handle=$2 WHERE id=$1")
            .bind(user)
            .bind(handle)
            .execute(&self.pool)
            .await;
        if let Err(sqlx::Error::Database(error)) = &result {
            if error.is_unique_violation() {
                return Err(Error::Conflict("That username is taken.".into()));
            }
        }
        result?;
        self.user(user).await
    }
    pub async fn identities(&self, user: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT DISTINCT CASE WHEN issuer='github' THEN 'github' WHEN issuer='https://accounts.google.com' THEN 'google' ELSE 'microsoft' END FROM identities WHERE user_id=$1").bind(user).fetch_all(&self.pool).await?)
    }
    pub(crate) async fn lock_user(db: &mut PgConnection, user: &str) -> Result<()> {
        sqlx::query("SELECT id FROM users WHERE id=$1 FOR NO KEY UPDATE")
            .bind(user)
            .fetch_optional(db)
            .await?
            .ok_or(Error::Unauthorized)?;
        Ok(())
    }
    async fn check_open(db: &mut PgConnection, user: &str, rated: bool) -> Result<()> {
        let rows:Vec<bool>=sqlx::query_scalar("SELECT rated FROM games WHERE (white_id=$1 OR black_id=$1) AND status IN ('waiting','active','paused')").bind(user).fetch_all(db).await?;
        if rows.len() >= 3 {
            return Err(Error::Conflict(
                "Finish or cancel an open match first.".into(),
            ));
        }
        if rated && rows.contains(&true) {
            return Err(Error::Conflict(
                "You already have an open rated match.".into(),
            ));
        }
        Ok(())
    }
    pub async fn create(
        &self,
        user: &str,
        spec: CreateGame,
        clock: impl Into<Clock>,
    ) -> Result<Snapshot> {
        if let Some(id) = &spec.request_id {
            Uuid::parse_str(id)
                .map_err(|_| Error::Invalid("Invalid invitation request ID.".into()))?;
        }
        let mut tx = self.pool.begin().await?;
        Self::lock_user(&mut tx, user).await?;
        let now = clock.into().resolve(&mut tx).await?;
        let request_id = spec.request_id.clone();
        let spec_json = serde_json::to_string(&spec)?;
        if let Some(request_id) = &request_id {
            if let Some(row) = sqlx::query("SELECT spec_json,game_id FROM invitation_requests WHERE user_id=$1 AND request_id=$2")
                .bind(user).bind(request_id).fetch_optional(&mut *tx).await? {
                if row.try_get::<String,_>("spec_json")? != spec_json {
                    return Err(Error::Conflict("That invitation request ID was already used with different settings.".into()));
                }
                let result = Self::snapshot_on(&mut tx,&row.try_get::<String,_>("game_id")?,user,now).await?;
                tx.commit().await?;
                return Ok(result);
            }
        }
        let id = Self::create_on(&mut tx, user, spec, now).await?;
        if let Some(request_id) = request_id {
            sqlx::query("INSERT INTO invitation_requests(user_id,request_id,spec_json,game_id) VALUES($1,$2,$3,$4)")
                .bind(user).bind(request_id).bind(spec_json).bind(&id).execute(&mut *tx).await?;
        }
        let result = Self::snapshot_on(&mut tx, &id, user, now).await?;
        tx.commit().await?;
        Ok(result)
    }
    async fn create_on(
        db: &mut PgConnection,
        user: &str,
        spec: CreateGame,
        now: i64,
    ) -> Result<String> {
        let player = Self::user_on(db, user).await?;
        let rows = spec.rows.unwrap_or(spec.size);
        if !matches!((spec.size, rows), (2, 2) | (4, 2) | (4, 4)) {
            return Err(Error::Invalid(
                "Choose a 2×2, 4×2, or 4×4 grid (columns × rows).".into(),
            ));
        }
        if spec.rated
            && (player.guest
                || spec.size != 2
                || rows != 2
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
        Self::check_open(db, user, spec.rated).await?;
        let game = match spec.layout.as_str() {
            "classical" => Game::rectangular(spec.size, rows)?,
            "chess960" => Game::chess960_rectangular(
                spec.size,
                rows,
                (Uuid::new_v4().as_u128() % 960) as u16,
            )?,
            "independent960" => Game::chess960_boards_rectangular(
                spec.size,
                rows,
                &(0..usize::from(spec.size) * usize::from(rows))
                    .map(|_| (Uuid::new_v4().as_u128() % 960) as u16)
                    .collect::<Vec<_>>(),
            )?,
            _ => return Err(Error::Invalid("Unknown starting layout.".into())),
        };
        let id = Uuid::new_v4().to_string();
        let white = Uuid::new_v4().as_u128() & 1 == 0;
        sqlx::query("INSERT INTO games(id,initial_save,checkpoint,white_id,black_id,status,size,layout,rated,white_ms,black_ms,increment_ms,created_ms,rows) VALUES($1,$2,$3,$4,$5,'waiting',$6,$7,$8,$9,$9,$10,$11,$12)")
            .bind(&id).bind(game.save_json()?).bind(game.checkpoint_json()?).bind(if white {Some(user)}else{None}).bind(if white {None}else{Some(user)})
            .bind(i16::from(spec.size)).bind(&spec.layout).bind(spec.rated).bind(spec.clock.map(|c|i64::from(c.seconds)*1000)).bind(spec.clock.map_or(0,|c|i64::from(c.increment)*1000)).bind(now).bind(i16::from(rows)).execute(db).await?;
        Ok(id)
    }
    pub async fn queue(&self, user: &str, clock: impl Into<Clock>) -> Result<Snapshot> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(QUEUE_LOCK)
            .execute(&mut *tx)
            .await?;
        Self::lock_user(&mut tx, user).await?;
        if Self::user_on(&mut tx, user).await?.guest {
            return Err(Error::Forbidden("Sign in to play rated matches.".into()));
        }
        let now = clock.into().resolve(&mut tx).await?;
        if let Some(id)=sqlx::query_scalar::<_,String>("SELECT id FROM games WHERE rated AND (white_id=$1 OR black_id=$1) AND status IN ('waiting','active','paused') LIMIT 1").bind(user).fetch_optional(&mut *tx).await? {
            let result=Self::snapshot_on(&mut tx,&id,user,now).await?;tx.commit().await?;return Ok(result);
        }
        let id = Self::create_on(
            &mut tx,
            user,
            CreateGame {
                rated: true,
                clock: Some(TimeControl::default()),
                ..Default::default()
            },
            now,
        )
        .await?;
        sqlx::query("UPDATE games SET queue_started_ms=$2 WHERE id=$1")
            .bind(&id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        Self::match_queue_on(&mut tx, now).await?;
        let result = Self::snapshot_on(&mut tx, &id, user, now).await?;
        tx.commit().await?;
        Ok(result)
    }
    async fn match_queue_on(db: &mut PgConnection, now: i64) -> Result<bool> {
        let pair = sqlx::query(
            "SELECT a.id AS first_id,b.id AS second_id,COALESCE(b.white_id,b.black_id) AS second_user
             FROM games a
             JOIN ratings ra ON ra.user_id=COALESCE(a.white_id,a.black_id) AND ra.pool=$6
             JOIN games b ON (a.queue_started_ms,a.ordinal)<(b.queue_started_ms,b.ordinal)
             JOIN ratings rb ON rb.user_id=COALESCE(b.white_id,b.black_id) AND rb.pool=$6
             WHERE a.status='waiting' AND a.queue_started_ms IS NOT NULL
               AND b.status='waiting' AND b.queue_started_ms IS NOT NULL
               AND abs(ra.rating-rb.rating)<=LEAST($5,
                 $2+$3*(GREATEST($1-a.queue_started_ms,0)/$4),
                 $2+$3*(GREATEST($1-b.queue_started_ms,0)/$4))
             ORDER BY a.queue_started_ms,a.ordinal,abs(ra.rating-rb.rating),b.queue_started_ms,b.ordinal
             LIMIT 1 FOR UPDATE OF a,b",
        )
        .bind(now).bind(QUEUE_INITIAL_GAP).bind(QUEUE_GAP_STEP)
        .bind(QUEUE_STEP_MS).bind(QUEUE_MAX_GAP).bind(RATING_POOL)
        .fetch_optional(&mut *db).await?;
        let Some(pair) = pair else {
            return Ok(false);
        };
        let first: String = pair.try_get("first_id")?;
        let second: String = pair.try_get("second_id")?;
        let user: String = pair.try_get("second_user")?;
        let mut info = Self::info_on(db, &second, now).await?;
        info.status = "cancelled".into();
        info.matched_id = Some(first.clone());
        sqlx::query("UPDATE games SET matched_id=$2 WHERE id=$1")
            .bind(&second)
            .bind(&first)
            .execute(&mut *db)
            .await?;
        Self::persist(
            db,
            info,
            EventAction::Matched {
                game_id: first.clone(),
            },
            None,
            None,
            None,
        )
        .await?;
        Self::join_on(db, &first, &user, now).await?;
        Ok(true)
    }
    pub async fn matchmake(&self, clock: impl Into<Clock>) -> Result<()> {
        let clock = clock.into();
        for _ in 0..QUEUE_BATCH {
            let mut tx = self.pool.begin().await?;
            sqlx::query("SELECT pg_advisory_xact_lock($1)")
                .bind(QUEUE_LOCK)
                .execute(&mut *tx)
                .await?;
            let now = clock.resolve(&mut tx).await?;
            if !Self::match_queue_on(&mut tx, now).await? {
                return Ok(());
            }
            tx.commit().await?;
        }
        Ok(())
    }
    pub async fn join(&self, id: &str, user: &str, clock: impl Into<Clock>) -> Result<Snapshot> {
        let mut tx = self.pool.begin().await?;
        Self::lock_user(&mut tx, user).await?;
        Self::lock_game(&mut tx, id).await?;
        let now = clock.into().resolve(&mut tx).await?;
        let info = Self::info_on(&mut tx, id, now).await?;
        if info.queue_started_ms.is_some() && info.seat(user).is_none() {
            return Err(Error::Conflict(
                "This match uses automatic matchmaking.".into(),
            ));
        }
        Self::join_on(&mut tx, id, user, now).await?;
        let result = Self::snapshot_on(&mut tx, id, user, now).await?;
        tx.commit().await?;
        Ok(result)
    }
    async fn join_on(db: &mut PgConnection, id: &str, user: &str, now: i64) -> Result<()> {
        let mut info = Self::info_on(db, id, now).await?;
        if info.seat(user).is_some() {
            return Ok(());
        }
        if info.status != "waiting" {
            return Err(Error::Conflict("Both seats are occupied.".into()));
        }
        let player = Self::user_on(db, user).await?;
        if info.rated && player.guest {
            return Err(Error::Forbidden("Sign in to play rated matches.".into()));
        }
        Self::check_open(db, user, info.rated).await?;
        if info.white.is_none() {
            info.white = Some(player);
        } else {
            info.black = Some(player);
        }
        info.status = "active".into();
        info.running_since_ms = info.white_ms.map(|_| now);
        Self::persist(db, info, EventAction::Joined, None, None, None).await?;
        Ok(())
    }
    async fn lock_game(db: &mut PgConnection, id: &str) -> Result<()> {
        sqlx::query("SELECT id FROM games WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(db)
            .await?
            .ok_or(Error::NotFound)?;
        Ok(())
    }
    pub(crate) async fn info_on(db: &mut PgConnection, id: &str, now: i64) -> Result<MatchInfo> {
        let r=sqlx::query("SELECT id,seq,status,white_id,black_id,rated,size,rows,layout,result,reason,white_ms,black_ms,increment_ms,running_since_ms,turn,ready_white,ready_black,created_ms,queue_started_ms,matched_id FROM games WHERE id=$1").bind(id).fetch_optional(&mut *db).await?.ok_or(Error::NotFound)?;
        let player = |row: &PgRow, key: &str| row.try_get::<Option<String>, _>(key);
        let white = match player(&r, "white_id")? {
            Some(id) => Some(Self::user_on(db, &id).await?),
            None => None,
        };
        let black = match player(&r, "black_id")? {
            Some(id) => Some(Self::user_on(db, &id).await?),
            None => None,
        };
        Ok(MatchInfo {
            id: r.try_get("id")?,
            seq: r
                .try_get::<i32, _>("seq")?
                .try_into()
                .map_err(|_| Error::Internal("Invalid match sequence.".into()))?,
            status: r.try_get("status")?,
            white,
            black,
            rated: r.try_get("rated")?,
            size: r
                .try_get::<i16, _>("size")?
                .try_into()
                .map_err(|_| Error::Internal("Invalid grid size.".into()))?,
            layout: r.try_get("layout")?,
            rows: Some(
                r.try_get::<i16, _>("rows")?
                    .try_into()
                    .map_err(|_| Error::Internal("Invalid grid rows.".into()))?,
            ),
            result: r.try_get("result")?,
            reason: r.try_get("reason")?,
            white_ms: r.try_get("white_ms")?,
            black_ms: r.try_get("black_ms")?,
            increment_ms: r.try_get("increment_ms")?,
            running_since_ms: r.try_get("running_since_ms")?,
            turn: color(&r.try_get::<String, _>("turn")?)?,
            ready_white: r.try_get("ready_white")?,
            ready_black: r.try_get("ready_black")?,
            created_ms: r.try_get("created_ms")?,
            server_ms: now,
            queue_started_ms: r.try_get("queue_started_ms")?,
            matched_id: r.try_get("matched_id")?,
        })
    }
    pub async fn info(&self, id: &str, clock: impl Into<Clock>) -> Result<MatchInfo> {
        let mut db = self.pool.acquire().await?;
        let now = clock.into().resolve(&mut db).await?;
        Self::info_on(&mut db, id, now).await
    }
    async fn load(db: &mut PgConnection, id: &str) -> Result<Game> {
        let checkpoint: String = sqlx::query_scalar("SELECT checkpoint FROM games WHERE id=$1")
            .bind(id)
            .fetch_optional(db)
            .await?
            .ok_or(Error::NotFound)?;
        Ok(Game::from_checkpoint_json(&checkpoint)?)
    }
    async fn snapshot_on(
        db: &mut PgConnection,
        id: &str,
        user: &str,
        now: i64,
    ) -> Result<Snapshot> {
        let mut info = Self::info_on(db, id, now).await?;
        if info.seat(user).is_none() {
            return Err(Error::Forbidden(
                "Join this match before viewing it.".into(),
            ));
        }
        if let Some(target) = &info.matched_id {
            info = Self::info_on(db, target, now).await?;
        }
        let seat = info
            .seat(user)
            .ok_or_else(|| Error::Forbidden("Join this match before viewing it.".into()))?;
        let game = Self::load(db, &info.id).await?;
        Ok(Snapshot {
            info,
            seat,
            save: game.save_json()?,
            state: game.state(),
        })
    }
    pub async fn snapshot(
        &self,
        id: &str,
        user: &str,
        clock: impl Into<Clock>,
    ) -> Result<Snapshot> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let now = clock.into().resolve(&mut tx).await?;
        let result = Self::snapshot_on(&mut tx, id, user, now).await?;
        tx.commit().await?;
        Ok(result)
    }
    fn charge_clock(info: &mut MatchInfo, now: i64) {
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
    pub async fn command(
        &self,
        id: &str,
        user: &str,
        command: Command,
        clock: impl Into<Clock>,
    ) -> Result<MatchEvent> {
        Uuid::parse_str(&command.request_id)
            .map_err(|_| Error::Invalid("Invalid request ID.".into()))?;
        let mut tx = self.pool.begin().await?;
        Self::lock_game(&mut tx, id).await?;
        let now = clock.into().resolve(&mut tx).await?;
        let mut info = Self::info_on(&mut tx, id, now).await?;
        let seat = info
            .seat(user)
            .ok_or_else(|| Error::Forbidden("You are not a player in this match.".into()))?;
        let request = serde_json::to_string(&command)?;
        if let Some(row)=sqlx::query("SELECT actor_id,request_json,event_json FROM events WHERE game_id=$1 AND request_id=$2").bind(id).bind(&command.request_id).fetch_optional(&mut *tx).await? {
            if row.try_get::<String,_>("actor_id")?!=user||row.try_get::<String,_>("request_json")?!=request {return Err(Error::Conflict("That request ID was already used.".into()));}
            return Ok(serde_json::from_str(&row.try_get::<String,_>("event_json")?)?);
        }
        if info.seq != command.expected_seq {
            return Err(Error::Conflict(
                "Position changed. Reconnect to refresh it.".into(),
            ));
        }
        if info.status == "active" && info.remaining_ms(info.turn, now) == Some(0) {
            Self::expire(&mut tx, info, now).await?;
            tx.commit().await?;
            return Err(Error::Conflict("The match ended on time.".into()));
        }
        let mut candidate = None;
        let action = match &command.action {
            Action::Move { board, uci } => {
                if info.status != "active" {
                    return Err(Error::Conflict("The match is not active.".into()));
                }
                if info.turn != seat {
                    return Err(Error::Forbidden("It is your opponent's turn.".into()));
                }
                let mut game = Self::load(&mut tx, id).await?;
                game.play(*board, uci)?;
                Self::charge_clock(&mut info, now);
                match seat {
                    Color::White => info.white_ms = info.white_ms.map(|v| v + info.increment_ms),
                    Color::Black => info.black_ms = info.black_ms.map(|v| v + info.increment_ms),
                };
                info.turn = game.turn();
                match game.outcome() {
                    Outcome::White => Self::finish(&mut info, "white", "rules"),
                    Outcome::Black => Self::finish(&mut info, "black", "rules"),
                    Outcome::Draw => Self::finish(&mut info, "draw", "rules"),
                    Outcome::Ongoing => {}
                }
                candidate = Some(game);
                EventAction::Move {
                    board: *board,
                    uci: uci.clone(),
                }
            }
            Action::Resign => match info.status.as_str() {
                "waiting" => {
                    info.status = "cancelled".into();
                    EventAction::Cancelled
                }
                "active" | "paused" => {
                    Self::charge_clock(&mut info, now);
                    Self::finish(&mut info, color_name(seat.other()), "resignation");
                    EventAction::Resigned { color: seat }
                }
                _ => return Err(Error::Conflict("The match has ended.".into())),
            },
            Action::Resume => {
                if info.status != "paused" {
                    return Err(Error::Conflict("The match is not paused.".into()));
                }
                let ready = match seat {
                    Color::White => &mut info.ready_white,
                    Color::Black => &mut info.ready_black,
                };
                if *ready {
                    return Err(Error::Conflict("You are already ready.".into()));
                }
                *ready = true;
                if info.ready_white && info.ready_black {
                    info.status = "active".into();
                    info.running_since_ms = Some(now);
                }
                EventAction::Ready { color: seat }
            }
        };
        let event = Self::persist(
            &mut tx,
            info,
            action,
            Some(user),
            Some((&command.request_id, &request)),
            candidate,
        )
        .await?;
        tx.commit().await?;
        Ok(event)
    }
    pub(crate) async fn persist(
        db: &mut PgConnection,
        mut info: MatchInfo,
        action: EventAction,
        actor: Option<&str>,
        request: Option<(&str, &str)>,
        candidate: Option<Game>,
    ) -> Result<MatchEvent> {
        info.seq += 1;
        if info.status == "finished" && info.rated {
            let ids = [
                info.white
                    .as_ref()
                    .ok_or_else(|| Error::Internal("Missing White player.".into()))?
                    .id
                    .clone(),
                info.black
                    .as_ref()
                    .ok_or_else(|| Error::Internal("Missing Black player.".into()))?
                    .id
                    .clone(),
            ];
            sqlx::query("SELECT user_id FROM ratings WHERE user_id=ANY($1) AND pool=$2 ORDER BY user_id FOR UPDATE").bind(&ids[..]).bind(RATING_POOL).fetch_all(&mut *db).await?;
            let white = Self::user_on(db, &ids[0]).await?;
            let black = Self::user_on(db, &ids[1]).await?;
            let score = match info.result.as_deref() {
                Some("white") => 1.0,
                Some("black") => 0.0,
                Some("draw") => 0.5,
                _ => return Err(Error::Internal("Missing rated result.".into())),
            };
            let change =
                32.0 * (score - 1.0 / (1.0 + 10_f64.powf((black.rating - white.rating) / 400.0)));
            for (mut player, delta) in [(white, change), (black, -change)] {
                sqlx::query(
                    "INSERT INTO rating_changes(game_id,user_id,before,after) VALUES($1,$2,$3,$4)",
                )
                .bind(&info.id)
                .bind(&player.id)
                .bind(player.rating)
                .bind(player.rating + delta)
                .execute(&mut *db)
                .await?;
                player.rating += delta;
                player.rated_games += 1;
                sqlx::query(
                    "UPDATE ratings SET rating=$3,games=games+1 WHERE user_id=$1 AND pool=$2",
                )
                .bind(&player.id)
                .bind(RATING_POOL)
                .bind(player.rating)
                .execute(&mut *db)
                .await?;
                if player.id == ids[0] {
                    info.white = Some(player);
                } else {
                    info.black = Some(player);
                }
            }
        }
        let deadline = info.running_since_ms.and_then(|since| {
            match info.turn {
                Color::White => info.white_ms,
                Color::Black => info.black_ms,
            }
            .map(|remaining| since + remaining)
        });
        sqlx::query("UPDATE games SET seq=$2,status=$3,white_id=$4,black_id=$5,result=$6,reason=$7,white_ms=$8,black_ms=$9,running_since_ms=$10,turn=$11,ready_white=$12,ready_black=$13,deadline_ms=$14 WHERE id=$1")
            .bind(&info.id).bind(info.seq as i32).bind(&info.status).bind(info.white.as_ref().map(|u|&u.id)).bind(info.black.as_ref().map(|u|&u.id)).bind(&info.result).bind(&info.reason)
            .bind(info.white_ms).bind(info.black_ms).bind(info.running_since_ms).bind(color_name(info.turn)).bind(info.ready_white).bind(info.ready_black).bind(deadline).execute(&mut *db).await?;
        if let Some(game) = candidate {
            sqlx::query("UPDATE games SET checkpoint=$2 WHERE id=$1")
                .bind(&info.id)
                .bind(game.checkpoint_json()?)
                .execute(&mut *db)
                .await?;
        }
        let event = MatchEvent {
            seq: info.seq,
            action,
            info,
        };
        sqlx::query("INSERT INTO events(game_id,seq,actor_id,request_id,request_json,event_json) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(&event.info.id).bind(event.seq as i32).bind(actor).bind(request.map(|r|r.0)).bind(request.map(|r|r.1)).bind(serde_json::to_string(&event)?).execute(&mut *db).await?;
        sqlx::query("SELECT pg_notify($1,$2)")
            .bind(MATCH_CHANNEL)
            .bind(format!("{}:{}", event.info.id, event.seq))
            .execute(db)
            .await?;
        Ok(event)
    }
    async fn expire(db: &mut PgConnection, mut info: MatchInfo, now: i64) -> Result<()> {
        let loser = info.turn;
        Self::charge_clock(&mut info, now);
        Self::finish(&mut info, color_name(loser.other()), "timeout");
        Self::persist(
            db,
            info,
            EventAction::Timeout { color: loser },
            None,
            None,
            None,
        )
        .await?;
        Ok(())
    }
    pub async fn tick(&self, clock: impl Into<Clock>) -> Result<()> {
        let clock = clock.into();
        for _ in 0..CLOCK_BATCH {
            let mut tx = self.pool.begin().await?;
            let now = clock.resolve(&mut tx).await?;
            let id:Option<String>=sqlx::query_scalar("SELECT id FROM games WHERE status='active' AND deadline_ms<=$1 ORDER BY deadline_ms LIMIT 1 FOR UPDATE SKIP LOCKED").bind(now).fetch_optional(&mut *tx).await?;
            let Some(id) = id else {
                return Ok(());
            };
            let info = Self::info_on(&mut tx, &id, now).await?;
            Self::expire(&mut tx, info, now).await?;
            tx.commit().await?;
        }
        Ok(())
    }
    pub async fn open_games(&self, user: &str, clock: impl Into<Clock>) -> Result<Vec<MatchInfo>> {
        let mut db = self.pool.acquire().await?;
        let now = clock.into().resolve(&mut db).await?;
        let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM games WHERE (white_id=$1 OR black_id=$1) AND status IN ('waiting','active','paused') ORDER BY ordinal DESC").bind(user).fetch_all(&mut *db).await?;
        let mut result = Vec::new();
        for id in ids {
            result.push(Self::info_on(&mut db, &id, now).await?);
        }
        Ok(result)
    }
    pub async fn history(
        &self,
        user: &str,
        before: Option<i64>,
        clock: impl Into<Clock>,
    ) -> Result<Vec<HistoryEntry>> {
        let mut db = self.pool.acquire().await?;
        let now = clock.into().resolve(&mut db).await?;
        let rows=sqlx::query("SELECT g.ordinal,g.id,r.before,r.after FROM games g LEFT JOIN rating_changes r ON r.game_id=g.id AND r.user_id=$1 WHERE (g.white_id=$1 OR g.black_id=$1) AND g.status='finished' AND g.ordinal<$2 ORDER BY g.ordinal DESC LIMIT 20").bind(user).bind(before.unwrap_or(i64::MAX)).fetch_all(&mut *db).await?;
        let mut result = Vec::new();
        for row in rows {
            result.push(HistoryEntry {
                cursor: row.try_get("ordinal")?,
                info: Self::info_on(&mut db, &row.try_get::<String, _>("id")?, now).await?,
                rating_before: row.try_get("before")?,
                rating_after: row.try_get("after")?,
            });
        }
        Ok(result)
    }
    pub async fn events_after(&self, id: &str, seq: u32) -> Result<Vec<MatchEvent>> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT event_json FROM events WHERE game_id=$1 AND seq>$2 ORDER BY seq LIMIT 256",
        )
        .bind(id)
        .bind(seq as i32)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|value| serde_json::from_str(&value).map_err(Error::from))
            .collect()
    }
    pub async fn versions(&self, ids: &[String]) -> Result<Vec<(String, u32)>> {
        let rows: Vec<(String, i32)> = sqlx::query_as("SELECT id,seq FROM games WHERE id=ANY($1)")
            .bind(ids)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|(id, seq)| {
                Ok((
                    id,
                    seq.try_into()
                        .map_err(|_| Error::Internal("Invalid match sequence.".into()))?,
                ))
            })
            .collect()
    }
}

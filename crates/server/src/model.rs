use nested_chess::{Color, State};
use serde::{Deserialize, Serialize};

pub const RATING_POOL: &str = "classical-2x2-15+10";
pub const SESSION_SECONDS: i64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub handle: String,
    pub guest: bool,
    pub rating: f64,
    pub rated_games: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeControl {
    pub seconds: u32,
    pub increment: u32,
}

impl Default for TimeControl {
    fn default() -> Self {
        Self {
            seconds: 900,
            increment: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CreateGame {
    pub size: u8,
    pub layout: String,
    pub rated: bool,
    pub clock: Option<TimeControl>,
}

impl Default for CreateGame {
    fn default() -> Self {
        Self {
            size: 2,
            layout: "classical".into(),
            rated: false,
            clock: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Move { board: usize, uci: String },
    Resign,
    Resume,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub request_id: String,
    pub expected_seq: u32,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventAction {
    Move { board: usize, uci: String },
    Joined,
    Resigned { color: Color },
    Timeout { color: Color },
    Paused,
    Ready { color: Color },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchInfo {
    pub id: String,
    pub seq: u32,
    pub status: String,
    pub white: Option<User>,
    pub black: Option<User>,
    pub rated: bool,
    pub size: u8,
    pub layout: String,
    pub result: Option<String>,
    pub reason: Option<String>,
    pub white_ms: Option<i64>,
    pub black_ms: Option<i64>,
    pub increment_ms: i64,
    pub running_since_ms: Option<i64>,
    pub turn: Color,
    pub ready_white: bool,
    pub ready_black: bool,
    pub created_ms: i64,
    pub server_ms: i64,
}

impl MatchInfo {
    pub fn seat(&self, user: &str) -> Option<Color> {
        if self.white.as_ref().is_some_and(|p| p.id == user) {
            Some(Color::White)
        } else if self.black.as_ref().is_some_and(|p| p.id == user) {
            Some(Color::Black)
        } else {
            None
        }
    }

    pub fn remaining_ms(&self, color: Color, now: i64) -> Option<i64> {
        let remaining = match color {
            Color::White => self.white_ms,
            Color::Black => self.black_ms,
        }?;
        Some(
            (remaining
                - if self.turn == color {
                    self.running_since_ms
                        .map_or(0, |since| (now - since).max(0))
                } else {
                    0
                })
            .max(0),
        )
    }
}

#[derive(Serialize)]
pub struct Snapshot {
    pub info: MatchInfo,
    pub seat: Color,
    pub save: String,
    pub state: State,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchEvent {
    pub seq: u32,
    pub action: EventAction,
    pub info: MatchInfo,
}

#[derive(Serialize)]
pub struct HistoryEntry {
    pub cursor: i64,
    pub info: MatchInfo,
    pub rating_before: Option<f64>,
    pub rating_after: Option<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("Sign in to continue.")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("Match not found.")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("Finish your guest matches before switching accounts.")]
    GuestMatchesOpen,
    #[error("That login already belongs to another account.")]
    IdentityInUse,
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Database migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("Invalid saved match: {0}")]
    Engine(#[from] nested_chess::Error),
    #[error("Invalid stored JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;

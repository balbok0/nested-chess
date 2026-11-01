use crate::{
    model::*,
    store::{Clock, Store},
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

pub const ACTIVITY_WINDOW_MS: i64 = 90_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityMode {
    Online,
    Engine,
    SameDevice,
}
impl ActivityMode {
    fn name(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Engine => "engine",
            Self::SameDevice => "same_device",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub client_id: String,
    pub sequence: i64,
    pub mode: ActivityMode,
    pub game_id: Option<String>,
    pub playing: bool,
    pub visible: bool,
}

#[derive(Debug, Serialize)]
pub struct ModeUsage {
    pub mode: ActivityMode,
    pub clients: i64,
    pub games: i64,
}
#[derive(Debug, Serialize)]
pub struct Usage {
    pub as_of_ms: i64,
    pub window_seconds: i64,
    pub active_clients: i64,
    pub signed_in_accounts: i64,
    pub online_in_progress: i64,
    pub modes: Vec<ModeUsage>,
}

impl Store {
    /// Record current foreground activity without storing game positions.
    pub async fn record_activity(
        &self,
        activity: Activity,
        user: Option<&User>,
        clock: impl Into<Clock>,
    ) -> Result<()> {
        let client_id = Uuid::parse_str(&activity.client_id)
            .map_err(|_| Error::Invalid("Invalid activity client ID.".into()))?
            .to_string();
        if activity.sequence < 1 {
            return Err(Error::Invalid("Activity sequence must be positive.".into()));
        }
        let game_id = activity
            .game_id
            .as_deref()
            .map(|id| {
                Uuid::parse_str(id)
                    .map(|id| id.to_string())
                    .map_err(|_| Error::Invalid("Invalid activity game ID.".into()))
            })
            .transpose()?;
        if activity.visible {
            if activity.mode == ActivityMode::Online {
                let user = user.ok_or(Error::Unauthorized)?;
                let id = game_id
                    .as_deref()
                    .ok_or_else(|| Error::Invalid("Online activity needs a game ID.".into()))?;
                let participant: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM games WHERE id=$1 AND (white_id=$2 OR black_id=$2))")
                    .bind(id).bind(&user.id).fetch_one(&self.pool).await?;
                if !participant {
                    return Err(Error::Forbidden(
                        "That match is not available to this account.".into(),
                    ));
                }
            } else if game_id.is_some() {
                return Err(Error::Invalid(
                    "Local activity must not include an online game ID.".into(),
                ));
            }
        }
        let mut db = self.pool.acquire().await?;
        let now = clock.into().resolve(&mut db).await?;
        sqlx::query("INSERT INTO activity_presence(client_id,sequence,mode,game_id,user_id,playing,visible,seen_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(client_id) DO UPDATE SET sequence=excluded.sequence,mode=excluded.mode,game_id=excluded.game_id,user_id=excluded.user_id,playing=excluded.playing,visible=excluded.visible,seen_ms=excluded.seen_ms WHERE activity_presence.sequence<excluded.sequence")
            .bind(client_id).bind(activity.sequence).bind(activity.mode.name()).bind(game_id)
            .bind(user.filter(|user| !user.guest).map(|user| user.id.as_str()))
            .bind(activity.playing).bind(activity.visible).bind(now).execute(&mut *db).await?;
        Ok(())
    }

    /// Count foreground clients and games within the activity window.
    pub async fn usage(&self, clock: impl Into<Clock>) -> Result<Usage> {
        let mut db = self.pool.acquire().await?;
        let now = clock.into().resolve(&mut db).await?;
        let rows = sqlx::query("SELECT p.mode,COUNT(*) AS clients,COUNT(DISTINCT p.user_id) AS accounts,COUNT(DISTINCT p.game_id) FILTER(WHERE p.playing AND g.status='active') + COUNT(*) FILTER(WHERE p.playing AND p.mode!='online') AS games FROM activity_presence p LEFT JOIN games g ON g.id=p.game_id WHERE p.visible AND p.seen_ms>$1 GROUP BY GROUPING SETS((p.mode),())")
            .bind(now-ACTIVITY_WINDOW_MS).fetch_all(&mut *db).await?;
        let mut usage = Usage {
            as_of_ms: now,
            window_seconds: ACTIVITY_WINDOW_MS / 1000,
            active_clients: 0,
            signed_in_accounts: 0,
            online_in_progress: sqlx::query_scalar(
                "SELECT COUNT(*) FROM games WHERE status='active'",
            )
            .fetch_one(&mut *db)
            .await?,
            modes: [
                ActivityMode::Online,
                ActivityMode::Engine,
                ActivityMode::SameDevice,
            ]
            .into_iter()
            .map(|mode| ModeUsage {
                mode,
                clients: 0,
                games: 0,
            })
            .collect(),
        };
        for row in rows {
            if let Some(mode) = row.try_get::<Option<String>, _>("mode")? {
                let entry = usage
                    .modes
                    .iter_mut()
                    .find(|entry| entry.mode.name() == mode)
                    .ok_or_else(|| Error::Internal("Invalid activity mode.".into()))?;
                entry.clients = row.try_get("clients")?;
                entry.games = row.try_get("games")?;
            } else {
                usage.active_clients = row.try_get("clients")?;
                usage.signed_in_accounts = row.try_get("accounts")?;
            }
        }
        Ok(usage)
    }

    /// Remove expired activity records.
    pub async fn expire_activity(&self, clock: impl Into<Clock>) -> Result<()> {
        let mut db = self.pool.acquire().await?;
        let now = clock.into().resolve(&mut db).await?;
        sqlx::query("DELETE FROM activity_presence WHERE seen_ms<=$1")
            .bind(now - ACTIVITY_WINDOW_MS)
            .execute(&mut *db)
            .await?;
        Ok(())
    }
}

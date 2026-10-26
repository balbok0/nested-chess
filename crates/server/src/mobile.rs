use crate::{
    auth::LoginStart,
    model::{Error, Result, User},
    store::{Clock, Store, hash, secret},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::Row;

pub const APP_CALLBACK: &str = "io.github.balbok0.nestedchess://auth";
const LOGIN_MS: i64 = 600_000;
const HANDOFF_MS: i64 = 60_000;
const STREAM_TICKET_MS: i64 = 30_000;
#[derive(Serialize)]
pub struct MobileSession {
    pub user: User,
    pub token: String,
}
pub struct MobileLogin {
    pub id: String,
    pub state: String,
    pub session_hash: Option<String>,
}
fn valid_challenge(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

impl Store {
    pub async fn request_mobile_login(
        &self,
        provider: &str,
        challenge: &str,
        state: &str,
        session: Option<&str>,
        linking: bool,
        clock: impl Into<Clock>,
    ) -> Result<String> {
        if !valid_challenge(challenge) || !valid_challenge(state) {
            return Err(Error::Invalid(
                "Invalid app sign-in challenge or state.".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        let now = clock.into().resolve(&mut tx).await?;
        let user = match session {
            Some(token) => Some(Self::session_on(&mut tx, &hash(token), now).await?),
            None => None,
        };
        if linking && user.is_none() {
            return Err(Error::Unauthorized);
        }
        sqlx::query("DELETE FROM mobile_auth WHERE expires_ms<=$1")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        let ticket = secret();
        sqlx::query("INSERT INTO mobile_auth(hash,provider,challenge,client_state,user_id,session_hash,linking,expires_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(hash(&ticket)).bind(provider).bind(challenge).bind(state).bind(user.map(|u|u.id)).bind(session.map(hash)).bind(linking).bind(now+LOGIN_MS).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(ticket)
    }
    pub async fn start_mobile_login(
        &self,
        ticket: &str,
        expected_provider: &str,
        start: &LoginStart,
        clock: impl Into<Clock>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT provider,user_id,session_hash,linking,expires_ms FROM mobile_auth WHERE hash=$1 AND state_hash IS NULL FOR UPDATE").bind(hash(ticket)).fetch_optional(&mut *tx).await?.ok_or(Error::Unauthorized)?;
        let now = clock.into().resolve(&mut tx).await?;
        if row.try_get::<String, _>("provider")? != expected_provider
            || row.try_get::<i64, _>("expires_ms")? <= now
        {
            return Err(Error::Unauthorized);
        }
        if let Some(session) = row.try_get::<Option<String>, _>("session_hash")? {
            Self::session_on(&mut tx, &session, now).await?;
        }
        sqlx::query("INSERT INTO auth_flows(hash,browser_hash,provider,verifier,nonce,user_id,linking,expires_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(hash(&start.state)).bind(hash(&start.browser_token)).bind(expected_provider).bind(&start.verifier).bind(&start.nonce).bind(row.try_get::<Option<String>,_>("user_id")?).bind(row.try_get::<bool,_>("linking")?).bind(now+LOGIN_MS).execute(&mut *tx).await?;
        sqlx::query("UPDATE mobile_auth SET state_hash=$2 WHERE hash=$1")
            .bind(hash(ticket))
            .bind(hash(&start.state))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn mobile_login(&self, state: &str) -> Result<Option<MobileLogin>> {
        let row = sqlx::query(
            "SELECT hash,client_state,session_hash FROM mobile_auth WHERE state_hash=$1",
        )
        .bind(hash(state))
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            Ok(MobileLogin {
                id: r.try_get("hash")?,
                state: r.try_get("client_state")?,
                session_hash: r.try_get("session_hash")?,
            })
        })
        .transpose()
    }
    pub async fn complete_mobile_login(
        &self,
        id: &str,
        user: Option<&str>,
        error: Option<&str>,
        clock: impl Into<Clock>,
    ) -> Result<String> {
        if user.is_some() == error.is_some() {
            return Err(Error::Invalid("A sign-in result is required.".into()));
        }
        let mut db = self.pool.acquire().await?;
        let now = clock.into().resolve(&mut db).await?;
        let code = secret();
        let result=sqlx::query("UPDATE mobile_auth SET result_user=$2,error=$3,code_hash=$4,expires_ms=$5 WHERE hash=$1 AND code_hash IS NULL AND state_hash IS NOT NULL AND expires_ms>$6").bind(id).bind(user).bind(error).bind(hash(&code)).bind(now+HANDOFF_MS).bind(now).execute(&mut *db).await?;
        if result.rows_affected() != 1 {
            return Err(Error::Unauthorized);
        }
        Ok(code)
    }
    pub async fn exchange_mobile_login(
        &self,
        code: &str,
        verifier: &str,
        clock: impl Into<Clock>,
    ) -> Result<MobileSession> {
        if !(43..=128).contains(&verifier.len())
            || !verifier
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-._~".contains(&c))
        {
            return Err(Error::Invalid("Invalid app sign-in verifier.".into()));
        }
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT challenge,result_user,error,session_hash,expires_ms FROM mobile_auth WHERE code_hash=$1 FOR UPDATE").bind(hash(code)).fetch_optional(&mut *tx).await?.ok_or(Error::Unauthorized)?;
        let now = clock.into().resolve(&mut tx).await?;
        if row.try_get::<i64, _>("expires_ms")? <= now
            || URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
                != row.try_get::<String, _>("challenge")?
        {
            return Err(Error::Unauthorized);
        }
        let old: Option<String> = row.try_get("session_hash")?;
        if let Some(old) = &old {
            Self::session_on(&mut tx, old, now).await?;
        }
        sqlx::query("DELETE FROM mobile_auth WHERE code_hash=$1")
            .bind(hash(code))
            .execute(&mut *tx)
            .await?;
        if let Some(error) = row.try_get::<Option<String>, _>("error")? {
            tx.commit().await?;
            return Err(match error.as_str() {
                "guest_matches" => Error::GuestMatchesOpen,
                "identity_in_use" => Error::IdentityInUse,
                _ => Error::Forbidden("Sign-in was not completed. Please try again.".into()),
            });
        }
        let id = row
            .try_get::<Option<String>, _>("result_user")?
            .ok_or(Error::Unauthorized)?;
        let token = Self::new_session_on(&mut tx, &id, now).await?;
        if let Some(old) = old {
            sqlx::query("DELETE FROM sessions WHERE hash=$1")
                .bind(old)
                .execute(&mut *tx)
                .await?;
        }
        let user = Self::user_on(&mut tx, &id).await?;
        tx.commit().await?;
        Ok(MobileSession { user, token })
    }
    pub async fn stream_ticket(
        &self,
        game: &str,
        session: &str,
        clock: impl Into<Clock>,
    ) -> Result<String> {
        let mut tx = self.pool.begin().await?;
        let now = clock.into().resolve(&mut tx).await?;
        let user = Self::session_on(&mut tx, &hash(session), now).await?;
        Self::info_on(&mut tx, game, now)
            .await?
            .seat(&user.id)
            .ok_or_else(|| Error::Forbidden("Join this match before viewing it.".into()))?;
        sqlx::query("DELETE FROM stream_tickets WHERE expires_ms<=$1")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        let ticket = secret();
        sqlx::query(
            "INSERT INTO stream_tickets(hash,game_id,session_hash,expires_ms) VALUES($1,$2,$3,$4)",
        )
        .bind(hash(&ticket))
        .bind(game)
        .bind(hash(session))
        .bind(now + STREAM_TICKET_MS)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(ticket)
    }
    pub async fn take_stream_ticket(
        &self,
        game: &str,
        ticket: &str,
        clock: impl Into<Clock>,
    ) -> Result<(User, String)> {
        let mut tx = self.pool.begin().await?;
        let now = clock.into().resolve(&mut tx).await?;
        let session:String=sqlx::query_scalar("DELETE FROM stream_tickets WHERE hash=$1 AND game_id=$2 AND expires_ms>$3 RETURNING session_hash").bind(hash(ticket)).bind(game).bind(now).fetch_optional(&mut *tx).await?.ok_or(Error::Unauthorized)?;
        let user = Self::session_on(&mut tx, &session, now).await?;
        tx.commit().await?;
        Ok((user, session))
    }
}

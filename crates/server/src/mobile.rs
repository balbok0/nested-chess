use crate::{
    auth::LoginStart,
    model::{Error, Result, SESSION_SECONDS, User},
    store::{Store, hash, secret},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

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
    pub fn request_mobile_login(
        &mut self,
        provider: &str,
        challenge: &str,
        state: &str,
        session: Option<&str>,
        linking: bool,
        now: i64,
    ) -> Result<String> {
        if !valid_challenge(challenge) || !valid_challenge(state) {
            return Err(Error::Invalid(
                "Invalid app sign-in challenge or state.".into(),
            ));
        }
        let user = session.map(|token| self.session(token, now)).transpose()?;
        if linking && user.is_none() {
            return Err(Error::Unauthorized);
        }
        self.db
            .execute("DELETE FROM mobile_auth WHERE expires_ms<=?1", [now])?;
        let ticket = secret();
        self.db.execute("INSERT INTO mobile_auth(hash,provider,challenge,client_state,user_id,session_hash,linking,expires_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![hash(&ticket), provider, challenge, state, user.map(|u|u.id), session.map(hash), linking, now+LOGIN_MS])?;
        Ok(ticket)
    }

    pub fn start_mobile_login(
        &mut self,
        ticket: &str,
        expected_provider: &str,
        start: &LoginStart,
        now: i64,
    ) -> Result<()> {
        let tx = self.db.transaction()?;
        let (provider, user, session, linking): (String, Option<String>, Option<String>, bool) = tx.query_row("SELECT provider,user_id,session_hash,linking FROM mobile_auth WHERE hash=?1 AND expires_ms>?2 AND state_hash IS NULL", params![hash(ticket),now], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?.ok_or(Error::Unauthorized)?;
        if provider != expected_provider {
            return Err(Error::Unauthorized);
        }
        if let Some(session) = session {
            let live: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE hash=?1 AND expires_ms>?2)",
                params![session, now],
                |row| row.get(0),
            )?;
            if !live {
                return Err(Error::Unauthorized);
            }
        }
        tx.execute("INSERT INTO auth_flows(hash,browser_hash,provider,verifier,nonce,user_id,linking,expires_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![hash(&start.state),hash(&start.browser_token),provider,start.verifier,start.nonce,user,linking,now+LOGIN_MS])?;
        tx.execute(
            "UPDATE mobile_auth SET state_hash=?2 WHERE hash=?1",
            params![hash(ticket), hash(&start.state)],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn mobile_login(&self, state: &str) -> Result<Option<MobileLogin>> {
        Ok(self
            .db
            .query_row(
                "SELECT hash,client_state,session_hash FROM mobile_auth WHERE state_hash=?1",
                [hash(state)],
                |row| {
                    Ok(MobileLogin {
                        id: row.get(0)?,
                        state: row.get(1)?,
                        session_hash: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn complete_mobile_login(
        &mut self,
        id: &str,
        user: Option<&str>,
        error: Option<&str>,
        now: i64,
    ) -> Result<String> {
        if user.is_some() == error.is_some() {
            return Err(Error::Invalid("A sign-in result is required.".into()));
        }
        let code = secret();
        let changed = self.db.execute("UPDATE mobile_auth SET result_user=?2,error=?3,code_hash=?4,expires_ms=?5 WHERE hash=?1 AND code_hash IS NULL AND state_hash IS NOT NULL AND expires_ms>?6",params![id,user,error,hash(&code),now+HANDOFF_MS,now])?;
        if changed != 1 {
            return Err(Error::Unauthorized);
        }
        Ok(code)
    }

    pub fn exchange_mobile_login(
        &mut self,
        code: &str,
        verifier: &str,
        now: i64,
    ) -> Result<MobileSession> {
        if !(43..=128).contains(&verifier.len())
            || !verifier
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-._~".contains(&c))
        {
            return Err(Error::Invalid("Invalid app sign-in verifier.".into()));
        }
        let tx = self.db.transaction()?;
        let (challenge,user,error,old): (String,Option<String>,Option<String>,Option<String>) = tx.query_row("SELECT challenge,result_user,error,session_hash FROM mobile_auth WHERE code_hash=?1 AND expires_ms>?2",params![hash(code),now], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?.ok_or(Error::Unauthorized)?;
        if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != challenge {
            return Err(Error::Unauthorized);
        }
        if let Some(old) = &old {
            let live: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE hash=?1 AND expires_ms>?2)",
                params![old, now],
                |row| row.get(0),
            )?;
            if !live {
                return Err(Error::Unauthorized);
            }
        }
        tx.execute("DELETE FROM mobile_auth WHERE code_hash=?1", [hash(code)])?;
        if let Some(error) = error {
            tx.commit()?;
            return Err(match error.as_str() {
                "guest_matches" => Error::GuestMatchesOpen,
                "identity_in_use" => Error::IdentityInUse,
                _ => Error::Forbidden("Sign-in was not completed. Please try again.".into()),
            });
        }
        let user = user.ok_or(Error::Unauthorized)?;
        let token = secret();
        tx.execute(
            "INSERT INTO sessions(hash,user_id,expires_ms) VALUES(?1,?2,?3)",
            params![hash(&token), user, now + SESSION_SECONDS * 1000],
        )?;
        if let Some(old) = old {
            tx.execute("DELETE FROM sessions WHERE hash=?1", [old])?;
        }
        tx.commit()?;
        Ok(MobileSession {
            user: self.user(&user)?,
            token,
        })
    }

    pub fn stream_ticket(&mut self, game: &str, session: &str, now: i64) -> Result<String> {
        let user = self.session(session, now)?;
        self.snapshot(game, &user.id, now)?;
        self.db
            .execute("DELETE FROM stream_tickets WHERE expires_ms<=?1", [now])?;
        let ticket = secret();
        self.db.execute(
            "INSERT INTO stream_tickets(hash,game_id,session_hash,expires_ms) VALUES(?1,?2,?3,?4)",
            params![hash(&ticket), game, hash(session), now + STREAM_TICKET_MS],
        )?;
        Ok(ticket)
    }

    pub fn take_stream_ticket(
        &mut self,
        game: &str,
        ticket: &str,
        now: i64,
    ) -> Result<(User, String)> {
        let tx = self.db.transaction()?;
        let session:String = tx.query_row("SELECT session_hash FROM stream_tickets WHERE hash=?1 AND game_id=?2 AND expires_ms>?3",params![hash(ticket),game,now],|row|row.get(0)).optional()?.ok_or(Error::Unauthorized)?;
        tx.execute("DELETE FROM stream_tickets WHERE hash=?1", [hash(ticket)])?;
        tx.commit()?;
        Ok((self.session_hash(&session, now)?, session))
    }
}

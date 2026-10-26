use crate::{
    model::{Error, Result, User},
    store::{Clock, Store, hash, secret},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse as OAuthTokenResponse, TokenUrl,
    basic::BasicClient,
};
use openidconnect::{
    AccessTokenHash, IssuerUrl, Nonce, TokenResponse as OidcTokenResponse,
    core::{CoreClient, CoreJsonWebKeySet, CoreProviderMetadata},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, Row};
use std::{collections::HashMap, time::Duration};
use url::Url;
use uuid::Uuid;

pub struct Provider {
    name: &'static str,
    client_id: String,
    client_secret: String,
}

impl Provider {
    fn authorize(&self) -> &'static str {
        match self.name {
            "google" => "https://accounts.google.com/o/oauth2/v2/auth",
            "github" => "https://github.com/login/oauth/authorize",
            _ => "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        }
    }
    fn token(&self) -> &'static str {
        match self.name {
            "google" => "https://oauth2.googleapis.com/token",
            "github" => "https://github.com/login/oauth/access_token",
            _ => "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        }
    }
}

pub struct Auth {
    pub public_url: Url,
    providers: HashMap<String, Provider>,
    http: reqwest::Client,
}

#[derive(Serialize)]
pub struct ProviderView {
    pub id: String,
    pub label: String,
}
pub struct LoginStart {
    pub url: String,
    pub browser_token: String,
    pub state: String,
    pub verifier: String,
    pub nonce: String,
}
pub struct AuthFlow {
    pub provider: String,
    pub verifier: String,
    pub nonce: String,
    pub user_id: Option<String>,
    pub linking: bool,
}
pub struct Identity {
    pub issuer: String,
    pub subject: String,
    pub name: String,
}

impl Auth {
    pub fn new(public_url: &str, credentials: Vec<(String, String, String)>) -> Result<Self> {
        let public_url = Url::parse(public_url)
            .map_err(|_| Error::Invalid("PUBLIC_URL must be an absolute URL.".into()))?;
        if !matches!(public_url.scheme(), "http" | "https")
            || public_url.host_str().is_none()
            || public_url.path() != "/"
            || public_url.query().is_some()
            || public_url.fragment().is_some()
        {
            return Err(Error::Invalid(
                "PUBLIC_URL must be the app origin, without a path or query.".into(),
            ));
        }
        if public_url.scheme() != "https"
            && !matches!(
                public_url.host_str(),
                Some("localhost" | "127.0.0.1" | "[::1]")
            )
        {
            return Err(Error::Invalid(
                "PUBLIC_URL must use HTTPS outside localhost.".into(),
            ));
        }
        let mut providers = HashMap::new();
        for (name, client_id, client_secret) in credentials {
            let name = match name.as_str() {
                "google" => "google",
                "github" => "github",
                "microsoft" => "microsoft",
                _ => return Err(Error::Invalid("Unknown login provider.".into())),
            };
            if client_id.is_empty() || client_secret.is_empty() {
                return Err(Error::Invalid(
                    "Both provider client ID and secret are required.".into(),
                ));
            }
            providers.insert(
                name.into(),
                Provider {
                    name,
                    client_id,
                    client_secret,
                },
            );
        }
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .user_agent("nested-chess")
            .build()
            .map_err(|_| Error::Internal("Could not initialize the login HTTP client.".into()))?;
        Ok(Self {
            public_url,
            providers,
            http,
        })
    }

    pub fn providers(&self) -> Vec<ProviderView> {
        ["google", "github", "microsoft"]
            .into_iter()
            .filter(|name| self.providers.contains_key(*name))
            .map(|name| ProviderView {
                id: name.into(),
                label: match name {
                    "google" => "Google",
                    "github" => "GitHub",
                    _ => "Microsoft",
                }
                .into(),
            })
            .collect()
    }

    fn provider(&self, name: &str) -> Result<&Provider> {
        self.providers
            .get(name)
            .ok_or_else(|| Error::Invalid("That login provider is unavailable.".into()))
    }
    fn redirect(&self, name: &str) -> Result<RedirectUrl> {
        RedirectUrl::new(format!("{}auth/{name}/callback", self.public_url))
            .map_err(|_| Error::Internal("Invalid callback URL.".into()))
    }

    pub fn begin(&self, name: &str) -> Result<LoginStart> {
        let p = self.provider(name)?;
        let client = BasicClient::new(ClientId::new(p.client_id.clone()))
            .set_auth_uri(AuthUrl::new(p.authorize().into()).unwrap())
            .set_redirect_uri(self.redirect(name)?);
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let nonce = secret();
        let mut request = client
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(challenge);
        if name != "github" {
            request = request
                .add_scope(Scope::new("openid".into()))
                .add_scope(Scope::new("profile".into()))
                .add_extra_param("nonce", nonce.clone());
        }
        let (url, state) = request.url();
        Ok(LoginStart {
            url: url.into(),
            browser_token: secret(),
            state: state.secret().clone(),
            verifier: verifier.secret().clone(),
            nonce,
        })
    }

    pub async fn identity(&self, flow: &AuthFlow, code: String) -> Result<Identity> {
        let p = self.provider(&flow.provider)?;
        let failure =
            || Error::Forbidden("Sign-in could not be verified. Please try again.".into());
        if p.name == "github" {
            let client = BasicClient::new(ClientId::new(p.client_id.clone()))
                .set_client_secret(ClientSecret::new(p.client_secret.clone()))
                .set_token_uri(TokenUrl::new(p.token().into()).unwrap())
                .set_auth_type(AuthType::RequestBody)
                .set_redirect_uri(self.redirect(p.name)?);
            let token = client
                .exchange_code(AuthorizationCode::new(code))
                .set_pkce_verifier(PkceCodeVerifier::new(flow.verifier.clone()))
                .request_async(&self.http)
                .await
                .map_err(|_| failure())?;
            #[derive(Deserialize)]
            struct GitHubUser {
                id: u64,
                login: String,
            }
            let user = self
                .http
                .get("https://api.github.com/user")
                .header("Accept", "application/vnd.github+json")
                .bearer_auth(token.access_token().secret())
                .send()
                .await
                .map_err(|_| failure())?
                .error_for_status()
                .map_err(|_| failure())?
                .json::<GitHubUser>()
                .await
                .map_err(|_| failure())?;
            return Ok(Identity {
                issuer: "github".into(),
                subject: user.id.to_string(),
                name: user.login,
            });
        }
        let exchange = CoreClient::new(
            ClientId::new(p.client_id.clone()),
            IssuerUrl::new("https://accounts.google.com".into()).unwrap(),
            CoreJsonWebKeySet::default(),
        )
        .set_client_secret(ClientSecret::new(p.client_secret.clone()))
        .set_token_uri(TokenUrl::new(p.token().into()).unwrap())
        .set_auth_type(AuthType::RequestBody)
        .set_redirect_uri(self.redirect(p.name)?);
        let token = exchange
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(PkceCodeVerifier::new(flow.verifier.clone()))
            .request_async(&self.http)
            .await
            .map_err(|_| failure())?;
        let id_token = token.id_token().ok_or_else(failure)?;
        let issuer = if p.name == "google" {
            "https://accounts.google.com".to_owned()
        } else {
            // An unverified tenant selects a fixed Microsoft discovery URL; claims are verified below.
            let payload = id_token
                .to_string()
                .split('.')
                .nth(1)
                .map(str::to_owned)
                .ok_or_else(failure)?;
            let claims: serde_json::Value =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).map_err(|_| failure())?)
                    .map_err(|_| failure())?;
            let tenant = Uuid::parse_str(claims["tid"].as_str().ok_or_else(failure)?)
                .map_err(|_| failure())?;
            format!("https://login.microsoftonline.com/{tenant}/v2.0")
        };
        let metadata = CoreProviderMetadata::discover_async(
            IssuerUrl::new(issuer.clone()).map_err(|_| failure())?,
            &self.http,
        )
        .await
        .map_err(|_| failure())?;
        let verifier =
            CoreClient::from_provider_metadata(metadata, ClientId::new(p.client_id.clone()), None);
        let claims = id_token
            .claims(
                &verifier.id_token_verifier(),
                &Nonce::new(flow.nonce.clone()),
            )
            .map_err(|_| failure())?;
        if let Some(expected) = claims.access_token_hash() {
            let actual = AccessTokenHash::from_token(
                token.access_token(),
                id_token.signing_alg().map_err(|_| failure())?,
                id_token
                    .signing_key(&verifier.id_token_verifier())
                    .map_err(|_| failure())?,
            )
            .map_err(|_| failure())?;
            if actual != *expected {
                return Err(failure());
            }
        }
        let name = claims
            .name()
            .and_then(|value| value.get(None))
            .map(|value| value.as_str())
            .unwrap_or(p.name);
        Ok(Identity {
            issuer,
            subject: claims.subject().as_str().into(),
            name: name.into(),
        })
    }
}

impl Store {
    pub async fn begin_auth(
        &self,
        start: &LoginStart,
        provider: &str,
        user_id: Option<&str>,
        linking: bool,
        clock: impl Into<Clock>,
    ) -> Result<()> {
        if linking && user_id.is_none() {
            return Err(Error::Unauthorized);
        }
        let mut db = self.pool.acquire().await?;
        let now = clock.into().resolve(&mut db).await?;
        sqlx::query("DELETE FROM auth_flows WHERE expires_ms<=$1")
            .bind(now)
            .execute(&mut *db)
            .await?;
        sqlx::query("INSERT INTO auth_flows(hash,browser_hash,provider,verifier,nonce,user_id,linking,expires_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(hash(&start.state)).bind(hash(&start.browser_token)).bind(provider).bind(&start.verifier).bind(&start.nonce).bind(user_id).bind(linking).bind(now+600_000).execute(&mut *db).await?;
        Ok(())
    }
    pub async fn take_auth(
        &self,
        state: &str,
        browser: &str,
        provider: &str,
        clock: impl Into<Clock>,
    ) -> Result<AuthFlow> {
        let mut tx = self.pool.begin().await?;
        let now = clock.into().resolve(&mut tx).await?;
        let row=sqlx::query("SELECT provider,verifier,nonce,user_id,linking,browser_hash FROM auth_flows WHERE hash=$1 AND expires_ms>$2 FOR UPDATE").bind(hash(state)).bind(now).fetch_optional(&mut *tx).await?.ok_or_else(||Error::Forbidden("Sign-in expired. Please try again.".into()))?;
        if row.try_get::<String, _>("provider")? != provider
            || row.try_get::<String, _>("browser_hash")? != hash(browser)
        {
            return Err(Error::Forbidden(
                "Sign-in did not match this browser.".into(),
            ));
        }
        let flow = AuthFlow {
            provider: row.try_get("provider")?,
            verifier: row.try_get("verifier")?,
            nonce: row.try_get("nonce")?,
            user_id: row.try_get("user_id")?,
            linking: row.try_get("linking")?,
        };
        sqlx::query("DELETE FROM auth_flows WHERE hash=$1")
            .bind(hash(state))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(flow)
    }
    pub async fn finish_auth(
        &self,
        flow: AuthFlow,
        identity: Identity,
        clock: impl Into<Clock>,
    ) -> Result<(User, String)> {
        let mut tx = self.pool.begin().await?;
        let now = clock.into().resolve(&mut tx).await?;
        let user = Self::identify_on(&mut tx, flow, identity, now).await?;
        let token = Self::new_session_on(&mut tx, &user.id, now).await?;
        tx.commit().await?;
        Ok((user, token))
    }
    pub async fn identify(
        &self,
        flow: AuthFlow,
        identity: Identity,
        clock: impl Into<Clock>,
    ) -> Result<User> {
        let mut tx = self.pool.begin().await?;
        let now = clock.into().resolve(&mut tx).await?;
        let user = Self::identify_on(&mut tx, flow, identity, now).await?;
        tx.commit().await?;
        Ok(user)
    }
    async fn identify_on(
        db: &mut PgConnection,
        flow: AuthFlow,
        identity: Identity,
        now: i64,
    ) -> Result<User> {
        let key = serde_json::to_string(&(&identity.issuer, &identity.subject))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(key)
            .execute(&mut *db)
            .await?;
        if let Some(current) = &flow.user_id {
            sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
                .bind(current)
                .fetch_optional(&mut *db)
                .await?
                .ok_or(Error::Unauthorized)?;
        }
        let existing: Option<String> =
            sqlx::query_scalar("SELECT user_id FROM identities WHERE issuer=$1 AND subject=$2")
                .bind(&identity.issuer)
                .bind(&identity.subject)
                .fetch_optional(&mut *db)
                .await?;
        if !flow.linking
            && let (Some(current), Some(target)) = (&flow.user_id, &existing)
        {
            if current != target && Self::user_on(db, current).await?.guest {
                let open:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM games WHERE (white_id=$1 OR black_id=$1) AND status IN ('waiting','active','paused'))").bind(current).fetch_one(&mut *db).await?;
                if open {
                    return Err(Error::GuestMatchesOpen);
                }
            }
        }
        let id = if flow.linking {
            let user = flow.user_id.ok_or(Error::Unauthorized)?;
            if existing.as_ref().is_some_and(|other| other != &user) {
                return Err(Error::IdentityInUse);
            }
            user
        } else if let Some(id) = existing {
            id
        } else if let Some(id) = flow.user_id.as_ref() {
            if Self::user_on(db, id).await?.guest {
                id.clone()
            } else {
                Self::identity_user(db, &identity.name, now).await?
            }
        } else {
            Self::identity_user(db, &identity.name, now).await?
        };
        sqlx::query("INSERT INTO identities(issuer,subject,user_id) VALUES($1,$2,$3) ON CONFLICT(issuer,subject) DO NOTHING").bind(identity.issuer).bind(identity.subject).bind(&id).execute(&mut *db).await?;
        sqlx::query("UPDATE users SET guest=false WHERE id=$1")
            .bind(&id)
            .execute(&mut *db)
            .await?;
        Self::user_on(db, &id).await
    }
    async fn identity_user(db: &mut PgConnection, name: &str, now: i64) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let base: String = name
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(10)
            .collect();
        let handle = format!(
            "{}_{}",
            if base.is_empty() { "Player" } else { &base },
            &Uuid::new_v4().simple().to_string()[..12]
        );
        Self::insert_user_on(db, &id, &handle, false, now).await?;
        Ok(id)
    }
}

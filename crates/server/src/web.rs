use crate::{
    auth::{Auth, ProviderView},
    model::*,
    store::{Store, Subscription},
};
use axum::{
    Json, Router,
    body::Body,
    extract::{
        DefaultBodyLimit, Path, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{get, patch, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Semaphore, broadcast, watch};
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

pub struct App {
    store: Arc<Mutex<Store>>,
    permits: Arc<Semaphore>,
    pub auth: Auth,
    epoch_ms: i64,
    started: Instant,
    shutdown: watch::Sender<bool>,
}

impl App {
    pub fn new(
        database: &std::path::Path,
        public_url: &str,
        credentials: Vec<(String, String, String)>,
    ) -> Result<Arc<Self>> {
        let epoch_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Internal("System clock is invalid.".into()))?
            .as_millis() as i64;
        Ok(Arc::new(Self {
            store: Arc::new(Mutex::new(Store::open(database, epoch_ms)?)),
            permits: Arc::new(Semaphore::new(64)),
            auth: Auth::new(public_url, credentials)?,
            epoch_ms,
            started: Instant::now(),
            shutdown: watch::channel(false).0,
        }))
    }
    pub fn now(&self) -> i64 {
        self.epoch_ms + self.started.elapsed().as_millis() as i64
    }
    pub async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Conflict("Server is busy. Please retry shortly.".into()))?;
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut store = store
                .lock()
                .map_err(|_| Error::Internal("Database worker stopped.".into()))?;
            f(&mut store)
        })
        .await
        .map_err(|_| Error::Internal("Database worker stopped.".into()))?
    }
    async fn user(&self, headers: &HeaderMap) -> Result<User> {
        let token = session_token(headers)
            .ok_or(Error::Unauthorized)?
            .to_owned();
        let now = self.now();
        self.run(move |store| store.session(&token, now)).await
    }
    async fn optional_user(&self, headers: &HeaderMap) -> Result<Option<User>> {
        match self.user(headers).await {
            Ok(user) => Ok(Some(user)),
            Err(Error::Unauthorized) => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub async fn stop(&self) -> Result<()> {
        let now = self.now();
        self.run(move |store| store.tick(now)).await?;
        let _ = self.shutdown.send(true);
        Ok(())
    }
    pub fn stopped(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }
    pub fn start_clock(self: &Arc<Self>) {
        let app = self.clone();
        let mut stopped = self.stopped();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {_=stopped.changed()=>break,_=interval.tick()=>{let now=app.now();if let Err(error)=app.run(move|store|store.tick(now)).await{tracing::error!(%error,"clock update failed");}}}
            }
        });
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match self {
            Error::Unauthorized => StatusCode::UNAUTHORIZED,
            Error::Forbidden(_) => StatusCode::FORBIDDEN,
            Error::NotFound => StatusCode::NOT_FOUND,
            Error::Conflict(_) => StatusCode::CONFLICT,
            Error::Invalid(_) | Error::Engine(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = if status.is_server_error() {
            tracing::error!(error=%self,"request failed");
            "The server could not complete this request.".into()
        } else {
            self.to_string()
        };
        (status, Json(json!({"error":message}))).into_response()
    }
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
}
fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| cookie(headers, "nc_session"))
}
fn session_cookie(app: &App, name: &str, token: &str, seconds: i64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={seconds}{}",
        if app.auth.public_url.scheme() == "https" {
            "; Secure"
        } else {
            ""
        }
    ))
    .unwrap()
}

async fn origin_check(State(app): State<Arc<App>>, request: Request<Body>, next: Next) -> Response {
    let requires_origin =
        request.method() != axum::http::Method::GET && request.method() != axum::http::Method::HEAD;
    let bearer = request
        .headers()
        .get(header::AUTHORIZATION)
        .is_some_and(|v| v.as_bytes().starts_with(b"Bearer "));
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok());
    if (requires_origin || request.headers().contains_key(header::UPGRADE))
        && !bearer
        && origin != Some(app.auth.public_url.origin().ascii_serialization().as_str())
    {
        return Error::Forbidden("Request origin was not accepted.".into()).into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub fn router(app: Arc<App>, static_dir: PathBuf) -> Router {
    let api = Router::new()
        .route(
            "/health",
            get(|| async { Json(json!({"ok":true,"api":1})) }),
        )
        .route("/me", get(me))
        .route("/guest", post(guest))
        .route("/logout", post(logout))
        .route("/profile", patch(profile))
        .route("/games", get(open_games).post(create))
        .route("/games/{id}", get(snapshot))
        .route("/games/{id}/join", post(join))
        .route("/games/{id}/commands", post(command))
        .route("/games/{id}/stream", get(stream))
        .route("/queue", post(queue))
        .route("/history", get(history))
        .route("/auth/{provider}/link", post(link))
        .layer(middleware::from_fn_with_state(app.clone(), origin_check))
        .layer(DefaultBodyLimit::max(8192));
    Router::new()
        .nest("/api/v1", api)
        .route("/auth/{provider}", get(login))
        .route("/auth/{provider}/callback", get(callback))
        .fallback_service(ServeDir::new(static_dir))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            "cross-origin-opener-policy"
                .parse::<header::HeaderName>()
                .unwrap(),
            HeaderValue::from_static("same-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            "cross-origin-embedder-policy"
                .parse::<header::HeaderName>()
                .unwrap(),
            HeaderValue::from_static("require-corp"),
        ))
        .with_state(app)
}

#[derive(Serialize)]
struct Account {
    user: Option<User>,
    providers: Vec<ProviderView>,
    identities: Vec<String>,
}
async fn me(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Json<Account>> {
    let user = app.optional_user(&headers).await?;
    let identities = if let Some(user) = &user {
        let id = user.id.clone();
        app.run(move|store|{Ok(store.db.prepare("SELECT DISTINCT CASE WHEN issuer='github' THEN 'github' WHEN issuer='https://accounts.google.com' THEN 'google' ELSE 'microsoft' END FROM identities WHERE user_id=?1")?.query_map([id],|row|row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?)}).await?
    } else {
        Vec::new()
    };
    Ok(Json(Account {
        user,
        providers: app.auth.providers(),
        identities,
    }))
}
async fn guest(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Response> {
    if let Some(user) = app.optional_user(&headers).await? {
        return Ok(Json(user).into_response());
    }
    let now = app.now();
    let (user, token) = app.run(move |store| store.guest(now)).await?;
    Ok((
        [(
            header::SET_COOKIE,
            session_cookie(&app, "nc_session", &token, SESSION_SECONDS),
        )],
        Json(user),
    )
        .into_response())
}
async fn logout(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Response> {
    if let Some(token) = session_token(&headers) {
        let token = token.to_owned();
        app.run(move |store| store.logout(&token)).await?;
    }
    Ok((
        [(
            header::SET_COOKIE,
            session_cookie(&app, "nc_session", "", 0),
        )],
        StatusCode::NO_CONTENT,
    )
        .into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    handle: String,
}
async fn profile(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(body): Json<Profile>,
) -> Result<Json<User>> {
    let user = app.user(&headers).await?;
    Ok(Json(
        app.run(move |store| store.rename(&user.id, &body.handle))
            .await?,
    ))
}
async fn create(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(spec): Json<CreateGame>,
) -> Result<Json<Snapshot>> {
    let user = app.user(&headers).await?;
    let now = app.now();
    Ok(Json(
        app.run(move |store| store.create(&user.id, spec, now))
            .await?,
    ))
}
async fn join(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Snapshot>> {
    let user = app.user(&headers).await?;
    let now = app.now();
    Ok(Json(
        app.run(move |store| store.join(&id, &user.id, now)).await?,
    ))
}
async fn snapshot(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Snapshot>> {
    let user = app.user(&headers).await?;
    let now = app.now();
    Ok(Json(
        app.run(move |store| store.snapshot(&id, &user.id, now))
            .await?,
    ))
}
async fn command(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(command): Json<Command>,
) -> Result<Json<MatchEvent>> {
    let user = app.user(&headers).await?;
    let now = app.now();
    Ok(Json(
        app.run(move |store| store.command(&id, &user.id, command, now))
            .await?,
    ))
}
async fn open_games(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<Json<Vec<MatchInfo>>> {
    let user = app.user(&headers).await?;
    let now = app.now();
    Ok(Json(
        app.run(move |store| store.open_games(&user.id, now))
            .await?,
    ))
}
async fn queue(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Json<Snapshot>> {
    let user = app.user(&headers).await?;
    let now = app.now();
    Ok(Json(
        app.run(move |store| store.queue(&user.id, now)).await?,
    ))
}
#[derive(Deserialize)]
struct HistoryQuery {
    before: Option<i64>,
}
async fn history(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<HistoryEntry>>> {
    let user = app.user(&headers).await?;
    let now = app.now();
    Ok(Json(
        app.run(move |store| store.history(&user.id, query.before, now))
            .await?,
    ))
}

async fn stream(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    let user = app.user(&headers).await?;
    let now = app.now();
    let game_id = id.clone();
    let user_id = user.id.clone();
    let subscription = app
        .run(move |store| store.subscribe(&game_id, &user_id, now))
        .await?;
    let token = session_token(&headers)
        .ok_or(Error::Unauthorized)?
        .to_owned();
    Ok(upgrade
        .max_message_size(8192)
        .max_frame_size(8192)
        .on_upgrade(move |socket| connection(socket, app, id, user.id, token, subscription)))
}
async fn connection(
    mut socket: WebSocket,
    app: Arc<App>,
    id: String,
    user: String,
    token: String,
    subscription: Subscription,
) {
    let Subscription {
        snapshot,
        mut updates,
    } = subscription;
    if socket
        .send(Message::Text(
            json!({"kind":"snapshot","snapshot":snapshot})
                .to_string()
                .into(),
        ))
        .await
        .is_err()
    {
        return;
    }
    let mut heartbeat = tokio::time::interval(Duration::from_secs(25));
    let mut stopped = app.stopped();
    loop {
        tokio::select! {
            _=stopped.changed()=>break,
            incoming=socket.recv()=>{if matches!(incoming,None|Some(Err(_))|Some(Ok(Message::Close(_)))){break;}},
            _=heartbeat.tick()=>{
                let token=token.clone();let now=app.now();
                if app.run(move|store|store.session(&token,now)).await.is_err(){break;}
                if socket.send(Message::Ping(Vec::new().into())).await.is_err(){break;}
            },
            update=updates.recv()=>{
                let payload=match update {
                    Ok(event)=>json!({"kind":"event","event":event}),
                    Err(broadcast::error::RecvError::Closed)=>break,
                    Err(broadcast::error::RecvError::Lagged(_))=>{
                        let id=id.clone();let user=user.clone();let now=app.now();
                        match app.run(move|store|store.snapshot(&id,&user,now)).await{Ok(snapshot)=>json!({"kind":"snapshot","snapshot":snapshot}),Err(_)=>break}
                    }
                };
                if socket.send(Message::Text(payload.to_string().into())).await.is_err(){break;}
            }
        }
    }
    let _ = socket.send(Message::Close(None)).await;
}

async fn begin(
    app: &Arc<App>,
    headers: &HeaderMap,
    provider: &str,
    linking: bool,
) -> Result<Response> {
    let user = app.optional_user(headers).await?;
    if linking && user.is_none() {
        return Err(Error::Unauthorized);
    }
    let start = app.auth.begin(provider)?;
    let redirect = start.url.clone();
    let cookie = session_cookie(app, "nc_auth", &start.browser_token, 600);
    let provider = provider.to_owned();
    let user = user.map(|u| u.id);
    let now = app.now();
    app.run(move |store| store.begin_auth(&start, &provider, user.as_deref(), linking, now))
        .await?;
    if linking {
        Ok((
            [(header::SET_COOKIE, cookie)],
            Json(json!({"url":redirect})),
        )
            .into_response())
    } else {
        Ok(([(header::SET_COOKIE, cookie)], Redirect::to(&redirect)).into_response())
    }
}
async fn login(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(provider): Path<String>,
) -> Result<Response> {
    begin(&app, &headers, &provider, false).await
}
async fn link(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(provider): Path<String>,
) -> Result<Response> {
    begin(&app, &headers, &provider, true).await
}
#[derive(Deserialize)]
struct Callback {
    state: String,
    code: Option<String>,
    error: Option<String>,
}
async fn callback(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(provider): Path<String>,
    Query(query): Query<Callback>,
) -> Response {
    let result = async {
        let browser = cookie(&headers, "nc_auth")
            .ok_or_else(|| Error::Forbidden("Sign-in expired.".into()))?
            .to_owned();
        let now = app.now();
        let flow = app
            .run(move |store| store.take_auth(&query.state, &browser, &provider, now))
            .await?;
        if flow.linking && app.user(&headers).await?.id != flow.user_id.as_deref().unwrap_or("") {
            return Err(Error::Forbidden("Sign-in account changed.".into()));
        }
        if query.error.is_some() {
            return Err(Error::Forbidden("Sign-in was declined.".into()));
        }
        let identity = app
            .auth
            .identity(
                &flow,
                query
                    .code
                    .ok_or_else(|| Error::Invalid("Missing sign-in code.".into()))?,
            )
            .await?;
        let old = session_token(&headers).map(str::to_owned);
        let now = app.now();
        app.run(move |store| {
            let result = store.finish_auth(flow, identity, now)?;
            if let Some(old) = old {
                store.logout(&old)?;
            }
            Ok(result)
        })
        .await
    }
    .await;
    match result {
        Ok((_, token)) => (
            [
                (
                    header::SET_COOKIE,
                    session_cookie(&app, "nc_session", &token, SESSION_SECONDS),
                ),
                (header::SET_COOKIE, session_cookie(&app, "nc_auth", "", 0)),
            ],
            Redirect::to("/?online=1"),
        )
            .into_response(),
        Err(_) => (
            [(header::SET_COOKIE, session_cookie(&app, "nc_auth", "", 0))],
            Redirect::to("/?online=1&auth_error=1"),
        )
            .into_response(),
    }
}

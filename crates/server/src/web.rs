use crate::{
    auth::{Auth, ProviderView},
    hub::Hub,
    mobile::{APP_CALLBACK, MobileSession},
    model::*,
    store::{Clock, Store, hash},
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
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::watch;
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

pub struct App {
    pub store: Store,
    hub: Arc<Hub>,
    pub auth: Auth,
    shutdown: watch::Sender<bool>,
}
impl App {
    pub async fn new(
        database: &str,
        public_url: &str,
        credentials: Vec<(String, String, String)>,
    ) -> Result<Arc<Self>> {
        let auth = Auth::new(public_url, credentials)?;
        let app = Arc::new(Self {
            store: Store::connect(database).await?,
            hub: Arc::default(),
            auth,
            shutdown: watch::channel(false).0,
        });
        app.hub.start(app.store.clone(), app.stopped());
        Ok(app)
    }
    async fn user(&self, headers: &HeaderMap) -> Result<User> {
        self.store
            .session(
                session_token(headers).ok_or(Error::Unauthorized)?,
                Clock::Database,
            )
            .await
    }
    async fn optional_user(&self, headers: &HeaderMap) -> Result<Option<User>> {
        match self.user(headers).await {
            Ok(user) => Ok(Some(user)),
            Err(Error::Unauthorized) => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub async fn stop(&self) -> Result<()> {
        self.shutdown.send_replace(true);
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
                tokio::select! {
                    _=stopped.changed()=>break,
                    _=interval.tick()=>{
                        if let Err(error)=app.store.tick(Clock::Database).await {tracing::error!(%error,"clock update failed");}
                        if let Err(error)=app.store.matchmake(Clock::Database).await {tracing::error!(%error,"matchmaking update failed");}
                    }
                }
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
            Error::Conflict(_) | Error::GuestMatchesOpen | Error::IdentityInUse => {
                StatusCode::CONFLICT
            }
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
    let mobile = Router::new()
        .route("/guest", post(mobile_guest))
        .route("/auth/{provider}", post(mobile_login))
        .route("/exchange", post(mobile_exchange))
        .route("/games/{id}/ticket", post(mobile_ticket))
        .route("/games/{id}/stream", get(mobile_stream))
        .layer(middleware::from_fn(native_request))
        .layer(DefaultBodyLimit::max(8192));
    let api = Router::new()
        .route(
            "/health",
            get(|| async { Json(json!({"ok":true,"api":1})) }),
        )
        .route("/ready", get(ready))
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
        .nest("/api/v1/mobile", mobile)
        .nest("/api/v1", api)
        .route("/auth/mobile/{provider}", get(mobile_browser))
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
        app.store.identities(&id).await?
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
    let now = Clock::Database;
    let (user, token) = app.store.guest(now).await?;
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
        app.store.logout(&token).await?;
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
    Ok(Json(app.store.rename(&user.id, &body.handle).await?))
}
async fn create(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(spec): Json<CreateGame>,
) -> Result<Json<Snapshot>> {
    let user = app.user(&headers).await?;
    Ok(Json(
        app.store.create(&user.id, spec, Clock::Database).await?,
    ))
}
async fn join(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Snapshot>> {
    let user = app.user(&headers).await?;
    Ok(Json(app.store.join(&id, &user.id, Clock::Database).await?))
}
async fn snapshot(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Snapshot>> {
    let user = app.user(&headers).await?;
    let now = Clock::Database;
    Ok(Json(app.store.snapshot(&id, &user.id, now).await?))
}
async fn command(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(command): Json<Command>,
) -> Result<Json<MatchEvent>> {
    let user = app.user(&headers).await?;
    Ok(Json(
        app.store
            .command(&id, &user.id, command, Clock::Database)
            .await?,
    ))
}
async fn open_games(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<Json<Vec<MatchInfo>>> {
    let user = app.user(&headers).await?;
    let now = Clock::Database;
    Ok(Json(app.store.open_games(&user.id, now).await?))
}
async fn queue(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Json<Snapshot>> {
    let user = app.user(&headers).await?;
    Ok(Json(app.store.queue(&user.id, Clock::Database).await?))
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
    let now = Clock::Database;
    Ok(Json(app.store.history(&user.id, query.before, now).await?))
}

async fn stream(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    let user = app.user(&headers).await?;
    let updates = app.hub.subscribe(&id);
    let snapshot = app.store.snapshot(&id, &user.id, Clock::Database).await?;
    let token = hash(session_token(&headers).ok_or(Error::Unauthorized)?);
    Ok(upgrade
        .max_message_size(8192)
        .max_frame_size(8192)
        .on_upgrade(move |socket| connection(socket, app, id, token, snapshot, updates)))
}
async fn connection(
    mut socket: WebSocket,
    app: Arc<App>,
    id: String,
    token: String,
    snapshot: Snapshot,
    mut updates: watch::Receiver<u32>,
) {
    let mut cursor = snapshot.info.seq;
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
            incoming=socket.recv()=>if matches!(incoming,None|Some(Err(_))|Some(Ok(Message::Close(_)))) {break;},
            _=heartbeat.tick()=>{
                if app.store.session_hash(&token,Clock::Database).await.is_err() {break;}
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {break;}
            },
            update=updates.changed()=>{
                if update.is_err() {break;}
                let target=*updates.borrow_and_update();
                while cursor<target {
                    let events=match app.store.events_after(&id,cursor).await {Ok(events)=>events,Err(_)=>return};
                    if events.is_empty() {break;}
                    for event in events {
                        cursor=event.seq;
                        if socket.send(Message::Text(json!({"kind":"event","event":event}).to_string().into())).await.is_err() {return;}
                    }
                }
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
    let now = Clock::Database;
    app.store
        .begin_auth(&start, &provider, user.as_deref(), linking, now)
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

async fn native_request(request: Request<Body>, next: Next) -> Response {
    if request.headers().contains_key(header::COOKIE) {
        return Error::Forbidden("Use an app session for this endpoint.".into()).into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn mobile_guest(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<Json<MobileSession>> {
    if let Some(token) = session_token(&headers) {
        return Ok(Json(MobileSession {
            user: app.user(&headers).await?,
            token: token.into(),
        }));
    }
    let (user, token) = app.store.guest(Clock::Database).await?;
    Ok(Json(MobileSession { user, token }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MobileStart {
    challenge: String,
    state: String,
    linking: bool,
}
async fn mobile_login(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(provider): Path<String>,
    Json(body): Json<MobileStart>,
) -> Result<Json<serde_json::Value>> {
    if !app.auth.providers().iter().any(|p| p.id == provider) {
        return Err(Error::Invalid("That login provider is unavailable.".into()));
    }
    let token = session_token(&headers).map(str::to_owned);
    let name = provider.clone();
    let ticket = app
        .store
        .request_mobile_login(
            &name,
            &body.challenge,
            &body.state,
            token.as_deref(),
            body.linking,
            Clock::Database,
        )
        .await?;
    let mut url = app
        .auth
        .public_url
        .join(&format!("auth/mobile/{provider}"))
        .unwrap();
    url.query_pairs_mut().append_pair("ticket", &ticket);
    Ok(Json(json!({"url":url})))
}
#[derive(Deserialize)]
struct TicketQuery {
    ticket: String,
}
async fn mobile_browser(
    State(app): State<Arc<App>>,
    Path(provider): Path<String>,
    Query(query): Query<TicketQuery>,
) -> Result<Response> {
    let start = app.auth.begin(&provider)?;
    let redirect = start.url.clone();
    let cookie = session_cookie(&app, "nc_auth", &start.browser_token, 600);
    app.store
        .start_mobile_login(&query.ticket, &provider, &start, Clock::Database)
        .await?;
    Ok((
        [
            (header::SET_COOKIE, cookie),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (
                header::REFERRER_POLICY,
                HeaderValue::from_static("no-referrer"),
            ),
        ],
        Redirect::to(&redirect),
    )
        .into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MobileExchange {
    code: String,
    verifier: String,
}
async fn mobile_exchange(
    State(app): State<Arc<App>>,
    Json(body): Json<MobileExchange>,
) -> Result<Json<MobileSession>> {
    Ok(Json(
        app.store
            .exchange_mobile_login(&body.code, &body.verifier, Clock::Database)
            .await?,
    ))
}
async fn mobile_ticket(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let token = session_token(&headers)
        .ok_or(Error::Unauthorized)?
        .to_owned();
    let ticket = app
        .store
        .stream_ticket(&id, &token, Clock::Database)
        .await?;
    Ok(Json(json!({"ticket":ticket})))
}
async fn mobile_stream(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(query): Query<TicketQuery>,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    let (user, token) = app
        .store
        .take_stream_ticket(&id, &query.ticket, Clock::Database)
        .await?;
    let updates = app.hub.subscribe(&id);
    let snapshot = app.store.snapshot(&id, &user.id, Clock::Database).await?;
    Ok(upgrade
        .max_message_size(8192)
        .max_frame_size(8192)
        .on_upgrade(move |socket| connection(socket, app, id, token, snapshot, updates)))
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
    let state = query.state.clone();
    let mobile = match app.store.mobile_login(&state).await {
        Ok(mobile) => mobile,
        Err(error) => return error.into_response(),
    };
    let result = async {
        let browser = cookie(&headers, "nc_auth")
            .ok_or_else(|| Error::Forbidden("Sign-in expired.".into()))?
            .to_owned();
        let now = Clock::Database;
        let flow = app
            .store
            .take_auth(&query.state, &browser, &provider, now)
            .await?;
        if let Some(mobile) = &mobile {
            if let Some(session) = &mobile.session_hash {
                let session = session.clone();
                let user = app.store.session_hash(&session, Clock::Database).await?;
                if Some(user.id.as_str()) != flow.user_id.as_deref() {
                    return Err(Error::Unauthorized);
                }
            }
        } else if flow.linking
            && app.user(&headers).await?.id != flow.user_id.as_deref().unwrap_or("")
        {
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
        let native = mobile.is_some();
        if native {
            Ok((
                app.store.identify(flow, identity, Clock::Database).await?,
                None,
            ))
        } else {
            let (user, token) = app
                .store
                .finish_auth(flow, identity, Clock::Database)
                .await?;
            if let Some(old) = old {
                app.store.logout(&old).await?;
            }
            Ok((user, Some(token)))
        }
    }
    .await;
    if let Some(mobile) = mobile {
        let (user, error) = match result {
            Ok((user, _)) => (Some(user.id), None),
            Err(error) => {
                tracing::warn!(%error,"app sign-in did not complete");
                (
                    None,
                    Some(match error {
                        Error::GuestMatchesOpen => "guest_matches",
                        Error::IdentityInUse => "identity_in_use",
                        _ => "failed",
                    }),
                )
            }
        };
        let code = match app
            .store
            .complete_mobile_login(&mobile.id, user.as_deref(), error, Clock::Database)
            .await
        {
            Ok(code) => code,
            Err(error) => return error.into_response(),
        };
        let mut url = url::Url::parse(APP_CALLBACK).unwrap();
        url.query_pairs_mut()
            .append_pair("code", &code)
            .append_pair("state", &mobile.state);
        return (
            [
                (header::SET_COOKIE, session_cookie(&app, "nc_auth", "", 0)),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
                (
                    header::REFERRER_POLICY,
                    HeaderValue::from_static("no-referrer"),
                ),
            ],
            Redirect::to(url.as_str()),
        )
            .into_response();
    }
    match result {
        Ok((_, Some(token))) => (
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
        Ok((_, None)) => Error::Internal("Missing browser session.".into()).into_response(),
        Err(error) => {
            tracing::warn!(%error, "sign-in did not complete");
            let target = match error {
                Error::GuestMatchesOpen => "/?online=1&auth_error=guest_matches",
                Error::IdentityInUse => "/?online=1&auth_error=identity_in_use",
                _ => "/?online=1&auth_error=1",
            };
            (
                [(header::SET_COOKIE, session_cookie(&app, "nc_auth", "", 0))],
                Redirect::to(target),
            )
                .into_response()
        }
    }
}

async fn ready(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>> {
    app.store.ready().await?;
    Ok(Json(json!({"ok":true})))
}

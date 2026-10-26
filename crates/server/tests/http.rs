use futures_util::StreamExt;
use nested_chess_server::web::{App, router};
use reqwest::{
    Client,
    header::{COOKIE, ORIGIN, SET_COOKIE},
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
mod common;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
use uuid::Uuid;

struct Server {
    database: String,
    app: Arc<App>,
    task: JoinHandle<()>,
    url: String,
}
struct Player {
    id: String,
    cookie: String,
    client: Client,
}
impl Server {
    async fn start() -> Self {
        Self::configured(Vec::new()).await
    }
    async fn configured(credentials: Vec<(String, String, String)>) -> Self {
        let (database, _store) = common::database().await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = App::new(&database, &url, credentials).await.unwrap();
        app.start_clock();
        let mut stopped = app.stopped();
        let routes = router(app.clone(), PathBuf::from("web"));
        let task = tokio::spawn(async move {
            axum::serve(listener, routes)
                .with_graceful_shutdown(async move {
                    let _ = stopped.changed().await;
                })
                .await
                .unwrap();
        });
        Self {
            database,
            app,
            task,
            url,
        }
    }
    async fn replica(&self) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = App::new(&self.database, &self.url, Vec::new())
            .await
            .unwrap();
        app.start_clock();
        let mut stopped = app.stopped();
        let routes = router(app.clone(), PathBuf::from("web"));
        let task = tokio::spawn(async move {
            axum::serve(listener, routes)
                .with_graceful_shutdown(async move {
                    let _ = stopped.changed().await;
                })
                .await
                .unwrap();
        });
        Self {
            database: self.database.clone(),
            app,
            task,
            url,
        }
    }
    async fn player(&self) -> Player {
        let response = Client::new()
            .post(format!("{}/api/v1/guest", self.url))
            .header(ORIGIN, &self.url)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let cookie = response.headers()[SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let user: Value = response.json().await.unwrap();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(COOKIE, cookie.parse().unwrap());
        headers.insert(ORIGIN, self.url.parse().unwrap());
        Player {
            id: user["id"].as_str().unwrap().into(),
            cookie,
            client: Client::builder().default_headers(headers).build().unwrap(),
        }
    }
    async fn stop(self) {
        self.app.stop().await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), self.task)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn replicas_deliver_committed_moves_and_survive_another_server_stopping() {
    let server = Server::start().await;
    let replica = server.replica().await;
    let a = server.player().await;
    let b = server.player().await;
    let created: Value = a
        .client
        .post(format!("{}/api/v1/games", server.url))
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["info"]["id"].as_str().unwrap();
    let joined: Value = b
        .client
        .post(format!("{}/api/v1/games/{id}/join", server.url))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let (white, black) = if joined["info"]["white"]["id"] == a.id {
        (&a, &b)
    } else {
        (&b, &a)
    };
    let url = format!(
        "{}/api/v1/games/{id}/stream",
        replica.url.replace("http:", "ws:")
    );
    let mut request = url.as_str().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Cookie", black.cookie.parse().unwrap());
    request
        .headers_mut()
        .insert("Origin", server.url.parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let initial: Value =
        serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(initial["kind"], "snapshot");
    assert_eq!(initial["snapshot"]["info"]["seq"], 1);
    let body = json!({"request_id":Uuid::new_v4().to_string(),"expected_seq":1,"action":{"kind":"move","board":0,"uci":"e2e4"}});
    let endpoint = format!("{}/api/v1/games/{id}/commands", server.url);
    let accepted: Value = white
        .client
        .post(&endpoint)
        .json(&body)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let received = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let message = socket.next().await.unwrap().unwrap();
            if message.is_text() {
                break serde_json::from_str::<Value>(message.to_text().unwrap()).unwrap();
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(received["event"], accepted);
    assert_eq!(accepted["seq"], 2);
    let retry: Value = white
        .client
        .post(&endpoint)
        .json(&body)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(retry, accepted);
    socket.close(None).await.unwrap();
    let refreshed: Value = black
        .client
        .get(format!("{}/api/v1/games/{id}", server.url))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(refreshed["state"]["ply"], 1);
    assert_eq!(refreshed["state"]["forced_board"], 1);
    let response=white.client.post(&endpoint).json(&json!({"request_id":Uuid::new_v4().to_string(),"expected_seq":2,"action":{"kind":"resign"}})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let history: Value = black
        .client
        .get(format!("{}/api/v1/history", server.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(history[0]["info"]["result"], "black");
    assert_eq!(history[0]["rating_after"], Value::Null);
    server.stop().await;
    let response = black
        .client
        .get(format!("{}/api/v1/games/{id}", replica.url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    replica.stop().await;
}

#[tokio::test]
async fn cross_origin_mutations_and_revoked_sessions_are_rejected() {
    let server = Server::start().await;
    let response = Client::new()
        .post(format!("{}/api/v1/guest", server.url))
        .header(ORIGIN, "https://other.example")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let player = server.player().await;
    let response = player
        .client
        .post(format!("{}/api/v1/queue", server.url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    player
        .client
        .post(format!("{}/api/v1/logout", server.url))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let response = player
        .client
        .post(format!("{}/api/v1/games", server.url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    server.stop().await;
}

#[tokio::test]
async fn native_bearer_sessions_receive_real_websocket_snapshots_with_single_use_tickets() {
    let server = Server::start().await;
    let client = Client::new();
    let response = client
        .post(format!("{}/api/v1/mobile/guest", server.url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(!response.headers().contains_key(SET_COOKIE));
    let session: Value = response.json().await.unwrap();
    let token = session["token"].as_str().unwrap();
    let game: Value = client
        .post(format!("{}/api/v1/games", server.url))
        .bearer_auth(token)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = game["info"]["id"].as_str().unwrap();
    let ticket: Value = client
        .post(format!("{}/api/v1/mobile/games/{id}/ticket", server.url))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let url = format!(
        "{}/api/v1/mobile/games/{id}/stream?ticket={}",
        server.url.replace("http:", "ws:"),
        ticket["ticket"].as_str().unwrap()
    );
    let mut request = url.as_str().into_client_request().unwrap();
    request
        .headers_mut()
        .insert(ORIGIN, "capacitor://localhost".parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let snapshot: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
    assert_eq!(snapshot["kind"], "snapshot");
    assert_eq!(snapshot["snapshot"]["info"]["id"], id);
    socket.close(None).await.unwrap();
    assert!(connect_async(url.as_str()).await.is_err());
    server.stop().await;
}

#[tokio::test]
async fn native_endpoints_reject_ambient_browser_cookies() {
    let server = Server::start().await;
    let player = server.player().await;
    let response = player
        .client
        .post(format!("{}/api/v1/mobile/guest", server.url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    server.stop().await;
}

#[tokio::test]
async fn declined_system_browser_login_returns_an_app_bound_one_time_handoff() {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use sha2::{Digest, Sha256};
    let server = Server::configured(vec![(
        "github".into(),
        "native-test-client".into(),
        "native-test-secret".into(),
    )])
    .await;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let verifier = "v".repeat(43);
    let state = "s".repeat(43);
    let start:Value=client.post(format!("{}/api/v1/mobile/auth/github",server.url)).json(&json!({"challenge":URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),"state":state,"linking":false})).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    let browser = client
        .get(start["url"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(browser.status(), 303);
    let cookie = browser.headers()[SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let authorize = url::Url::parse(browser.headers()["location"].to_str().unwrap()).unwrap();
    assert_eq!(authorize.host_str(), Some("github.com"));
    let oauth_state = authorize
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    let callback = client
        .get(format!("{}/auth/github/callback", server.url))
        .header(COOKIE, cookie)
        .query(&[("state", oauth_state.as_str()), ("error", "access_denied")])
        .send()
        .await
        .unwrap();
    assert_eq!(callback.status(), 303);
    let handoff = url::Url::parse(callback.headers()["location"].to_str().unwrap()).unwrap();
    assert_eq!(handoff.scheme(), "io.github.balbok0.nestedchess");
    assert_eq!(handoff.host_str(), Some("auth"));
    let params = handoff
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(params.get("state").unwrap().as_ref(), state);
    let body = json!({"code":params.get("code").unwrap(),"verifier":verifier});
    let response = client
        .post(format!("{}/api/v1/mobile/exchange", server.url))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let replay = client
        .post(format!("{}/api/v1/mobile/exchange", server.url))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), 401);
    server.stop().await;
}

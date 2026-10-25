use futures_util::StreamExt;
use nested_chess_server::web::{App, router};
use reqwest::{
    Client,
    header::{COOKIE, ORIGIN, SET_COOKIE},
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tempfile::TempDir;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
use uuid::Uuid;

struct Server {
    _dir: TempDir,
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
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scratch/claude/20261025_online-tests");
        std::fs::create_dir_all(&parent).unwrap();
        let dir = TempDir::new_in(parent).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = App::new(&dir.path().join("http.sqlite"), &url, Vec::new()).unwrap();
        app.start_clock();
        let mut stopped = app.stopped();
        let routes = router(app.clone(), dir.path().into());
        let task = tokio::spawn(async move {
            axum::serve(listener, routes)
                .with_graceful_shutdown(async move {
                    let _ = stopped.changed().await;
                })
                .await
                .unwrap();
        });
        Self {
            _dir: dir,
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
async fn two_players_receive_committed_moves_and_resync_over_real_websockets() {
    let server = Server::start().await;
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
        server.url.replace("http:", "ws:")
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

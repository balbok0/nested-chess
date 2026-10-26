use futures_util::StreamExt;
use nested_chess::{Color, Game, Outcome};
use reqwest::Client;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{net::TcpStream, sync::Semaphore, task::JoinSet};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use uuid::Uuid;

type AnyError = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, AnyError>;
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
struct Case {
    id: String,
    tokens: [String; 2],
    sockets: [Socket; 2],
    game: Game,
    seq: u32,
}
#[derive(Default)]
struct Measures {
    http: Vec<f64>,
    delivery: Vec<f64>,
    replicas: BTreeMap<String, usize>,
    finished: usize,
}
async fn response(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {body}").into());
    }
    Ok(serde_json::from_str(&body)?)
}
async fn received(socket: &mut Socket, seq: u32) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(message) = socket.next().await {
            let message = message?;
            if message.is_text() {
                let value: Value = serde_json::from_str(message.to_text()?)?;
                let actual = if value["kind"] == "snapshot" {
                    value["snapshot"]["info"]["seq"].as_u64()
                } else {
                    value["event"]["seq"].as_u64()
                };
                if actual == Some(u64::from(seq)) {
                    return Ok(());
                }
                if actual.is_some_and(|actual| actual > u64::from(seq)) {
                    return Err("Stream skipped an event".into());
                }
            }
        }
        Err("Stream closed".into())
    })
    .await?
}
impl Case {
    async fn setup(client: &Client, url: &str) -> Result<Self> {
        let a = response(
            client
                .post(format!("{url}/api/v1/mobile/guest"))
                .send()
                .await?,
        )
        .await?;
        let b = response(
            client
                .post(format!("{url}/api/v1/mobile/guest"))
                .send()
                .await?,
        )
        .await?;
        let a_token = a["token"].as_str().ok_or("Missing session")?.to_owned();
        let b_token = b["token"].as_str().ok_or("Missing session")?.to_owned();
        let created = response(
            client
                .post(format!("{url}/api/v1/games"))
                .bearer_auth(&a_token)
                .json(&json!({"clock":{"seconds":900,"increment":10}}))
                .send()
                .await?,
        )
        .await?;
        let id = created["info"]["id"]
            .as_str()
            .ok_or("Missing match")?
            .to_owned();
        let joined = response(
            client
                .post(format!("{url}/api/v1/games/{id}/join"))
                .bearer_auth(&b_token)
                .send()
                .await?,
        )
        .await?;
        let tokens = if joined["info"]["white"]["id"] == a["user"]["id"] {
            [a_token, b_token]
        } else {
            [b_token, a_token]
        };
        let mut sockets = Vec::new();
        for token in &tokens {
            let ticket = response(
                client
                    .post(format!("{url}/api/v1/mobile/games/{id}/ticket"))
                    .bearer_auth(token)
                    .send()
                    .await?,
            )
            .await?;
            let endpoint = format!(
                "{}/api/v1/mobile/games/{id}/stream?ticket={}",
                url.replacen("http", "ws", 1),
                ticket["ticket"].as_str().ok_or("Missing stream ticket")?
            );
            let (mut socket, _) = connect_async(endpoint).await?;
            received(&mut socket, 1).await?;
            sockets.push(socket);
        }
        Ok(Self {
            id,
            tokens,
            sockets: sockets.try_into().map_err(|_| "Missing sockets")?,
            game: Game::new(2)?,
            seq: 1,
        })
    }
    async fn play(
        &mut self,
        client: &Client,
        url: &str,
        index: usize,
        measures: &mut Measures,
    ) -> Result<bool> {
        if self.game.outcome() != Outcome::Ongoing {
            return Ok(false);
        }
        let boards = self.game.legal_boards();
        let board = boards[index % boards.len()];
        let moves = self.game.legal_moves(board)?;
        let uci = moves[(index.wrapping_mul(7919) + self.seq as usize * 104729) % moves.len()]
            .to_string();
        let token = &self.tokens[usize::from(self.game.turn() == Color::Black)];
        let start = Instant::now();
        let reply=client.post(format!("{url}/api/v1/games/{}/commands",self.id)).bearer_auth(token)
            .json(&json!({"request_id":Uuid::new_v4().to_string(),"expected_seq":self.seq,"action":{"kind":"move","board":board,"uci":uci}})).send().await?;
        let replica = reply
            .headers()
            .get("x-nested-replica")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("direct")
            .to_owned();
        let event = response(reply).await?;
        if event["seq"].as_u64() != Some(u64::from(self.seq + 1)) {
            return Err("Unexpected committed sequence".into());
        }
        measures.http.push(start.elapsed().as_secs_f64() * 1000.0);
        self.seq += 1;
        self.game.play(board, &uci)?;
        let (white, black) = self.sockets.split_at_mut(1);
        let (a, b) = tokio::join!(
            received(&mut white[0], self.seq),
            received(&mut black[0], self.seq)
        );
        a?;
        b?;
        measures
            .delivery
            .push(start.elapsed().as_secs_f64() * 1000.0);
        *measures.replicas.entry(replica).or_default() += 1;
        Ok(true)
    }
}
fn stats(mut values: Vec<f64>) -> Value {
    values.sort_by(f64::total_cmp);
    if values.is_empty() {
        return json!({"count":0});
    }
    let at = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
    json!({"count":values.len(),"p50_ms":at(0.5),"p95_ms":at(0.95),"p99_ms":at(0.99),"max_ms":values.last()})
}
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 7 {
        return Err("Usage: capacity URL GAMES SECONDS MOVES_PER_SECOND WARMUP_PLIES CONCURRENCY OUTPUT.json".into());
    }
    let url = args[0].trim_end_matches('/').to_owned();
    let count: usize = args[1].parse()?;
    let seconds: f64 = args[2].parse()?;
    let rate: f64 = args[3].parse()?;
    let warmup: usize = args[4].parse()?;
    let concurrency: usize = args[5].parse()?;
    if count == 0
        || concurrency == 0
        || !seconds.is_finite()
        || seconds <= 0.0
        || !rate.is_finite()
        || rate <= 0.0
    {
        return Err("Counts, duration and rate must be positive".into());
    }
    let client = Client::builder().timeout(Duration::from_secs(15)).build()?;
    let permits = Arc::new(Semaphore::new(concurrency));
    let mut setup = JoinSet::new();
    let setup_start = Instant::now();
    for index in 0..count {
        let client = client.clone();
        let url = url.clone();
        let permits = permits.clone();
        setup.spawn(async move {
            let _permit = permits.acquire().await?;
            let mut case = Case::setup(&client, &url).await?;
            let mut measures = Measures::default();
            for _ in 0..warmup {
                if !case.play(&client, &url, index, &mut measures).await? {
                    break;
                }
            }
            Ok::<_, AnyError>((index, case))
        });
    }
    let mut cases = Vec::new();
    while let Some(result) = setup.join_next().await {
        cases.push(result??);
    }
    let setup_seconds = setup_start.elapsed().as_secs_f64();
    eprintln!(
        "{} matches / {} sockets ready in {:.1}s",
        count,
        count * 2,
        setup_seconds
    );
    let start = tokio::time::Instant::now();
    let mut jobs = JoinSet::new();
    let total = (seconds * rate).floor() as usize;
    for (index, mut case) in cases {
        let url = url.clone();
        let client = client.clone();
        jobs.spawn(async move {
            let mut measures = Measures::default();
            for slot in (index..total).step_by(count) {
                tokio::time::sleep_until(start + Duration::from_secs_f64(slot as f64 / rate)).await;
                if !case
                    .play(&client, &url, index + slot, &mut measures)
                    .await?
                {
                    measures.finished = 1;
                    break;
                }
            }
            tokio::time::sleep_until(start + Duration::from_secs_f64(seconds)).await;
            Ok::<_, AnyError>(measures)
        });
    }
    let mut all = Measures::default();
    while let Some(result) = jobs.join_next().await {
        let result = result??;
        all.http.extend(result.http);
        all.delivery.extend(result.delivery);
        all.finished += result.finished;
        for (replica, count) in result.replicas {
            *all.replicas.entry(replica).or_default() += count;
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    let report = json!({"games":count,"sockets":count*2,"duration_seconds":seconds,"elapsed_seconds":elapsed,"offered_moves_per_second":rate,
        "completed_moves":all.http.len(),"completed_moves_per_second":all.http.len() as f64/elapsed,"warmup_plies":warmup,"setup_seconds":setup_seconds,
        "finished_matches":all.finished,"errors":0,"http":stats(all.http),"both_streams":stats(all.delivery),"replica_commands":all.replicas});
    std::fs::write(&args[6], serde_json::to_string_pretty(&report)?)?;
    println!("{report}");
    Ok(())
}

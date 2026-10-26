mod common;
use nested_chess::Game;
use nested_chess_server::{
    model::{Action, Command, Error},
    store::hash,
};
use serde_json::{Value, json};
use uuid::Uuid;

fn export() -> Value {
    let spec: Value = serde_json::from_str(include_str!("../legacy-tables.json")).unwrap();
    let mut tables = serde_json::Map::new();
    for name in spec.as_object().unwrap().keys() {
        tables.insert(name.clone(), json!([]));
    }
    tables.insert(
        "users".into(),
        json!([
            {"id":"white","handle":"LegacyWhite","guest":false,"created_ms":1000},
            {"id":"black","handle":"LegacyBlack","guest":false,"created_ms":1000}
        ]),
    );
    tables.insert(
        "ratings".into(),
        json!([
            {"user_id":"white","pool":"classical-2x2-15+10","rating":1500.0,"games":0},
            {"user_id":"black","pool":"classical-2x2-15+10","rating":1500.0,"games":0}
        ]),
    );
    tables.insert(
        "sessions".into(),
        json!([{"hash":hash("old_session"),"user_id":"white","expires_ms":9_000_000_000_000_i64}]),
    );
    tables.insert("games".into(),json!([{
        "id":"legacy","ordinal":42,"initial_save":Game::new(2).unwrap().save_json().unwrap(),
        "white_id":"white","black_id":"black","seq":2,"status":"active","size":2,"layout":"classical","rated":false,
        "result":null,"reason":null,"white_ms":900000,"black_ms":900000,"increment_ms":10000,"running_since_ms":1000,
        "turn":"black","ready_white":false,"ready_black":false,"created_ms":1000
    }]));
    let mut info = json!({
        "id":"legacy","seq":1,"status":"active","white":{"id":"white","handle":"LegacyWhite","guest":false,"rating":1500.0,"rated_games":0},
        "black":{"id":"black","handle":"LegacyBlack","guest":false,"rating":1500.0,"rated_games":0},
        "rated":false,"size":2,"layout":"classical","result":null,"reason":null,"white_ms":900000,"black_ms":900000,
        "increment_ms":10000,"running_since_ms":1000,"turn":"white","ready_white":false,"ready_black":false,"created_ms":1000,"server_ms":1000
    });
    let joined = json!({"seq":1,"action":{"kind":"joined"},"info":info}).to_string();
    info["seq"] = json!(2);
    info["turn"] = json!("black");
    let moved =
        json!({"seq":2,"action":{"kind":"move","board":0,"uci":"e2e4"},"info":info}).to_string();
    tables.insert("events".into(),json!([
        {"game_id":"legacy","seq":1,"actor_id":null,"request_id":null,"request_json":null,"event_json":joined},
        {"game_id":"legacy","seq":2,"actor_id":"white","request_id":Uuid::new_v4().to_string(),"request_json":null,"event_json":moved}
    ]));
    json!({"version":1,"tables":tables})
}
#[tokio::test]
async fn legacy_import_preserves_sessions_positions_and_pauses_timed_matches() {
    let (_, store) = common::database().await;
    store.import_sqlite(&export().to_string()).await.unwrap();
    assert_eq!(
        store.session("old_session", 1000).await.unwrap().id,
        "white"
    );
    let snapshot = store.snapshot("legacy", "white", 2000).await.unwrap();
    assert_eq!(snapshot.info.status, "paused");
    assert_eq!(snapshot.info.seq, 3);
    assert_eq!(snapshot.state.ply, 1);
    assert_eq!(snapshot.state.forced_board, Some(1));
    assert_eq!(snapshot.info.white_ms, Some(900000));
    for (user, seq) in [("white", 3), ("black", 4)] {
        store
            .command(
                "legacy",
                user,
                Command {
                    request_id: Uuid::new_v4().to_string(),
                    expected_seq: seq,
                    action: Action::Resume,
                },
                3000,
            )
            .await
            .unwrap();
    }
    assert_eq!(store.info("legacy", 3000).await.unwrap().status, "active");
    let user = store.guest(3000).await.unwrap().0;
    store
        .create(&user.id, Default::default(), 3000)
        .await
        .unwrap();
}
#[tokio::test]
async fn malformed_legacy_log_leaves_the_database_empty() {
    let (_, store) = common::database().await;
    let mut data = export();
    data["tables"]["events"][1]["seq"] = json!(8);
    assert!(matches!(
        store.import_sqlite(&data.to_string()).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        store.user("white").await,
        Err(Error::Unauthorized)
    ));
}
#[tokio::test]
async fn import_rejects_an_existing_database_without_changing_accounts() {
    let (_, store) = common::database().await;
    let (user, token) = store.guest(1000).await.unwrap();
    assert!(matches!(
        store.import_sqlite(&export().to_string()).await,
        Err(Error::Conflict(_))
    ));
    assert_eq!(store.session(&token, 1000).await.unwrap().id, user.id);
    assert!(matches!(
        store.user("white").await,
        Err(Error::Unauthorized)
    ));
}

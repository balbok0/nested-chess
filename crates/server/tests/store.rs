use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use nested_chess::Color;
use nested_chess_server::{
    auth::{Auth, AuthFlow, Identity},
    model::*,
    store::Store,
};
use sha2::{Digest, Sha256};
mod common;
use common::database;
use uuid::Uuid;

async fn registered(store: &Store) -> User {
    let guest = store.guest(1000).await.unwrap().0;
    store
        .finish_auth(
            AuthFlow {
                provider: "google".into(),
                verifier: String::new(),
                nonce: String::new(),
                user_id: Some(guest.id),
                linking: false,
            },
            Identity {
                issuer: "https://accounts.google.com".into(),
                subject: Uuid::new_v4().to_string(),
                name: "Player".into(),
            },
            1000,
        )
        .await
        .unwrap()
        .0
}

struct Match {
    id: String,
    white: String,
    black: String,
}
async fn pair(store: &Store, spec: CreateGame) -> Match {
    let a = if spec.rated {
        registered(store).await
    } else {
        store.guest(1000).await.unwrap().0
    };
    let b = if spec.rated {
        registered(store).await
    } else {
        store.guest(1000).await.unwrap().0
    };
    let created = store.create(&a.id, spec, 1000).await.unwrap();
    let joined = store.join(&created.info.id, &b.id, 1000).await.unwrap();
    Match {
        id: joined.info.id,
        white: joined.info.white.unwrap().id,
        black: joined.info.black.unwrap().id,
    }
}
fn command(seq: u32, action: Action) -> Command {
    Command {
        request_id: Uuid::new_v4().to_string(),
        expected_seq: seq,
        action,
    }
}

#[tokio::test]
async fn guest_session_expires() {
    let (_dir, store) = database().await;
    let (user, token) = store.guest(1000).await.unwrap();
    assert_eq!(store.session(&token, 1001).await.unwrap().id, user.id);
    assert!(matches!(
        store.session(&token, 1000 + SESSION_SECONDS * 1000).await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn logout_revokes_the_session() {
    let (_dir, store) = database().await;
    let (_, token) = store.guest(1000).await.unwrap();
    store.logout(&token).await.unwrap();
    assert!(matches!(
        store.session(&token, 1001).await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn online_routing_allows_black_to_open_an_untouched_board() {
    let (_dir, store) = database().await;
    let p = pair(&store, CreateGame::default()).await;
    store
        .command(
            &p.id,
            &p.white,
            command(
                1,
                Action::Move {
                    board: 0,
                    uci: "e2e4".into(),
                },
            ),
            1100,
        )
        .await
        .unwrap();
    store
        .command(
            &p.id,
            &p.black,
            command(
                2,
                Action::Move {
                    board: 1,
                    uci: "e7e5".into(),
                },
            ),
            1200,
        )
        .await
        .unwrap();
    let actual = store.snapshot(&p.id, &p.white, 1200).await.unwrap();
    assert_eq!(actual.info.seq, 3);
    assert_eq!(actual.state.turn, Color::White);
    assert_eq!(actual.state.boards[1].white_moves, 0);
    assert_eq!(actual.state.boards[1].black_moves, 1);
    assert_eq!(actual.state.forced_board, Some(3));
}

#[tokio::test]
async fn rejected_commands_preserve_the_position_and_sequence() {
    for case in [
        "wrong_player",
        "wrong_board",
        "stale_sequence",
        "illegal_move",
    ] {
        let (_dir, store) = database().await;
        let p = pair(&store, CreateGame::default()).await;
        store
            .command(
                &p.id,
                &p.white,
                command(
                    1,
                    Action::Move {
                        board: 0,
                        uci: "e2e4".into(),
                    },
                ),
                1100,
            )
            .await
            .unwrap();
        let before = store.snapshot(&p.id, &p.white, 1200).await.unwrap();
        let actor = if case == "wrong_player" {
            &p.white
        } else {
            &p.black
        };
        let seq = if case == "stale_sequence" { 1 } else { 2 };
        let board = if case == "wrong_board" { 0 } else { 1 };
        let uci = if case == "illegal_move" {
            "e7e2"
        } else {
            "e7e5"
        };
        assert!(
            store
                .command(
                    &p.id,
                    actor,
                    command(
                        seq,
                        Action::Move {
                            board,
                            uci: uci.into()
                        }
                    ),
                    1200
                )
                .await
                .is_err(),
            "{case}"
        );
        let after = store.snapshot(&p.id, &p.white, 1200).await.unwrap();
        assert_eq!(after.save, before.save, "{case}");
        assert_eq!(after.info.seq, before.info.seq, "{case}");
    }
}

#[tokio::test]
async fn a_retried_move_returns_its_original_event() {
    let (_dir, store) = database().await;
    let p = pair(&store, CreateGame::default()).await;
    let request = command(
        1,
        Action::Move {
            board: 0,
            uci: "e2e4".into(),
        },
    );
    let first = store
        .command(&p.id, &p.white, request.clone(), 1100)
        .await
        .unwrap();
    let retry = store.command(&p.id, &p.white, request, 2000).await.unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(retry).unwrap()
    );
    assert_eq!(
        store
            .snapshot(&p.id, &p.white, 2000)
            .await
            .unwrap()
            .state
            .ply,
        1
    );
}

#[tokio::test]
async fn request_ids_cannot_be_reused_for_other_actions() {
    let (_dir, store) = database().await;
    let p = pair(&store, CreateGame::default()).await;
    let mut request = command(
        1,
        Action::Move {
            board: 0,
            uci: "e2e4".into(),
        },
    );
    store
        .command(&p.id, &p.white, request.clone(), 1100)
        .await
        .unwrap();
    request.action = Action::Resign;
    assert!(matches!(
        store.command(&p.id, &p.white, request, 1200).await,
        Err(Error::Conflict(_))
    ));
}

#[tokio::test]
async fn another_user_cannot_take_an_occupied_seat() {
    let (_dir, store) = database().await;
    let p = pair(&store, CreateGame::default()).await;
    let outsider = store.guest(1000).await.unwrap().0;
    assert!(matches!(
        store.join(&p.id, &outsider.id, 1100).await,
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        store.snapshot(&p.id, &outsider.id, 1100).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn untimed_games_and_sessions_survive_a_restart() {
    let (dir, store) = database().await;
    let p = pair(&store, CreateGame::default()).await;
    let (_, token) = store.guest(1000).await.unwrap();
    store
        .command(
            &p.id,
            &p.white,
            command(
                1,
                Action::Move {
                    board: 0,
                    uci: "e2e4".into(),
                },
            ),
            1100,
        )
        .await
        .unwrap();
    let before = store.snapshot(&p.id, &p.black, 1200).await.unwrap();
    drop(store);
    let reopened = Store::connect(&dir).await.unwrap();
    let actual = reopened.snapshot(&p.id, &p.black, 5000).await.unwrap();
    assert_eq!(actual.save, before.save);
    assert_eq!(actual.info.seq, 2);
    assert_eq!(actual.info.status, "active");
    assert!(reopened.session(&token, 5000).await.is_ok());
}

#[tokio::test]
async fn completed_moves_charge_the_global_clock_and_add_increment() {
    let (_dir, store) = database().await;
    let p = pair(
        &store,
        CreateGame {
            clock: Some(TimeControl::default()),
            ..Default::default()
        },
    )
    .await;
    store
        .command(
            &p.id,
            &p.white,
            command(
                1,
                Action::Move {
                    board: 0,
                    uci: "e2e4".into(),
                },
            ),
            2000,
        )
        .await
        .unwrap();
    let info = store.info(&p.id, 3000).await.unwrap();
    assert_eq!(info.white_ms, Some(909_000));
    assert_eq!(info.remaining_ms(Color::Black, 3000), Some(899_000));
}

#[tokio::test]
async fn another_replica_does_not_pause_running_clocks() {
    let (url, store) = database().await;
    let p = pair(
        &store,
        CreateGame {
            clock: Some(TimeControl::default()),
            ..Default::default()
        },
    )
    .await;
    let replica = Store::connect(&url).await;
    let info = replica.unwrap().info(&p.id, 2000).await;
    let info = info.unwrap();
    assert_eq!(info.status, "active");
    assert_eq!(info.seq, 1);
    assert_eq!(info.running_since_ms, Some(1000));
    assert_eq!(info.remaining_ms(Color::White, 2000), Some(899_000));
}
#[tokio::test]
async fn clocks_continue_when_all_application_processes_restart() {
    let (url, store) = database().await;
    let p = pair(
        &store,
        CreateGame {
            clock: Some(TimeControl::default()),
            ..Default::default()
        },
    )
    .await;
    store.tick(2000).await.unwrap();
    drop(store);
    let store = Store::connect(&url).await.unwrap();
    let info = store.info(&p.id, 100_000).await.unwrap();
    assert_eq!(info.status, "active");
    assert_eq!(info.seq, 1);
    assert_eq!(info.remaining_ms(Color::White, 100_000), Some(801_000));
}

#[tokio::test]
async fn an_expired_global_clock_finishes_the_match() {
    let (_dir, store) = database().await;
    let p = pair(
        &store,
        CreateGame {
            clock: Some(TimeControl {
                seconds: 60,
                increment: 0,
            }),
            ..Default::default()
        },
    )
    .await;
    store.tick(61_000).await.unwrap();
    let info = store.info(&p.id, 61_000).await.unwrap();
    assert_eq!(info.result.as_deref(), Some("black"));
    assert_eq!(info.reason.as_deref(), Some("timeout"));
}

#[tokio::test]
async fn rated_results_update_whole_match_ratings_once() {
    let (_dir, store) = database().await;
    let p = pair(
        &store,
        CreateGame {
            rated: true,
            clock: Some(TimeControl::default()),
            ..Default::default()
        },
    )
    .await;
    store
        .command(
            &p.id,
            &p.white,
            command(
                1,
                Action::Move {
                    board: 0,
                    uci: "e2e4".into(),
                },
            ),
            1100,
        )
        .await
        .unwrap();
    assert_eq!(store.user(&p.white).await.unwrap().rated_games, 0);
    let resign = command(2, Action::Resign);
    store
        .command(&p.id, &p.white, resign.clone(), 1200)
        .await
        .unwrap();
    store.command(&p.id, &p.white, resign, 1300).await.unwrap();
    assert_eq!(store.user(&p.white).await.unwrap().rating, 1484.0);
    assert_eq!(store.user(&p.black).await.unwrap().rating, 1516.0);
    assert_eq!(store.user(&p.white).await.unwrap().rated_games, 1);
    let history = store.history(&p.white, None, 1400).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].rating_before, Some(1500.0));
    assert_eq!(history[0].rating_after, Some(1484.0));
}

#[tokio::test]
async fn guests_cannot_enter_rated_matchmaking() {
    let (_dir, store) = database().await;
    let user = store.guest(1000).await.unwrap().0;
    assert!(matches!(
        store.queue(&user.id, 1100).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn a_rules_draw_records_one_half_point_match_result() {
    let (_dir, store) = database().await;
    let p = pair(
        &store,
        CreateGame {
            rated: true,
            clock: Some(TimeControl::default()),
            ..Default::default()
        },
    )
    .await;
    let moves = [
        (0, "b1a3"),
        (0, "b8a6"),
        (2, "b1a3"),
        (0, "a6b8"),
        (2, "a3b1"),
        (0, "b8a6"),
        (2, "b1a3"),
        (0, "a6b8"),
        (2, "a3b1"),
    ];
    for (index, (board, uci)) in moves.into_iter().enumerate() {
        let user = if index % 2 == 0 { &p.white } else { &p.black };
        store
            .command(
                &p.id,
                user,
                command(
                    index as u32 + 1,
                    Action::Move {
                        board,
                        uci: uci.into(),
                    },
                ),
                1100 + index as i64,
            )
            .await
            .unwrap();
    }
    let result = store.snapshot(&p.id, &p.white, 1200).await.unwrap();
    assert_eq!(result.info.result.as_deref(), Some("draw"));
    assert_eq!(result.info.reason.as_deref(), Some("rules"));
    assert_eq!(result.state.score.white, 2.0);
    assert_eq!(result.state.score.black, 2.0);
    assert_eq!(store.user(&p.white).await.unwrap().rating, 1500.0);
    assert_eq!(store.user(&p.white).await.unwrap().rated_games, 1);
}

#[tokio::test]
async fn authentication_state_is_bound_to_the_browser_and_consumed_once() {
    let (_dir, store) = database().await;
    let auth = Auth::new(
        "http://localhost:4180",
        vec![("google".into(), "client".into(), "secret".into())],
    )
    .unwrap();
    let start = auth.begin("google").unwrap();
    store
        .begin_auth(&start, "google", None, false, 1000)
        .await
        .unwrap();
    assert!(
        store
            .take_auth(&start.state, "other-browser", "google", 1100)
            .await
            .is_err()
    );
    assert!(
        store
            .take_auth(&start.state, &start.browser_token, "google", 1100)
            .await
            .is_ok()
    );
    assert!(
        store
            .take_auth(&start.state, &start.browser_token, "google", 1100)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rated_queue_pairs_registered_players_and_resumes_the_same_match() {
    let (_dir, store) = database().await;
    let a = registered(&store).await;
    let b = registered(&store).await;
    let waiting = store.queue(&a.id, 1100).await.unwrap();
    assert_eq!(waiting.info.status, "waiting");
    let joined = store.queue(&b.id, 1200).await.unwrap();
    assert_eq!(joined.info.id, waiting.info.id);
    assert_eq!(joined.info.status, "active");
    assert!(joined.info.rated);
    assert_eq!(joined.info.white_ms, Some(900_000));
    assert_eq!(joined.info.increment_ms, 10_000);
    assert_eq!(
        store.queue(&a.id, 1300).await.unwrap().info.id,
        joined.info.id
    );
}

#[tokio::test]
async fn provider_sign_in_upgrades_the_existing_guest_account() {
    let (_dir, store) = database().await;
    let guest = store.guest(1000).await.unwrap().0;
    let created = store
        .create(&guest.id, CreateGame::default(), 1100)
        .await
        .unwrap();
    let user = store
        .finish_auth(
            AuthFlow {
                provider: "github".into(),
                verifier: String::new(),
                nonce: String::new(),
                user_id: Some(guest.id.clone()),
                linking: false,
            },
            Identity {
                issuer: "github".into(),
                subject: "12345".into(),
                name: "new_player".into(),
            },
            1200,
        )
        .await
        .unwrap()
        .0;
    assert_eq!(user.id, guest.id);
    assert!(!user.guest);
    assert_eq!(
        store
            .snapshot(&created.info.id, &user.id, 1300)
            .await
            .unwrap()
            .info
            .id,
        created.info.id
    );
}

#[tokio::test]
async fn an_identity_cannot_be_linked_to_two_accounts() {
    let (_dir, store) = database().await;
    let owner = store
        .finish_auth(
            AuthFlow {
                provider: "github".into(),
                verifier: String::new(),
                nonce: String::new(),
                user_id: None,
                linking: false,
            },
            Identity {
                issuer: "github".into(),
                subject: "12345".into(),
                name: "owner".into(),
            },
            1000,
        )
        .await
        .unwrap()
        .0;
    let other = store.guest(1100).await.unwrap().0;
    let result = store
        .finish_auth(
            AuthFlow {
                provider: "github".into(),
                verifier: String::new(),
                nonce: String::new(),
                user_id: Some(other.id.clone()),
                linking: true,
            },
            Identity {
                issuer: "github".into(),
                subject: "12345".into(),
                name: "owner".into(),
            },
            1200,
        )
        .await;
    assert!(matches!(result, Err(Error::IdentityInUse)));
    assert!(store.user(&other.id).await.unwrap().guest);
    assert!(!store.user(&owner.id).await.unwrap().guest);
}

#[tokio::test]
async fn each_provider_uses_a_web_callback_and_pkce() {
    for (provider, host) in [
        ("google", "accounts.google.com"),
        ("github", "github.com"),
        ("microsoft", "login.microsoftonline.com"),
    ] {
        let auth = Auth::new(
            "http://localhost:4180",
            vec![(provider.into(), "client".into(), "secret".into())],
        )
        .unwrap();
        let start = auth.begin(provider).unwrap();
        let url = url::Url::parse(&start.url).unwrap();
        assert_eq!(url.host_str(), Some(host));
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            query.get("code_challenge_method").map(|v| v.as_ref()),
            Some("S256")
        );
        assert_eq!(
            query.get("redirect_uri").unwrap(),
            &format!("http://localhost:4180/auth/{provider}/callback")
        );
    }
}

#[tokio::test]
async fn switching_accounts_cannot_strand_an_open_guest_match() {
    let (_dir, store) = database().await;
    store
        .finish_auth(
            AuthFlow {
                provider: "github".into(),
                verifier: String::new(),
                nonce: String::new(),
                user_id: None,
                linking: false,
            },
            Identity {
                issuer: "github".into(),
                subject: "registered_owner".into(),
                name: "owner".into(),
            },
            1000,
        )
        .await
        .unwrap();
    let guest = store.guest(1100).await.unwrap().0;
    let game = store
        .create(&guest.id, CreateGame::default(), 1100)
        .await
        .unwrap();
    let result = store
        .finish_auth(
            AuthFlow {
                provider: "github".into(),
                verifier: String::new(),
                nonce: String::new(),
                user_id: Some(guest.id.clone()),
                linking: false,
            },
            Identity {
                issuer: "github".into(),
                subject: "registered_owner".into(),
                name: "owner".into(),
            },
            1200,
        )
        .await;
    assert!(matches!(result, Err(Error::GuestMatchesOpen)));
    assert!(store.snapshot(&game.info.id, &guest.id, 1300).await.is_ok());
    assert!(store.user(&guest.id).await.unwrap().guest);
}

#[tokio::test]
async fn mobile_handoff_requires_its_app_verifier_and_can_only_be_exchanged_once() {
    let (_dir, store) = database().await;
    let (user, token) = store.guest(1000).await.unwrap();
    let verifier = "a".repeat(43);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let request = store
        .request_mobile_login(
            "github",
            &challenge,
            &"s".repeat(43),
            Some(&token),
            false,
            1100,
        )
        .await
        .unwrap();
    let auth = Auth::new(
        "http://localhost:4180",
        vec![("github".into(), "client".into(), "secret".into())],
    )
    .unwrap();
    let start = auth.begin("github").unwrap();
    store
        .start_mobile_login(&request, "github", &start, 1200)
        .await
        .unwrap();
    let login = store.mobile_login(&start.state).await.unwrap().unwrap();
    let code = store
        .complete_mobile_login(&login.id, Some(&user.id), None, 1300)
        .await
        .unwrap();
    assert!(matches!(
        store
            .exchange_mobile_login(&code, &"b".repeat(43), 1400)
            .await,
        Err(Error::Unauthorized)
    ));
    let session = store
        .exchange_mobile_login(&code, &verifier, 1400)
        .await
        .unwrap();
    assert_eq!(session.user.id, user.id);
    assert_eq!(
        store.session(&session.token, 1401).await.unwrap().id,
        user.id
    );
    assert!(matches!(
        store.session(&token, 1401).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        store.exchange_mobile_login(&code, &verifier, 1401).await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn mobile_browser_start_is_provider_bound_and_single_use() {
    let (_dir, store) = database().await;
    let auth = Auth::new(
        "http://localhost:4180",
        vec![("github".into(), "client".into(), "secret".into())],
    )
    .unwrap();
    let start = auth.begin("github").unwrap();
    let request = store
        .request_mobile_login(
            "github",
            &"c".repeat(43),
            &"s".repeat(43),
            None,
            false,
            1000,
        )
        .await
        .unwrap();
    assert!(matches!(
        store
            .start_mobile_login(&request, "google", &start, 1100)
            .await,
        Err(Error::Unauthorized)
    ));
    store
        .start_mobile_login(&request, "github", &start, 1100)
        .await
        .unwrap();
    assert!(matches!(
        store
            .start_mobile_login(&request, "github", &auth.begin("github").unwrap(), 1100)
            .await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn mobile_handoff_expires_and_logout_cancels_pending_sign_in() {
    for logout in [false, true] {
        let (_dir, store) = database().await;
        let (user, token) = store.guest(1000).await.unwrap();
        let verifier = "a".repeat(43);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let request = store
            .request_mobile_login(
                "github",
                &challenge,
                &"s".repeat(43),
                Some(&token),
                false,
                1000,
            )
            .await
            .unwrap();
        let auth = Auth::new(
            "http://localhost:4180",
            vec![("github".into(), "client".into(), "secret".into())],
        )
        .unwrap();
        let start = auth.begin("github").unwrap();
        store
            .start_mobile_login(&request, "github", &start, 1100)
            .await
            .unwrap();
        let login = store.mobile_login(&start.state).await.unwrap().unwrap();
        let code = store
            .complete_mobile_login(&login.id, Some(&user.id), None, 1200)
            .await
            .unwrap();
        if logout {
            store.logout(&token).await.unwrap();
        }
        let now = if logout { 1300 } else { 61_200 };
        assert!(matches!(
            store.exchange_mobile_login(&code, &verifier, now).await,
            Err(Error::Unauthorized)
        ));
    }
}

#[tokio::test]
async fn stream_tickets_are_match_bound_single_use_and_expire() {
    let (_dir, store) = database().await;
    let (user, token) = store.guest(1000).await.unwrap();
    let a = store
        .create(&user.id, CreateGame::default(), 1000)
        .await
        .unwrap();
    let b = store
        .create(&user.id, CreateGame::default(), 1000)
        .await
        .unwrap();
    let ticket = store.stream_ticket(&a.info.id, &token, 1100).await.unwrap();
    assert!(matches!(
        store.take_stream_ticket(&b.info.id, &ticket, 1200).await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        store
            .take_stream_ticket(&a.info.id, &ticket, 1200)
            .await
            .unwrap()
            .0
            .id,
        user.id
    );
    assert!(matches!(
        store.take_stream_ticket(&a.info.id, &ticket, 1200).await,
        Err(Error::Unauthorized)
    ));
    let ticket = store.stream_ticket(&a.info.id, &token, 1100).await.unwrap();
    assert!(matches!(
        store.take_stream_ticket(&a.info.id, &ticket, 31_100).await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn stream_tickets_do_not_outlive_their_session() {
    let (_dir, store) = database().await;
    let (user, token) = store.guest(1000).await.unwrap();
    let game = store
        .create(&user.id, CreateGame::default(), 1000)
        .await
        .unwrap();
    let ticket = store
        .stream_ticket(&game.info.id, &token, 1100)
        .await
        .unwrap();
    store.logout(&token).await.unwrap();
    assert!(matches!(
        store.take_stream_ticket(&game.info.id, &ticket, 1200).await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn conflicting_moves_on_different_replicas_commit_once() {
    let (url, store) = database().await;
    let other = Store::connect(&url).await.unwrap();
    let p = pair(&store, CreateGame::default()).await;
    let a = command(
        1,
        Action::Move {
            board: 0,
            uci: "e2e4".into(),
        },
    );
    let b = command(
        1,
        Action::Move {
            board: 0,
            uci: "d2d4".into(),
        },
    );
    let (a, b) = tokio::join!(
        store.command(&p.id, &p.white, a, 1100),
        other.command(&p.id, &p.white, b, 1100)
    );
    assert!(matches!(
        (&a, &b),
        (Ok(_), Err(Error::Conflict(_))) | (Err(Error::Conflict(_)), Ok(_))
    ));
    let result = other.snapshot(&p.id, &p.white, 1200).await.unwrap();
    assert_eq!(result.info.seq, 2);
    assert_eq!(result.state.ply, 1);
    assert_eq!(store.events_after(&p.id, 1).await.unwrap().len(), 1);
}
#[tokio::test]
async fn simultaneous_retries_return_one_committed_event_across_replicas() {
    let (url, store) = database().await;
    let other = Store::connect(&url).await.unwrap();
    let p = pair(&store, CreateGame::default()).await;
    let request = command(
        1,
        Action::Move {
            board: 0,
            uci: "e2e4".into(),
        },
    );
    let (a, b) = tokio::join!(
        store.command(&p.id, &p.white, request.clone(), 1100),
        other.command(&p.id, &p.white, request, 1200)
    );
    assert_eq!(
        serde_json::to_value(a.unwrap()).unwrap(),
        serde_json::to_value(b.unwrap()).unwrap()
    );
    assert_eq!(
        other
            .snapshot(&p.id, &p.white, 1300)
            .await
            .unwrap()
            .state
            .ply,
        1
    );
}
#[tokio::test]
async fn competing_joiners_cannot_overwrite_the_occupied_seat() {
    let (url, store) = database().await;
    let other = Store::connect(&url).await.unwrap();
    let owner = store.guest(1000).await.unwrap().0;
    let a = store.guest(1000).await.unwrap().0;
    let b = store.guest(1000).await.unwrap().0;
    let game = store
        .create(&owner.id, CreateGame::default(), 1000)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        store.join(&game.info.id, &a.id, 1100),
        other.join(&game.info.id, &b.id, 1100)
    );
    assert!(matches!(
        (&a, &b),
        (Ok(_), Err(Error::Conflict(_))) | (Err(Error::Conflict(_)), Ok(_))
    ));
    assert_eq!(store.info(&game.info.id, 1200).await.unwrap().seq, 1);
}
#[tokio::test]
async fn competing_queue_requests_pair_players_once() {
    let (url, store) = database().await;
    let other = Store::connect(&url).await.unwrap();
    let a = registered(&store).await;
    let b = registered(&store).await;
    let (a, b) = tokio::join!(store.queue(&a.id, 1100), other.queue(&b.id, 1100));
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.info.id, b.info.id);
    let info = store.info(&a.info.id, 1200).await.unwrap();
    assert_eq!(info.seq, 1);
    assert_eq!(info.status, "active");
}
#[tokio::test]
async fn simultaneous_timeout_workers_apply_ratings_once() {
    let (url, store) = database().await;
    let other = Store::connect(&url).await.unwrap();
    let p = pair(
        &store,
        CreateGame {
            rated: true,
            clock: Some(TimeControl::default()),
            ..Default::default()
        },
    )
    .await;
    let (a, b) = tokio::join!(store.tick(901_000), other.tick(901_000));
    a.unwrap();
    b.unwrap();
    assert_eq!(store.user(&p.white).await.unwrap().rated_games, 1);
    assert_eq!(store.user(&p.black).await.unwrap().rating, 1516.0);
    assert_eq!(other.events_after(&p.id, 1).await.unwrap().len(), 1);
}
#[tokio::test]
async fn concurrent_authentication_consumes_state_once() {
    let (url, store) = database().await;
    let other = Store::connect(&url).await.unwrap();
    let auth = Auth::new(
        "http://localhost:4180",
        vec![("github".into(), "client".into(), "secret".into())],
    )
    .unwrap();
    let start = auth.begin("github").unwrap();
    store
        .begin_auth(&start, "github", None, false, 1000)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        store.take_auth(&start.state, &start.browser_token, "github", 1100),
        other.take_auth(&start.state, &start.browser_token, "github", 1100)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
}
#[tokio::test]
async fn concurrent_native_stream_tickets_are_single_use() {
    let (url, store) = database().await;
    let other = Store::connect(&url).await.unwrap();
    let (user, session) = store.guest(1000).await.unwrap();
    let game = store
        .create(&user.id, CreateGame::default(), 1000)
        .await
        .unwrap();
    let ticket = store
        .stream_ticket(&game.info.id, &session, 1000)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        store.take_stream_ticket(&game.info.id, &ticket, 1100),
        other.take_stream_ticket(&game.info.id, &ticket, 1100)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
}

#[tokio::test]
async fn rejoining_while_a_rated_match_finishes_does_not_deadlock() {
    let (url, store) = database().await;
    let other = Store::connect(&url).await.unwrap();
    for _ in 0..12 {
        let p = pair(
            &store,
            CreateGame {
                rated: true,
                clock: Some(TimeControl::default()),
                ..Default::default()
            },
        )
        .await;
        let (joined, finished) = tokio::join!(
            store.join(&p.id, &p.white, 1100),
            other.command(&p.id, &p.black, command(1, Action::Resign), 1100)
        );
        joined.unwrap();
        finished.unwrap();
        assert_eq!(store.user(&p.white).await.unwrap().rated_games, 1);
    }
}

#[tokio::test]
async fn concurrent_invitation_retries_create_one_match_across_replicas() {
    let (url, store) = database().await;
    let other = Store::connect(&url).await.unwrap();
    let user = store.guest(1000).await.unwrap().0;
    let spec = CreateGame {
        request_id: Some(Uuid::new_v4().to_string()),
        ..Default::default()
    };
    let (a, b) = tokio::join!(
        store.create(&user.id, spec.clone(), 1000),
        other.create(&user.id, spec, 1100)
    );
    assert_eq!(a.unwrap().info.id, b.unwrap().info.id);
    assert_eq!(store.open_games(&user.id, 1200).await.unwrap().len(), 1);
}

#[tokio::test]
async fn an_invitation_retry_returns_the_joined_match_after_restart() {
    let (url, store) = database().await;
    let owner = store.guest(1000).await.unwrap().0;
    let guest = store.guest(1000).await.unwrap().0;
    let spec = CreateGame {
        request_id: Some(Uuid::new_v4().to_string()),
        ..Default::default()
    };
    let created = store.create(&owner.id, spec.clone(), 1000).await.unwrap();
    store.join(&created.info.id, &guest.id, 1100).await.unwrap();
    drop(store);
    let store = Store::connect(&url).await.unwrap();
    let retried = store.create(&owner.id, spec, 1200).await.unwrap();
    assert_eq!(retried.info.id, created.info.id);
    assert_eq!(retried.info.status, "active");
    assert_eq!(retried.info.seq, 1);
}

#[tokio::test]
async fn invitation_request_ids_cannot_change_settings() {
    let (_, store) = database().await;
    let user = store.guest(1000).await.unwrap().0;
    let mut spec = CreateGame {
        request_id: Some(Uuid::new_v4().to_string()),
        ..Default::default()
    };
    store.create(&user.id, spec.clone(), 1000).await.unwrap();
    spec.size = 4;
    assert!(matches!(
        store.create(&user.id, spec, 1100).await,
        Err(Error::Conflict(_))
    ));
    assert_eq!(store.open_games(&user.id, 1200).await.unwrap().len(), 1);
}

#[tokio::test]
async fn invitation_request_ids_are_scoped_to_the_account() {
    let (_, store) = database().await;
    let a = store.guest(1000).await.unwrap().0;
    let b = store.guest(1000).await.unwrap().0;
    let spec = CreateGame {
        request_id: Some(Uuid::new_v4().to_string()),
        ..Default::default()
    };
    let a = store.create(&a.id, spec.clone(), 1000).await.unwrap();
    let b = store.create(&b.id, spec, 1000).await.unwrap();
    assert_ne!(a.info.id, b.info.id);
}

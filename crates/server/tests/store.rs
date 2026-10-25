use nested_chess::Color;
use nested_chess_server::{
    auth::{Auth, AuthFlow, Identity},
    model::*,
    store::Store,
};
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

fn database() -> (TempDir, Store) {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/claude/20261025_online-tests");
    std::fs::create_dir_all(&parent).unwrap();
    let dir = TempDir::new_in(parent).unwrap();
    let store = Store::open(&dir.path().join("app.sqlite"), 1000).unwrap();
    (dir, store)
}

fn registered(store: &mut Store) -> User {
    let guest = store.guest(1000).unwrap().0;
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
        .unwrap()
        .0
}

struct Match {
    id: String,
    white: String,
    black: String,
}
fn pair(store: &mut Store, spec: CreateGame) -> Match {
    let a = if spec.rated {
        registered(store)
    } else {
        store.guest(1000).unwrap().0
    };
    let b = if spec.rated {
        registered(store)
    } else {
        store.guest(1000).unwrap().0
    };
    let created = store.create(&a.id, spec, 1000).unwrap();
    let joined = store.join(&created.info.id, &b.id, 1000).unwrap();
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

#[test]
fn guest_session_expires() {
    let (_dir, mut store) = database();
    let (user, token) = store.guest(1000).unwrap();
    assert_eq!(store.session(&token, 1001).unwrap().id, user.id);
    assert!(matches!(
        store.session(&token, 1000 + SESSION_SECONDS * 1000),
        Err(Error::Unauthorized)
    ));
}

#[test]
fn logout_revokes_the_session() {
    let (_dir, mut store) = database();
    let (_, token) = store.guest(1000).unwrap();
    store.logout(&token).unwrap();
    assert!(matches!(
        store.session(&token, 1001),
        Err(Error::Unauthorized)
    ));
}

#[test]
fn online_routing_allows_black_to_open_an_untouched_board() {
    let (_dir, mut store) = database();
    let p = pair(&mut store, CreateGame::default());
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
        .unwrap();
    let actual = store.snapshot(&p.id, &p.white, 1200).unwrap();
    assert_eq!(actual.info.seq, 3);
    assert_eq!(actual.state.turn, Color::White);
    assert_eq!(actual.state.boards[1].white_moves, 0);
    assert_eq!(actual.state.boards[1].black_moves, 1);
    assert_eq!(actual.state.forced_board, Some(3));
}

#[test]
fn rejected_commands_preserve_the_position_and_sequence() {
    for case in [
        "wrong_player",
        "wrong_board",
        "stale_sequence",
        "illegal_move",
    ] {
        let (_dir, mut store) = database();
        let p = pair(&mut store, CreateGame::default());
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
            .unwrap();
        let before = store.snapshot(&p.id, &p.white, 1200).unwrap();
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
                .is_err(),
            "{case}"
        );
        let after = store.snapshot(&p.id, &p.white, 1200).unwrap();
        assert_eq!(after.save, before.save, "{case}");
        assert_eq!(after.info.seq, before.info.seq, "{case}");
    }
}

#[test]
fn a_retried_move_returns_its_original_event() {
    let (_dir, mut store) = database();
    let p = pair(&mut store, CreateGame::default());
    let request = command(
        1,
        Action::Move {
            board: 0,
            uci: "e2e4".into(),
        },
    );
    let first = store
        .command(&p.id, &p.white, request.clone(), 1100)
        .unwrap();
    let retry = store.command(&p.id, &p.white, request, 2000).unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(retry).unwrap()
    );
    assert_eq!(store.snapshot(&p.id, &p.white, 2000).unwrap().state.ply, 1);
}

#[test]
fn request_ids_cannot_be_reused_for_other_actions() {
    let (_dir, mut store) = database();
    let p = pair(&mut store, CreateGame::default());
    let mut request = command(
        1,
        Action::Move {
            board: 0,
            uci: "e2e4".into(),
        },
    );
    store
        .command(&p.id, &p.white, request.clone(), 1100)
        .unwrap();
    request.action = Action::Resign;
    assert!(matches!(
        store.command(&p.id, &p.white, request, 1200),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn another_user_cannot_take_an_occupied_seat() {
    let (_dir, mut store) = database();
    let p = pair(&mut store, CreateGame::default());
    let outsider = store.guest(1000).unwrap().0;
    assert!(matches!(
        store.join(&p.id, &outsider.id, 1100),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        store.snapshot(&p.id, &outsider.id, 1100),
        Err(Error::Forbidden(_))
    ));
}

#[test]
fn untimed_games_and_sessions_survive_a_restart() {
    let (dir, mut store) = database();
    let p = pair(&mut store, CreateGame::default());
    let (_, token) = store.guest(1000).unwrap();
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
        .unwrap();
    let before = store.snapshot(&p.id, &p.black, 1200).unwrap();
    drop(store);
    let mut reopened = Store::open(&dir.path().join("app.sqlite"), 5000).unwrap();
    let actual = reopened.snapshot(&p.id, &p.black, 5000).unwrap();
    assert_eq!(actual.save, before.save);
    assert_eq!(actual.info.seq, 2);
    assert_eq!(actual.info.status, "active");
    assert!(reopened.session(&token, 5000).is_ok());
}

#[test]
fn completed_moves_charge_the_global_clock_and_add_increment() {
    let (_dir, mut store) = database();
    let p = pair(
        &mut store,
        CreateGame {
            clock: Some(TimeControl::default()),
            ..Default::default()
        },
    );
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
        .unwrap();
    let info = store.info(&p.id, 3000).unwrap();
    assert_eq!(info.white_ms, Some(909_000));
    assert_eq!(info.remaining_ms(Color::Black, 3000), Some(899_000));
}

#[test]
fn a_second_database_owner_cannot_pause_a_running_match() {
    let (dir, mut store) = database();
    let p = pair(
        &mut store,
        CreateGame {
            clock: Some(TimeControl::default()),
            ..Default::default()
        },
    );
    assert!(matches!(
        Store::open(&dir.path().join("app.sqlite"), 2000),
        Err(Error::Conflict(_))
    ));
    let info = store.info(&p.id, 2000).unwrap();
    assert_eq!(info.status, "active");
    assert_eq!(info.seq, 1);
    assert_eq!(info.running_since_ms, Some(1000));
}

#[test]
fn a_restart_pauses_clocked_games_until_both_players_resume() {
    let (dir, mut store) = database();
    let p = pair(
        &mut store,
        CreateGame {
            clock: Some(TimeControl::default()),
            ..Default::default()
        },
    );
    store.tick(2000).unwrap();
    drop(store);
    let mut store = Store::open(&dir.path().join("app.sqlite"), 100_000).unwrap();
    assert_eq!(store.info(&p.id, 100_000).unwrap().white_ms, Some(899_000));
    assert_eq!(store.info(&p.id, 100_000).unwrap().status, "paused");
    store
        .command(&p.id, &p.white, command(2, Action::Resume), 100_100)
        .unwrap();
    assert_eq!(store.info(&p.id, 100_200).unwrap().status, "paused");
    store
        .command(&p.id, &p.black, command(3, Action::Resume), 100_300)
        .unwrap();
    let info = store.info(&p.id, 100_400).unwrap();
    assert_eq!(info.status, "active");
    assert_eq!(info.remaining_ms(Color::White, 100_400), Some(898_900));
}

#[test]
fn an_expired_global_clock_finishes_the_match() {
    let (_dir, mut store) = database();
    let p = pair(
        &mut store,
        CreateGame {
            clock: Some(TimeControl {
                seconds: 60,
                increment: 0,
            }),
            ..Default::default()
        },
    );
    store.tick(61_000).unwrap();
    let info = store.info(&p.id, 61_000).unwrap();
    assert_eq!(info.result.as_deref(), Some("black"));
    assert_eq!(info.reason.as_deref(), Some("timeout"));
}

#[test]
fn rated_results_update_whole_match_ratings_once() {
    let (_dir, mut store) = database();
    let p = pair(
        &mut store,
        CreateGame {
            rated: true,
            clock: Some(TimeControl::default()),
            ..Default::default()
        },
    );
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
        .unwrap();
    assert_eq!(store.user(&p.white).unwrap().rated_games, 0);
    let resign = command(2, Action::Resign);
    store
        .command(&p.id, &p.white, resign.clone(), 1200)
        .unwrap();
    store.command(&p.id, &p.white, resign, 1300).unwrap();
    assert_eq!(store.user(&p.white).unwrap().rating, 1484.0);
    assert_eq!(store.user(&p.black).unwrap().rating, 1516.0);
    assert_eq!(store.user(&p.white).unwrap().rated_games, 1);
    let history = store.history(&p.white, None, 1400).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].rating_before, Some(1500.0));
    assert_eq!(history[0].rating_after, Some(1484.0));
}

#[test]
fn guests_cannot_enter_rated_matchmaking() {
    let (_dir, mut store) = database();
    let user = store.guest(1000).unwrap().0;
    assert!(matches!(
        store.queue(&user.id, 1100),
        Err(Error::Forbidden(_))
    ));
}

#[test]
fn a_rules_draw_records_one_half_point_match_result() {
    let (_dir, mut store) = database();
    let p = pair(
        &mut store,
        CreateGame {
            rated: true,
            clock: Some(TimeControl::default()),
            ..Default::default()
        },
    );
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
            .unwrap();
    }
    let result = store.snapshot(&p.id, &p.white, 1200).unwrap();
    assert_eq!(result.info.result.as_deref(), Some("draw"));
    assert_eq!(result.info.reason.as_deref(), Some("rules"));
    assert_eq!(result.state.score.white, 2.0);
    assert_eq!(result.state.score.black, 2.0);
    assert_eq!(store.user(&p.white).unwrap().rating, 1500.0);
    assert_eq!(store.user(&p.white).unwrap().rated_games, 1);
}

#[test]
fn authentication_state_is_bound_to_the_browser_and_consumed_once() {
    let (_dir, mut store) = database();
    let auth = Auth::new(
        "http://localhost:4180",
        vec![("google".into(), "client".into(), "secret".into())],
    )
    .unwrap();
    let start = auth.begin("google").unwrap();
    store
        .begin_auth(&start, "google", None, false, 1000)
        .unwrap();
    assert!(
        store
            .take_auth(&start.state, "other-browser", "google", 1100)
            .is_err()
    );
    assert!(
        store
            .take_auth(&start.state, &start.browser_token, "google", 1100)
            .is_ok()
    );
    assert!(
        store
            .take_auth(&start.state, &start.browser_token, "google", 1100)
            .is_err()
    );
}

#[test]
fn rated_queue_pairs_registered_players_and_resumes_the_same_match() {
    let (_dir, mut store) = database();
    let a = registered(&mut store);
    let b = registered(&mut store);
    let waiting = store.queue(&a.id, 1100).unwrap();
    assert_eq!(waiting.info.status, "waiting");
    let joined = store.queue(&b.id, 1200).unwrap();
    assert_eq!(joined.info.id, waiting.info.id);
    assert_eq!(joined.info.status, "active");
    assert!(joined.info.rated);
    assert_eq!(joined.info.white_ms, Some(900_000));
    assert_eq!(joined.info.increment_ms, 10_000);
    assert_eq!(store.queue(&a.id, 1300).unwrap().info.id, joined.info.id);
}

#[test]
fn provider_sign_in_upgrades_the_existing_guest_account() {
    let (_dir, mut store) = database();
    let guest = store.guest(1000).unwrap().0;
    let created = store
        .create(&guest.id, CreateGame::default(), 1100)
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
        .unwrap()
        .0;
    assert_eq!(user.id, guest.id);
    assert!(!user.guest);
    assert_eq!(
        store
            .snapshot(&created.info.id, &user.id, 1300)
            .unwrap()
            .info
            .id,
        created.info.id
    );
}

#[test]
fn an_identity_cannot_be_linked_to_two_accounts() {
    let (_dir, mut store) = database();
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
        .unwrap()
        .0;
    let other = store.guest(1100).unwrap().0;
    let result = store.finish_auth(
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
    );
    assert!(matches!(result, Err(Error::IdentityInUse)));
    assert!(store.user(&other.id).unwrap().guest);
    assert!(!store.user(&owner.id).unwrap().guest);
}

#[test]
fn each_provider_uses_a_web_callback_and_pkce() {
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

#[test]
fn switching_accounts_cannot_strand_an_open_guest_match() {
    let (_dir, mut store) = database();
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
        .unwrap();
    let guest = store.guest(1100).unwrap().0;
    let game = store
        .create(&guest.id, CreateGame::default(), 1100)
        .unwrap();
    let result = store.finish_auth(
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
    );
    assert!(matches!(result, Err(Error::GuestMatchesOpen)));
    assert!(store.snapshot(&game.info.id, &guest.id, 1300).is_ok());
    assert!(store.user(&guest.id).unwrap().guest);
}

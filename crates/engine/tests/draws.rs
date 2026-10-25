use nested_chess::{Color, Game, Outcome};

#[test]
fn agreed_draw_awards_half_each_without_consuming_a_turn() {
    let mut game = Game::new(2).unwrap();
    let initial = game.state();
    game.agree_draw(0).unwrap();
    let actual = game.state();
    assert_eq!((actual.score.white, actual.score.black), (0.5, 0.5));
    assert_eq!(actual.turn, Color::White);
    assert_eq!(actual.ply, 0);
    assert_eq!(actual.boards[0].outcome, Outcome::Draw);
    assert_eq!(actual.legal_boards, vec![1, 2, 3]);
    assert_eq!(
        Game::load_json(&game.save_json().unwrap()).unwrap().state(),
        actual
    );
    game.undo().unwrap();
    assert_eq!(game.state(), initial);
}

#[test]
fn drawing_the_required_board_releases_the_route() {
    let mut game = Game::new(2).unwrap();
    game.play(0, "e2e4").unwrap();
    let before = game.state();
    game.agree_draw(1).unwrap();
    let actual = game.state();
    assert_eq!(actual.turn, Color::Black);
    assert_eq!(actual.ply, 1);
    assert_eq!(actual.forced_board, None);
    assert_eq!(actual.legal_boards, vec![0, 2, 3]);
    game.undo().unwrap();
    assert_eq!(game.state(), before);
}

#[test]
fn majority_points_end_the_match_before_all_boards_finish() {
    let mut game = Game::from_fens(
        2,
        Color::White,
        &vec!["4k3/8/8/8/8/8/4R3/4K3 w - - 0 1".into(); 4],
    )
    .unwrap();
    for (board, mv) in [(0, "e2e8"), (3, "e8d8"), (2, "e2e8"), (3, "d8c8")] {
        game.play(board, mv).unwrap();
    }
    assert_eq!(game.outcome(), Outcome::Ongoing);
    game.agree_draw(1).unwrap();
    let actual = game.state();
    assert_eq!((actual.score.white, actual.score.black), (2.5, 0.5));
    assert_eq!(actual.score.remaining, 1);
    assert_eq!(actual.outcome, Outcome::White);
    assert!(actual.legal_boards.is_empty());
    assert!(game.play(3, "e2e3").is_err());
    game.undo().unwrap();
    assert_eq!(game.outcome(), Outcome::Ongoing);
}

#[test]
fn rejected_draws_leave_the_match_unchanged() {
    let mut game = Game::new(2).unwrap();
    game.agree_draw(0).unwrap();
    let expected = game.state();
    for board in [0, 64] {
        assert!(game.agree_draw(board).is_err());
        assert_eq!(game.state(), expected);
    }
}

#[test]
fn all_drawn_boards_produce_equal_total_points() {
    for size in [2, 4, 8] {
        let mut game = Game::new(size).unwrap();
        for board in 0..usize::from(size).pow(2) {
            game.agree_draw(board).unwrap();
        }
        assert_eq!(game.outcome(), Outcome::Draw);
        assert_eq!(game.score().white, f32::from(size).powi(2) / 2.0);
        assert_eq!(game.score().white, game.score().black);
    }
}

#[test]
fn version_one_games_keep_their_original_finish_rule() {
    let moves: Vec<_> = [
        (0, "e2e8"),
        (3, "e8d8"),
        (2, "e2e8"),
        (3, "d8c8"),
        (1, "e2e8"),
        (3, "c8b8"),
        (3, "e2e8"),
        (3, "b8a8"),
        (3, "e8a8"),
    ]
    .into_iter()
    .map(|(board, uci)| serde_json::json!({"board":board,"uci":uci}))
    .collect();
    let old = serde_json::json!({"version":1,"size":2,"first_player":"white","initial_fens":vec!["4k3/8/8/8/8/8/4R3/4K3 w - - 0 1";4],"moves":moves});
    let game = Game::load_json(&old.to_string()).unwrap();
    assert_eq!(game.state().ply, 9);
    assert_eq!(game.score().white, 4.0);
    assert_eq!(game.outcome(), Outcome::White);
    assert_eq!(
        Game::load_json(&game.save_json().unwrap()).unwrap().state(),
        game.state()
    );
}

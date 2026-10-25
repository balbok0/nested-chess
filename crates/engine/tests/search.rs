use nested_chess::{
    Color, Game, Outcome,
    search::{LocalEvaluation, OutcomeEstimate, SearchLimits, match_estimate},
    uci::UciEngine,
};

fn two_losses() -> Game {
    let mut game = Game::from_fens(
        2,
        Color::Black,
        &[
            "7k/8/8/8/8/8/7R/Kr6 w - - 0 1".into(),
            "4k3/8/8/8/8/8/7r/7K w - - 0 1".into(),
            "3Kk3/8/8/8/8/8/3r4/8 b - - 0 1".into(),
            "3Kk3/8/8/8/8/8/3r4/8 b - - 0 1".into(),
        ],
    )
    .unwrap();
    for (board, uci) in [(2, "d2d8"), (0, "h2h7"), (3, "d2d8")] {
        game.play(board, uci).unwrap();
    }
    game
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

fn oracle() -> UciEngine {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    UciEngine::spawn(
        "node",
        &[root.join("scripts/fairy-uci.cjs").to_str().unwrap().into()],
        root.join("variants/nested.ini").to_str().unwrap(),
    )
    .unwrap()
}

#[test]
fn two_losses_require_both_remaining_wins_just_to_draw() {
    let actual = match_estimate(
        &two_losses(),
        &[OutcomeEstimate::new(0.8, 0.0, 0.2).unwrap(); 2],
    )
    .unwrap();
    close(actual.white, 0.0);
    close(actual.draw, 0.64);
    close(actual.black, 0.36);
    close(actual.expected_score(Color::White), 0.32);
}

#[test]
fn uncertain_board_draws_contribute_half_points() {
    let game = Game::new(2).unwrap();
    let actual = match_estimate(
        &game,
        &[
            OutcomeEstimate::new(1.0, 0.0, 0.0).unwrap(),
            OutcomeEstimate::new(0.0, 0.0, 1.0).unwrap(),
            OutcomeEstimate::new(0.0, 1.0, 0.0).unwrap(),
            OutcomeEstimate::new(0.2, 0.3, 0.5).unwrap(),
        ],
    )
    .unwrap();
    close(actual.white, 0.2);
    close(actual.draw, 0.3);
    close(actual.black, 0.5);
}

#[test]
fn settled_draws_shift_the_majority_threshold() {
    let mut game = Game::new(2).unwrap();
    for board in 0..3 {
        game.agree_draw(board).unwrap();
    }
    let actual = match_estimate(&game, &[OutcomeEstimate::new(0.2, 0.3, 0.5).unwrap()]).unwrap();
    close(actual.white, 0.2);
    close(actual.draw, 0.3);
    close(actual.black, 0.5);
}

#[test]
fn deterministic_predictions_work_on_every_grid() {
    for size in [2, 4, 8] {
        for (estimate, expected) in [
            (
                OutcomeEstimate::new(1.0, 0.0, 0.0).unwrap(),
                (1.0, 0.0, 0.0),
            ),
            (
                OutcomeEstimate::new(0.0, 1.0, 0.0).unwrap(),
                (0.0, 1.0, 0.0),
            ),
            (
                OutcomeEstimate::new(0.0, 0.0, 1.0).unwrap(),
                (0.0, 0.0, 1.0),
            ),
        ] {
            let actual = match_estimate(
                &Game::new(size).unwrap(),
                &vec![estimate; usize::from(size).pow(2)],
            )
            .unwrap();
            close(actual.white, expected.0);
            close(actual.draw, expected.1);
            close(actual.black, expected.2);
        }
    }
}

#[test]
fn a_third_loss_overrides_all_remaining_estimates() {
    let mut game = two_losses();
    game.play(0, "h7h8").unwrap();
    game.play(1, "h2h1").unwrap();
    assert_eq!(game.outcome(), Outcome::Black);
    let actual = match_estimate(&game, &[]).unwrap();
    assert_eq!(actual.expected_score(Color::White), 0.0);
    assert_eq!(actual.expected_score(Color::Black), 1.0);
}

#[test]
fn invalid_probabilities_and_estimate_counts_are_rejected() {
    for (w, d, b) in [
        (-0.1, 0.5, 0.6),
        (f64::NAN, 0.0, 1.0),
        (f64::INFINITY, 0.0, 0.0),
        (0.2, 0.2, 0.2),
    ] {
        assert!(OutcomeEstimate::new(w, d, b).is_err());
    }
    assert!(match_estimate(&Game::new(2).unwrap(), &[]).is_err());
    assert!(
        match_estimate(
            &Game::new(2).unwrap(),
            &[OutcomeEstimate {
                white: 2.0,
                draw: 0.0,
                black: -1.0
            }; 4]
        )
        .is_err()
    );
}

#[test]
fn local_scores_are_symmetric_uncalibrated_and_never_terminal() {
    for cp in [0, 100, 400, 10000] {
        let positive = LocalEvaluation::Cp { value: cp }.estimate();
        let negative = LocalEvaluation::Cp { value: -cp }.estimate();
        close(positive.white, negative.black);
        assert!(positive.white > 0.0 && positive.white < 1.0);
        close(positive.white + positive.draw + positive.black, 1.0);
    }
    assert!(
        LocalEvaluation::Capture {
            winner: Color::White
        }
        .estimate()
        .white
            < 1.0
    );
}

#[test]
fn negamax_refuses_a_board_win_that_allows_the_third_loss() {
    let game = two_losses();
    let before = game.save_json().unwrap();
    let result = oracle()
        .search(
            &game,
            SearchLimits {
                max_depth: 2,
                max_nodes: 100000,
                milliseconds: 10000,
            },
        )
        .unwrap();
    assert_eq!(result.completed_depth, 2);
    assert!(!(result.best_move.board == 0 && result.best_move.uci.to_string() == "h7h8"));
    assert!(result.expected_score.unwrap() > 0.0);
    assert!(result.cutoffs > 0);
    assert!(result.cache_hits > 0);
    assert_eq!(game.save_json().unwrap(), before);
}

#[test]
fn negamax_takes_an_actual_match_win_without_local_estimation() {
    let mut fens = vec!["4k3/8/8/8/8/8/4R3/4K3 w - - 100 1".into(); 4];
    fens[0] = "4k3/8/8/8/8/8/4R3/4K3 w - - 0 1".into();
    let game = Game::from_fens(2, Color::White, &fens).unwrap();
    let result = oracle()
        .search(
            &game,
            SearchLimits {
                max_depth: 3,
                max_nodes: 10000,
                milliseconds: 1000,
            },
        )
        .unwrap();
    assert_eq!(result.best_move.board, 0);
    assert_eq!(result.best_move.uci.to_string(), "e2e8");
    assert_eq!(result.expected_score, Some(1.0));
    assert_eq!(result.evaluations, 0);
}

#[test]
fn a_node_budget_returns_a_legal_fallback_without_claiming_a_completed_depth() {
    let game = Game::new(2).unwrap();
    let result = oracle()
        .search(
            &game,
            SearchLimits {
                max_depth: 3,
                max_nodes: 1,
                milliseconds: 1000,
            },
        )
        .unwrap();
    assert!(
        game.legal_moves(result.best_move.board)
            .unwrap()
            .contains(&result.best_move.uci)
    );
    assert_eq!(result.completed_depth, 0);
    assert_eq!(result.expected_score, None);
    assert_eq!(result.nodes, 1);
}

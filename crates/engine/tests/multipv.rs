use nested_chess::{
    Board, Color, Game,
    search::{self, LocalEvaluation, SearchLimits},
    uci::UciEngine,
    uci_output,
};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

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
fn real_multipv_returns_ten_distinct_ranked_legal_moves_for_both_colors() {
    let mut engine = oracle();
    engine.set_analysis_nodes(64).unwrap();
    for (color, turn) in [(Color::White, "w"), (Color::Black, "b")] {
        let fen = START.replace(" w ", &format!(" {turn} "));
        let actual = engine.candidate_moves(&fen, false, 10).unwrap();
        let legal = Board::from_fen(&fen).unwrap().legal_moves(color);
        assert_eq!(actual.len(), 10);
        for (index, candidate) in actual.iter().enumerate() {
            assert!(legal.contains(&candidate.uci));
            assert!(
                !actual[..index]
                    .iter()
                    .any(|earlier| earlier.uci == candidate.uci)
            );
        }
        for pair in actual.windows(2) {
            let (LocalEvaluation::Cp { value: first }, LocalEvaluation::Cp { value: second }) =
                (pair[0].evaluation, pair[1].evaluation)
            else {
                panic!("unexpected capture score at initial position");
            };
            assert!(if color == Color::White {
                first >= second
            } else {
                first <= second
            });
        }
    }
}

#[test]
fn real_multipv_returns_all_moves_when_fewer_than_ten_are_legal() {
    let fen = "7k/8/8/8/8/8/P7/K7 w - - 0 1";
    let actual = oracle().candidate_moves(fen, false, 10).unwrap();
    let legal = Board::from_fen(fen).unwrap().legal_moves(Color::White);
    assert_eq!(actual.len(), legal.len());
    assert!(
        legal
            .iter()
            .all(|mv| actual.iter().any(|candidate| candidate.uci == *mv))
    );
}

#[test]
fn real_multipv_supports_chess960_positions() {
    let game = Game::chess960(2, 123).unwrap();
    let board = game.board(0).unwrap();
    let actual = oracle()
        .candidate_moves(&board.fen(Color::White), true, 10)
        .unwrap();
    assert_eq!(actual.len(), 10);
    assert!(
        actual
            .iter()
            .all(|candidate| game.legal_moves(0).unwrap().contains(&candidate.uci))
    );
}

#[test]
fn real_multipv_preserves_chess960_castling_notation() {
    let fen = "4k3/8/8/8/8/8/8/R3K2R w AH - 0 1";
    let actual = oracle().candidate_moves(fen, true, 30).unwrap();
    let moves: Vec<_> = actual
        .iter()
        .map(|candidate| candidate.uci.to_string())
        .collect();
    assert!(moves.iter().any(|mv| mv == "e1a1"));
    assert!(moves.iter().any(|mv| mv == "e1h1"));
}

#[test]
fn parser_keeps_a_complete_iteration_when_a_deeper_one_is_partial() {
    let output = "info depth 1 multipv 1 score cp 20 pv e2e4\ninfo depth 1 multipv 2 score cp 10 pv d2d4\ninfo depth 2 multipv 1 score cp 30 pv g1f3\nbestmove g1f3";
    let actual = uci_output::candidates(output, START, 2).unwrap();
    assert_eq!(
        actual
            .iter()
            .map(|candidate| candidate.uci.to_string())
            .collect::<Vec<_>>(),
        ["e2e4", "d2d4"]
    );
}

#[test]
fn parser_accepts_a_contiguous_partial_first_iteration() {
    let actual = uci_output::candidates(
        "info depth 1 multipv 1 score cp 20 pv e2e4\nbestmove e2e4",
        START,
        10,
    )
    .unwrap();
    assert_eq!(actual.len(), 1);
    assert_eq!(actual[0].uci.to_string(), "e2e4");
}

#[test]
fn parser_does_not_mix_ranks_from_separate_output_batches() {
    let output = "info depth 1 multipv 1 score cp 20 pv e2e4\ninfo depth 1 multipv 2 score cp 10 pv d2d4\ninfo depth 2 multipv 1 score cp 30 pv g1f3\ninfo depth 1 multipv 2 score cp 20 pv e2e4\nbestmove g1f3";
    let actual = uci_output::candidates(output, START, 2).unwrap();
    assert_eq!(
        actual
            .iter()
            .map(|candidate| candidate.uci.to_string())
            .collect::<Vec<_>>(),
        ["e2e4", "d2d4"]
    );
}

#[test]
fn parser_normalizes_black_scores_to_white() {
    let actual = uci_output::candidates(
        "info depth 1 multipv 1 score cp 90 pv e7e5",
        &START.replace(" w ", " b "),
        1,
    )
    .unwrap();
    assert!(matches!(
        actual[0].evaluation,
        LocalEvaluation::Cp { value: -90 }
    ));
}

#[test]
fn parser_rejects_illegal_duplicate_and_missing_candidates() {
    for output in [
        "info depth 1 multipv 1 score cp 20 pv e2e5",
        "info depth 1 multipv 1 score cp 20 pv e2e4\ninfo depth 1 multipv 2 score cp 10 pv e2e4",
        "info depth 1 multipv 1 score cp 20 lowerbound pv e2e4",
        "bestmove (none)",
    ] {
        assert!(uci_output::candidates(output, START, 2).is_err());
    }
}

#[test]
fn selective_search_completes_a_depth_on_both_rectangular_grids() {
    for rows in [2, 4] {
        let game = Game::rectangular(4, rows).unwrap();
        let before = game.save_json().unwrap();
        let actual = oracle()
            .search_selective(
                &game,
                SearchLimits {
                    max_depth: 1,
                    max_nodes: 100000,
                    milliseconds: 10000,
                },
                10,
            )
            .unwrap();
        assert_eq!(actual.completed_depth, 1);
        assert!(actual.candidate_evaluations > 0);
        assert!(actual.candidate_evaluations <= actual.evaluations);
        assert!(
            game.legal_moves(actual.best_move.board)
                .unwrap()
                .contains(&actual.best_move.uci)
        );
        assert_eq!(game.save_json().unwrap(), before);
    }
}

#[test]
fn selective_search_takes_immediate_match_wins_without_candidate_analysis() {
    for (color, fen, expected) in [
        (Color::White, "4k3/8/8/8/8/8/4R3/4K3 w - - 0 1", "e2e8"),
        (Color::Black, "4k3/4r3/8/8/8/8/8/4K3 b - - 0 1", "e7e1"),
    ] {
        let mut fens = vec![fen.replace("0 1", "100 1"); 4];
        fens[0] = fen.into();
        let game = Game::from_fens(2, color, &fens).unwrap();
        let actual = oracle()
            .search_selective(
                &game,
                SearchLimits {
                    max_depth: 2,
                    max_nodes: 10000,
                    milliseconds: 1000,
                },
                10,
            )
            .unwrap();
        assert_eq!(actual.best_move.uci.to_string(), expected);
        assert_eq!(actual.expected_score, Some(1.0));
        assert_eq!(actual.evaluations, 0);
    }
}

#[test]
fn selective_search_validates_candidate_count_and_honors_node_limits() {
    let game = Game::new(2).unwrap();
    let mut engine = oracle();
    let limits = SearchLimits {
        max_depth: 2,
        max_nodes: 1,
        milliseconds: 1000,
    };
    for count in [0, 501] {
        assert!(matches!(
            engine.search_selective(&game, limits, count),
            Err(search::SearchError::InvalidCandidateCount)
        ));
    }
    let actual = engine.search_selective(&game, limits, 10).unwrap();
    assert_eq!(actual.completed_depth, 0);
    assert_eq!(actual.evaluations, 0);
    assert!(
        game.legal_moves(actual.best_move.board)
            .unwrap()
            .contains(&actual.best_move.uci)
    );
}

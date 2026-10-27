use nested_chess::{Color, Error, Game, Outcome, parse_square, route_rectangular};

#[test]
fn four_columns_two_rows_route_every_square_to_its_two_file_four_rank_region() {
    for rank in 0..8 {
        let expected = if rank < 4 {
            [0, 0, 1, 1, 2, 2, 3, 3]
        } else {
            [4, 4, 5, 5, 6, 6, 7, 7]
        };
        for file in 0..8 {
            assert_eq!(
                route_rectangular(4, 2, rank * 8 + file).unwrap(),
                expected[usize::from(file)]
            );
        }
    }
    assert!(matches!(
        route_rectangular(4, 2, 64),
        Err(Error::InvalidSquare)
    ));
}

#[test]
fn rectangular_opening_has_eight_boards_and_explicit_dimensions() {
    let game = Game::rectangular(4, 2).unwrap();
    let state = game.state();
    assert_eq!((state.columns, state.rows), (4, 2));
    assert_eq!(state.boards.len(), 8);
    assert_eq!(state.legal_boards, (0..8).collect::<Vec<_>>());
    assert_eq!(game.piece_planes().len(), 8 * 12 * 64);
}

#[test]
fn rectangular_routing_save_load_and_undo_preserve_local_move_counts() {
    let mut game = Game::rectangular(4, 2).unwrap();
    for (board, uci, next) in [(0, "e2e4", 2), (2, "e7e5", 6), (6, "h2h4", 3)] {
        game.play(board, uci).unwrap();
        assert_eq!(game.state().forced_board, Some(next));
    }
    let mut restored = Game::load_json(&game.save_json().unwrap()).unwrap();
    assert_eq!(restored.state(), game.state());
    assert_eq!(restored.state().boards[2].white_moves, 0);
    assert_eq!(restored.state().boards[2].black_moves, 1);
    restored.undo().unwrap();
    assert_eq!((restored.state().columns, restored.state().rows), (4, 2));
    assert_eq!(restored.state().forced_board, Some(6));
    assert_eq!(restored.state().ply, 2);
}

#[test]
fn rectangular_chess960_supports_shared_and_independent_starts() {
    let shared = Game::chess960_rectangular(4, 2, 123).unwrap();
    assert!(
        shared
            .state()
            .boards
            .iter()
            .all(|board| board.fen == shared.state().boards[0].fen)
    );
    assert_eq!(shared.state().start_position, Some(123));
    let independent = Game::chess960_boards_rectangular(4, 2, &[0, 1, 2, 3, 4, 5, 6, 7]).unwrap();
    assert_eq!(independent.state().boards.len(), 8);
    assert_eq!(independent.state().start_position, None);
    assert_eq!(
        independent
            .state()
            .boards
            .iter()
            .map(|board| &board.fen)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        8
    );
    assert!(Game::chess960_boards_rectangular(4, 2, &[0, 1]).is_err());
}

#[test]
fn eight_board_matches_need_more_than_four_points_to_win_early() {
    let mut game = Game::from_fens_rectangular(
        4,
        2,
        Color::White,
        &vec!["4k3/4R3/8/8/8/8/8/K7 w - - 0 1".to_owned(); 8],
    )
    .unwrap();
    for (index, board) in [6, 5, 4, 3, 2].into_iter().enumerate() {
        game.play(board, "e7e8").unwrap();
        assert_eq!(game.score().white, (index + 1) as f32);
        assert_eq!(
            game.outcome(),
            if index == 4 {
                Outcome::White
            } else {
                Outcome::Ongoing
            }
        );
        if index != 4 {
            game.play(7, if index % 2 == 0 { "e8d8" } else { "d8e8" })
                .unwrap();
        }
    }
    assert_eq!(
        route_rectangular(4, 2, parse_square("h8").unwrap()).unwrap(),
        7
    );
}

#[test]
fn unsupported_rectangles_and_wrong_position_counts_are_rejected() {
    for (columns, rows) in [(0, 2), (2, 4), (4, 3), (8, 2)] {
        assert!(matches!(
            Game::rectangular(columns, rows),
            Err(Error::InvalidGrid)
        ));
    }
    assert!(matches!(
        Game::from_fens_rectangular(4, 2, Color::White, &[]),
        Err(Error::InvalidBoard)
    ));
}

#[cfg(feature = "checkpoint")]
#[test]
fn rectangular_checkpoint_preserves_routing_and_history() {
    let mut game = Game::rectangular(4, 2).unwrap();
    game.play(0, "e2e4").unwrap();
    let restored = Game::from_checkpoint_json(&game.checkpoint_json().unwrap()).unwrap();
    assert_eq!(restored.state(), game.state());
}

#[test]
fn legacy_sixty_four_board_saves_keep_their_original_shape_and_routes() {
    let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    let save = serde_json::json!({"version":3,"size":8,"first_player":"white","initial_fens":vec![fen;64],"finish_rule":"majority","events":[{"kind":"move","board":0,"uci":"e2e4"}],"en_passant_rule":"local"});
    let game = Game::load_json(&save.to_string()).unwrap();
    assert_eq!((game.state().columns, game.state().rows), (8, 8));
    assert_eq!(game.state().boards.len(), 64);
    assert_eq!(game.state().forced_board, Some(28));
}

#[cfg(feature = "checkpoint")]
#[test]
fn legacy_square_checkpoint_keeps_position_counts_and_routing() {
    let game = Game::from_checkpoint_json(include_str!("fixtures/legacy-checkpoint.json")).unwrap();
    let state = game.state();
    assert_eq!((state.columns, state.rows), (2, 2));
    assert_eq!(state.ply, 2);
    assert_eq!(state.turn, Color::White);
    assert_eq!(state.forced_board, Some(3));
    assert_eq!(state.boards[0].white_moves, 1);
    assert_eq!(state.boards[1].black_moves, 1);
}

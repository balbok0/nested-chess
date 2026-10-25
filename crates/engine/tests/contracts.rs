use nested_chess::{Board, Color, Game, Outcome, Piece, Role, parse_square, route, square_name};

fn custom(fen: &str, turn: Color) -> Game {
    Game::from_fens(2, turn, &vec![fen.into(); 4]).unwrap()
}

#[test]
fn destination_regions_cover_every_square() {
    for size in [2, 4, 8] {
        for rank in 0..8u8 {
            for file in 0..8u8 {
                assert_eq!(
                    route(size, rank * 8 + file).unwrap(),
                    usize::from(rank / (8 / size) * size + file / (8 / size))
                );
            }
        }
    }
    for (size, square, expected) in [
        (2, "a1", 0),
        (2, "e4", 1),
        (2, "d5", 2),
        (2, "h8", 3),
        (4, "c3", 5),
        (8, "e4", 28),
    ] {
        assert_eq!(
            route(size, parse_square(square).unwrap()).unwrap(),
            expected
        );
    }
}

#[test]
fn all_grid_sizes_start_with_white_free_to_choose() {
    for size in [2, 4, 8] {
        let game = Game::new(size).unwrap();
        assert_eq!(game.legal_boards().len(), usize::from(size).pow(2));
        assert_eq!(game.legal_moves(0).unwrap().len(), 20);
        assert_eq!(game.turn(), Color::White);
    }
}

#[test]
fn black_opens_a_fresh_board_and_can_move_twice_before_white_visits() {
    let mut game = Game::new(2).unwrap();
    for (board, mv) in [(0, "e2e4"), (1, "e7e5"), (3, "e2e4"), (1, "d7d5")] {
        game.play(board, mv).unwrap();
    }
    let state = game.state();
    assert_eq!(state.turn, Color::White);
    assert_eq!(state.forced_board, Some(2));
    assert_eq!(
        (state.boards[1].white_moves, state.boards[1].black_moves),
        (0, 2)
    );
    assert_eq!(
        game.board(1).unwrap().piece_at(parse_square("e5").unwrap()),
        Some(Piece {
            color: Color::Black,
            role: Role::Pawn
        })
    );
}

#[test]
fn attacked_kings_may_ignore_attacks_and_capture_each_other() {
    let mut game = custom("8/8/8/8/8/4k3/4K3/8 w - - 0 1", Color::White);
    assert!(
        game.legal_moves(0)
            .unwrap()
            .iter()
            .any(|mv| mv.to_string() == "e2e3")
    );
    game.play(0, "e2e3").unwrap();
    let state = game.state();
    assert_eq!(state.boards[0].outcome, Outcome::White);
    assert_eq!(state.forced_board, Some(1));
}

#[test]
fn king_capture_settles_board_and_final_score_counts_wins() {
    let mut fens = vec!["4k3/8/8/8/8/8/4R3/4K3 w - - 100 1".into(); 4];
    fens[0] = "4k3/8/8/8/8/8/4R3/4K3 w - - 0 1".into();
    let mut game = Game::from_fens(2, Color::White, &fens).unwrap();
    game.play(0, "e2e8").unwrap();
    let state = game.state();
    assert_eq!(state.outcome, Outcome::White);
    assert_eq!(
        (state.score.white, state.score.black, state.score.draws),
        (2.5, 1.5, 3)
    );
    assert!(state.legal_boards.is_empty());
    assert!(game.play(0, "e1d1").is_err());
}

#[test]
fn routing_to_finished_board_releases_board_choice() {
    let mut fens = vec!["4k3/8/8/8/8/8/8/4K3 w - - 0 1".into(); 4];
    fens[1] = "4k3/8/8/8/8/8/8/4K3 w - - 100 1".into();
    let mut game = Game::from_fens(2, Color::White, &fens).unwrap();
    game.play(0, "e1f1").unwrap();
    let state = game.state();
    assert_eq!(state.requested_board, Some(1));
    assert_eq!(state.forced_board, None);
    assert_eq!(state.legal_boards, vec![0, 2, 3]);
}

#[test]
fn rejected_actions_are_transactional() {
    for (name, board, uci) in [
        ("own_piece", 1, "e2e4"),
        ("illegal_geometry", 0, "e4e7"),
        ("out_of_range", 99, "a7a6"),
        ("malformed", 0, "🥐abc"),
    ] {
        let mut game = Game::new(2).unwrap();
        game.play(0, "a2a3").unwrap();
        let expected = game.state();
        assert!(game.play(board, uci).is_err(), "{name}");
        assert_eq!(game.state(), expected, "{name}");
    }
}

#[test]
fn pawns_promote_to_each_supported_piece() {
    for (suffix, role) in [
        ("q", Role::Queen),
        ("r", Role::Rook),
        ("b", Role::Bishop),
        ("n", Role::Knight),
    ] {
        let mut game = custom("4k3/P7/8/8/8/8/8/4K3 w - - 0 1", Color::White);
        game.play(0, &format!("a7a8{suffix}")).unwrap();
        assert_eq!(
            game.board(0).unwrap().piece_at(56),
            Some(Piece {
                color: Color::White,
                role
            })
        );
    }
}

#[test]
fn castling_ignores_attacks_and_moves_both_pieces() {
    for (color, fen, mv, king, rook) in [
        (
            Color::White,
            "4kr2/8/8/8/8/8/8/R3K2R w KQ - 0 1",
            "e1g1",
            6,
            5,
        ),
        (
            Color::White,
            "3rk3/8/8/8/8/8/8/R3K2R w KQ - 0 1",
            "e1c1",
            2,
            3,
        ),
        (
            Color::Black,
            "r3k2r/8/8/8/8/8/8/4KR2 b kq - 0 1",
            "e8g8",
            62,
            61,
        ),
        (
            Color::Black,
            "r3k2r/8/8/8/8/8/8/3RK3 b kq - 0 1",
            "e8c8",
            58,
            59,
        ),
    ] {
        let mut game = custom(fen, color);
        game.play(0, mv).unwrap();
        assert_eq!(
            game.board(0).unwrap().piece_at(king),
            Some(Piece {
                color,
                role: Role::King
            })
        );
        assert_eq!(
            game.board(0).unwrap().piece_at(rook),
            Some(Piece {
                color,
                role: Role::Rook
            })
        );
        assert!(
            !game
                .board(0)
                .unwrap()
                .fen(color)
                .split_whitespace()
                .nth(2)
                .unwrap()
                .contains(if color == Color::White { 'K' } else { 'k' })
        );
    }
}

#[test]
fn movement_respects_piece_geometry_blockers_and_capture_colors() {
    for (fen, color, from, expected) in [
        (
            "7k/8/3P4/8/3R1p2/8/8/K7 w - - 0 1",
            Color::White,
            "d4",
            vec!["a4", "b4", "c4", "d1", "d2", "d3", "d5", "e4", "f4"],
        ),
        (
            "7k/8/1P3p2/8/3B4/8/8/K7 w - - 0 1",
            Color::White,
            "d4",
            vec!["b2", "c3", "c5", "e3", "e5", "f2", "f6", "g1"],
        ),
        (
            "7k/8/8/8/8/8/3P4/KN6 w - - 0 1",
            Color::White,
            "b1",
            vec!["a3", "c3"],
        ),
        (
            "7k/8/8/8/8/3pPP2/4P3/K7 w - - 0 1",
            Color::White,
            "e2",
            vec!["d3"],
        ),
        (
            "7k/4p3/3Ppp2/8/8/8/8/K7 b - - 0 1",
            Color::Black,
            "e7",
            vec!["d6"],
        ),
    ] {
        let mut actual: Vec<_> = custom(fen, color)
            .legal_moves(0)
            .unwrap()
            .iter()
            .filter(|mv| mv.from == parse_square(from).unwrap())
            .map(|mv| square_name(mv.to).unwrap())
            .collect();
        actual.sort();
        assert_eq!(actual, expected, "{fen}: {from}");
    }
}

#[test]
fn capturing_a_corner_rook_revokes_its_castling_right() {
    let mut game = custom("r3k3/8/8/8/8/8/8/R3K3 w Qq - 0 1", Color::White);
    game.play(0, "a1a8").unwrap();
    assert_eq!(
        game.board(0)
            .unwrap()
            .fen(Color::Black)
            .split_whitespace()
            .nth(2),
        Some("-")
    );
}

#[test]
fn unavailable_destination_releases_choice_without_closing_that_board() {
    let frozen = "BBBBBBBK/PPPPPPPP/8/8/8/8/8/k7 w - - 0 1";
    let mut fens = vec![Board::default().fen(Color::Black); 4];
    fens[3] = frozen.into();
    let mut game = Game::from_fens(2, Color::Black, &fens).unwrap();
    game.play(0, "e7e5").unwrap();
    let actual = game.state();
    assert_eq!(actual.requested_board, Some(3));
    assert_eq!(actual.forced_board, None);
    assert_eq!(actual.legal_boards, vec![0, 1, 2]);
    assert_eq!(actual.boards[3].outcome, Outcome::Ongoing);
}

#[test]
fn no_move_anywhere_draws_unfinished_boards() {
    let game = custom("BBBBBBBK/PPPPPPPP/8/8/8/8/8/k7 w - - 0 1", Color::White);
    assert_eq!(game.outcome(), Outcome::Draw);
    assert_eq!(game.score().draws, 4);
}

#[test]
fn en_passant_captures_the_double_stepped_pawn() {
    for (color, fen, mv, victim) in [
        (
            Color::White,
            "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
            "e5d6",
            "d5",
        ),
        (
            Color::Black,
            "4k3/8/8/8/3Pp3/8/8/4K3 b - d3 0 1",
            "e4d3",
            "d4",
        ),
    ] {
        let mut fens = vec![Board::default().fen(color); 4];
        fens[0] = fen.into();
        let mut game = Game::from_fens(2, color, &fens).unwrap();
        game.play(0, mv).unwrap();
        assert_eq!(
            game.board(0)
                .unwrap()
                .piece_at(parse_square(victim).unwrap()),
            None
        );
        assert_eq!(
            game.board(0)
                .unwrap()
                .piece_at(parse_square(&mv[2..]).unwrap()),
            Some(Piece {
                color,
                role: Role::Pawn
            })
        );
    }
}

#[test]
fn en_passant_survives_moves_on_other_boards_and_can_be_captured_later() {
    let mut fens = vec!["3rk3/8/8/8/8/8/8/4K3 w - - 0 1".into(); 4];
    fens[0] = "4k3/3p4/8/4P3/8/8/8/4K3 b - - 0 1".into();
    let mut game = Game::from_fens(2, Color::Black, &fens).unwrap();
    for (board, mv) in [(0, "d7d5"), (2, "e1f1"), (1, "d8d4")] {
        game.play(board, mv).unwrap();
    }
    let before_capture = game.state();
    game.play(0, "e5d6").unwrap();
    assert_eq!(
        game.board(0).unwrap().piece_at(parse_square("d5").unwrap()),
        None
    );
    assert_eq!(
        game.board(0).unwrap().piece_at(parse_square("d6").unwrap()),
        Some(Piece {
            color: Color::White,
            role: Role::Pawn
        })
    );
    let mut restored = Game::load_json(&game.save_json().unwrap()).unwrap();
    assert_eq!(restored.state(), game.state());
    restored.undo().unwrap();
    assert_eq!(restored.state(), before_capture);
}

#[test]
fn the_next_local_move_by_either_color_expires_en_passant() {
    for (color, mv) in [(Color::White, "e1f1"), (Color::Black, "e8f8")] {
        let mut fens = vec![Board::default().fen(color); 4];
        fens[0] = "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1".into();
        let mut game = Game::from_fens(2, color, &fens).unwrap();
        game.play(0, mv).unwrap();
        assert!(
            !game
                .board(0)
                .unwrap()
                .legal_moves(Color::White)
                .iter()
                .any(|mv| mv.to_string() == "e5d6")
        );
        assert_eq!(
            game.board(0)
                .unwrap()
                .fen(Color::White)
                .split_whitespace()
                .nth(3),
            Some("-")
        );
    }
}

#[test]
fn several_boards_can_retain_en_passant_for_different_colors() {
    let fens = [
        "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1".into(),
        "4k3/8/8/8/3Pp3/8/8/4K3 b - d3 0 1".into(),
        Board::default().fen(Color::White),
        Board::default().fen(Color::White),
    ];
    let mut game = Game::from_fens(2, Color::White, &fens).unwrap();
    game.play(2, "e2e4").unwrap();
    assert!(
        game.legal_moves(1)
            .unwrap()
            .iter()
            .any(|mv| mv.to_string() == "e4d3")
    );
    assert!(
        game.board(0)
            .unwrap()
            .legal_moves(Color::White)
            .iter()
            .any(|mv| mv.to_string() == "e5d6")
    );
}

#[test]
fn legacy_saves_keep_global_en_passant_expiry_after_resaving() {
    for version in [1, 2] {
        let mut fens = vec![Board::default().fen(Color::White); 4];
        fens[0] = "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1".into();
        let mut save = serde_json::json!({"version":version,"size":2,"first_player":"white","initial_fens":fens});
        if version == 1 {
            save["moves"] = serde_json::json!([]);
        } else {
            save["finish_rule"] = serde_json::json!("majority");
            save["events"] = serde_json::json!([]);
        }
        let game = Game::load_json(&save.to_string()).unwrap();
        let mut restored = Game::load_json(&game.save_json().unwrap()).unwrap();
        restored.play(1, "e2e4").unwrap();
        assert_eq!(
            restored
                .board(0)
                .unwrap()
                .fen(Color::White)
                .split_whitespace()
                .nth(3),
            Some("-")
        );
    }
}

#[test]
fn hundred_local_quiet_moves_draw_a_board() {
    let mut game = custom("4k3/8/8/8/8/8/8/4K3 w - - 99 1", Color::White);
    game.play(0, "e1d1").unwrap();
    let state = game.state();
    assert_eq!(state.boards[0].outcome, Outcome::Draw);
    assert_eq!(state.score.draws, 1);
}

#[test]
fn full_nested_position_repeated_three_times_draws_remaining_boards() {
    let mut game = custom("8/8/8/8/8/2k5/8/K7 w - - 0 1", Color::White);
    for _ in 0..3 {
        for mv in ["a1b1", "c3d3", "b1a1", "d3c3"] {
            if game.outcome() == Outcome::Ongoing {
                game.play(0, mv).unwrap();
            }
        }
    }
    assert_eq!(game.outcome(), Outcome::Draw);
}

#[test]
fn saves_replay_turns_results_and_en_passant_and_undo_restores_position() {
    let mut game = Game::new(2).unwrap();
    game.play(0, "e2e4").unwrap();
    let expected = game.state();
    game.play(1, "e7e5").unwrap();
    assert_eq!(
        Game::load_json(&game.save_json().unwrap()).unwrap().state(),
        game.state()
    );
    game.undo().unwrap();
    assert_eq!(game.state(), expected);
    assert!(Game::load_json("{\"version\":42}").is_err());
}

#[test]
fn piece_planes_have_stable_board_color_role_rank_file_order() {
    for size in [2, 4, 8] {
        let planes = Game::new(size).unwrap().piece_planes();
        assert_eq!(planes.len(), usize::from(size).pow(2) * 12 * 64);
        assert_eq!(
            planes
                .iter()
                .map(|value| usize::from(*value))
                .sum::<usize>(),
            usize::from(size).pow(2) * 32
        );
        assert_eq!(planes[8], 1);
        assert_eq!(planes[5 * 64 + 4], 1);
        assert_eq!(planes[11 * 64 + 60], 1);
    }
}

#[test]
fn invalid_setup_is_rejected() {
    for fen in [
        "8/8/8/8/8/8/8/8 w - - 0 1",
        "4k3/8/8/8/8/8/8/4K3 w K - 0 1",
        "4k3/8/8/8/8/8/8/4K3 w - e3 0 1",
        "4k3/8/8/8/8/8/8/P3K3 w - - 0 1",
    ] {
        assert!(Board::from_fen(fen).is_err(), "{fen}");
    }
    for size in [0, 1, 3, 16] {
        assert!(Game::new(size).is_err());
    }
}

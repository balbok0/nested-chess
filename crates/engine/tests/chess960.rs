use nested_chess::{Board, Color, Game, Piece, Role, parse_square};
use std::collections::HashSet;

#[test]
fn every_start_is_unique_mirrored_and_obeys_chess960_constraints() {
    let mut positions = HashSet::new();
    for index in 0..960 {
        let board = Board::chess960(index).unwrap();
        assert!(board.is_chess960());
        assert!(positions.insert(board.fen(Color::White)));
        let mut bishops = Vec::new();
        let mut rooks = Vec::new();
        let mut king = 0;
        for file in 0..8 {
            let white = board.piece_at(file).unwrap();
            assert_eq!(
                board.piece_at(56 + file),
                Some(Piece {
                    color: Color::Black,
                    role: white.role
                })
            );
            assert_eq!(
                board.piece_at(8 + file),
                Some(Piece {
                    color: Color::White,
                    role: Role::Pawn
                })
            );
            assert_eq!(
                board.piece_at(48 + file),
                Some(Piece {
                    color: Color::Black,
                    role: Role::Pawn
                })
            );
            match white.role {
                Role::Bishop => bishops.push(file),
                Role::Rook => rooks.push(file),
                Role::King => king = file,
                _ => {}
            }
        }
        assert_ne!(bishops[0] % 2, bishops[1] % 2);
        assert!(rooks[0] < king && king < rooks[1]);
    }
    for (index, expected) in [(0, "BBQNNRKR"), (518, "RNBQKBNR"), (959, "RKRNNQBB")] {
        assert_eq!(
            Board::chess960(index)
                .unwrap()
                .fen(Color::White)
                .split_whitespace()
                .next()
                .unwrap()
                .split('/')
                .next_back(),
            Some(expected)
        );
    }
    assert!(Board::chess960(960).is_err());
}

#[test]
fn castling_supports_swaps_and_stationary_pieces_for_both_colors() {
    for (rank, rights, uci, king, rook) in [
        ("5KR1", "G", "f1g1", "g1", "f1"),
        ("6KR", "H", "g1h1", "g1", "f1"),
        ("4KR2", "F", "e1f1", "g1", "f1"),
        ("2RK4", "C", "d1c1", "c1", "d1"),
        ("R1K5", "A", "c1a1", "c1", "d1"),
        ("3RK3", "D", "e1d1", "c1", "d1"),
    ] {
        for color in [Color::White, Color::Black] {
            let (fen, uci, king, rook) = if color == Color::White {
                (
                    format!("4k3/8/8/8/8/8/8/{rank} w {rights} - 0 1"),
                    uci.to_owned(),
                    king.to_owned(),
                    rook.to_owned(),
                )
            } else {
                (
                    format!(
                        "{}/8/8/8/8/8/8/4K3 b {} - 0 1",
                        rank.to_lowercase(),
                        rights.to_lowercase()
                    ),
                    uci.replace('1', "8"),
                    king.replace('1', "8"),
                    rook.replace('1', "8"),
                )
            };
            let mut game = Game::from_fens(2, color, &vec![fen; 4]).unwrap();
            let details = game.move_details(0, &uci).unwrap();
            assert_eq!(details.destination, parse_square(&king).unwrap());
            assert_eq!(details.rook.unwrap().to, parse_square(&rook).unwrap());
            game.play(0, &uci).unwrap();
            for (square, role) in [(&king, Role::King), (&rook, Role::Rook)] {
                assert_eq!(
                    game.board(0)
                        .unwrap()
                        .piece_at(parse_square(square).unwrap()),
                    Some(Piece { color, role }),
                    "{uci}"
                );
            }
            assert_eq!(
                game.board(0).unwrap().fen(color).split_whitespace().nth(2),
                Some("-")
            );
        }
    }
}

#[test]
fn castling_routes_to_the_kings_landing_square() {
    let mut game = Game::from_fens(
        8,
        Color::White,
        &vec!["4k3/8/8/8/8/8/8/1K5R w H - 0 1".into(); 64],
    )
    .unwrap();
    game.play(0, "b1h1").unwrap();
    assert_eq!(game.state().requested_board, Some(6));
    assert_eq!(game.state().forced_board, Some(6));
}

#[test]
fn castling_requires_clear_paths_and_an_unmoved_rook() {
    for (fen, first, castle) in [
        ("4k3/8/8/8/8/8/8/1K2N2R w H - 0 1", None, "b1h1"),
        ("4k3/8/8/8/8/8/8/1K5R w H - 0 1", Some("h1h2"), "b1h1"),
        ("4k3/8/8/8/8/8/8/1K5R w H - 0 1", Some("b1b2"), "b1h1"),
    ] {
        let mut game = Game::from_fens(2, Color::White, &vec![fen.into(); 4]).unwrap();
        if let Some(mv) = first {
            game.play(0, mv).unwrap();
        }
        assert!(
            !game
                .board(0)
                .unwrap()
                .legal_moves(Color::White)
                .iter()
                .any(|mv| mv.to_string() == castle)
        );
    }
}

#[test]
fn capturing_a_chess960_rook_revokes_its_right() {
    let mut game = Game::from_fens(
        2,
        Color::White,
        &vec!["1k5r/8/8/8/8/8/8/1K5R w Hh - 0 1".into(); 4],
    )
    .unwrap();
    game.play(0, "h1h8").unwrap();
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
fn saves_and_undo_preserve_the_start_and_castling_rights() {
    let mut game = Game::chess960(2, 123).unwrap();
    let initial = game.state();
    assert!(initial.chess960);
    assert_eq!(initial.start_position, Some(123));
    assert!(
        initial
            .boards
            .iter()
            .all(|board| board.fen == initial.boards[0].fen)
    );
    game.play(0, "a2a3").unwrap();
    let mut restored = Game::load_json(&game.save_json().unwrap()).unwrap();
    assert_eq!(restored.state(), game.state());
    restored.undo().unwrap();
    assert_eq!(restored.state(), initial);
}

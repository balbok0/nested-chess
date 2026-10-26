#![cfg(feature = "checkpoint")]
use nested_chess::{Color, Game, Outcome, Role, parse_square};

#[test]
fn checkpoint_preserves_routing_counts_and_legal_moves() {
    for size in [2, 4, 8] {
        let mut game = Game::chess960(size, 123).unwrap();
        for _ in 0..12 {
            let board = game.legal_boards()[0];
            let uci = game.legal_moves(board).unwrap()[0].to_string();
            game.play(board, &uci).unwrap();
        }
        let restored = Game::from_checkpoint_json(&game.checkpoint_json().unwrap()).unwrap();
        assert_eq!(restored.state(), game.state());
        assert_eq!(restored.save_json().unwrap(), game.save_json().unwrap());
        for board in game.legal_boards() {
            assert_eq!(
                restored.legal_moves(board).unwrap(),
                game.legal_moves(board).unwrap()
            );
        }
    }
}
#[test]
fn checkpoint_retains_a_delayed_en_passant_capture() {
    let mut fens = vec!["3rk3/8/8/8/8/8/8/4K3 w - - 0 1".into(); 4];
    fens[0] = "4k3/3p4/8/4P3/8/8/8/4K3 b - - 0 1".into();
    let mut game = Game::from_fens(2, Color::Black, &fens).unwrap();
    for (board, uci) in [(0, "d7d5"), (2, "e1f1"), (1, "d8d4")] {
        game.play(board, uci).unwrap();
    }
    let mut game = Game::from_checkpoint_json(&game.checkpoint_json().unwrap()).unwrap();
    game.play(0, "e5d6").unwrap();
    assert_eq!(
        game.board(0).unwrap().piece_at(parse_square("d5").unwrap()),
        None
    );
    assert_eq!(
        game.board(0)
            .unwrap()
            .piece_at(parse_square("d6").unwrap())
            .unwrap()
            .role,
        Role::Pawn
    );
}
#[test]
fn checkpoint_retains_repetitions_across_repeated_restores() {
    let mut game = Game::new(2).unwrap();
    for (board, uci) in [
        (0, "b1a3"),
        (0, "b8a6"),
        (2, "b1a3"),
        (0, "a6b8"),
        (2, "a3b1"),
        (0, "b8a6"),
        (2, "b1a3"),
        (0, "a6b8"),
        (2, "a3b1"),
    ] {
        game = Game::from_checkpoint_json(&game.checkpoint_json().unwrap()).unwrap();
        game.play(board, uci).unwrap();
    }
    assert_eq!(game.outcome(), Outcome::Draw);
}

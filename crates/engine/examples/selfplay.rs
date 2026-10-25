use nested_chess::{Game, Outcome, Role};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut game = Game::new(2)?;
    let mut random = 42u64;
    while game.outcome() == Outcome::Ongoing && game.state().ply < 10_000 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let boards = game.legal_boards();
        let board = boards[random as usize % boards.len()];
        let moves = game.legal_moves(board)?;
        let mv = moves
            .iter()
            .find(|mv| {
                game.board(board)
                    .unwrap()
                    .piece_at(mv.to)
                    .is_some_and(|piece| piece.role == Role::King)
            })
            .unwrap_or(&moves[(random >> 16) as usize % moves.len()]);
        game.play(board, &mv.to_string())?;
    }
    println!("{}", serde_json::to_string_pretty(&game.state())?);
    Ok(())
}

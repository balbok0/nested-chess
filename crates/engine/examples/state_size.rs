use nested_chess::{Board, Game};

fn main() {
    println!("Local Board: {} bytes", std::mem::size_of::<Board>());
    for (columns, rows) in [(2, 2), (4, 2), (4, 4)] {
        let game = Game::rectangular(columns, rows).unwrap();
        let boards = usize::from(columns) * usize::from(rows);
        println!(
            "{columns}x{rows}: {} boards, {} board bytes, {} binary piece-plane bytes, {} AlphaZero-style float32 bytes",
            boards,
            boards * std::mem::size_of::<Board>(),
            game.piece_planes().len(),
            boards * 119 * 64 * 4
        );
    }
}

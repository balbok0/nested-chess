use nested_chess::{Board, Game};

fn main() {
    println!("Local Board: {} bytes", std::mem::size_of::<Board>());
    for size in [2, 4, 8] {
        let game = Game::new(size).unwrap();
        println!(
            "{size}x{size}: {} boards, {} board bytes, {} binary piece-plane bytes, {} AlphaZero-style float32 bytes",
            usize::from(size).pow(2),
            usize::from(size).pow(2) * std::mem::size_of::<Board>(),
            game.piece_planes().len(),
            usize::from(size).pow(2) * 119 * 64 * 4
        );
    }
}

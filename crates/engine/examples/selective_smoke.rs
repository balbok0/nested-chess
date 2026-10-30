//! Exercise ten-candidate search with real Fairy-Stockfish on both rectangular grids.
use nested_chess::{
    Game, Outcome,
    search::{DEFAULT_CANDIDATES, SearchLimits},
    uci::UciEngine,
};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for rows in [2, 4] {
        let mut game = Game::rectangular(4, rows)?;
        let mut engine = UciEngine::spawn(
            "node",
            &[root
                .join("scripts/fairy-uci.cjs")
                .to_str()
                .ok_or("invalid root path")?
                .into()],
            root.join("variants/nested.ini")
                .to_str()
                .ok_or("invalid variant path")?,
        )?;
        engine.set_analysis_nodes(64)?;
        let mut evaluations = 0;
        let mut candidates = 0;
        let mut depth = 0;
        let mut fallbacks = 0;
        let mut plies = 0;
        while plies < 20 && game.outcome() == Outcome::Ongoing {
            let result = engine.search_selective(
                &game,
                SearchLimits {
                    max_depth: 2,
                    max_nodes: 100000,
                    milliseconds: 600,
                },
                DEFAULT_CANDIDATES,
            )?;
            evaluations += result.evaluations;
            candidates += result.candidate_evaluations;
            depth += u64::from(result.completed_depth);
            fallbacks += u64::from(result.completed_depth == 0);
            game.play(result.best_move.board, &result.best_move.uci.to_string())?;
            plies += 1;
        }
        let saved = game.save_json()?;
        assert_eq!(Game::load_json(&saved)?.save_json()?, saved);
        println!(
            "{}",
            serde_json::json!({"columns": 4, "rows": rows, "plies": plies, "candidate_limit": DEFAULT_CANDIDATES, "evaluations": evaluations, "candidate_evaluations": candidates, "completed_depth_sum": depth, "fallback_moves": fallbacks})
        );
    }
    Ok(())
}

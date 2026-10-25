use nested_chess::{Game, Outcome};
use serde_json::{Value, json};
use std::{
    fs,
    hint::black_box,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const MATCHES: usize = 100;
const MOVES_PER_MATCH: usize = 200;
const SEED: u64 = 20261025;

fn random(seed: &mut u64, count: usize) -> usize {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed % count as u64) as usize
}

fn rss_bytes() -> u64 {
    fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:").map(|value| {
                value
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
                    * 1024
            })
        })
        .unwrap()
}

fn distribution(values: &mut [f64]) -> Value {
    values.sort_by(f64::total_cmp);
    json!({
        "mean": values.iter().sum::<f64>() / values.len() as f64,
        "p50": values[(values.len() - 1) / 2],
        "p95": values[(values.len() - 1) * 95 / 100],
        "p99": values[(values.len() - 1) * 99 / 100],
        "max": values[values.len() - 1],
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let moves_per_match = std::env::args()
        .nth(1)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(MOVES_PER_MATCH);
    if !(1..=5000).contains(&moves_per_match) {
        return Err("moves per match must be between 1 and 5000".into());
    }
    let mut results = Vec::new();
    for size in [2, 4, 8] {
        let baseline_rss = rss_bytes();
        let mut games = (0..MATCHES)
            .map(|_| Game::new(size))
            .collect::<Result<Vec<_>, _>>()?;
        let mut seed = SEED;
        let mut play_us = Vec::new();
        let mut clone_us = Vec::new();
        let mut state_us = Vec::new();
        let mut wire_bytes = Vec::new();
        let mut resets = 0;
        let started = Instant::now();
        for _ in 0..moves_per_match {
            for game in &mut games {
                if game.outcome() != Outcome::Ongoing {
                    *game = Game::new(size)?;
                    resets += 1;
                }
                let boards = game.legal_boards();
                let board = boards[random(&mut seed, boards.len())];
                let moves = game.legal_moves(board)?;
                let uci = moves[random(&mut seed, moves.len())].to_string();
                let before = Instant::now();
                let mut candidate = game.clone();
                clone_us.push(before.elapsed().as_secs_f64() * 1e6);
                let before = Instant::now();
                candidate.play(board, &uci)?;
                play_us.push(before.elapsed().as_secs_f64() * 1e6);
                *game = candidate;
                let before = Instant::now();
                let state = serde_json::to_vec(&game.state())?;
                state_us.push(before.elapsed().as_secs_f64() * 1e6);
                wire_bytes.push(state.len() as f64);
                black_box(state);
            }
        }
        let elapsed_seconds = started.elapsed().as_secs_f64();
        let live_rss = rss_bytes();
        let mut save_bytes = Vec::new();
        let mut replay_us = Vec::new();
        let mut retained_plies = Vec::new();
        for game in &games {
            let save = game.save_json()?;
            save_bytes.push(save.len() as f64);
            retained_plies.push(game.state().ply as f64);
            let before = Instant::now();
            let restored = Game::load_json(&save)?;
            replay_us.push(before.elapsed().as_secs_f64() * 1e6);
            assert_eq!(restored.save_json()?, save);
        }
        results.push(json!({
            "grid": size, "matches": MATCHES, "accepted_moves": play_us.len(),
            "resets_after_finished_games": resets, "elapsed_seconds": elapsed_seconds,
            "play_us": distribution(&mut play_us), "full_state_serialization_us": distribution(&mut state_us),
            "clone_us": distribution(&mut clone_us),
            "full_state_bytes_over_run": distribution(&mut wire_bytes), "retained_plies": distribution(&mut retained_plies),
            "save_bytes_at_end": distribution(&mut save_bytes), "replay_us": distribution(&mut replay_us),
            "rss_before_bytes": baseline_rss, "rss_with_100_games_bytes": live_rss,
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "seed": SEED, "build_profile": "release", "unix_seconds": SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        "moves_per_match": moves_per_match,
            "logical_cpus": std::thread::available_parallelism()?.get(),
            "load_average": fs::read_to_string("/proc/loadavg")?.trim(), "results": results,
        }))?
    );
    Ok(())
}

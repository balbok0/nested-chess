//! Replay ten-candidate benchmark games and validate their search accounting.
use nested_chess::{Color, Game, Outcome};
use serde::Deserialize;
use serde_json::Value;
use std::{fs, io};

#[derive(Deserialize)]
struct Job {
    id: String,
    family: String,
    a_color: Color,
    a_level: usize,
    columns: u8,
    rows: u8,
    opening: Vec<Value>,
}

#[derive(Deserialize)]
struct Metrics {
    moves: u64,
    engine_calls: u64,
    candidate_evaluations: u64,
    material_evaluations: u64,
    completed_depth_sum: u64,
    fallback_moves: u64,
    depth_histogram: [u64; 7],
}

#[derive(Deserialize)]
struct Record {
    job: Job,
    outcome: Outcome,
    a_score: f64,
    plies: usize,
    a: Metrics,
    b: Metrics,
    save: Value,
}

fn verify(record: Record) -> Result<u64, Box<dyn std::error::Error>> {
    let invalid = || io::Error::other(format!("invalid MultiPV record {}", record.job.id));
    let game = Game::load_json(&record.save.to_string())?;
    let events = record.save["events"].as_array().ok_or_else(invalid)?;
    let expected_score = match game.outcome() {
        Outcome::Draw => 0.5,
        Outcome::White if record.job.a_color == Color::White => 1.0,
        Outcome::Black if record.job.a_color == Color::Black => 1.0,
        _ => 0.0,
    };
    let depth_sum: u64 = record
        .a
        .depth_histogram
        .iter()
        .enumerate()
        .map(|(depth, count)| depth as u64 * count)
        .sum();
    let state = game.state();
    if record.job.family != "multipv"
        || record.job.a_level >= 6
        || (state.columns, state.rows) != (record.job.columns, record.job.rows)
        || game.outcome() == Outcome::Ongoing
        || game.outcome() != record.outcome
        || record.a_score != expected_score
        || record.plies != events.len()
        || record.a.candidate_evaluations == 0
        || record.a.engine_calls < record.a.candidate_evaluations
        || record.a.material_evaluations != 0
        || record.a.completed_depth_sum != depth_sum
        || record.a.fallback_moves != record.a.depth_histogram[0]
        || record.a.depth_histogram.iter().sum::<u64>() != record.a.moves
        || record.a.depth_histogram[record.job.a_level + 2..]
            .iter()
            .any(|count| *count != 0)
        || record.b.engine_calls != record.b.moves
        || record.b.material_evaluations != 0
        || record.b.candidate_evaluations != 0
        || record.a.moves + record.b.moves + record.job.opening.len() as u64 != events.len() as u64
        || serde_json::from_str::<Value>(&game.save_json()?)? != record.save
    {
        return Err(invalid().into());
    }
    Ok(record.a.moves)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        return Err("usage: verify_multipv RESULTS.jsonl".into());
    }
    let mut games = 0;
    let mut moves = 0;
    for line in fs::read_to_string(&args[1])?.lines() {
        moves += verify(serde_json::from_str(line)?)?;
        games += 1;
    }
    println!("Verified {games} MultiPV-game replays and accounting for {moves} bot moves");
    Ok(())
}

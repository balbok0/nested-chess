//! Replay fallback benchmarks and verify every fallback choice with the Rust engine.
use nested_chess::{Color, Game, Outcome, search::fallback_move};
use serde::Deserialize;
use serde_json::Value;
use std::{fs, io};

#[derive(Deserialize)]
struct Job {
    id: String,
    family: String,
    a_color: Color,
    opening: Vec<Value>,
    columns: u8,
    rows: u8,
}

#[derive(Deserialize)]
struct Metrics {
    moves: u64,
    engine_calls: u64,
    search_nodes: u64,
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
    let invalid = || io::Error::other(format!("invalid fallback record {}", record.job.id));
    let mut initial = record.save.clone();
    initial["events"] = serde_json::json!([]);
    let mut game = Game::load_json(&initial.to_string())?;
    let events = record.save["events"].as_array().ok_or_else(invalid)?;
    let mut choices = 0;
    for (ply, event) in events.iter().enumerate() {
        if event["kind"] != "move" {
            return Err(invalid().into());
        }
        let board = event["board"].as_u64().ok_or_else(invalid)? as usize;
        let uci = event["uci"].as_str().ok_or_else(invalid)?;
        if ply < record.job.opening.len() {
            if record.job.opening[ply]["board"] != event["board"]
                || record.job.opening[ply]["uci"] != event["uci"]
            {
                return Err(invalid().into());
            }
        } else if game.turn() == record.job.a_color {
            let actual = fallback_move(&game)?;
            if actual.board != board || actual.uci.to_string() != uci {
                return Err(io::Error::other(format!(
                    "fallback mismatch {} ply {ply}",
                    record.job.id
                ))
                .into());
            }
            choices += 1;
        }
        game.play(board, uci)?;
    }
    let expected_score = match game.outcome() {
        Outcome::Draw => 0.5,
        Outcome::White if record.job.a_color == Color::White => 1.0,
        Outcome::Black if record.job.a_color == Color::Black => 1.0,
        _ => 0.0,
    };
    let state = game.state();
    if record.job.family != "fallback"
        || (state.columns, state.rows) != (record.job.columns, record.job.rows)
        || game.outcome() == Outcome::Ongoing
        || game.outcome() != record.outcome
        || record.a_score != expected_score
        || record.plies != events.len()
        || choices != record.a.moves
        || record.a.engine_calls != 0
        || record.a.search_nodes != 0
        || record.a.completed_depth_sum != 0
        || record.a.fallback_moves != choices
        || record.a.depth_histogram != [choices, 0, 0, 0, 0, 0, 0]
        || record.b.engine_calls != record.b.moves
        || record.a.moves + record.b.moves + record.job.opening.len() as u64 != events.len() as u64
        || serde_json::from_str::<Value>(&game.save_json()?)? != record.save
    {
        return Err(invalid().into());
    }
    Ok(choices)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        return Err("usage: verify_fallback RESULTS.jsonl".into());
    }
    let mut games = 0;
    let mut choices = 0;
    for line in fs::read_to_string(&args[1])?.lines() {
        choices += verify(serde_json::from_str(line)?)?;
        games += 1;
    }
    println!("Verified {games} replays and {choices} exact fallback choices");
    Ok(())
}

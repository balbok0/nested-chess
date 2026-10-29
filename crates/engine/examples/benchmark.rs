use nested_chess::{
    Color, Game, Outcome,
    search::{self, MaterialEvaluator, SearchLimits, fallback_move},
    uci::UciEngine,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Instant,
};

const GLOBAL_MS: [u64; 6] = [300, 600, 1000, 1600, 2400, 4000];
const GLOBAL_NODES: [u64; 6] = [32, 64, 128, 128, 256, 256];
const LOCAL_MS: [u64; 6] = [60, 100, 180, 300, 500, 900];
const LOCAL_SKILL: [u8; 6] = [0, 2, 5, 10, 15, 20];

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct OpeningMove {
    board: usize,
    uci: String,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Job {
    id: String,
    family: String,
    a_level: usize,
    b_level: usize,
    a_color: Color,
    pair: usize,
    opening: Vec<OpeningMove>,
    #[serde(default = "default_dimension")]
    columns: u8,
    #[serde(default = "default_dimension")]
    rows: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    initial_fens: Option<Vec<String>>,
}

fn default_dimension() -> u8 {
    2
}

#[derive(Default, Serialize)]
struct Metrics {
    moves: u64,
    elapsed_ms: f64,
    engine_calls: u64,
    material_evaluations: u64,
    search_nodes: u64,
    cutoffs: u64,
    cache_hits: u64,
    completed_depth_sum: u64,
    fallback_moves: u64,
    depth_histogram: [u64; 7],
}

#[derive(Serialize)]
struct Record {
    job: Job,
    outcome: Outcome,
    a_score: f64,
    plies: usize,
    elapsed_ms: f64,
    a: Metrics,
    b: Metrics,
    save: serde_json::Value,
}

fn engine(root: &Path) -> Result<UciEngine, Box<dyn std::error::Error>> {
    Ok(UciEngine::spawn(
        "node",
        &[root
            .join("scripts/fairy-uci.cjs")
            .to_str()
            .ok_or("non-UTF8 repository path")?
            .into()],
        root.join("variants/nested.ini")
            .to_str()
            .ok_or("non-UTF8 variant path")?,
    )?)
}

fn play(job: Job) -> Result<Record, Box<dyn std::error::Error>> {
    if job.a_level > job.b_level
        || job.b_level >= 6
        || !matches!(
            job.family.as_str(),
            "global" | "local" | "fallback" | "material"
        )
        || (job.family == "fallback" && job.a_level != 0)
    {
        return Err("invalid benchmark pairing".into());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut a_engine = if matches!(job.family.as_str(), "fallback" | "material") {
        None
    } else {
        Some(engine(&root)?)
    };
    let mut b_engine = engine(&root)?;
    if let Some(engine) = &mut a_engine {
        engine.set_analysis_nodes(GLOBAL_NODES[job.a_level])?;
    }
    let mut game = match &job.initial_fens {
        Some(fens) => Game::from_fens_rectangular(job.columns, job.rows, Color::White, fens)?,
        None => Game::rectangular(job.columns, job.rows)?,
    };
    for mv in &job.opening {
        game.play(mv.board, &mv.uci)?;
    }
    if game.outcome() != Outcome::Ongoing {
        return Err("opening already finishes the match".into());
    }
    let started = Instant::now();
    let mut a = Metrics::default();
    let mut b = Metrics::default();
    let mut plies = job.opening.len();
    while game.outcome() == Outcome::Ongoing {
        let is_a = game.turn() == job.a_color;
        let (metrics, level) = if is_a {
            (&mut a, job.a_level)
        } else {
            (&mut b, job.b_level)
        };
        let move_started = Instant::now();
        let (board, mv) = if is_a && job.family == "fallback" {
            let action = fallback_move(&game)?;
            metrics.fallback_moves += 1;
            metrics.depth_histogram[0] += 1;
            (action.board, action.uci)
        } else if is_a && matches!(job.family.as_str(), "global" | "material") {
            let limits = SearchLimits {
                max_depth: (level + 1) as u8,
                max_nodes: 100000,
                milliseconds: GLOBAL_MS[level],
            };
            let result = if job.family == "material" {
                let result = futures_executor::block_on(search::best_move(
                    &game,
                    &mut MaterialEvaluator,
                    limits,
                ))?;
                metrics.material_evaluations += result.evaluations;
                result
            } else {
                let result = a_engine
                    .as_mut()
                    .expect("global policy has an engine")
                    .search(&game, limits)?;
                metrics.engine_calls += result.evaluations;
                result
            };
            metrics.search_nodes += result.nodes;
            metrics.cutoffs += result.cutoffs;
            metrics.cache_hits += result.cache_hits;
            metrics.completed_depth_sum += u64::from(result.completed_depth);
            metrics.depth_histogram[usize::from(result.completed_depth)] += 1;
            metrics.fallback_moves += u64::from(result.completed_depth == 0);
            (result.best_move.board, result.best_move.uci)
        } else {
            let engine = if is_a {
                a_engine.as_mut().expect("local policy has an engine")
            } else {
                &mut b_engine
            };
            let boards = game.legal_boards();
            let board = boards[plies % boards.len()];
            let mv = engine.best_move(&game, board, LOCAL_SKILL[level], LOCAL_MS[level])?;
            metrics.engine_calls += 1;
            (board, mv)
        };
        metrics.elapsed_ms += move_started.elapsed().as_secs_f64() * 1000.0;
        metrics.moves += 1;
        game.play(board, &mv.to_string())?;
        plies += 1;
    }
    let outcome = game.outcome();
    let a_score = match outcome {
        Outcome::Draw => 0.5,
        Outcome::White if job.a_color == Color::White => 1.0,
        Outcome::Black if job.a_color == Color::Black => 1.0,
        _ => 0.0,
    };
    Ok(Record {
        job,
        outcome,
        a_score,
        plies,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        a,
        b,
        save: serde_json::from_str(&game.save_json()?)?,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: benchmark JOBS.json RESULTS.jsonl WORKERS".into());
    }
    let workers: usize = args[3].parse()?;
    if workers == 0 || workers > 32 {
        return Err("workers must be 1–32".into());
    }
    let mut jobs: Vec<Job> = serde_json::from_str(&fs::read_to_string(&args[1])?)?;
    let mut ids = HashSet::new();
    if jobs.iter().any(|job| !ids.insert(job.id.clone())) {
        return Err("duplicate job IDs".into());
    }
    let expected: HashMap<_, _> = jobs.iter().map(|job| (job.id.as_str(), job)).collect();
    let done: HashSet<String> = if Path::new(&args[2]).exists() {
        fs::read_to_string(&args[2])?
            .lines()
            .map(|line| {
                let row: serde_json::Value = serde_json::from_str(line)?;
                let recorded: Job = serde_json::from_value(row["job"].clone())?;
                if expected
                    .get(recorded.id.as_str())
                    .is_some_and(|job| **job != recorded)
                {
                    return Err(std::io::Error::other("recorded job differs from manifest").into());
                }
                Ok(recorded.id)
            })
            .collect::<Result<_, Box<dyn std::error::Error>>>()?
    } else {
        HashSet::new()
    };
    jobs.retain(|job| !done.contains(&job.id));
    let count = jobs.len();
    let jobs = Arc::new(jobs);
    let next = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicBool::new(false));
    let (send, receive) = mpsc::channel();
    let started = Instant::now();
    std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        for _ in 0..workers {
            let jobs = Arc::clone(&jobs);
            let next = Arc::clone(&next);
            let failed = Arc::clone(&failed);
            let send = send.clone();
            scope.spawn(move || {
                while !failed.load(Ordering::Relaxed) {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(index) else {
                        break;
                    };
                    let result = play(job.clone()).map_err(|error| format!("{}: {error}", job.id));
                    if result.is_err() {
                        failed.store(true, Ordering::Relaxed);
                    }
                    if send.send(result).is_err() {
                        break;
                    }
                }
            });
        }
        drop(send);
        let mut output = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&args[2])?;
        let mut errors = Vec::new();
        let mut completed = 0;
        for result in receive {
            match result {
                Ok(record) => {
                    serde_json::to_writer(&mut output, &record)?;
                    output.write_all(b"\n")?;
                    output.flush()?;
                    completed += 1;
                    eprintln!(
                        "{completed}/{count} {} {:?} plies={} game={:.1}s elapsed={:.1}s",
                        record.job.id,
                        record.outcome,
                        record.plies,
                        record.elapsed_ms / 1000.0,
                        started.elapsed().as_secs_f64()
                    );
                }
                Err(error) => {
                    eprintln!("ERROR {error}");
                    errors.push(error);
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors.join("; ").into());
        }
        Ok(())
    })
}

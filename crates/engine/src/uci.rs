//! Native adapter for a single-board Fairy-Stockfish process.
use crate::{Error, Game, Move};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum UciError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Game(#[from] Error),
    #[error("engine timed out or closed its output")]
    Unavailable,
    #[error("engine returned an invalid move: {0}")]
    InvalidResponse(String),
    #[error("skill level must be between 0 and 20")]
    InvalidSkill,
    #[error("move time must be between 1 and 10000 milliseconds")]
    InvalidMoveTime,
    #[error("engine does not advertise the nested variant")]
    UnsupportedVariant,
}

/// A persistent UCI opponent that receives only the selected local board.
pub struct UciEngine {
    child: Child,
    input: ChildStdin,
    output: Receiver<String>,
}

impl UciEngine {
    /// Start Fairy-Stockfish with its nested.ini variant file.
    pub fn spawn(executable: &str, args: &[String], variant_path: &str) -> Result<Self, UciError> {
        let mut child = Command::new(executable)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = child.stdin.take().ok_or(UciError::Unavailable)?;
        let stdout = child.stdout.take().ok_or(UciError::Unavailable)?;
        let (sender, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut engine = Self {
            child,
            input,
            output,
        };
        engine.send("uci")?;
        engine.wait_for("uciok", Duration::from_secs(10))?;
        engine.send(&format!("setoption name VariantPath value {variant_path}"))?;
        engine.send("uci")?;
        let options = engine.wait_for("uciok", Duration::from_secs(10))?;
        if !options.iter().any(|line| {
            line.starts_with("option name UCI_Variant ")
                && line.split_whitespace().any(|word| word == "nested")
        }) {
            return Err(UciError::UnsupportedVariant);
        }
        engine.send("setoption name UCI_Variant value nested")?;
        engine.send("setoption name Use NNUE value false")?;
        engine.send("isready")?;
        engine.wait_for("readyok", Duration::from_secs(10))?;
        Ok(engine)
    }

    /// Choose a move on one eligible board at an uncalibrated skill level.
    pub fn best_move(
        &mut self,
        game: &Game,
        board: usize,
        skill: u8,
        milliseconds: u64,
    ) -> Result<Move, UciError> {
        if skill > 20 {
            return Err(UciError::InvalidSkill);
        }
        if !(1..=10_000).contains(&milliseconds) {
            return Err(UciError::InvalidMoveTime);
        }
        let legal = game.legal_moves(board)?;
        self.send("ucinewgame")?;
        self.send(&format!(
            "setoption name UCI_Chess960 value {}",
            game.board(board)?.is_chess960()
        ))?;
        self.send("setoption name UCI_LimitStrength value false")?;
        self.send(&format!("setoption name Skill Level value {skill}"))?;
        self.send("isready")?;
        self.wait_for("readyok", Duration::from_secs(10))?;
        self.send(&format!(
            "position fen {}",
            game.board(board)?.fen(game.turn())
        ))?;
        self.send(&format!("go movetime {milliseconds}"))?;
        let output = self.wait_for("bestmove ", Duration::from_millis(milliseconds + 10_000))?;
        let response = output
            .last()
            .and_then(|line| line.split_whitespace().nth(1))
            .ok_or(UciError::Unavailable)?;
        let mv: Move = response
            .parse()
            .map_err(|_| UciError::InvalidResponse(response.into()))?;
        if !legal.contains(&mv) {
            return Err(UciError::InvalidResponse(response.into()));
        }
        Ok(mv)
    }

    fn send(&mut self, command: &str) -> Result<(), UciError> {
        writeln!(self.input, "{command}")?;
        self.input.flush()?;
        Ok(())
    }

    fn wait_for(&mut self, prefix: &str, timeout: Duration) -> Result<Vec<String>, UciError> {
        let deadline = Instant::now() + timeout;
        let mut lines = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.output.recv_timeout(remaining) {
                Ok(line) => {
                    let complete = line.starts_with(prefix);
                    lines.push(line);
                    if complete {
                        return Ok(lines);
                    }
                }
                Err(_) => {
                    let _ = self.child.kill();
                    return Err(UciError::Unavailable);
                }
            }
        }
    }
}

impl Drop for UciEngine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

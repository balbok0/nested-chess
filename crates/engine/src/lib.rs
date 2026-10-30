mod board;
pub mod search;
#[cfg(not(target_arch = "wasm32"))]
pub mod uci;
pub mod uci_output;

pub use board::{
    BOARD_WIDTH, Board, CHESS960_POSITIONS, Move, MoveDetails, Piece, Role, RookMove, SQUARES,
    parse_square, square_name,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const SAVE_VERSION: u8 = 4;
const REPETITION_LIMIT: u8 = 3;
const DRAW_POINTS: f32 = 0.5;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("grid must be 2×2, 4×2, 4×4, or legacy 8×8 (columns × rows)")]
    InvalidGrid,
    #[error("Chess960 starting position must be between 0 and 959")]
    InvalidChess960,
    #[error("square must be a1 through h8")]
    InvalidSquare,
    #[error("move must be UCI notation, such as e2e4 or a7a8q")]
    InvalidMove,
    #[error("invalid king-capture FEN position")]
    InvalidFen,
    #[error("board index is out of range")]
    InvalidBoard,
    #[error("this board is not available on the current turn")]
    WrongBoard,
    #[error("move is not legal for the current player")]
    IllegalMove,
    #[error("match is already finished")]
    Finished,
    #[error("save is invalid: {0}")]
    InvalidSave(String),
    #[error("there is no move to undo")]
    NothingToUndo,
    #[error("this board is already finished")]
    BoardFinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub fn other(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Outcome {
    Ongoing,
    White,
    Black,
    Draw,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Move { board: usize, uci: String },
    Draw { board: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub white: f32,
    pub black: f32,
    pub draws: usize,
    pub remaining: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardView {
    pub id: usize,
    pub fen: String,
    /// Characters at a1 through h8; dots denote empty squares.
    pub squares: String,
    pub outcome: Outcome,
    pub white_moves: u32,
    pub black_moves: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// Legacy column count; rectangular consumers should use columns and rows.
    pub size: u8,
    pub columns: u8,
    pub rows: u8,
    pub turn: Color,
    pub forced_board: Option<usize>,
    pub requested_board: Option<usize>,
    pub legal_boards: Vec<usize>,
    pub outcome: Outcome,
    pub score: Score,
    pub ply: usize,
    pub boards: Vec<BoardView>,
    pub history: Vec<Action>,
    pub chess960: bool,
    pub start_position: Option<u16>,
    pub en_passant_rule: EnPassantRule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    version: u8,
    columns: u8,
    rows: u8,
    first_player: Color,
    initial_fens: Vec<String>,
    finish_rule: FinishRule,
    events: Vec<Action>,
    start_position: Option<u16>,
    en_passant_rule: EnPassantRule,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySave {
    #[serde(rename = "version")]
    _version: u8,
    size: u8,
    first_player: Color,
    initial_fens: Vec<String>,
    finish_rule: FinishRule,
    events: Vec<Action>,
    #[serde(default)]
    start_position: Option<u16>,
    #[serde(default)]
    en_passant_rule: EnPassantRule,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnPassantRule {
    #[default]
    Global,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FinishRule {
    Majority,
    AllBoards,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionOneSave {
    #[serde(rename = "version")]
    _version: u8,
    size: u8,
    first_player: Color,
    initial_fens: Vec<String>,
    moves: Vec<VersionOneMove>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionOneMove {
    board: usize,
    uci: String,
}

/// A routed match whose players alternate globally and capture kings to win boards.
#[derive(Debug, Clone)]
pub struct Game {
    grid: Grid,
    turn: Color,
    boards: Vec<Board>,
    outcomes: Vec<Outcome>,
    forced_board: Option<usize>,
    requested_board: Option<usize>,
    counts: Vec<[u32; 2]>,
    repetitions: HashMap<Vec<u64>, u8>,
    save: Save,
}

#[cfg(feature = "checkpoint")]
#[derive(Serialize, Deserialize)]
struct Checkpoint {
    version: u8,
    columns: u8,
    rows: u8,
    turn: Color,
    boards: Vec<Board>,
    outcomes: Vec<Outcome>,
    forced_board: Option<usize>,
    requested_board: Option<usize>,
    counts: Vec<[u32; 2]>,
    repetitions: Vec<(Vec<u64>, u8)>,
    save: Save,
}

#[cfg(feature = "checkpoint")]
impl Game {
    /// Encode an engine checkpoint for trusted persistent storage.
    pub fn checkpoint_json(&self) -> Result<String, Error> {
        serde_json::to_string(&Checkpoint {
            version: 2,
            columns: self.grid.columns,
            rows: self.grid.rows,
            turn: self.turn,
            boards: self.boards.clone(),
            outcomes: self.outcomes.clone(),
            forced_board: self.forced_board,
            requested_board: self.requested_board,
            counts: self.counts.clone(),
            repetitions: self
                .repetitions
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            save: self.save.clone(),
        })
        .map_err(|error| Error::InvalidSave(error.to_string()))
    }

    /// Restore an engine checkpoint from trusted storage without replaying moves.
    pub fn from_checkpoint_json(json: &str) -> Result<Self, Error> {
        let mut value: serde_json::Value =
            serde_json::from_str(json).map_err(|error| Error::InvalidSave(error.to_string()))?;
        if value.get("version").and_then(serde_json::Value::as_u64) == Some(1) {
            value["columns"] = value["size"].clone();
            value["rows"] = value["size"].clone();
            value["save"] = serde_json::to_value(decode_save(value["save"].take())?)
                .map_err(|error| Error::InvalidSave(error.to_string()))?;
            value["version"] = 2.into();
        }
        let value: Checkpoint =
            serde_json::from_value(value).map_err(|error| Error::InvalidSave(error.to_string()))?;
        let grid = Grid::new(value.columns, value.rows)?;
        let count = grid.count();
        if value.version != 2
            || value.save.version != SAVE_VERSION
            || value.save.columns != value.columns
            || value.save.rows != value.rows
            || value.boards.len() != count
            || value.outcomes.len() != count
            || value.counts.len() != count
            || value.forced_board.is_some_and(|id| id >= count)
            || value.requested_board.is_some_and(|id| id >= count)
            || value.boards.iter().any(|b| {
                b.castling.iter().flatten().any(|&s| s >= 64)
                    || b.en_passant.is_some_and(|s| s >= 64)
            })
        {
            return Err(Error::InvalidSave("invalid engine checkpoint".into()));
        }
        Ok(Self {
            grid,
            turn: value.turn,
            boards: value.boards,
            outcomes: value.outcomes,
            forced_board: value.forced_board,
            requested_board: value.requested_board,
            counts: value.counts,
            repetitions: value.repetitions.into_iter().collect(),
            save: value.save,
        })
    }
}

/// Map a destination square to a macro board; both use a1 as the lower left.
pub fn route(size: u8, square: u8) -> Result<usize, Error> {
    route_rectangular(size, size, square)
}

/// Map a destination square to a board in a columns-by-rows grid.
pub fn route_rectangular(columns: u8, rows: u8, square: u8) -> Result<usize, Error> {
    Grid::new(columns, rows)?.route(square)
}

#[derive(Debug, Clone, Copy)]
struct Grid {
    columns: u8,
    rows: u8,
}

impl Grid {
    fn new(columns: u8, rows: u8) -> Result<Self, Error> {
        if matches!((columns, rows), (2, 2) | (4, 2) | (4, 4) | (8, 8)) {
            Ok(Self { columns, rows })
        } else {
            Err(Error::InvalidGrid)
        }
    }
    fn count(self) -> usize {
        usize::from(self.columns) * usize::from(self.rows)
    }
    fn route(self, square: u8) -> Result<usize, Error> {
        if square >= SQUARES as u8 {
            return Err(Error::InvalidSquare);
        }
        Ok(usize::from(
            (square / BOARD_WIDTH / (BOARD_WIDTH / self.rows)) * self.columns
                + square % BOARD_WIDTH / (BOARD_WIDTH / self.columns),
        ))
    }
}

impl Game {
    /// Create a match with the same Chess960 arrangement on every local board.
    pub fn chess960(size: u8, index: u16) -> Result<Self, Error> {
        Self::chess960_rectangular(size, size, index)
    }

    pub fn chess960_rectangular(columns: u8, rows: u8, index: u16) -> Result<Self, Error> {
        let grid = Grid::new(columns, rows)?;
        let mut game = Self::from_grid_fens(
            grid,
            Color::White,
            &vec![Board::chess960(index)?.fen(Color::White); grid.count()],
        )?;
        game.save.start_position = Some(index);
        Ok(game)
    }

    /// Create a match with one Chess960 starting index per board, in board-ID order.
    pub fn chess960_boards(size: u8, indices: &[u16]) -> Result<Self, Error> {
        Self::chess960_boards_rectangular(size, size, indices)
    }

    pub fn chess960_boards_rectangular(
        columns: u8,
        rows: u8,
        indices: &[u16],
    ) -> Result<Self, Error> {
        let grid = Grid::new(columns, rows)?;
        let fens = indices
            .iter()
            .map(|index| Ok(Board::chess960(*index)?.fen(Color::White)))
            .collect::<Result<Vec<_>, Error>>()?;
        Self::from_grid_fens(grid, Color::White, &fens)
    }
    pub fn new(size: u8) -> Result<Self, Error> {
        Self::rectangular(size, size)
    }

    pub fn rectangular(columns: u8, rows: u8) -> Result<Self, Error> {
        let grid = Grid::new(columns, rows)?;
        Self::from_grid_fens(
            grid,
            Color::White,
            &vec![Board::default().fen(Color::White); grid.count()],
        )
    }

    /// Start a match from one FEN per board, ordered from macro a1 upward.
    pub fn from_fens(size: u8, turn: Color, fens: &[String]) -> Result<Self, Error> {
        Self::from_fens_rectangular(size, size, turn, fens)
    }

    pub fn from_fens_rectangular(
        columns: u8,
        rows: u8,
        turn: Color,
        fens: &[String],
    ) -> Result<Self, Error> {
        Self::from_grid_fens(Grid::new(columns, rows)?, turn, fens)
    }

    fn from_grid_fens(grid: Grid, turn: Color, fens: &[String]) -> Result<Self, Error> {
        let count = grid.count();
        if fens.len() != count {
            return Err(Error::InvalidBoard);
        }
        let boards = fens
            .iter()
            .map(|fen| Board::from_fen(fen))
            .collect::<Result<Vec<_>, _>>()?;
        let outcomes = boards.iter().map(Board::outcome).collect();
        let mut game = Self {
            grid,
            turn,
            boards,
            outcomes,
            forced_board: None,
            requested_board: None,
            counts: vec![[0; 2]; count],
            repetitions: HashMap::new(),
            save: Save {
                version: SAVE_VERSION,
                columns: grid.columns,
                rows: grid.rows,
                first_player: turn,
                initial_fens: fens.to_vec(),
                finish_rule: FinishRule::Majority,
                events: Vec::new(),
                start_position: None,
                en_passant_rule: EnPassantRule::Local,
            },
        };
        game.resolve_turn();
        game.repetitions.insert(game.position_key(), 1);
        Ok(game)
    }

    pub fn turn(&self) -> Color {
        self.turn
    }
    pub fn board(&self, id: usize) -> Result<&Board, Error> {
        self.boards.get(id).ok_or(Error::InvalidBoard)
    }

    pub fn legal_boards(&self) -> Vec<usize> {
        if self.outcome() != Outcome::Ongoing {
            return Vec::new();
        }
        if let Some(board) = self.forced_board {
            return vec![board];
        }
        self.boards
            .iter()
            .enumerate()
            .filter(|(id, board)| {
                self.outcomes[*id] == Outcome::Ongoing && !board.legal_moves(self.turn).is_empty()
            })
            .map(|(id, _)| id)
            .collect()
    }

    pub fn legal_moves(&self, id: usize) -> Result<Vec<Move>, Error> {
        self.board(id)?;
        if !self.legal_boards().contains(&id) {
            return Err(Error::WrongBoard);
        }
        Ok(self.boards[id].legal_moves(self.turn))
    }

    pub fn move_details(&self, id: usize, uci: &str) -> Result<MoveDetails, Error> {
        let mv: Move = uci.parse()?;
        if !self.legal_moves(id)?.contains(&mv) {
            return Err(Error::IllegalMove);
        }
        Ok(self.boards[id].move_details(self.turn, mv))
    }

    /// Play one global turn. Rejected actions leave the match unchanged.
    pub fn play(&mut self, id: usize, uci: &str) -> Result<(), Error> {
        if self.outcome() != Outcome::Ongoing {
            return Err(Error::Finished);
        }
        let mv: Move = uci.parse()?;
        if !self.legal_moves(id)?.contains(&mv) {
            return Err(Error::IllegalMove);
        }
        let destination = self.boards[id].move_details(self.turn, mv).destination;
        self.boards[id].apply(self.turn, mv);
        if self.save.en_passant_rule == EnPassantRule::Global {
            for (other, board) in self.boards.iter_mut().enumerate() {
                if other != id {
                    board.en_passant = None;
                }
            }
        }
        self.outcomes[id] = self.boards[id].outcome();
        self.counts[id][self.turn as usize] += 1;
        self.save.events.push(Action::Move {
            board: id,
            uci: mv.to_string(),
        });
        self.turn = self.turn.other();
        self.requested_board = Some(self.grid.route(destination)?);
        self.resolve_turn();
        let key = self.position_key();
        let count = self.repetitions.entry(key).or_default();
        *count = count.saturating_add(1);
        if *count >= REPETITION_LIMIT {
            self.draw_remaining();
        }
        Ok(())
    }

    fn resolve_turn(&mut self) {
        if self.outcome() != Outcome::Ongoing {
            self.forced_board = None;
            return;
        }
        self.forced_board = self.requested_board.filter(|id| {
            self.outcomes[*id] == Outcome::Ongoing
                && !self.boards[*id].legal_moves(self.turn).is_empty()
        });
        if self.legal_boards().is_empty() {
            self.draw_remaining();
        }
    }

    /// Record both players' agreement to draw one board without consuming a turn.
    pub fn agree_draw(&mut self, id: usize) -> Result<(), Error> {
        if self.outcome() != Outcome::Ongoing {
            return Err(Error::Finished);
        }
        self.board(id)?;
        if self.outcomes[id] != Outcome::Ongoing {
            return Err(Error::BoardFinished);
        }
        self.outcomes[id] = Outcome::Draw;
        self.save.events.push(Action::Draw { board: id });
        self.resolve_turn();
        Ok(())
    }

    fn draw_remaining(&mut self) {
        for outcome in &mut self.outcomes {
            if *outcome == Outcome::Ongoing {
                *outcome = Outcome::Draw;
            }
        }
        self.forced_board = None;
    }

    fn position_key(&self) -> Vec<u64> {
        let mut key = vec![
            self.turn as u64,
            self.forced_board.map_or(u64::MAX, |id| id as u64),
        ];
        for (board, outcome) in self.boards.iter().zip(&self.outcomes) {
            key.extend(board.pieces);
            let [wk, wq, bk, bq] = board.castling;
            key.push(u64::from_le_bytes([
                wk.unwrap_or(64),
                wq.unwrap_or(64),
                bk.unwrap_or(64),
                bq.unwrap_or(64),
                board.en_passant.unwrap_or(64),
                *outcome as u8,
                u8::from(board.chess960),
                0,
            ]));
        }
        key
    }

    pub fn score(&self) -> Score {
        self.outcomes.iter().fold(
            Score {
                white: 0.0,
                black: 0.0,
                draws: 0,
                remaining: 0,
            },
            |mut score, outcome| {
                match outcome {
                    Outcome::White => score.white += 1.0,
                    Outcome::Black => score.black += 1.0,
                    Outcome::Draw => {
                        score.white += DRAW_POINTS;
                        score.black += DRAW_POINTS;
                        score.draws += 1;
                    }
                    Outcome::Ongoing => score.remaining += 1,
                }
                score
            },
        )
    }

    pub fn outcome(&self) -> Outcome {
        let score = self.score();
        if self.save.finish_rule == FinishRule::Majority {
            let majority = self.boards.len() as f32 / 2.0;
            if score.white > majority {
                return Outcome::White;
            }
            if score.black > majority {
                return Outcome::Black;
            }
        }
        if score.remaining > 0 {
            Outcome::Ongoing
        } else {
            match score.white.total_cmp(&score.black) {
                std::cmp::Ordering::Greater => Outcome::White,
                std::cmp::Ordering::Less => Outcome::Black,
                std::cmp::Ordering::Equal => Outcome::Draw,
            }
        }
    }

    pub fn state(&self) -> State {
        State {
            size: self.grid.columns,
            columns: self.grid.columns,
            rows: self.grid.rows,
            turn: self.turn,
            forced_board: self.forced_board,
            requested_board: self.requested_board,
            legal_boards: self.legal_boards(),
            outcome: self.outcome(),
            score: self.score(),
            ply: self
                .save
                .events
                .iter()
                .filter(|event| matches!(event, Action::Move { .. }))
                .count(),
            boards: self
                .boards
                .iter()
                .enumerate()
                .map(|(id, board)| BoardView {
                    id,
                    fen: board.fen(self.turn),
                    squares: (0..SQUARES as u8)
                        .map(|square| {
                            board.piece_at(square).map_or('.', |piece| {
                                if piece.color == Color::White {
                                    piece.role.symbol().to_ascii_uppercase()
                                } else {
                                    piece.role.symbol()
                                }
                            })
                        })
                        .collect(),
                    outcome: self.outcomes[id],
                    white_moves: self.counts[id][0],
                    black_moves: self.counts[id][1],
                })
                .collect(),
            history: self.save.events.clone(),
            chess960: self.boards.iter().any(Board::is_chess960),
            start_position: self.save.start_position,
            en_passant_rule: self.save.en_passant_rule,
        }
    }

    /// Return binary piece planes in [board, color × role, rank, file] order.
    pub fn piece_planes(&self) -> Vec<u8> {
        self.boards
            .iter()
            .flat_map(|board| {
                board
                    .pieces
                    .iter()
                    .flat_map(|bits| (0..SQUARES).map(move |square| ((bits >> square) & 1) as u8))
            })
            .collect()
    }

    pub fn save_json(&self) -> Result<String, Error> {
        serde_json::to_string(&self.save).map_err(|error| Error::InvalidSave(error.to_string()))
    }

    pub fn load_json(json: &str) -> Result<Self, Error> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|error| Error::InvalidSave(error.to_string()))?;
        let save = decode_save(value)?;
        let mut game = Self::from_fens_rectangular(
            save.columns,
            save.rows,
            save.first_player,
            &save.initial_fens,
        )?;
        game.save.finish_rule = save.finish_rule;
        game.save.en_passant_rule = save.en_passant_rule;
        if save
            .start_position
            .is_some_and(|index| index >= CHESS960_POSITIONS)
        {
            return Err(Error::InvalidChess960);
        }
        game.save.start_position = save.start_position;
        for action in save.events {
            match action {
                Action::Move { board, uci } => game.play(board, &uci)?,
                Action::Draw { board } => game.agree_draw(board)?,
            }
        }
        Ok(game)
    }

    pub fn undo(&mut self) -> Result<(), Error> {
        let mut save = self.save.clone();
        save.events.pop().ok_or(Error::NothingToUndo)?;
        *self = Self::load_json(
            &serde_json::to_string(&save).map_err(|error| Error::InvalidSave(error.to_string()))?,
        )?;
        Ok(())
    }
}

fn decode_save(value: serde_json::Value) -> Result<Save, Error> {
    Ok(
        match value.get("version").and_then(serde_json::Value::as_u64) {
            Some(1) => {
                let old: VersionOneSave = serde_json::from_value(value)
                    .map_err(|error| Error::InvalidSave(error.to_string()))?;
                Save {
                    version: SAVE_VERSION,
                    columns: old.size,
                    rows: old.size,
                    first_player: old.first_player,
                    initial_fens: old.initial_fens,
                    finish_rule: FinishRule::AllBoards,
                    events: old
                        .moves
                        .into_iter()
                        .map(|event| Action::Move {
                            board: event.board,
                            uci: event.uci,
                        })
                        .collect(),
                    start_position: None,
                    en_passant_rule: EnPassantRule::Global,
                }
            }
            Some(2 | 3) => {
                let old: LegacySave = serde_json::from_value(value)
                    .map_err(|error| Error::InvalidSave(error.to_string()))?;
                Save {
                    version: SAVE_VERSION,
                    columns: old.size,
                    rows: old.size,
                    first_player: old.first_player,
                    initial_fens: old.initial_fens,
                    finish_rule: old.finish_rule,
                    events: old.events,
                    start_position: old.start_position,
                    en_passant_rule: old.en_passant_rule,
                }
            }
            Some(4) => serde_json::from_value(value)
                .map_err(|error| Error::InvalidSave(error.to_string()))?,
            _ => return Err(Error::InvalidSave("unsupported version".into())),
        },
    )
}

mod board;
#[cfg(not(target_arch = "wasm32"))]
pub mod uci;

pub use board::{BOARD_WIDTH, Board, Move, Piece, Role, SQUARES, parse_square, square_name};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const SAVE_VERSION: u8 = 1;
const REPETITION_LIMIT: u8 = 3;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("grid width must be 2, 4, or 8")]
    InvalidGrid,
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
pub struct Action {
    pub board: usize,
    pub uci: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub white: usize,
    pub black: usize,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub size: u8,
    pub turn: Color,
    pub forced_board: Option<usize>,
    pub requested_board: Option<usize>,
    pub legal_boards: Vec<usize>,
    pub outcome: Outcome,
    pub score: Score,
    pub ply: usize,
    pub boards: Vec<BoardView>,
    pub history: Vec<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    version: u8,
    size: u8,
    first_player: Color,
    initial_fens: Vec<String>,
    moves: Vec<Action>,
}

/// A routed match whose players alternate globally and capture kings to win boards.
#[derive(Debug, Clone)]
pub struct Game {
    size: u8,
    turn: Color,
    boards: Vec<Board>,
    outcomes: Vec<Outcome>,
    forced_board: Option<usize>,
    requested_board: Option<usize>,
    counts: Vec<[u32; 2]>,
    repetitions: HashMap<Vec<u64>, u8>,
    save: Save,
}

/// Map a destination square to a macro board; both use a1 as the lower left.
pub fn route(size: u8, square: u8) -> Result<usize, Error> {
    validate_size(size)?;
    if square >= SQUARES as u8 {
        return Err(Error::InvalidSquare);
    }
    let block = BOARD_WIDTH / size;
    Ok(((square / BOARD_WIDTH / block) * size + square % BOARD_WIDTH / block) as usize)
}

fn validate_size(size: u8) -> Result<(), Error> {
    if matches!(size, 2 | 4 | 8) {
        Ok(())
    } else {
        Err(Error::InvalidGrid)
    }
}

impl Game {
    pub fn new(size: u8) -> Result<Self, Error> {
        validate_size(size)?;
        Self::from_fens(
            size,
            Color::White,
            &vec![Board::default().fen(Color::White); usize::from(size).pow(2)],
        )
    }

    /// Start a match from one FEN per board, ordered from macro a1 upward.
    pub fn from_fens(size: u8, turn: Color, fens: &[String]) -> Result<Self, Error> {
        validate_size(size)?;
        let count = usize::from(size).pow(2);
        if fens.len() != count {
            return Err(Error::InvalidBoard);
        }
        let boards = fens
            .iter()
            .map(|fen| Board::from_fen(fen))
            .collect::<Result<Vec<_>, _>>()?;
        if boards
            .iter()
            .filter(|board| board.en_passant.is_some())
            .count()
            > 1
        {
            return Err(Error::InvalidFen);
        }
        if boards.iter().any(|board| {
            board.en_passant.is_some_and(|square| {
                square / BOARD_WIDTH != if turn == Color::White { 5 } else { 2 }
            })
        }) {
            return Err(Error::InvalidFen);
        }
        let outcomes = boards.iter().map(Board::outcome).collect();
        let mut game = Self {
            size,
            turn,
            boards,
            outcomes,
            forced_board: None,
            requested_board: None,
            counts: vec![[0; 2]; count],
            repetitions: HashMap::new(),
            save: Save {
                version: SAVE_VERSION,
                size,
                first_player: turn,
                initial_fens: fens.to_vec(),
                moves: Vec::new(),
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

    /// Play one global turn. Rejected actions leave the match unchanged.
    pub fn play(&mut self, id: usize, uci: &str) -> Result<(), Error> {
        if self.outcome() != Outcome::Ongoing {
            return Err(Error::Finished);
        }
        let mv: Move = uci.parse()?;
        if !self.legal_moves(id)?.contains(&mv) {
            return Err(Error::IllegalMove);
        }
        self.boards[id].apply(self.turn, mv);
        for (other, board) in self.boards.iter_mut().enumerate() {
            if other != id {
                board.en_passant = None;
            }
        }
        self.outcomes[id] = self.boards[id].outcome();
        self.counts[id][self.turn as usize] += 1;
        self.save.moves.push(Action {
            board: id,
            uci: mv.to_string(),
        });
        self.turn = self.turn.other();
        self.requested_board = Some(route(self.size, mv.to)?);
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
        self.forced_board = self.requested_board.filter(|id| {
            self.outcomes[*id] == Outcome::Ongoing
                && !self.boards[*id].legal_moves(self.turn).is_empty()
        });
        if self.legal_boards().is_empty() {
            self.draw_remaining();
        }
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
            key.push(
                board.castling as u64
                    | (board.en_passant.map_or(64, u64::from) << 8)
                    | ((*outcome as u64) << 16),
            );
        }
        key
    }

    pub fn score(&self) -> Score {
        Score {
            white: self
                .outcomes
                .iter()
                .filter(|outcome| **outcome == Outcome::White)
                .count(),
            black: self
                .outcomes
                .iter()
                .filter(|outcome| **outcome == Outcome::Black)
                .count(),
            draws: self
                .outcomes
                .iter()
                .filter(|outcome| **outcome == Outcome::Draw)
                .count(),
            remaining: self
                .outcomes
                .iter()
                .filter(|outcome| **outcome == Outcome::Ongoing)
                .count(),
        }
    }

    pub fn outcome(&self) -> Outcome {
        let score = self.score();
        if score.remaining > 0 {
            Outcome::Ongoing
        } else {
            match score.white.cmp(&score.black) {
                std::cmp::Ordering::Greater => Outcome::White,
                std::cmp::Ordering::Less => Outcome::Black,
                std::cmp::Ordering::Equal => Outcome::Draw,
            }
        }
    }

    pub fn state(&self) -> State {
        State {
            size: self.size,
            turn: self.turn,
            forced_board: self.forced_board,
            requested_board: self.requested_board,
            legal_boards: self.legal_boards(),
            outcome: self.outcome(),
            score: self.score(),
            ply: self.save.moves.len(),
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
            history: self.save.moves.clone(),
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
        let save: Save =
            serde_json::from_str(json).map_err(|error| Error::InvalidSave(error.to_string()))?;
        if save.version != SAVE_VERSION {
            return Err(Error::InvalidSave("unsupported version".into()));
        }
        let mut game = Self::from_fens(save.size, save.first_player, &save.initial_fens)?;
        for action in save.moves {
            game.play(action.board, &action.uci)?;
        }
        Ok(game)
    }

    pub fn undo(&mut self) -> Result<(), Error> {
        let mut save = self.save.clone();
        save.moves.pop().ok_or(Error::NothingToUndo)?;
        *self = Self::load_json(
            &serde_json::to_string(&save).map_err(|error| Error::InvalidSave(error.to_string()))?,
        )?;
        Ok(())
    }
}

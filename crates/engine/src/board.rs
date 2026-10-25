use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

use crate::{Color, Error, Outcome};

pub const BOARD_WIDTH: u8 = 8;
pub const SQUARES: usize = 64;
pub const CHESS960_POSITIONS: u16 = 960;
const NO_PROGRESS_LIMIT: u32 = 100;
const KNIGHT_PAIRS: [(usize, usize); 10] = [
    (0, 1),
    (0, 2),
    (0, 3),
    (0, 4),
    (1, 2),
    (1, 3),
    (1, 4),
    (2, 3),
    (2, 4),
    (3, 4),
];
const KNIGHT_STEPS: [(i8, i8); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];
const RAYS: [(i8, i8); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];
const PROMOTIONS: [Role; 4] = [Role::Queen, Role::Rook, Role::Bishop, Role::Knight];
const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Role {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl Role {
    pub fn symbol(self) -> char {
        ['p', 'n', 'b', 'r', 'q', 'k'][self as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub role: Role,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Move {
    pub from: u8,
    pub to: u8,
    pub promotion: Option<Role>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RookMove {
    pub from: u8,
    pub to: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveDetails {
    pub destination: u8,
    pub rook: Option<RookMove>,
}

struct Castle {
    king_to: u8,
    rook_from: u8,
    rook_to: u8,
}

pub fn square_name(square: u8) -> Result<String, Error> {
    if square >= SQUARES as u8 {
        return Err(Error::InvalidSquare);
    }
    Ok(format!(
        "{}{}",
        char::from(b'a' + square % BOARD_WIDTH),
        1 + square / BOARD_WIDTH
    ))
}

pub fn parse_square(name: &str) -> Result<u8, Error> {
    let bytes = name.as_bytes();
    if bytes.len() != 2 || !(b'a'..=b'h').contains(&bytes[0]) || !(b'1'..=b'8').contains(&bytes[1])
    {
        return Err(Error::InvalidSquare);
    }
    Ok((bytes[1] - b'1') * BOARD_WIDTH + bytes[0] - b'a')
}

impl FromStr for Move {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Error> {
        if !value.is_ascii() || !matches!(value.len(), 4 | 5) {
            return Err(Error::InvalidMove);
        }
        let promotion = if value.len() == 5 {
            Some(
                *PROMOTIONS
                    .iter()
                    .find(|role| role.symbol() as u8 == value.as_bytes()[4])
                    .ok_or(Error::InvalidMove)?,
            )
        } else {
            None
        };
        Ok(Self {
            from: parse_square(&value[..2])?,
            to: parse_square(&value[2..4])?,
            promotion,
        })
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let from = square_name(self.from).map_err(|_| fmt::Error)?;
        let to = square_name(self.to).map_err(|_| fmt::Error)?;
        write!(f, "{from}{to}")?;
        if let Some(role) = self.promotion {
            write!(f, "{}", role.symbol())?;
        }
        Ok(())
    }
}

/// A local king-capture position, without a local side to move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub(crate) pieces: [u64; 12],
    pub(crate) castling: [Option<u8>; 4],
    pub(crate) chess960: bool,
    pub(crate) en_passant: Option<u8>,
    pub(crate) halfmoves: u32,
}

impl Default for Board {
    fn default() -> Self {
        Self::from_fen(START_FEN).expect("valid starting position")
    }
}

impl Board {
    /// Create a mirrored Chess960 start using its Scharnagl number, 0 through 959.
    pub fn chess960(index: u16) -> Result<Self, Error> {
        if index >= CHESS960_POSITIONS {
            return Err(Error::InvalidChess960);
        }
        let mut number = usize::from(index);
        let mut back = [None; BOARD_WIDTH as usize];
        back[(number % 4) * 2 + 1] = Some(Role::Bishop);
        number /= 4;
        back[(number % 4) * 2] = Some(Role::Bishop);
        number /= 4;
        back[empty_files(&back)[number % 6]] = Some(Role::Queen);
        number /= 6;
        let empty = empty_files(&back);
        let (first, second) = KNIGHT_PAIRS[number];
        back[empty[first]] = Some(Role::Knight);
        back[empty[second]] = Some(Role::Knight);
        let empty = empty_files(&back);
        for (file, role) in empty.iter().zip([Role::Rook, Role::King, Role::Rook]) {
            back[*file] = Some(role);
        }
        let white: String = back
            .into_iter()
            .map(|role| {
                role.expect("complete back rank")
                    .symbol()
                    .to_ascii_uppercase()
            })
            .collect();
        let black = white.to_ascii_lowercase();
        let left = char::from(b'A' + empty[0] as u8);
        let right = char::from(b'A' + empty[2] as u8);
        Self::from_fen(&format!(
            "{black}/pppppppp/8/8/8/8/PPPPPPPP/{white} w {right}{left}{}{} - 0 1",
            right.to_ascii_lowercase(),
            left.to_ascii_lowercase()
        ))
    }

    pub fn is_chess960(&self) -> bool {
        self.chess960
    }

    /// Read a FEN position. The nested game supplies the side to move.
    pub fn from_fen(fen: &str) -> Result<Self, Error> {
        let fields: Vec<_> = fen.split_whitespace().collect();
        if fields.len() != 6
            || !matches!(fields[1], "w" | "b")
            || fields[5].parse::<u32>().ok().filter(|n| *n > 0).is_none()
        {
            return Err(Error::InvalidFen);
        }
        let mut board = Self {
            pieces: [0; 12],
            castling: [None; 4],
            chess960: false,
            en_passant: None,
            halfmoves: fields[4].parse().map_err(|_| Error::InvalidFen)?,
        };
        let ranks: Vec<_> = fields[0].split('/').collect();
        if ranks.len() != BOARD_WIDTH as usize {
            return Err(Error::InvalidFen);
        }
        for (rank, text) in ranks.iter().rev().enumerate() {
            let mut file = 0usize;
            for symbol in text.chars() {
                if ('1'..='8').contains(&symbol) {
                    file += symbol.to_digit(10).ok_or(Error::InvalidFen)? as usize;
                } else {
                    let role = [
                        Role::Pawn,
                        Role::Knight,
                        Role::Bishop,
                        Role::Rook,
                        Role::Queen,
                        Role::King,
                    ]
                    .into_iter()
                    .find(|role| role.symbol() == symbol.to_ascii_lowercase())
                    .ok_or(Error::InvalidFen)?;
                    if file >= BOARD_WIDTH as usize {
                        return Err(Error::InvalidFen);
                    }
                    board.put(
                        (rank * BOARD_WIDTH as usize + file) as u8,
                        Piece {
                            color: if symbol.is_ascii_uppercase() {
                                Color::White
                            } else {
                                Color::Black
                            },
                            role,
                        },
                    );
                    file += 1;
                }
                if file > BOARD_WIDTH as usize {
                    return Err(Error::InvalidFen);
                }
            }
            if file != BOARD_WIDTH as usize {
                return Err(Error::InvalidFen);
            }
        }
        for color in [Color::White, Color::Black] {
            if board.bitboard(color, Role::King).count_ones() != 1 {
                return Err(Error::InvalidFen);
            }
        }
        if board.pieces[Role::Pawn as usize] & 0xff000000000000ff != 0
            || board.pieces[6 + Role::Pawn as usize] & 0xff000000000000ff != 0
        {
            return Err(Error::InvalidFen);
        }
        if fields[2] != "-" {
            for symbol in fields[2].chars() {
                let (color, rook, side) = match symbol {
                    'K' => (Color::White, 7, 0),
                    'Q' => (Color::White, 0, 1),
                    'k' => (Color::Black, 63, 0),
                    'q' => (Color::Black, 56, 1),
                    'A'..='H' | 'a'..='h' => {
                        board.chess960 = true;
                        let color = if symbol.is_ascii_uppercase() {
                            Color::White
                        } else {
                            Color::Black
                        };
                        let base = if color == Color::White { 0 } else { 56 };
                        let rook = base + symbol.to_ascii_lowercase() as u8 - b'a';
                        let king = board.bitboard(color, Role::King).trailing_zeros() as u8;
                        (color, rook, usize::from(rook < king))
                    }
                    _ => return Err(Error::InvalidFen),
                };
                let king = board.bitboard(color, Role::King).trailing_zeros() as u8;
                let base = if color == Color::White { 0 } else { 56 };
                let slot = color as usize * 2 + side;
                if board.castling[slot].is_some()
                    || king / BOARD_WIDTH != base / BOARD_WIDTH
                    || board.piece_at(rook)
                        != Some(Piece {
                            color,
                            role: Role::Rook,
                        })
                    || (matches!(symbol, 'K' | 'Q' | 'k' | 'q') && king != base + 4)
                {
                    return Err(Error::InvalidFen);
                }
                board.castling[slot] = Some(rook);
            }
        }
        if fields[3] != "-" {
            let target = parse_square(fields[3]).map_err(|_| Error::InvalidFen)?;
            let (pawn, color) = match target / BOARD_WIDTH {
                2 => (target + BOARD_WIDTH, Color::White),
                5 => (target - BOARD_WIDTH, Color::Black),
                _ => return Err(Error::InvalidFen),
            };
            if board.piece_at(target).is_some()
                || board.piece_at(pawn)
                    != Some(Piece {
                        color,
                        role: Role::Pawn,
                    })
            {
                return Err(Error::InvalidFen);
            }
            board.en_passant = Some(target);
        }
        Ok(board)
    }

    pub fn bitboard(&self, color: Color, role: Role) -> u64 {
        self.pieces[color as usize * 6 + role as usize]
    }

    pub fn piece_at(&self, square: u8) -> Option<Piece> {
        if square >= SQUARES as u8 {
            return None;
        }
        self.pieces
            .iter()
            .position(|bits| bits & (1u64 << square) != 0)
            .map(|index| Piece {
                color: if index < 6 {
                    Color::White
                } else {
                    Color::Black
                },
                role: [
                    Role::Pawn,
                    Role::Knight,
                    Role::Bishop,
                    Role::Rook,
                    Role::Queen,
                    Role::King,
                ][index % 6],
            })
    }

    fn put(&mut self, square: u8, piece: Piece) {
        self.pieces[piece.color as usize * 6 + piece.role as usize] |= 1u64 << square;
    }
    fn remove(&mut self, square: u8) {
        for bits in &mut self.pieces {
            *bits &= !(1u64 << square);
        }
    }

    /// Generate moves for the global player; king safety imposes no restrictions.
    pub fn legal_moves(&self, color: Color) -> Vec<Move> {
        let mut moves = Vec::new();
        for from in 0..SQUARES as u8 {
            let Some(piece) = self.piece_at(from).filter(|piece| piece.color == color) else {
                continue;
            };
            let mut add = |to: u8| {
                if piece.role == Role::Pawn && matches!(to / BOARD_WIDTH, 0 | 7) {
                    for promotion in PROMOTIONS {
                        moves.push(Move {
                            from,
                            to,
                            promotion: Some(promotion),
                        });
                    }
                } else {
                    moves.push(Move {
                        from,
                        to,
                        promotion: None,
                    });
                }
            };
            if piece.role == Role::Pawn {
                let forward = if color == Color::White { 1 } else { -1 };
                if let Some(to) = offset(from, 0, forward).filter(|to| self.piece_at(*to).is_none())
                {
                    add(to);
                    if from / BOARD_WIDTH == if color == Color::White { 1 } else { 6 } {
                        if let Some(to) =
                            offset(from, 0, forward * 2).filter(|to| self.piece_at(*to).is_none())
                        {
                            add(to);
                        }
                    }
                }
                for file_step in [-1, 1] {
                    if let Some(to) = offset(from, file_step, forward) {
                        let capture = self.piece_at(to).is_some_and(|piece| piece.color != color);
                        let ep = self.en_passant == Some(to)
                            && offset(to, 0, -forward).is_some_and(|victim| {
                                self.piece_at(victim)
                                    == Some(Piece {
                                        color: color.other(),
                                        role: Role::Pawn,
                                    })
                            });
                        if capture || ep {
                            add(to);
                        }
                    }
                }
                continue;
            }
            let directions: &[(i8, i8)] = match piece.role {
                Role::Knight => &KNIGHT_STEPS,
                Role::Bishop => &RAYS[4..],
                Role::Rook => &RAYS[..4],
                _ => &RAYS,
            };
            for &(dx, dy) in directions {
                let mut cursor = from;
                while let Some(to) = offset(cursor, dx, dy) {
                    if self.piece_at(to).is_some_and(|piece| piece.color == color) {
                        break;
                    }
                    add(to);
                    if self.piece_at(to).is_some()
                        || matches!(piece.role, Role::King | Role::Knight)
                    {
                        break;
                    }
                    cursor = to;
                }
            }
            if piece.role == Role::King {
                let base = if color == Color::White { 0 } else { 56 };
                for side in 0..2 {
                    if let Some(rook) = self.castling[color as usize * 2 + side] {
                        let to = if self.chess960 {
                            rook
                        } else {
                            base + if side == 0 { 6 } else { 2 }
                        };
                        let castle = self
                            .castle(
                                color,
                                Move {
                                    from,
                                    to,
                                    promotion: None,
                                },
                            )
                            .expect("matching castling right");
                        let occupied = self.pieces.iter().fold(0, |bits, next| bits | next)
                            & !(1u64 << from | 1u64 << rook);
                        if (from.min(castle.king_to)..=from.max(castle.king_to))
                            .chain(rook.min(castle.rook_to)..=rook.max(castle.rook_to))
                            .all(|square| occupied & (1u64 << square) == 0)
                        {
                            add(to);
                        }
                    }
                }
            }
        }
        moves
    }

    pub(crate) fn apply(&mut self, color: Color, mv: Move) {
        let piece = self.piece_at(mv.from).expect("validated move has a piece");
        let castle = self.castle(color, mv);
        let captured = if castle.is_some() {
            None
        } else {
            self.piece_at(mv.to)
        };
        if piece.role == Role::Pawn && self.en_passant == Some(mv.to) && captured.is_none() {
            self.remove(if color == Color::White {
                mv.to - BOARD_WIDTH
            } else {
                mv.to + BOARD_WIDTH
            });
        }
        self.en_passant = None;
        if let Some(castle) = castle {
            self.remove(mv.from);
            self.remove(castle.rook_from);
            self.put(
                castle.king_to,
                Piece {
                    color,
                    role: Role::King,
                },
            );
            self.put(
                castle.rook_to,
                Piece {
                    color,
                    role: Role::Rook,
                },
            );
        } else {
            self.remove(mv.from);
            self.remove(mv.to);
            self.put(
                mv.to,
                Piece {
                    color,
                    role: mv.promotion.unwrap_or(piece.role),
                },
            );
        }
        if piece.role == Role::King {
            self.castling[color as usize * 2..color as usize * 2 + 2].fill(None);
        }
        if captured.is_some_and(|piece| piece.role == Role::King) {
            self.castling[color.other() as usize * 2..color.other() as usize * 2 + 2].fill(None);
        }
        for right in &mut self.castling {
            if *right == Some(mv.from) || *right == Some(mv.to) {
                *right = None;
            }
        }
        if piece.role == Role::Pawn && mv.from.abs_diff(mv.to) == BOARD_WIDTH * 2 {
            self.en_passant = Some((mv.from + mv.to) / 2);
        }
        self.halfmoves = if piece.role == Role::Pawn || captured.is_some() {
            0
        } else {
            self.halfmoves.saturating_add(1)
        };
    }

    fn castle(&self, color: Color, mv: Move) -> Option<Castle> {
        if self.piece_at(mv.from)
            != Some(Piece {
                color,
                role: Role::King,
            })
        {
            return None;
        }
        let base = if color == Color::White { 0 } else { 56 };
        for side in 0..2 {
            if let Some(rook) = self.castling[color as usize * 2 + side] {
                let king_to = base + if side == 0 { 6 } else { 2 };
                if mv.to == if self.chess960 { rook } else { king_to } {
                    return Some(Castle {
                        king_to,
                        rook_from: rook,
                        rook_to: base + if side == 0 { 5 } else { 3 },
                    });
                }
            }
        }
        None
    }

    pub(crate) fn move_details(&self, color: Color, mv: Move) -> MoveDetails {
        self.castle(color, mv).map_or(
            MoveDetails {
                destination: mv.to,
                rook: None,
            },
            |castle| MoveDetails {
                destination: castle.king_to,
                rook: Some(RookMove {
                    from: castle.rook_from,
                    to: castle.rook_to,
                }),
            },
        )
    }

    pub(crate) fn outcome(&self) -> Outcome {
        if self.bitboard(Color::White, Role::King) == 0 {
            Outcome::Black
        } else if self.bitboard(Color::Black, Role::King) == 0 {
            Outcome::White
        } else if self.halfmoves >= NO_PROGRESS_LIMIT {
            Outcome::Draw
        } else {
            Outcome::Ongoing
        }
    }

    pub fn fen(&self, turn: Color) -> String {
        let mut rows = Vec::new();
        for rank in (0..BOARD_WIDTH).rev() {
            let mut row = String::new();
            let mut empty = 0u8;
            for file in 0..BOARD_WIDTH {
                if let Some(piece) = self.piece_at(rank * BOARD_WIDTH + file) {
                    if empty > 0 {
                        row.push(char::from(b'0' + empty));
                        empty = 0;
                    }
                    row.push(if piece.color == Color::White {
                        piece.role.symbol().to_ascii_uppercase()
                    } else {
                        piece.role.symbol()
                    });
                } else {
                    empty += 1;
                }
            }
            if empty > 0 {
                row.push(char::from(b'0' + empty));
            }
            rows.push(row);
        }
        let rights: String = self
            .castling
            .iter()
            .enumerate()
            .filter_map(|(index, square)| {
                square.map(|square| {
                    if self.chess960 {
                        char::from(if index < 2 { b'A' } else { b'a' } + square % BOARD_WIDTH)
                    } else {
                        ['K', 'Q', 'k', 'q'][index]
                    }
                })
            })
            .collect();
        let rights = if rights.is_empty() { "-" } else { &rights };
        let ep = self
            .en_passant
            .filter(|square| square / BOARD_WIDTH == if turn == Color::White { 5 } else { 2 })
            .map(|square| square_name(square).expect("valid en passant square"))
            .unwrap_or_else(|| "-".into());
        let placement = rows.join("/");
        let side = if turn == Color::White { "w" } else { "b" };
        let halfmoves = self.halfmoves;
        format!("{placement} {side} {rights} {ep} {halfmoves} 1")
    }
}

fn empty_files(back: &[Option<Role>; BOARD_WIDTH as usize]) -> Vec<usize> {
    back.iter()
        .enumerate()
        .filter_map(|(index, piece)| piece.is_none().then_some(index))
        .collect()
}

fn offset(square: u8, dx: i8, dy: i8) -> Option<u8> {
    let file = (square % BOARD_WIDTH) as i8 + dx;
    let rank = (square / BOARD_WIDTH) as i8 + dy;
    if (0..BOARD_WIDTH as i8).contains(&file) && (0..BOARD_WIDTH as i8).contains(&rank) {
        Some(rank as u8 * BOARD_WIDTH + file as u8)
    } else {
        None
    }
}

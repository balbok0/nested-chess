//! Global negamax search with independent-board match outcome estimates.
use crate::{Board, Color, Error, Game, Move, Outcome, Role};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, future::Future};
use web_time::{Duration, Instant};

const CP_SCALE: f64 = 400.0;
const ESTIMATE_FLOOR: f64 = 0.001;
const TERMINAL_VALUE: f64 = 2.0;
const HEURISTIC_LIMIT: f64 = 1.0 - 1e-9;
/// Ranked moves per eligible board before king-capture and routing additions.
pub const DEFAULT_CANDIDATES: u16 = 10;

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error(transparent)]
    Game(#[from] Error),
    #[error("board probabilities must be finite, nonnegative, and sum to one")]
    InvalidProbabilities,
    #[error("provide one probability estimate per unfinished board")]
    InvalidEstimateCount,
    #[error("search requires depth 1–64, a positive node budget, and 1–60000 milliseconds")]
    InvalidLimits,
    #[error("candidate count must be between 1 and 500")]
    InvalidCandidateCount,
    #[error("local evaluation failed: {0}")]
    Evaluation(String),
}

/// White/draw/Black outcome probabilities for a board or match.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OutcomeEstimate {
    pub white: f64,
    pub draw: f64,
    pub black: f64,
}

impl OutcomeEstimate {
    pub fn new(white: f64, draw: f64, black: f64) -> Result<Self, SearchError> {
        if [white, draw, black]
            .iter()
            .any(|p| !p.is_finite() || *p < 0.0)
            || (white + draw + black - 1.0).abs() > 1e-9
        {
            return Err(SearchError::InvalidProbabilities);
        }
        Ok(Self { white, draw, black })
    }

    pub fn expected_score(self, color: Color) -> f64 {
        (match color {
            Color::White => self.white,
            Color::Black => self.black,
        }) + self.draw * 0.5
    }
}

/// Combine estimates in unfinished-board order with the match's settled score.
pub fn match_estimate(
    game: &Game,
    boards: &[OutcomeEstimate],
) -> Result<OutcomeEstimate, SearchError> {
    match game.outcome() {
        Outcome::White => return OutcomeEstimate::new(1.0, 0.0, 0.0),
        Outcome::Black => return OutcomeEstimate::new(0.0, 0.0, 1.0),
        Outcome::Draw => return OutcomeEstimate::new(0.0, 1.0, 0.0),
        Outcome::Ongoing => {}
    }
    if boards.len() != game.score().remaining {
        return Err(SearchError::InvalidEstimateCount);
    }
    let total = game.boards.len();
    let mut distribution = vec![0.0; total * 2 + 1];
    distribution[(game.score().white * 2.0) as usize] = 1.0;
    for board in boards {
        OutcomeEstimate::new(board.white, board.draw, board.black)?;
        let mut next = vec![0.0; distribution.len()];
        for (score, probability) in distribution.iter().enumerate().filter(|(_, p)| **p != 0.0) {
            next[score] += probability * board.black;
            next[score + 1] += probability * board.draw;
            next[score + 2] += probability * board.white;
        }
        distribution = next;
    }
    Ok(OutcomeEstimate {
        white: distribution[total + 1..].iter().sum(),
        draw: distribution[total],
        black: distribution[..total].iter().sum(),
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalEvaluation {
    Cp { value: i32 },
    Capture { winner: Color },
}

impl LocalEvaluation {
    /// Convert a White-relative local score to uncalibrated, decisive-board odds.
    pub fn estimate(self) -> OutcomeEstimate {
        let white = match self {
            Self::Cp { value } => 0.5 + 0.5 * (f64::from(value) / CP_SCALE).tanh(),
            Self::Capture {
                winner: Color::White,
            } => 1.0,
            Self::Capture {
                winner: Color::Black,
            } => 0.0,
        }
        .clamp(ESTIMATE_FLOOR, 1.0 - ESTIMATE_FLOOR);
        OutcomeEstimate {
            white,
            draw: 0.0,
            black: 1.0 - white,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAnalysis {
    pub evaluation: LocalEvaluation,
    pub best_move: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalCandidate {
    #[serde(serialize_with = "serialize_uci")]
    pub uci: Move,
    pub evaluation: LocalEvaluation,
}

/// A local oracle receiving a single FEN and its castling convention.
pub trait Evaluator {
    fn evaluate(
        &mut self,
        fen: &str,
        chess960: bool,
    ) -> impl Future<Output = Result<LocalAnalysis, SearchError>>;

    /// Return up to `count` ranked legal moves and White-relative scores.
    fn candidates(
        &mut self,
        _fen: &str,
        _chess960: bool,
        _count: u16,
    ) -> impl Future<Output = Result<Vec<LocalCandidate>, SearchError>> {
        std::future::ready(Err(SearchError::Evaluation(
            "candidate analysis is unavailable".into(),
        )))
    }
}

/// White-relative material evaluation without positional terms or local search.
pub struct MaterialEvaluator;

impl Evaluator for MaterialEvaluator {
    async fn evaluate(&mut self, fen: &str, _chess960: bool) -> Result<LocalAnalysis, SearchError> {
        let board = Board::from_fen(fen)?;
        let value = [
            (Role::Pawn, 100),
            (Role::Knight, 300),
            (Role::Bishop, 300),
            (Role::Rook, 500),
            (Role::Queen, 900),
        ]
        .into_iter()
        .map(|(role, value)| {
            value
                * (board.bitboard(Color::White, role).count_ones() as i32
                    - board.bitboard(Color::Black, role).count_ones() as i32)
        })
        .sum();
        Ok(LocalAnalysis {
            evaluation: LocalEvaluation::Cp { value },
            best_move: None,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchLimits {
    pub max_depth: u8,
    pub max_nodes: u64,
    pub milliseconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SearchMove {
    pub board: usize,
    #[serde(serialize_with = "serialize_uci")]
    pub uci: Move,
}

fn serialize_uci<S: serde::Serializer>(mv: &Move, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&mv.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub best_move: SearchMove,
    /// Expected match score for the root player at the last completed depth.
    pub expected_score: Option<f64>,
    pub completed_depth: u8,
    pub nodes: u64,
    pub cutoffs: u64,
    pub evaluations: u64,
    /// Candidate-analysis calls, included in `evaluations`.
    pub candidate_evaluations: u64,
    pub cache_hits: u64,
}

struct Search<'a, E> {
    oracle: &'a mut E,
    cache: HashMap<(String, bool), LocalAnalysis>,
    candidate_cache: HashMap<(String, bool), Vec<LocalCandidate>>,
    candidate_count: Option<u16>,
    deadline: Instant,
    limits: SearchLimits,
    result: SearchResult,
}

/// Return the capture/promotion-priority move used before any search depth completes.
pub fn fallback_move(game: &Game) -> Result<SearchMove, SearchError> {
    if game.outcome() != Outcome::Ongoing {
        return Err(Error::Finished.into());
    }
    legal_actions(game)
        .into_iter()
        .next()
        .ok_or_else(|| Error::Finished.into())
}

/// Search all legal global moves within the supplied depth, node, and time limits.
pub async fn best_move<E: Evaluator>(
    game: &Game,
    oracle: &mut E,
    limits: SearchLimits,
) -> Result<SearchResult, SearchError> {
    run_search(game, oracle, limits, None).await
}

/// Search ranked local candidates, king captures, and routing representatives for both players.
pub async fn best_move_selective<E: Evaluator>(
    game: &Game,
    oracle: &mut E,
    limits: SearchLimits,
    candidates: u16,
) -> Result<SearchResult, SearchError> {
    if !(1..=500).contains(&candidates) {
        return Err(SearchError::InvalidCandidateCount);
    }
    run_search(game, oracle, limits, Some(candidates)).await
}

async fn run_search<E: Evaluator>(
    game: &Game,
    oracle: &mut E,
    limits: SearchLimits,
    candidate_count: Option<u16>,
) -> Result<SearchResult, SearchError> {
    if limits.max_depth == 0
        || limits.max_depth > 64
        || limits.max_nodes == 0
        || !(1..=60_000).contains(&limits.milliseconds)
    {
        return Err(SearchError::InvalidLimits);
    }
    let first = fallback_move(game)?;
    let mut search = Search {
        oracle,
        cache: HashMap::new(),
        candidate_cache: HashMap::new(),
        candidate_count,
        deadline: Instant::now() + Duration::from_millis(limits.milliseconds),
        limits,
        result: SearchResult {
            best_move: first,
            expected_score: None,
            completed_depth: 0,
            nodes: 0,
            cutoffs: 0,
            evaluations: 0,
            candidate_evaluations: 0,
            cache_hits: 0,
        },
    };
    for depth in 1..=limits.max_depth {
        let Some((value, action)) = search
            .negamax(
                game,
                depth,
                -TERMINAL_VALUE,
                TERMINAL_VALUE,
                Some(search.result.best_move),
            )
            .await?
        else {
            break;
        };
        search.result.best_move = action.expect("ongoing root has legal moves");
        search.result.expected_score = Some(((value + 1.0) * 0.5).clamp(0.0, 1.0));
        search.result.completed_depth = depth;
        if value == TERMINAL_VALUE {
            break;
        }
    }
    Ok(search.result)
}

impl<E: Evaluator> Search<'_, E> {
    async fn actions(&mut self, game: &Game) -> Result<Option<Vec<SearchMove>>, SearchError> {
        let legal = legal_actions(game);
        let Some(count) = self.candidate_count else {
            return Ok(Some(legal));
        };
        for action in legal
            .iter()
            .filter(|action| is_king_capture(game, **action))
        {
            let mut child = game.clone();
            child.play(action.board, &action.uci.to_string())?;
            if matches!(
                (game.turn(), child.outcome()),
                (Color::White, Outcome::White) | (Color::Black, Outcome::Black)
            ) {
                return Ok(Some(vec![*action]));
            }
        }
        let mut selected = Vec::new();
        for board in game.legal_boards() {
            if self.exhausted() {
                return Ok(None);
            }
            let board_moves: Vec<_> = legal
                .iter()
                .filter(|action| action.board == board)
                .copied()
                .collect();
            if board_moves.len() <= usize::from(count) {
                selected.extend(board_moves);
                continue;
            }
            let key = (
                game.boards[board].fen(game.turn()),
                game.boards[board].is_chess960(),
            );
            if !self.candidate_cache.contains_key(&key) {
                let candidates = self.oracle.candidates(&key.0, key.1, count).await?;
                self.result.evaluations += 1;
                self.result.candidate_evaluations += 1;
                let first = candidates
                    .first()
                    .ok_or_else(|| SearchError::Evaluation("no candidate moves".into()))?;
                self.cache.insert(
                    key.clone(),
                    LocalAnalysis {
                        evaluation: first.evaluation,
                        best_move: Some(first.uci.to_string()),
                    },
                );
                self.candidate_cache.insert(key.clone(), candidates);
            } else {
                self.result.cache_hits += 1;
            }
            if self.exhausted() {
                return Ok(None);
            }
            let candidates = &self.candidate_cache[&key];
            let mut routes = std::collections::HashSet::new();
            for action in &board_moves {
                if candidates
                    .iter()
                    .any(|candidate| candidate.uci == action.uci)
                    || is_king_capture(game, *action)
                {
                    routes.insert(
                        game.grid.route(
                            game.boards[board]
                                .move_details(game.turn(), action.uci)
                                .destination,
                        )?,
                    );
                    selected.push(*action);
                }
            }
            for action in board_moves {
                if routes.insert(
                    game.grid.route(
                        game.boards[board]
                            .move_details(game.turn(), action.uci)
                            .destination,
                    )?,
                ) {
                    selected.push(action);
                }
            }
        }
        Ok(Some(selected))
    }

    fn exhausted(&self) -> bool {
        self.result.nodes >= self.limits.max_nodes || Instant::now() >= self.deadline
    }

    async fn negamax(
        &mut self,
        game: &Game,
        depth: u8,
        mut alpha: f64,
        beta: f64,
        preferred: Option<SearchMove>,
    ) -> Result<Option<(f64, Option<SearchMove>)>, SearchError> {
        if self.exhausted() {
            return Ok(None);
        }
        self.result.nodes += 1;
        if game.outcome() != Outcome::Ongoing {
            return Ok(Some((
                (match_estimate(game, &[])?.expected_score(game.turn()) * 2.0 - 1.0)
                    * TERMINAL_VALUE,
                None,
            )));
        }
        if depth == 0 {
            return Ok(self.evaluate_match(game).await?.map(|value| (value, None)));
        }
        let Some(mut actions) = self.actions(game).await? else {
            return Ok(None);
        };
        actions.sort_by_cached_key(|action| {
            let key = (
                game.boards[action.board].fen(game.turn()),
                game.boards[action.board].is_chess960(),
            );
            let cached = self.cache.get(&key);
            std::cmp::Reverse((
                Some(*action) == preferred,
                cached.is_some_and(|analysis| {
                    analysis.best_move.as_deref() == Some(&action.uci.to_string())
                }),
                self.candidate_cache
                    .get(&key)
                    .and_then(|candidates| {
                        candidates
                            .iter()
                            .position(|candidate| candidate.uci == action.uci)
                    })
                    .map_or(0, |rank| usize::MAX - rank),
                move_priority(game, *action),
            ))
        });
        let mut best = -f64::INFINITY;
        let mut chosen = None;
        for action in actions {
            let mut child = game.clone();
            child.play(action.board, &action.uci.to_string())?;
            let Some((child_value, _)) =
                Box::pin(self.negamax(&child, depth - 1, -beta, -alpha, None)).await?
            else {
                return Ok(None);
            };
            let value = -child_value;
            if value > best {
                best = value;
                chosen = Some(action);
            }
            alpha = alpha.max(best);
            if alpha >= beta {
                self.result.cutoffs += 1;
                break;
            }
        }
        Ok(Some((best, chosen)))
    }

    async fn conditional(
        &mut self,
        board: &Board,
        mut color: Color,
    ) -> Result<Option<OutcomeEstimate>, SearchError> {
        if board.legal_moves(color).is_empty() {
            color = color.other();
        }
        if board.legal_moves(color).is_empty() {
            return Ok(Some(OutcomeEstimate {
                white: 0.0,
                draw: 1.0,
                black: 0.0,
            }));
        }
        let key = (board.fen(color), board.is_chess960());
        if let Some(analysis) = self.cache.get(&key) {
            self.result.cache_hits += 1;
            return Ok(Some(analysis.evaluation.estimate()));
        }
        if Instant::now() >= self.deadline {
            return Ok(None);
        }
        let analysis = self.oracle.evaluate(&key.0, key.1).await?;
        self.result.evaluations += 1;
        let estimate = analysis.evaluation.estimate();
        self.cache.insert(key, analysis);
        if Instant::now() >= self.deadline {
            return Ok(None);
        }
        Ok(Some(estimate))
    }

    async fn evaluate_match(&mut self, game: &Game) -> Result<Option<f64>, SearchError> {
        let mut estimates = Vec::new();
        for (id, board) in game
            .boards
            .iter()
            .enumerate()
            .filter(|(id, _)| game.outcomes[*id] == Outcome::Ongoing)
        {
            let Some(current) = self.conditional(board, game.turn()).await? else {
                return Ok(None);
            };
            if game.forced_board == Some(id) {
                estimates.push(current);
            } else {
                let Some(other) = self.conditional(board, game.turn().other()).await? else {
                    return Ok(None);
                };
                estimates.push(OutcomeEstimate {
                    white: (current.white + other.white) * 0.5,
                    draw: (current.draw + other.draw) * 0.5,
                    black: (current.black + other.black) * 0.5,
                });
            }
        }
        Ok(Some(
            (match_estimate(game, &estimates)?.expected_score(game.turn()) * 2.0 - 1.0)
                .clamp(-HEURISTIC_LIMIT, HEURISTIC_LIMIT),
        ))
    }
}

fn is_king_capture(game: &Game, action: SearchMove) -> bool {
    game.boards[action.board]
        .piece_at(action.uci.to)
        .is_some_and(|piece| piece.color != game.turn() && piece.role == Role::King)
}

fn legal_actions(game: &Game) -> Vec<SearchMove> {
    let mut actions: Vec<_> = game
        .legal_boards()
        .into_iter()
        .flat_map(|board| {
            game.boards[board]
                .legal_moves(game.turn())
                .into_iter()
                .map(move |uci| SearchMove { board, uci })
        })
        .collect();
    actions.sort_by_cached_key(|action| std::cmp::Reverse(move_priority(game, *action)));
    actions
}

fn move_priority(game: &Game, action: SearchMove) -> i32 {
    let value = |role| match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 1000,
    };
    let capture = game.boards[action.board]
        .piece_at(action.uci.to)
        .filter(|piece| piece.color != game.turn())
        .map_or(0, |piece| value(piece.role));
    capture * 10 + action.uci.promotion.map_or(0, value)
}

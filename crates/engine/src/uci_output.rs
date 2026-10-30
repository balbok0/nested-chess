//! Portable parsing of ranked UCI candidate moves.
use crate::{
    Board, Color,
    search::{LocalCandidate, LocalEvaluation, SearchError},
};
use std::collections::BTreeMap;

/// Read the deepest complete MultiPV iteration, or its contiguous partial first iteration.
pub fn candidates(output: &str, fen: &str, count: u16) -> Result<Vec<LocalCandidate>, SearchError> {
    if !(1..=500).contains(&count) {
        return Err(SearchError::InvalidCandidateCount);
    }
    let board = Board::from_fen(fen)?;
    let white = fen.split_whitespace().nth(1) == Some("w");
    let legal = board.legal_moves(if white { Color::White } else { Color::Black });
    let expected = usize::from(count).min(legal.len());
    let mut batches: Vec<BTreeMap<usize, (u16, LocalCandidate)>> = Vec::new();
    for line in output.lines().filter(|line| line.starts_with("info ")) {
        let words: Vec<_> = line.split_whitespace().collect();
        let field = |name| {
            words
                .iter()
                .position(|word| *word == name)
                .and_then(|index| words.get(index + 1))
                .copied()
        };
        let (Some(depth), Some(score), Some(pv)) = (
            field("depth"),
            words.iter().position(|word| *word == "score"),
            field("pv"),
        ) else {
            continue;
        };
        let depth: u16 = depth
            .parse()
            .map_err(|_| SearchError::Evaluation("invalid UCI depth".into()))?;
        let rank: usize = field("multipv")
            .unwrap_or("1")
            .parse()
            .map_err(|_| SearchError::Evaluation("invalid MultiPV rank".into()))?;
        if rank == 0 || rank > usize::from(count) {
            return Err(SearchError::Evaluation("unexpected MultiPV rank".into()));
        }
        if rank == 1 {
            batches.push(BTreeMap::new());
        }
        if words.contains(&"lowerbound") || words.contains(&"upperbound") {
            continue;
        }
        let value: i32 = words
            .get(score + 2)
            .ok_or_else(|| SearchError::Evaluation("missing UCI score".into()))?
            .parse()
            .map_err(|_| SearchError::Evaluation("invalid UCI score".into()))?;
        let value = if white {
            value
        } else {
            value
                .checked_neg()
                .ok_or_else(|| SearchError::Evaluation("UCI score overflow".into()))?
        };
        let evaluation = match words.get(score + 1).copied() {
            Some("cp") => LocalEvaluation::Cp { value },
            Some("mate") => LocalEvaluation::Capture {
                winner: if value > 0 || (value == 0 && !white) {
                    Color::White
                } else {
                    Color::Black
                },
            },
            _ => return Err(SearchError::Evaluation("unknown UCI score kind".into())),
        };
        let uci = pv
            .parse()
            .map_err(|_| SearchError::Evaluation("invalid candidate UCI".into()))?;
        if !legal.contains(&uci) {
            return Err(SearchError::Evaluation(format!("illegal candidate {pv}")));
        }
        if let Some(batch) = batches.last_mut() {
            batch.insert(rank, (depth, LocalCandidate { uci, evaluation }));
        }
    }
    let complete = batches
        .iter()
        .filter(|batch| {
            batch.get(&1).is_some_and(|(depth, _)| {
                (1..=expected).all(|rank| batch.get(&rank).is_some_and(|(other, _)| other == depth))
            })
        })
        .max_by_key(|batch| batch[&1].0);
    let moves = complete
        .or_else(|| batches.iter().find(|batch| batch.contains_key(&1)))
        .ok_or_else(|| SearchError::Evaluation("no ranked UCI moves".into()))?;
    let depth = moves[&1].0;
    let mut selected = Vec::new();
    for rank in 1..=expected {
        let Some((candidate_depth, candidate)) = moves.get(&rank) else {
            break;
        };
        if *candidate_depth != depth {
            break;
        }
        if selected
            .iter()
            .any(|previous: &LocalCandidate| previous.uci == candidate.uci)
        {
            return Err(SearchError::Evaluation("duplicate candidate move".into()));
        }
        selected.push(candidate.clone());
    }
    if selected.is_empty() {
        return Err(SearchError::Evaluation(
            "no contiguous candidate ranks".into(),
        ));
    }
    Ok(selected)
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 nested-chess contributors

const BAR_SCALE_CP = 400;
const BAR_MIN_PERCENT = 2;

export function parseEvaluation(lines, turn) {
  if (!['white', 'black'].includes(turn)) throw new TypeError('Evaluation turn must be white or black.');
  for (const line of lines.toReversed()) {
    if (!line.startsWith('info ') || /\b(?:lowerbound|upperbound)\b/.test(line)) continue;
    const variation = line.match(/\bmultipv (\d+)/);
    if (variation && Number(variation[1]) !== 1) continue;
    const score = line.match(/\bscore (cp|mate) (-?\d+)\b/);
    if (!score) continue;
    const value = Number(score[2]) * (turn === 'white' ? 1 : -1);
    if (score[1] === 'cp') return { kind: 'cp', value: value === 0 ? 0 : value };
    return { kind: 'capture', winner: value > 0 ? 'white' : value < 0 ? 'black' : turn === 'white' ? 'black' : 'white' };
  }
  throw new Error('The engine returned no board evaluation.');
}

export function describeEvaluation(evaluation) {
  if (evaluation.kind === 'capture') {
    return {
      label: `${evaluation.winner === 'white' ? 'W' : 'B'} capture`,
      title: `The local search predicts ${evaluation.winner === 'white' ? 'White' : 'Black'} can force a king capture. Routing may change this.`,
      barLabel: evaluation.winner === 'white' ? 'W' : 'B',
      whitePercent: evaluation.winner === 'white' ? 100 : 0,
    };
  }
  const pawns = Math.abs(evaluation.value) < 5 ? 0 : evaluation.value / 100;
  const label = `${pawns > 0 ? '+' : ''}${pawns.toFixed(1)}`;
  return {
    label,
    title: `Local evaluation: ${label} pawn units from White’s perspective. Routing is not considered.`,
    barLabel: label,
    whitePercent: Math.max(BAR_MIN_PERCENT, Math.min(100 - BAR_MIN_PERCENT, Math.round(50 + 50 * Math.tanh(evaluation.value / BAR_SCALE_CP)))),
  };
}

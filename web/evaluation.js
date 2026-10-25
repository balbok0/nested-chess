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
    return { label: `${evaluation.winner === 'white' ? 'W' : 'B'} capture`, title: `The local search predicts ${evaluation.winner === 'white' ? 'White' : 'Black'} can force a king capture. Routing may change this.` };
  }
  const pawns = Math.abs(evaluation.value) < 5 ? 0 : evaluation.value / 100;
  const label = `${pawns > 0 ? '+' : ''}${pawns.toFixed(1)}`;
  return { label, title: `Local evaluation: ${label} pawn units from White’s perspective. Routing is not considered.` };
}

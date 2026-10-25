import { readFileSync } from 'node:fs';
import { initSync, NestedGame } from '../web/pkg/nested_chess_wasm.js';

initSync({ module: readFileSync('web/pkg/nested_chess_wasm_bg.wasm') });
let seed = 20261025;
const random = limit => {
  seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5;
  return (seed >>> 0) % limit;
};
const openings = [];
const seen = new Set();
while (openings.length < 50) {
  const game = new NestedGame(2);
  const opening = [];
  for (let ply = 0; ply < 2; ply++) {
    const boards = JSON.parse(game.state()).legal_boards;
    const board = boards[random(boards.length)];
    const moves = JSON.parse(game.legal_moves(board));
    const uci = moves[random(moves.length)];
    game.play(board, uci);
    opening.push({ board, uci });
  }
  game.free();
  const key = JSON.stringify(opening);
  if (!seen.has(key)) { seen.add(key); openings.push(opening); }
}
process.stdout.write(`${JSON.stringify({ seed: 20261025, plies: 2, openings }, null, 2)}\n`);

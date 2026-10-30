import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import test from 'node:test';
import { initSync, NestedGame } from '../web/pkg/nested_chess_wasm.js';
import { parseEvaluation } from '../web/evaluation.js';

initSync({ module: readFileSync(new URL('../web/pkg/nested_chess_wasm_bg.wasm', import.meta.url)) });

async function fairyOracle() {
  const child = spawn('node', ['scripts/fairy-uci.cjs'], { stdio: ['pipe', 'pipe', 'inherit'] });
  const output = createInterface({ input: child.stdout });
  let pending;
  output.on('line', line => {
    if (!pending) return;
    pending.lines.push(line);
    if (line.startsWith(pending.prefix)) {
      const { resolve, lines, timer } = pending;
      clearTimeout(timer); pending = null; resolve(lines);
    }
  });
  const command = (prefix, commands) => new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending = null; reject(new Error('Real Fairy-Stockfish timed out')); }, 10000);
    pending = { prefix, resolve, reject, timer, lines: [] };
    child.stdin.write(`${commands.join('\n')}\n`);
  });
  child.on('exit', () => {
    if (pending) { clearTimeout(pending.timer); pending.reject(new Error('Fairy-Stockfish exited')); pending = null; }
  });
  try {
    await command('uciok', ['uci']);
    await command('uciok', ['setoption name VariantPath value variants/nested.ini', 'uci']);
    await command('readyok', ['setoption name UCI_Variant value nested', 'setoption name Use NNUE value false', 'isready']);
  } catch (error) { child.kill(); output.close(); throw error; }
  return {
    async evaluate(fen, chess960) {
      await command('readyok', ['ucinewgame', `setoption name UCI_Chess960 value ${chess960}`, 'setoption name MultiPV value 1', 'isready']);
      const lines = await command('bestmove ', [`position fen ${fen}`, 'go nodes 128']);
      return JSON.stringify({ evaluation: parseEvaluation(lines, fen.split(' ')[1] === 'w' ? 'white' : 'black'), best_move: lines.at(-1).split(' ')[1] });
    },
    async candidates(fen, chess960, count) {
      await command('readyok', ['ucinewgame', `setoption name UCI_Chess960 value ${chess960}`, 'setoption name Skill Level value 20', `setoption name MultiPV value ${count}`, 'isready']);
      return (await command('bestmove ', [`position fen ${fen}`, `go nodes ${128 * count}`])).join('\n');
    },
    close() { child.kill(); output.close(); },
  };
}

const limits = JSON.stringify({ max_depth: 2, max_nodes: 100000, milliseconds: 10000 });

test('WASM negamax avoids a local win that permits a third board loss', async () => {
  const game = NestedGame.load(JSON.stringify({
    version: 3, size: 2, first_player: 'black', finish_rule: 'majority', en_passant_rule: 'local',
    initial_fens: ['7k/8/8/8/8/8/7R/Kr6 w - - 0 1', '4k3/8/8/8/8/8/7r/7K w - - 0 1', '3Kk3/8/8/8/8/8/3r4/8 b - - 0 1', '3Kk3/8/8/8/8/8/3r4/8 b - - 0 1'],
    events: [[2,'d2d8'],[0,'h2h7'],[3,'d2d8']].map(([board,uci])=>({kind:'move',board,uci})),
  }));
  const oracle = await fairyOracle();
  try {
    const before = game.state();
    const actual = JSON.parse(await game.best_move(limits, oracle.evaluate));
    assert.equal(actual.completed_depth, 2);
    assert.notDeepEqual(actual.best_move, { board: 0, uci: 'h7h8' });
    assert.ok(actual.expected_score > 0);
    assert.ok(actual.cutoffs > 0);
    assert.ok(actual.cache_hits > 0);
    assert.equal(game.state(), before);
    assert.ok(JSON.parse(game.legal_moves(actual.best_move.board)).includes(actual.best_move.uci));
  } finally { oracle.close(); game.free(); }
});

test('WASM search keeps an independent snapshot while the caller frees the game', async () => {
  const game = NestedGame.chess960(2, 123);
  game.play(0, 'a2a3');
  const before = JSON.parse(game.state());
  const oracle = await fairyOracle();
  try {
    const searching = game.best_move(JSON.stringify({max_depth:1,max_nodes:100000,milliseconds:10000}), oracle.evaluate);
    game.free();
    const actual = JSON.parse(await searching);
    assert.equal(actual.completed_depth, 1);
    assert.ok(before.legal_boards.includes(actual.best_move.board));
    assert.ok(actual.evaluations > 0);
  } finally { oracle.close(); }
});

for (const rows of [2, 4]) {
  test(`WASM selective search uses real ten-move MultiPV on 4x${rows}`, async () => {
    const game = NestedGame.rectangular(4, rows);
    const before = game.state();
    const oracle = await fairyOracle();
    try {
      const actual = JSON.parse(await game.best_move_selective(JSON.stringify({max_depth:1,max_nodes:100000,milliseconds:10000}), 10, oracle.evaluate, oracle.candidates));
      assert.equal(actual.completed_depth, 1);
      assert.ok(actual.candidate_evaluations > 0);
      assert.ok(actual.evaluations >= actual.candidate_evaluations);
      assert.ok(JSON.parse(game.legal_moves(actual.best_move.board)).includes(actual.best_move.uci));
      assert.equal(game.state(), before);
    } finally { oracle.close(); game.free(); }
  });
}

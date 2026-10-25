import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { initSync, NestedGame, destination_board } from '../web/pkg/nested_chess_wasm.js';

initSync({ module: readFileSync(new URL('../web/pkg/nested_chess_wasm_bg.wasm', import.meta.url)) });

for (const size of [2, 4, 8]) {
  test(`${size}x${size} exposes the complete opening state`, () => {
    const game = new NestedGame(size);
    const state = JSON.parse(game.state());
    assert.equal(state.boards.length, size * size);
    assert.equal(state.legal_boards.length, size * size);
    assert.equal(game.piece_planes().length, size * size * 12 * 64);
    assert.equal(destination_board(size, 63), size * size - 1);
    game.free();
  });
}

test('WASM routes Black to a fresh board and preserves uneven local turns', () => {
  const game = new NestedGame(2);
  for (const [board, move] of [[0, 'e2e4'], [1, 'e7e5'], [3, 'e2e4'], [1, 'd7d5']]) game.play(board, move);
  const state = JSON.parse(game.state());
  assert.equal(state.turn, 'white');
  assert.equal(state.forced_board, 2);
  assert.equal(state.boards[1].white_moves, 0);
  assert.equal(state.boards[1].black_moves, 2);
  const restored = NestedGame.load(game.save());
  assert.deepEqual(JSON.parse(restored.state()), state);
  assert.equal(JSON.parse(game.undo()).ply, 3);
  game.free(); restored.free();
});

test('invalid WASM actions raise an error without changing state', () => {
  const game = new NestedGame(2);
  const expected = game.state();
  assert.throws(() => game.play(0, 'e2e8'));
  assert.equal(game.state(), expected);
  assert.throws(() => new NestedGame(3));
  game.free();
});

test('agreed board draws award half each and survive save, load, and undo', () => {
  const game = new NestedGame(2);
  game.play(0, 'e2e4');
  const before = game.state();
  const actual = JSON.parse(game.agree_draw(1));
  assert.equal(actual.score.white, 0.5);
  assert.equal(actual.score.black, 0.5);
  assert.equal(actual.turn, 'black');
  assert.equal(actual.ply, 1);
  assert.equal(actual.forced_board, null);
  const restored = NestedGame.load(game.save());
  assert.deepEqual(JSON.parse(restored.state()), actual);
  assert.equal(restored.undo(), before);
  game.free(); restored.free();
});

test('Chess960 exposes its shared start, legal moves, and replay metadata', () => {
  const game = NestedGame.chess960(2, 0);
  const state = JSON.parse(game.state());
  assert.equal(state.chess960, true);
  assert.equal(state.start_position, 0);
  assert.ok(state.boards.every(board => board.fen === state.boards[0].fen));
  assert.equal(state.boards[0].fen.split('/').at(-1).split(' ')[0], 'BBQNNRKR');
  const before = game.state();
  game.play(0, 'a2a3');
  const restored = NestedGame.load(game.save());
  assert.equal(restored.state(), game.state());
  assert.equal(restored.undo(), before);
  assert.throws(() => NestedGame.chess960(2, 960));
  game.free(); restored.free();
});

test('Chess960 castling exposes the actual king and rook destinations', () => {
  const game = NestedGame.load(JSON.stringify({ version: 2, size: 2, first_player: 'white', initial_fens: Array(4).fill('4k3/8/8/8/8/8/8/6KR w H - 0 1'), finish_rule: 'majority', events: [] }));
  assert.deepEqual(JSON.parse(game.move_details(0, 'g1h1')), { destination: 6, rook: { from: 7, to: 5 } });
  const state = JSON.parse(game.play(0, 'g1h1'));
  assert.equal(state.boards[0].squares[6], 'K');
  assert.equal(state.boards[0].squares[5], 'R');
  game.free();
});

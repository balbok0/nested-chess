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

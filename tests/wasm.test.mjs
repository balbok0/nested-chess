import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { initSync, NestedGame, destination_board, destination_board_rectangular } from '../web/pkg/nested_chess_wasm.js';

initSync({ module: readFileSync(new URL('../web/pkg/nested_chess_wasm_bg.wasm', import.meta.url)) });

test('4x2 exposes eight boards and routes with four columns and two rows', () => {
  const game = NestedGame.rectangular(4, 2);
  let state = JSON.parse(game.state());
  assert.equal(state.columns, 4);
  assert.equal(state.rows, 2);
  assert.equal(state.boards.length, 8);
  assert.equal(game.piece_planes().length, 8 * 12 * 64);
  assert.equal(destination_board_rectangular(4, 2, 28), 2);
  game.play(0, 'e2e4');
  state = JSON.parse(game.play(2, 'e7e5'));
  assert.equal(state.forced_board, 6);
  assert.equal(state.boards[2].white_moves, 0);
  assert.equal(state.boards[2].black_moves, 1);
  const restored = NestedGame.load(game.save());
  assert.deepEqual(JSON.parse(restored.state()), state);
  assert.equal(JSON.parse(restored.undo()).forced_board, 2);
  restored.free(); game.free();
});

test('rectangular Chess960 exposes eight shared or independent starts', () => {
  const shared = NestedGame.chess960_rectangular(4, 2, 123);
  const independent = NestedGame.chess960_boards_rectangular(4, 2, Uint16Array.from([0,1,2,3,4,5,6,7]));
  assert.equal(new Set(JSON.parse(shared.state()).boards.map(board => board.fen)).size, 1);
  assert.equal(new Set(JSON.parse(independent.state()).boards.map(board => board.fen)).size, 8);
  shared.free(); independent.free();
});

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

test('independent Chess960 starts retain their board order and save history', () => {
  const game = NestedGame.chess960_boards(2, new Uint16Array([0, 518, 959, 123]));
  const state = JSON.parse(game.state());
  assert.equal(state.start_position, null);
  assert.equal(state.chess960, true);
  assert.equal(new Set(state.boards.map(board => board.fen)).size, 4);
  assert.ok(state.boards[1].fen.startsWith('rnbqkbnr/'));
  game.play(0, 'a2a3');
  const restored = NestedGame.load(game.save());
  assert.equal(restored.state(), game.state());
  assert.deepEqual(JSON.parse(restored.undo()), state);
  assert.throws(() => NestedGame.chess960_boards(2, new Uint16Array([0])));
  game.free(); restored.free();
});

test('local en passant survives routing away and returns after undo', () => {
  const fens = Array(4).fill('3rk3/8/8/8/8/8/8/4K3 w - - 0 1');
  fens[0] = '4k3/3p4/8/4P3/8/8/8/4K3 b - - 0 1';
  const game = NestedGame.load(JSON.stringify({version:3,size:2,first_player:'black',initial_fens:fens,finish_rule:'majority',en_passant_rule:'local',events:[]}));
  for (const [board, move] of [[0,'d7d5'],[2,'e1f1'],[1,'d8d4']]) game.play(board,move);
  assert.ok(JSON.parse(game.legal_moves(0)).includes('e5d6'));
  const before = game.state();
  const state = JSON.parse(game.play(0,'e5d6'));
  assert.equal(state.boards[0].squares[35], '.');
  assert.equal(state.boards[0].squares[43], 'P');
  const restored = NestedGame.load(game.save());
  assert.equal(restored.state(),game.state());
  assert.equal(restored.undo(),before);
  game.free(); restored.free();
});

test('new games save the local en passant rule explicitly', () => {
  const game = new NestedGame(2);
  assert.equal(JSON.parse(game.state()).en_passant_rule, 'local');
  const save = JSON.parse(game.save());
  assert.equal(save.version, 4);
  assert.equal(save.en_passant_rule, 'local');
  game.free();
});

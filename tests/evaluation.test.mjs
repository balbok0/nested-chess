import assert from 'node:assert/strict';
import test from 'node:test';
import { describeEvaluation, parseEvaluation } from '../web/evaluation.js';

for (const { name, turn, lines, expected } of [
  { name: 'white advantage with white to move', turn: 'white', lines: ['info depth 8 multipv 1 score cp 85 nodes 100'], expected: { kind: 'cp', value: 85 } },
  { name: 'white advantage with black to move', turn: 'black', lines: ['info depth 8 multipv 1 score cp -85 nodes 100'], expected: { kind: 'cp', value: 85 } },
  { name: 'black advantage with white to move', turn: 'white', lines: ['info depth 8 multipv 1 score cp -130 nodes 100'], expected: { kind: 'cp', value: -130 } },
  { name: 'black advantage with black to move', turn: 'black', lines: ['info depth 8 multipv 1 score cp 130 nodes 100'], expected: { kind: 'cp', value: -130 } },
  { name: 'black-to-move zero stays unsigned', turn: 'black', lines: ['info depth 8 score cp 0'], expected: { kind: 'cp', value: 0 } },
  { name: 'latest completed main variation wins', turn: 'white', lines: ['info depth 6 multipv 1 score cp 30', 'info depth 7 multipv 1 score cp 40', 'info depth 7 multipv 2 score cp 10', 'info depth 8 multipv 1 score cp 75 lowerbound', 'bestmove e2e4'], expected: { kind: 'cp', value: 40 } },
  { name: 'white forced king capture', turn: 'white', lines: ['info depth 8 score mate 2'], expected: { kind: 'capture', winner: 'white' } },
  { name: 'black forced king capture', turn: 'black', lines: ['info depth 8 score mate 2'], expected: { kind: 'capture', winner: 'black' } },
  { name: 'black loses its king in local search', turn: 'black', lines: ['info depth 8 score mate -2'], expected: { kind: 'capture', winner: 'white' } },
  { name: 'white loses its king in local search', turn: 'white', lines: ['info depth 8 score mate -2'], expected: { kind: 'capture', winner: 'black' } },
]) {
  test(name, () => assert.deepEqual(parseEvaluation(lines, turn), expected));
}

for (const { value, expected } of [
  { value: 85, expected: '+0.8' },
  { value: -130, expected: '-1.3' },
  { value: -4, expected: '0.0' },
]) {
  test(`centipawn value ${value} displays in pawn units`, () => assert.equal(describeEvaluation({ kind: 'cp', value }).label, expected));
}

test('a missing evaluation raises an error', () => assert.throws(() => parseEvaluation(['bestmove e2e4'], 'white'), /no board evaluation/));
test('an invalid perspective raises an error', () => assert.throws(() => parseEvaluation(['info score cp 85'], 'green'), TypeError));

for (const [winner, expected] of [['white', 'W capture'], ['black', 'B capture']]) {
  test(`${winner} forced capture is labeled as a capture`, () => assert.equal(describeEvaluation({ kind: 'capture', winner }).label, expected));
}

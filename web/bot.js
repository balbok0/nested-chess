// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 nested-chess contributors

import config from '../variants/nested.ini?raw';
import { parseEvaluation } from './evaluation.js';

const EVALUATION_TIME_MS = 100;
const SEARCH_NODE_BUDGET = 100000;

export const LEVELS = [
  { label: 'Beginner', depth: 1, ms: 300, nodes: 32 },
  { label: 'Casual', depth: 2, ms: 600, nodes: 64 },
  { label: 'Club', depth: 3, ms: 1000, nodes: 128 },
  { label: 'Strong', depth: 4, ms: 1600, nodes: 128 },
  { label: 'Expert', depth: 5, ms: 2400, nodes: 256 },
  { label: 'Maximum', depth: 6, ms: 4000, nodes: 256 },
];

export class Opponent {
  constructor() {
    this.worker = new Worker(`${import.meta.env.BASE_URL}bot-worker.js`);
    this.worker.onmessage = ({ data }) => {
      if (data.error) return this.fail(new Error(data.error));
      if (!this.pending) return;
      this.pending.lines.push(data.line);
      if (data.line.startsWith(this.pending.prefix)) {
        const { resolve, lines, timer } = this.pending;
        clearTimeout(timer); this.pending = null; resolve(lines);
      }
    };
    this.worker.onerror = event => this.fail(new Error(event.message || 'Opponent worker failed to load.'));
    this.ready = this.initialize();
  }
  async initialize() {
    if (!globalThis.crossOriginIsolated) throw new Error('The engine could not start in this browser. Reload the page or switch to local two-player mode.');
    const loaded = this.wait('loaded', 20000);
    this.worker.postMessage({ type: 'init', config });
    await loaded;
    const options = await this.command('uciok', ['setoption name VariantPath value /nested.ini', 'uci']);
    if (!options.some(line => line.startsWith('option name UCI_Variant ') && line.split(' ').includes('nested'))) throw new Error('Opponent does not support king-capture rules.');
    await this.command('readyok', ['setoption name UCI_Variant value nested', 'setoption name Use NNUE value false', 'isready']);
  }
  wait(prefix, milliseconds = 12000) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => this.fail(new Error('Opponent timed out. Retry the opponent or switch to two players.')), milliseconds);
      this.pending = { prefix, resolve, reject, timer, lines: [] };
    });
  }
  command(prefix, commands, milliseconds) {
    const response = this.wait(prefix, milliseconds);
    this.worker.postMessage({ commands });
    return response;
  }
  async search(fen, level, chess960 = /[A-Ha-h]/.test(fen.split(' ')[2])) {
    await this.ready;
    await this.command('readyok', ['ucinewgame', `setoption name UCI_Chess960 value ${chess960}`, 'setoption name UCI_LimitStrength value false', 'setoption name Skill Level value 20', 'setoption name MultiPV value 1', 'isready']);
    return this.command('bestmove ', [`position fen ${fen}`, level.nodes ? `go nodes ${level.nodes}` : `go movetime ${level.ms}`]);
  }
  async bestMove(game, level) {
    await this.ready;
    return JSON.parse(await game.best_move(JSON.stringify({ max_depth: level.depth, max_nodes: SEARCH_NODE_BUDGET, milliseconds: level.ms }), async (fen, chess960) => {
      const lines = await this.search(fen, { nodes: level.nodes }, chess960);
      return JSON.stringify({ evaluation: parseEvaluation(lines, fen.split(' ')[1] === 'w' ? 'white' : 'black'), best_move: lines.at(-1).split(' ')[1] });
    }));
  }
  async evaluate(fen) {
    return parseEvaluation(await this.search(fen, { ms: EVALUATION_TIME_MS }), fen.split(' ')[1] === 'w' ? 'white' : 'black');
  }
  fail(error) {
    if (this.pending) {
      clearTimeout(this.pending.timer); this.pending.reject(error); this.pending = null;
    }
  }
  destroy() { this.fail(new Error('Search canceled.')); this.worker.terminate(); }
}

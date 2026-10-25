import config from '../variants/nested.ini?raw';
import { parseEvaluation } from './evaluation.js';

const EVALUATION_TIME_MS = 100;

export const LEVELS = [
  { label: 'Beginner', skill: 0, ms: 60 },
  { label: 'Casual', skill: 2, ms: 100 },
  { label: 'Club', skill: 5, ms: 180 },
  { label: 'Strong', skill: 10, ms: 300 },
  { label: 'Expert', skill: 15, ms: 500 },
  { label: 'Maximum', skill: 20, ms: 900 },
];

export class Opponent {
  constructor() {
    this.worker = new Worker('/bot-worker.js');
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
    if (!globalThis.crossOriginIsolated) throw new Error('The opponent needs cross-origin isolation. Start the app with npm run dev or npm run preview.');
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
  async search(fen, level) {
    await this.ready;
    await this.command('readyok', ['ucinewgame', 'setoption name UCI_LimitStrength value false', `setoption name Skill Level value ${level.skill}`, 'setoption name MultiPV value 1', 'isready']);
    return this.command('bestmove ', [`position fen ${fen}`, `go movetime ${level.ms}`]);
  }
  async bestMove(fen, level) {
    const lines = await this.search(fen, level);
    return lines.at(-1).split(' ')[1];
  }
  async evaluate(fen) {
    return parseEvaluation(await this.search(fen, { skill: 20, ms: EVALUATION_TIME_MS }), fen.split(' ')[1] === 'w' ? 'white' : 'black');
  }
  fail(error) {
    if (this.pending) {
      clearTimeout(this.pending.timer); this.pending.reject(error); this.pending = null;
    }
  }
  destroy() { this.fail(new Error('Search canceled.')); this.worker.terminate(); }
}

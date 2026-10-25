import { copyFileSync, mkdirSync } from 'node:fs';
mkdirSync('web/vendor/fairy', { recursive: true });
copyFileSync('scripts/bot-worker.js', 'web/vendor/bot-worker.js');
for (const file of ['stockfish.js', 'stockfish.wasm', 'stockfish.worker.js', 'Copying.txt', 'AUTHORS']) {
  copyFileSync(`node_modules/fairy-stockfish-nnue.wasm/${file}`, `web/vendor/fairy/${file}`);
}

import { copyFileSync, mkdirSync } from 'node:fs';
mkdirSync('web/vendor/fairy', { recursive: true });
copyFileSync('scripts/bot-worker.js', 'web/vendor/bot-worker.js');
copyFileSync('node_modules/coi-serviceworker/coi-serviceworker.js', 'web/vendor/coi-serviceworker.js');
copyFileSync('node_modules/coi-serviceworker/LICENSE', 'web/vendor/coi-serviceworker.LICENSE');
for (const file of ['LICENSE', 'LICENSE-MIT', 'LICENSING.md', 'THIRD_PARTY.md']) {
  copyFileSync(file, `web/vendor/${file}`);
}
for (const file of ['stockfish.js', 'stockfish.wasm', 'stockfish.worker.js', 'Copying.txt', 'AUTHORS']) {
  copyFileSync(`node_modules/fairy-stockfish-nnue.wasm/${file}`, `web/vendor/fairy/${file}`);
}

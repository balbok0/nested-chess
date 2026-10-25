import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
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
const mobileLicenses = [
  '@capacitor/core/LICENSE', '@capacitor/android/LICENSE', '@capacitor/ios/LICENSE',
  '@capacitor/app/LICENSE', '@capacitor/browser/LICENSE', '@capacitor/clipboard/LICENSE',
  '@capacitor/filesystem/LICENSE', '@capacitor/preferences/LICENSE', '@capacitor/share/LICENSE',
  '@capacitor/synapse/LICENSE.md', '@aparajita/capacitor-secure-storage/LICENSE', 'tslib/LICENSE.txt',
];
writeFileSync('web/vendor/MOBILE_NOTICES.txt', mobileLicenses.map(file => `${file}\n\n${readFileSync(`node_modules/${file}`, 'utf8')}`).join('\n\n')
  + `\n\nKeychainSwift\n\n${readFileSync('third_party/mobile/KeychainSwift.LICENSE', 'utf8')}`);

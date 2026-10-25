import { spawnSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';

for (const [command, args] of [
  ['cargo', ['build', '-p', 'nested-chess-wasm', '--target', 'wasm32-unknown-unknown', '--release', '--locked']],
  ['wasm-bindgen', ['target/wasm32-unknown-unknown/release/nested_chess_wasm.wasm', '--target', 'web', '--out-dir', 'web/pkg']],
]) {
  mkdirSync('web/pkg', { recursive: true });
  const result = spawnSync(command, args, { stdio: 'inherit' });
  if (result.error) { console.error(`${command}: ${result.error.message}. See README.md for setup.`); process.exit(1); }
  if (result.status !== 0) process.exit(result.status ?? 1);
}

const fs = require('node:fs');
const readline = require('node:readline');
const Stockfish = require('fairy-stockfish-nnue.wasm/stockfish.js');

async function main() {
  const engine = await Stockfish({ wasmBinary: fs.readFileSync(require.resolve('fairy-stockfish-nnue.wasm/stockfish.wasm')) });
  engine.FS.writeFile('/nested.ini', fs.readFileSync(require('node:path').join(__dirname, '../variants/nested.ini'), 'utf8'));
  engine.addMessageListener(line => process.stdout.write(`${line}\n`));
  const input = readline.createInterface({ input: process.stdin });
  for await (const command of input) {
    engine.postMessage(command.replace(/setoption name VariantPath value .*/, 'setoption name VariantPath value /nested.ini'));
    if (command === 'quit') process.exit(0);
  }
  process.exit(0);
}
main().catch(error => { console.error(error); process.exit(1); });

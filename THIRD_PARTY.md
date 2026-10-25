# Third-party components

The Rust engine and application code in this repository are MIT licensed.
Dependencies retain their own licenses.

The browser and optional Node opponent use unmodified
`fairy-stockfish-nnue.wasm` **1.1.12**, GPL-3.0, by Fabian Fichter and the
Fairy-Stockfish/Stockfish contributors. npm records its source commit as
`b2e693ef1e111233ce3fb40685921708b3276ed6`.

- [Source, including build configuration](https://github.com/fairy-stockfish/fairy-stockfish.wasm/tree/b2e693ef1e111233ce3fb40685921708b3276ed6)
- [npm distribution](https://www.npmjs.com/package/fairy-stockfish-nnue.wasm/v/1.1.12)
- License: `node_modules/fairy-stockfish-nnue.wasm/Copying.txt`
- Authors: `node_modules/fairy-stockfish-nnue.wasm/AUTHORS`

The build copies the engine, license, and authors into `dist/fairy/`.
Redistributing that engine requires preserving its GPL license and providing
the corresponding source under its terms; the MIT license does not relicense
the engine. No Fairy-Stockfish neural weights are bundled.

The interface uses system fonts.

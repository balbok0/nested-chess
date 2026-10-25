# Licensing

Nested chess has separately licensed components. The browser application and
its Fairy-Stockfish integration are distributed under **GPL-3.0-only**. The
independent Rust rules engine and its bindings remain **MIT**.

| Component | License |
| --- | --- |
| `web/`, `scripts/bot-worker.js`, `scripts/fairy-uci.cjs`, `variants/`, JavaScript tests | GPL-3.0-only; see `LICENSE` |
| `crates/`, Cargo workspace files, Python package metadata and Python tests | MIT; see `LICENSE-MIT` and each crate's `LICENSE` |
| Other original build/deployment scripts and documentation | MIT; see `LICENSE-MIT` |
| Fairy-Stockfish and other third-party components | Their upstream licenses; see `THIRD_PARTY.md` |

The GPL application distribution includes compatible MIT components without
removing their MIT notices or changing the independent Rust engine's license.

Copyright (c) 2026 nested-chess contributors. The GPL application is provided
without warranty; see `LICENSE` for the complete terms.

## Corresponding source

The deployed site provides these downloads beside its binary assets:

- `source/nested-chess.tar.gz`: this application's source, lockfiles, configuration,
  notices, and build scripts, including the Rust engine and bindings.
- `source/fairy-stockfish.tar.gz`: the unmodified source archive for the
  Fairy-Stockfish version used in the site, including its build configuration.
- `source/fairy-stockfish.README.md`: version, provenance, and build instructions.

The Fairy-Stockfish archive is also committed at
`third_party/fairy-stockfish/source.tar.gz`. Its provenance and checksum are
recorded in that directory. Preserve the source downloads and notices when
redistributing the application.

## Rebuild

Extract `source/nested-chess.tar.gz` into an empty directory. Install Rust, GNU tar,
Node.js 22+, and npm, then follow the setup commands in `README.md`. `npm ci` uses
the pinned dependency lockfile; `npm run build` builds the Rust/WASM rules engine
and browser application. No Git checkout is required to build the source archive.

To rebuild Fairy-Stockfish itself rather than use the pinned npm binary, follow
`third_party/fairy-stockfish/README.md` and the upstream files inside its archive.

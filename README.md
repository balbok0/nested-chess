# Nested chess

A Rust king-capture chess engine on a routed 2×2, 4×4, or 8×8 grid of games,
with a playable browser UI, WebAssembly bindings, Python bindings, and a
single-board Fairy-Stockfish opponent.

[Play on GitHub Pages](https://balbok0.github.io/nested-chess/).

## Play locally

Requires Rust, Node.js 22+, npm, and GNU tar for source packaging.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.105 --locked
npm ci
npm run dev
```

Open the printed local URL (normally <http://localhost:5173>). You play White
against Fairy-Stockfish, or choose two players on the same device. The app supports
all three grids, legal-move highlights, route previews, promotion choice, board
inspection, board flipping, undo, automatic local saves, and JSON import/export.

The default **System** theme uses Classic or Classic Dark according to the browser's
light/dark preference, including changes while the page is open. **Settings** also
lets you choose a fixed Classic, Classic Dark, Light, or Dark theme; an explicit
choice overrides the system preference and is preserved across reloads. The optional
**Show board evaluations** setting displays Fairy-Stockfish scores on each board
and the selected board. Positive scores favor White; negative scores favor Black,
in pawn units. The selected board has a vertical bar with White at the bottom and
Black at the top, independent of board orientation. The bar uses a nonlinear
visual scale, not a win probability; pending or unavailable scores are not shown
as an equal position. Forced king-capture predictions have separate labels. Evaluations
ignore routing and assume alternating local turns. They use full engine skill,
independently of the opponent difficulty, and run one position at a time. Identical
positions share a result, and stale searches are canceled when the position changes.
The setting is off by default and is saved on this device with the selected theme.
**Animate moves** controls engine piece movement and evaluation-bar transitions.
System reduced-motion preferences take priority. Engine turns have a short pause
so the move and its routing are easier to follow.

## Publish to GitHub Pages

After committing source changes on `main`, run `npm run deploy:pages`. This builds
the project at `/nested-chess/`, runs the JavaScript tests, and pushes the output to
the `gh-pages` branch. GitHub Pages publishes that branch. The site's service worker
provides the isolation needed by Fairy-Stockfish on static hosting; the first visit
may reload once. Local development still uses the normal server headers.

```sh
npm run build
npm run preview
```

The production output is `dist/`. The Fairy-Stockfish browser build uses shared
WebAssembly memory. Serve the app with these HTTP headers; Vite dev and preview
already set them:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

Two-player play works without these headers; the opponent requires them. No
backend, accounts, or external inference service is needed.

## Rules

- White and Black alternate **globally**. Individual boards have no turn counter
  deciding who may move. Black can open an untouched board, and either color may
  move repeatedly on a local board across separate global turns.
- The first move can use any board. The landing square routes the opponent to a
  board in the macro grid: quadrants for 2×2, 2×2 square regions for 4×4, individual
  squares for 8×8.
- All routing coordinates use White's orientation, independent of UI flipping.
  Both local squares and macro boards run left to right, bottom to top. Board 0 is
  A1. For 2×2, `e4` routes to B1 (index 1), `d5` to A2 (index 2).
- **There is no check, king-safety restriction, or checkmate.** Capture the opposing
  king to win that board. Kings may be adjacent and may move into attacks.
- Standard piece movement, blocking, captures, pawn double steps, and all four
  promotions apply. Castling requires retained rights, the original king and
  rook, and an empty path; attacked squares do not prevent it. The king's landing
  square determines the route when castling.
- En passant is available only on the immediately following **global** turn,
  on the board of the double step. Moving elsewhere expires it.
- If the destination is finished or has no moves for the incoming player, that
  player chooses any unfinished board where they can move. A board with no moves
  for one color stays open for the other color.
- A board automatically draws after 100 local moves without a pawn move or
  capture. Threefold repetition of the **entire nested position** draws all
  unfinished boards. A player having no move anywhere also draws unfinished
  boards. These are explicit v0 draw rules, not FIDE adjudication.
- Play ends when every board is settled. The player with more board wins wins
  the match; equal wins draw it. Wins award 1 point and draws award 0.5 points to
  each player. A match also ends immediately when one player's points exceed half
  the total boards. There is no clock.
- In local two-player mode, **Offer draw** asks the other player to accept or
  decline a draw on the viewed board. Acceptance closes it, awards 0.5 points each,
  and does not consume a turn. A closed required board releases board choice.

![Routing and uneven local turns, demonstrated in 14.6 seconds](web/rules.gif)

## Rust API

```rust
use nested_chess::{Color, Game};

let mut game = Game::new(2)?;
game.play(0, "e2e4")?;
assert_eq!(game.turn(), Color::Black);
assert_eq!(game.legal_boards(), vec![1]);
game.play(1, "e7e5")?; // Black opens an untouched board.
let restored = Game::load_json(&game.save_json()?)?;
# Ok::<(), nested_chess::Error>(())
```

`crates/engine` owns move generation, routing, scoring, draws, replay saves,
and feature planes. `Board` uses twelve bitboards and has **no local side to move**.
`Game` owns the single global color. `play` validates before mutation; a rejected
move leaves the state unchanged. Native `play`/`undo` return `Result<(), Error>`;
request `state()` separately when needed. Save files store initial positions plus verified
actions, including all repetition history. Undo replays that log.

`Game::from_fens` supports analysis positions. Provide one FEN per macro board;
each initial board must contain exactly one king of each color. FEN side fields
do not control local turns. At most one board may have a valid en passant target,
which must be capturable by the initial global player. Finished boards are restored
by replay, not by importing kingless starting positions.

The `state()` view includes board results, legal board choices, move counts, and
the global turn. Local FEN views use that global turn and a synthetic fullmove
number of 1; they are position adapters, not standalone local game histories.

## Python

```sh
uv venv
uv run --with maturin maturin develop
uv run python
```

```python
import json
import nested_chess

game = nested_chess.Game(2)
game.play(0, "e2e4")
print(json.loads(game.state())["forced_board"])  # 1

# Uses the bundled npm WASM engine through a local Node UCI process.
opponent = nested_chess.Opponent(
    "node", ["scripts/fairy-uci.cjs"], "variants/nested.ini"
)
move = opponent.best_move(game, 1, skill=5, milliseconds=200)
game.play(1, move)
```

`state()`, `play()`, and `undo()` return JSON state strings in both bindings.
`legal_moves()` returns UCI strings (a Python list; a JSON array string in WASM).
`save()` returns a versioned replay string; `Game.load()` / `NestedGame.load()`
reconstruct a match. `piece_planes()` returns binary plane values.
`agree_draw(board)` records an agreement after both players have consented.
Version-2 saves record moves and draw agreements in one event history. Version-1
saves still load and retain their original finish-at-all-boards rule, including
games whose history continued past a majority; new matches use early completion.

For a native Fairy-Stockfish binary, use
`Opponent("/path/to/fairy-stockfish", [], "/absolute/path/to/variants/nested.ini")`.
Rust exposes the same persistent, timeout-bounded process adapter as
`nested_chess::uci::UciEngine`. Ordinary Stockfish is incompatible with these rules.

## Opponent strength

The custom [Fairy-Stockfish variant](variants/nested.ini) inherits extinction
chess, retaining only king extinction as the loss condition and normal
promotions. Kings are nonroyal, capturable pieces. The bot receives one local
FEN with the global mover as its active color. It does not receive the macro
position; on a free choice, it selects a board deterministically without comparing
board evaluations. Each analysis clears the previous local game context.

The browser offers Beginner, Casual, Club, Strong, Expert, and Maximum presets
using Fairy-Stockfish skill levels 0, 2, 5, 10, 15, and 20, with increasing search
time. These are **uncalibrated presets**, not 400/800/1200/1600/2000/2500 Elo bots.
Even the weakest preset may be difficult for a beginner.

A further limitation is deliberate for v0: its search assumes locally alternating
colors. It understands king capture but cannot anticipate routing or a color
getting multiple consecutive local moves. Standard-chess ratings therefore cannot
be promised even for strong engines. Beginner-level error profiles and meaningful
nested-chess ratings require playtesting and calibration.

See [the design notes](docs/design.md) for representation costs, training
feasibility, engine research, and related games.

## Verification

```sh
cargo test --workspace
npm run build
npm test
uv run --with maturin maturin develop
uv run --with pytest pytest -q tests/test_python.py
cargo run --example selfplay
cargo run --example state_size
```

The Python suite runs real Fairy-Stockfish processes, including king captures and
Black opening an untouched board. It requires `npm ci` first. Browser QA covered
a real opponent reply, routing, grid sizes, saves, undo, and mobile layout.

The browser app and Fairy-Stockfish integration are **GPL-3.0-only**; see
[LICENSE](LICENSE). The independent Rust engine, WASM/Python bindings, and their
Rust code remain **MIT**; see [LICENSE-MIT](LICENSE-MIT). See the complete
[licensing map](LICENSING.md) and [third-party notices](THIRD_PARTY.md).

Every build packages downloadable application source, lockfiles, build scripts,
and the pinned Fairy-Stockfish source under `dist/source/`. The engine archive is
also retained in `third_party/fairy-stockfish/` with provenance, build instructions,
and a checksum. The live site's Rules dialog links these downloads and licenses.

[Buy me a coffee](https://www.buymeacoffee.com/balbok0).

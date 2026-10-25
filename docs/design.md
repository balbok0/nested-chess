# Representation, search, and opponents

## Exact game state

The authoritative engine stores one `Board` per macro cell, using twelve `u64`
piece bitboards, four optional castling-rook squares, an optional en passant target, and a local
no-progress counter. A board deliberately has no side-to-move field: that belongs
to `Game`, once, for the entire match. The game also stores the forced destination,
board results, move counts, and repetition history. Board coordinates are stable
when the UI is flipped.

The measured `Board` size on x86-64 is 112 bytes. Reproduce with
`cargo run --example state_size`. This excludes allocations, match bookkeeping,
and history, which grow separately.

| Grid | Local games | Local board storage | Binary 12-plane features | Float32 119-plane features |
| --- | ---: | ---: | ---: | ---: |
| 2×2 | 4 | 448 B | 3 KiB | 119 KiB |
| 4×4 | 16 | 1,792 B | 12 KiB | 476 KiB |
| 8×8 | 64 | 7,168 B | 48 KiB | 1.86 MiB |

The final column is a hypothetical AlphaZero-style input for all boards; it is
not the engine's storage format or a model implemented here. It excludes extra
macro metadata, batch size, weights, optimizer state, and neural-network
activations. Training memory is much larger than one input tensor.

`piece_planes()` exports `[B, 12, 8, 8]` binary values. Plane order is White
pawn/knight/bishop/rook/queen/king, then Black in the same order. Within a plane,
rank 1 comes first, and files run a through h. It is a **piece-only feature view**,
not a sufficient game state. A model also needs global color, the forced-board
mask, castling rights, en passant, no-progress counters, results, and information
about repetition. The versioned replay save preserves the full state and its
history. FEN strings alone do not preserve repetition.

## What AlphaZero encodes

AlphaZero does not assign one vector index to each possible complete chess
position. It encodes a position as spatial feature planes and learns shared
patterns over them. The original chess representation used 119 planes of 8×8
values, including history. Its move representation used 8×8×73 = 4,672 potential
actions, with illegal actions masked. See the
[AlphaZero paper](https://arxiv.org/abs/1712.01815) and
[Lc0's AlphaZero primer](https://lczero.org/dev/lc0/search/alphazero/).

For this game, a direct extension would add a board dimension. The position tensor
grows linearly with the number of boards. The number of possible complete
positions grows combinatorially: if a local board has S configurations and there
are B boards, S^B is a crude Cartesian-product bound before routing, global-turn,
and reachability constraints. That enormous count does not require an enormous
tensor or a table of all positions.

Likewise, a flat macro action head would have B×4,672 entries: 18,688, 74,752, or
299,008. Most turns permit only one board, so allocating and learning a full flat
head is avoidable. A shared local policy head plus a board-selection head is a
more natural future design.

## Search cost and feasibility

At the initial free turn there are exactly 80, 320, or 1,280 legal actions for the
three grid sizes. A forced turn has only one board's moves; it does not multiply
the branch count by 64. Later free turns can still be much wider than ordinary
chess. The match also lasts many local games, with routing coupling their tactics.

Storage and browser play are straightforward. An optimal solver is unrealistic.
Learning a strong agent from scratch is a substantial research project: longer
episodes, sparse global rewards, and a new strategic relationship between local
moves and future access to boards all matter. A normal chess evaluator also
misjudges positions where a player may move again before their local opponent.

My recommendation is to start with 2×2, measure game length and first-player
advantage, and keep Fairy-Stockfish as a local baseline. For a later learned agent,
use a shared board encoder, a small cross-board model aware of routing, masked
local move policies, and a value predicting the final board-win difference.
No model or speculative training infrastructure is part of v0.

## Why Fairy-Stockfish

Ordinary Stockfish enforces royal kings and legal check responses. These rules
allow positions that it cannot evaluate correctly or safely. Fairy-Stockfish
supports nonroyal kings through its commoner piece and king extinction through
its configurable variant system. The shipped configuration inherits `extinction`
and narrows the extinction set to kings, with queen/rook/bishop/knight promotions.
Simply setting `checking = false` would be wrong: in Fairy-Stockfish that can
prohibit giving check, rather than make kings nonroyal. See the upstream
[variant definitions](https://github.com/fairy-stockfish/Fairy-Stockfish/blob/master/src/variant.cpp)
and [configuration documentation](https://github.com/fairy-stockfish/Fairy-Stockfish/blob/master/src/variants.ini).

The integration uses `fairy-stockfish-nnue.wasm` 1.1.12 with its classical evaluator
(`Use NNUE = false`), because no neural weights for this custom variant are
provided. The same binary runs in a browser worker or a persistent Node process
behind the native Rust/Python UCI adapter. Each Fairy-Stockfish request receives just one board,
with the global player written into the FEN side field. The Rust engine validates
the result before accepting it.

Fairy-Stockfish understands the local king-capture objective. Its search still
alternates colors locally, which is an approximation of the full nested game.
The Rust global bot wraps these local estimates in negamax with alpha-beta,
iterative deepening, and depth/node/time budgets. It searches every legal global
move, with actual routing and repetition history. A local evaluation cache is
shared across iterations; there is no full-match transposition table.

The leaf utility is expected match score, not point margin. A dynamic program over
half-point totals combines independent board win/draw/loss estimates with settled
points, then computes the probability of finishing above or at the majority
threshold. Terminal match results override the estimates. The initial local
conversion uses bounded tanh centipawn estimates and zero ongoing draw mass except
for boards immobile for both colors. This is a heuristic, not calibrated WDL.
See [the HTML analysis](../web/analysis.html) and `crates/engine/src/search.rs`.

## About 400–2500 Elo presets

For ordinary chess, Stockfish offers `Skill Level` and `UCI_LimitStrength` /
`UCI_Elo`, with supported ranges depending on the engine version. These are not
universal ratings; they depend on calibration and conditions. They do not provide
valid ratings for this new king-capture, uneven-turn game. See the
[Stockfish options source](https://github.com/official-stockfish/Stockfish/blob/master/src/engine.cpp).

[Maia](https://github.com/CSSLab/maia-chess) is another open-source option for
human-like standard-chess play, trained for human rating bands. It is not a drop-in
solution here because its normal-chess training and legal positions differ.

Six practical difficulty presets exist in this app, but promising the requested
400/800/1200/1600/2000/2500 ratings would be misleading. A useful next step would
be a nested-game rating pool, human testing, and calibrated error policies for
beginners. The current low skill settings do not guarantee easy human opponents.

## Does this already exist?

I found a closely related playable
[Ultimate Chess](https://ralphs16.github.io/ultimate-chess/). It explicitly takes
inspiration from Ultimate Tic-Tac-Toe, uses six linked boards named after piece
types, routes according to the piece moved, and aims for checkmate on the King
board. That differs from destination-region routing, variable grids, capturable
kings, and total board-win scoring here.

[Recursive Chess](https://jdh.hamkins.org/what-is-the-game-of-recursive-chess/)
is another related idea: a capture is decided by a nested game between the two
pieces. It also has different mechanics.

The searches did not establish an existing implementation with this exact rule
combination. That is a limited search result, not proof the game is original.

# Material-only global negamax

Global negamax with alpha-beta, using a Rust material evaluator instead of
Fairy-Stockfish leaf analysis. Local material is White minus Black centipawns:
pawn 100, knight/bishop 300, rook 500, queen 900, king 0. Captured kings and match
outcomes are handled by the game rules. No piece-square tables, mobility,
quiescence, positional terms, or local search are added.

The existing global search, iterative deepening, move ordering, capture-priority
fallback, centipawn-to-odds conversion, and dynamic match-score aggregation remain
unchanged. The converted odds are heuristic estimates, not calibrated probabilities.

Six conditions: Casual vs Strong, Strong vs Maximum, Maximum vs Maximum on both
4×2 and 4×4. Each has 100 scored games, 50 per color: 600 games total. Ten
unscored determinism pilots precede each condition; force paired two-ply classical
openings only if all five complete traces repeat within both colors. Use the
preceding study's 24 workers, release profile, depth/time budgets, and 100,000-node
cap. Do not adjudicate games or impose a move-limit draw.

The material player starts no UCI engine. Its material evaluations are counted
separately from engine calls. The opposing local Fairy-Stockfish uses the previous
skill/time presets and free-board selection policy. Full replays, depth,
fallbacks, calls, nodes, cutoffs, move times and CPU/memory are archived.

Compare with the historical global, fallback-only and local controls. These are
separate time-budgeted runs; point differences do not establish a causal effect
of the evaluator. Win rates and match scores use the same conservative 95%
color-paired Bonferroni–Clopper–Pearson bounds, per condition. They do not establish
optimal play or calibrated Elo.

## Reproduction

```sh
npm ci
cargo build --release -p nested-chess --example benchmark
uv run --no-project --with psutil python -m scripts.run_rectangular_benchmark \
  scratch/claude/20261028_material-negamax/run --material --workers 24
```

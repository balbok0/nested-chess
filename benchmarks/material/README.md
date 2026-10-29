# Material-only global negamax

All 600 scored games and 60 pilots completed. Every replay and the accounting for
123,129 searched moves passed validation. No condition needed forced openings.
The scored phase took 3 hours 48 minutes with 24 workers.

| Grid | Material player / local opponent | W–D–L | Win rate | Match score | 95% score bounds | Mean depth |
| --- | --- | --- | --- | --- | --- | --- |
| 4×2 | Casual / Strong | 9–5–86 | 9% | 11.5% | 4.3–30.5% | 2.00 |
| 4×2 | Strong / Maximum | 13–13–74 | 13% | 19.5% | 8.3–41.5% | 3.70 |
| 4×2 | Maximum / Maximum | 22–10–68 | 22% | 27.0% | 12.8–49.1% | 4.29 |
| 4×4 | Casual / Strong | 8–6–86 | 8% | 11.0% | 4.7–30.2% | 2.00 |
| 4×4 | Strong / Maximum | 22–6–72 | 22% | 25.0% | 11.7–46.4% | 2.98 |
| 4×4 | Maximum / Maximum | 13–14–73 | 13% | 20.0% | 11.0–40.6% | 3.43 |

All six score upper bounds are below 50%. The material player made zero engine
calls and used no fallback moves. It completed substantially more search depth
than the earlier Fairy-Stockfish-backed global bot, but scored much lower.
On 4×4 against Strong local, Casual material scored 11%, versus 80% for fallback
alone and 83% for Fairy-Stockfish-backed Casual global in the earlier runs.
These observations concern this material-only policy within the existing search
and match-score aggregation, not negamax as an algorithm in general.

Scored-phase system CPU averaged 78.8%, peaking at 92.9%. Peak summed benchmark
RSS was 7.73 GiB; peak system memory use was 23.4%. Summed RSS includes shared
pages, and the host runs other services. Full win-rate bounds, colors, depth,
evaluation costs and historical comparisons are in the standalone HTML report.

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

Replay and validate the completed sample, then generate the standalone report:

```sh
cargo run --release -p nested-chess --example verify_material -- \
  scratch/claude/20261028_material-negamax/run/results.jsonl
uv run --no-project python claude/20261028_material-benchmark/validate.py \
  scratch/claude/20261028_material-negamax/run target/release/examples/benchmark
uv run --no-project python claude/20261028_material-benchmark/report.py
```

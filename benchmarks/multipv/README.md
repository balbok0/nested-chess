# Ten-candidate bot versus local Fairy-Stockfish

All 600 scored games and 60 pilots completed. Every replay and accounting for
86,589 scored bot moves passed validation. No condition needed forced openings.
The scored phase took 3 hours 18 minutes with 24 workers.

| Grid | Global / local preset | W–D–L | Win rate | Match score | 95% score bounds |
| --- | --- | --- | --- | --- | --- |
| 4×2 | Casual / Strong | 18–29–53 | 18% | 32.5% | 19.0–53.1% |
| 4×2 | Strong / Maximum | 50–25–25 | 50% | 62.5% | 41.3–79.1% |
| 4×2 | Maximum / Maximum | 34–27–39 | 34% | 47.5% | 30.5–65.0% |
| 4×4 | Casual / Strong | 74–9–17 | 74% | 78.5% | 59.0–87.2% |
| 4×4 | Strong / Maximum | 89–5–6 | 89% | 91.5% | 72.9–96.9% |
| 4×4 | Maximum / Maximum | 84–10–6 | 84% | 89.0% | 68.6–96.2% |

All three 4×4 score lower bounds exceed 50%; all three 4×2 intervals include 50%.
Casual 4×4 used its capture-priority fallback on 97.4% of moves and averaged depth
0.026, so its result does not isolate a benefit from global planning. Strong and
Maximum 4×4 averaged depth 0.69 and 1.12, with fallback on 34.0% and 23.3% of moves.

The earlier full-width 4×4 scores were 83.0%, 90.5%, and 87.5%. These separate
runs do not establish a strength gain from MultiPV. Total engine calls per move
fell, but candidate calls use ten times the scalar node budget; this is not proof
of compute savings. Exact call counts, move times, colors and all historical
controls are in the HTML report and summary.

Scored-phase system CPU averaged 87.2%, peaking at 99.3%. Peak summed benchmark
RSS was 15.42 GiB, and peak system memory use was 36.3%.

Six conditions: Casual vs Strong, Strong vs Maximum, Maximum vs Maximum on both
4×2 and 4×4. Each has 100 scored games, 50 per color: 600 total. Sixty unscored
determinism pilots precede scoring. Force paired two-ply classical openings only
if all five complete traces repeat within both colors of a condition.

The global bot is the unchanged ten-candidate implementation from feat/multipv-search:
MultiPV up to ten per eligible board for both players, king captures and missing
routing-region representatives, alpha-beta, iterative deepening, cached analyses,
existing match-score aggregation and capture-priority fallback. Candidate requests
use ten times the scalar Fairy-Stockfish node budget. Candidate calls are a subset
of total engine calls, not an additional uncounted cost.

The opponent is local-only Fairy-Stockfish using the same king-capture variant,
skill/time presets and free-board policy as earlier studies. Both receive legal
local positions; only the global bot models routing in its outer search. This
measures playing strength, depth, engine-call cost and resources at the existing
unequal preset budgets, rather than equal-compute efficiency or calibrated Elo.

Release build, 24 concurrent games, single-threaded Fairy-Stockfish 1.1.12, NNUE
off. Games end under the actual rules, without evaluation adjudication or a
move-limit draw. Preserve full replay saves and five-second resource samples.
Summed RSS includes shared pages; system CPU also includes other host services.

Win rate is wins / games; match score is (wins + ½ draws) / games. Use the prior
conservative 95% color-paired Bonferroni–Clopper–Pearson bounds, per condition.
Historical full-width, local-only, fallback and material controls ran separately;
point differences do not establish a causal improvement under changing host load.

## Reproduction

```sh
npm ci
cargo build --release -p nested-chess --example benchmark
uv run --no-project --with psutil python -m scripts.run_rectangular_benchmark \
  scratch/claude/20261030_multipv-benchmark/run --multipv --workers 24
```

The runner journals results and requires the same binary and worker count when
resuming. Protocol metadata includes hashes of the binary, search, UCI adapter,
and MultiPV parser. The final standalone HTML is
`claude/20261030_multipv-benchmark/report.html`.

Replay the completed results, validate the full sample, and export the report:

```sh
cargo run --release -p nested-chess --example verify_multipv -- \
  scratch/claude/20261030_multipv-benchmark/run/results.jsonl
uv run --no-project python claude/20261030_multipv-benchmark/validate.py \
  scratch/claude/20261030_multipv-benchmark/run target/release/examples/benchmark
uv run --no-project python claude/20261030_multipv-benchmark/report.py
```

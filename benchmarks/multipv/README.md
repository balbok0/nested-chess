# Ten-candidate bot versus local Fairy-Stockfish

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

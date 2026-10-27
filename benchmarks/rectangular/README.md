# 4×2 and 4×4 benchmark

The targeted study uses eight boards in four columns/two rows, and sixteen boards
in four columns/four rows. It compares Casual vs Strong, Strong vs Maximum, and
Maximum vs Maximum, with global-vs-local and local-vs-local controls on each grid.
Each condition has 100 scored games, 50 per color: 1,200 games total.

The bots, local board-choice policy, leaf-node limits, time budgets, and 100,000
global-node cap match the [2×2 protocol](../classical-2x2/README.md). The engine adds
rectangular routing; it does not change the search or evaluation heuristic.

Each condition first has five unscored games per color. Two-ply openings are
forced only when all five full traces repeat identically within both colors.
Opening pools are generated for each grid, and each scored pair shares its
opening with colors swapped. Pilots are excluded from the final sample.

Results retain full replay saves, per-color outcomes, engine calls, completed
depth, fallbacks, nodes, cutoffs and elapsed time. Five-second CPU/memory samples
cover the runner and its children. Summed RSS includes shared pages and is not
unique physical memory. The host also runs other services; results are conditional
on that load. The final summary reports telemetry from the scored phase.

Win rate is wins / games. Match score is (wins + ½ draws) / games. Confidence
bounds use the same conservative color-paired Bonferroni–Clopper–Pearson method
as the 2×2 study, per condition. They do not establish optimal-play strength or Elo.

## Reproduction

```sh
npm ci
npm run build:wasm
cargo build --release --example benchmark
uv run --no-project --with psutil python -m scripts.run_rectangular_benchmark \
  scratch/claude/20261027_rectangular-benchmark/run --workers 24
```

The runner journals completed games and resumes with the same binary and worker
count. `protocol.json` records the release binary hash, source revision, exact
budgets and opening decisions. The HTML report is `rectangular-analysis.html`.

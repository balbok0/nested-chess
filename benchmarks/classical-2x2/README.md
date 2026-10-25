# Classical 2×2 benchmark

The scored sample contains 100 games for each of 21 global/local difficulty pairs
and 21 local/local controls: 4,200 games, with 50 games per color per pairing.
Only pairs with row difficulty <= column difficulty are included. All games use
the classical starting arrangement and the current king-capture rules.

## Controls

| Preset | Global depth cap | Global ms | Global leaf nodes | Local skill | Local ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Beginner | 1 | 300 | 32 | 0 | 60 |
| Casual | 2 | 600 | 64 | 2 | 100 |
| Club | 3 | 1,000 | 128 | 5 | 180 |
| Strong | 4 | 1,600 | 128 | 10 | 300 |
| Expert | 5 | 2,400 | 256 | 15 | 500 |
| Maximum | 6 | 4,000 | 256 | 20 | 900 |

Global search retains all legal moves and has a 100,000-node cap. The local
baseline selects a free board with `legal_boards[global_ply % count]`. Engine
processes are fresh for each game; Fairy-Stockfish uses one thread, its classical
evaluator, and the bundled pinned version. No resignation, score adjudication,
or artificial move-limit draw is used. All completed games retain replay saves.

Each pairing first has 10 unscored pilot games: five per color. Forced openings
are used only if the five complete move traces for each color repeat identically.
If needed, 50 reproducible, unique two-ply openings are generated from classical
starts with seed 20261025; each is played with the players' colors swapped.
Pilots are excluded from the final 100 regardless of their determinism result.

The initial timing pilot used 10 workers. The matrix pilot used 32, which
saturated CPU. The scored batch uses 24 workers. Five-second resource samples
record system CPU/memory and summed RSS for the runner and its descendants.
Summed RSS includes shared pages and is not unique physical memory consumption.

## Statistics

Win rate is wins / games. Match score is (wins + draws / 2) / games. Results also
retain color-specific scores, move counts, average completed search depth,
fallback frequency, engine-call counts, search nodes, and wall time.

One statistical unit is a color-swapped pair. For win rate, its outcome is the
number of row-player wins (0, 1, or 2). For match score, its outcome is the combined
score in half-point units (0 through 4). For each category, a Clopper–Pearson
binomial interval is computed with alpha = 0.05 / number of categories. Bonferroni
therefore gives simultaneous coverage of at least 95% for that pairing's category
probabilities. Minimizing/maximizing the weighted mean over these bounds, subject
to probabilities summing to one, gives conservative bounds for win rate or score.

These are per-pairing intervals, not simultaneous bounds across all 42 pairings.
They assume independent sampled pairs. Hardware load, time controls, and
Fairy-Stockfish's internal randomness can affect traces and achieved search depth.
They measure these policies under this protocol, not optimal-play fairness or Elo.

Reference: [NIST binomial confidence intervals](https://itl.nist.gov/div898/handbook/prc/section2/prc241.htm).

## Reproduction

```sh
npm ci
npm run build:wasm
cargo build --release --example benchmark
uv run --no-project --with psutil python scripts/monitor-benchmark.py RUN/resources.jsonl RUN
uv run --no-project python scripts/run_benchmark.py RUN --workers 24
```

`RUN` is a scratch directory. Create it first, and run the monitor in a separate
terminal. Use `--wait-pilot` only to attach to an independently running pilot.
The Rust runner accepts `JOBS.json RESULTS.jsonl WORKERS`,
checkpoints after each completed game, and skips recorded job IDs on resume.
`scripts/run_benchmark.py` applies the pilot decision, runs the final matrix,
updates `web/benchmark-results.json`, and writes the archived games here.

`protocol.json` records source revision, binary SHA-256, exact controls, and the
per-pairing determinism decisions. `summary.json` is the data shown in the HTML.
`global-A-B.jsonl.gz` and `local-A-B.jsonl.gz` contain all scored games; level
indices run 0 (Beginner) through 5 (Maximum). `pilot.jsonl.gz` and
`resources.jsonl.gz` retain pilot games and telemetry.

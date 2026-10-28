# Capture-priority fallback control

This control runs the exact move returned by global search before its first
completed depth, without search, local evaluation, or an engine for that player.
It scores captures as 10 × captured-piece value, plus promotion value. Piece
values are king 1000, queen 9, rook 5, bishop/knight 3, pawn 1. Ties retain legal
action order, including board order. En passant has an empty destination and
therefore receives no capture bonus in this existing heuristic.

There are four conditions: Strong and Maximum local opponents on 4×2 and 4×4.
The fallback has no difficulty setting; `a_level=0` is an ignored manifest field.
Each condition has 100 scored games, 50 per color: 400 games total. The local
opponent uses the preceding study's unchanged skill/time presets and board
selection policy. Games end under the actual rules, with no move-limit draw or
evaluation adjudication.

Ten unscored pilots per condition check determinism. Force paired two-ply
classical openings only when all five complete traces repeat within both colors.
Use the preceding study's release build/concurrency protocol: 24 workers,
single-threaded Fairy-Stockfish 1.1.12, NNUE off, resource sampling every five
seconds. Save full replays, engine-call counts, depth, move times, and telemetry.

Win rate is wins / games; match score is (wins + ½ draws) / games. Report the same
conservative 95% color-paired Bonferroni–Clopper–Pearson bounds as the earlier
study, per condition. The prior global and local controls are historical runs,
not simultaneous matched trials. Their point differences do not establish a
causal search benefit, and time-limited engine strength can depend on host load.

## Reproduction

```sh
npm ci
cargo build --release -p nested-chess --example benchmark
uv run --no-project --with psutil python -m scripts.run_rectangular_benchmark \
  scratch/claude/20261028_fallback-only/run --fallback --workers 24
```

The runner journals games and resumes with the same binary and concurrency.
The final HTML comparison is `claude/20261028_fallback-benchmark/report.html`.

Validate every replay and fallback choice, then verify the complete sample:

```sh
cargo run --release -p nested-chess --example verify_fallback -- \
  scratch/claude/20261028_fallback-only/run/results.jsonl
uv run --no-project python claude/20261028_fallback-benchmark/validate.py \
  scratch/claude/20261028_fallback-only/run target/release/examples/benchmark
uv run --no-project python claude/20261028_fallback-benchmark/report.py
```

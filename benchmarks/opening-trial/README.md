# Independent classical openings

A separate 20-game diversity trial: Casual global versus Strong local, plus
Casual local versus Strong local, with five color-swapped position pairs each.
Every setup contains four independently developed classical positions after
five full moves per board. White starts with free board choice. Setup moves
bypass routing; continuation moves follow normal rules.

`openings.json` records the 20 exact book lines, UCI moves, FENs, pinned Lichess
CC0 source revision, and dataset hashes. `jobs.json` reproduces the game inputs.
The release runner used one worker alongside the primary 24-worker study.
Search code and preset budgets match the primary study.

`developed.jsonl.gz` contains the trial games. `standard.jsonl.gz` contains the
matched 20 fresh-start pilots, excluded from the primary scored sample.
`summary.json` contains W/D/L, paired confidence bounds, search costs, and
sequence comparisons. `resources.jsonl.gz` contains five-second telemetry.

Local similarity is one minus normalized Levenshtein distance over color-labeled
local move streams, weighted by maximum stream lengths, taking the best of all
24 board permutations. Comparisons stay within policy and player color. Setup
moves are excluded. This permits board relabeling and global interleaving, so
cosmetic routing differences alone do not establish diversity.

Median similarity was 8.7% for fresh starts and 4.5% for developed positions;
maximum similarity was 26.5% and 13.3%. Both trial policies finished 0–3–7.
Five selected setups cannot establish general strength or opening coverage.
Concurrency differed from the pilots and global search completion rates changed;
do not interpret outcome differences as a controlled opening effect.

Prepare inputs:

```sh
uv run --no-project --with python-chess python scripts/prepare_opening_trial.py PATH
```

Build and run the Rust example in release mode, then use
`scripts/benchmark_diversity.py` with `uv run --with rapidfuzz` for similarity.
The primary study and this trial remain separate.

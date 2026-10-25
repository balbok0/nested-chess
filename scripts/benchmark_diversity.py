"""Measure opening overlap and move-sequence variation within benchmark pilots."""

import argparse
import itertools
import json
import statistics
from collections import Counter, defaultdict
from pathlib import Path

from rapidfuzz.distance import Levenshtein

from benchmark_results import read_records


def diversity(rows: list[dict]) -> dict:
    """Compare complete five-game samples for the same policies and colors."""
    groups = defaultdict(list)
    for row in rows:
        job = row["job"]
        groups[(job["family"], job["a_level"], job["b_level"], job["a_color"])].append(row)
    comparisons = []
    summaries = []
    for key, games in sorted(groups.items()):
        if len(games) != 5:
            continue
        tokens = [tuple(f"{action['board']}:{action.get('uci', 'draw')}" for action in row["save"]["events"]) for row in games]
        pairs = []
        for first, second in itertools.combinations(tokens, 2):
            prefix = next((i for i, (a, b) in enumerate(zip(first, second)) if a != b), min(len(first), len(second)))
            distance = Levenshtein.normalized_distance(first, second)
            moves_only = Levenshtein.normalized_distance(tuple(move.split(":", 1)[1] for move in first), tuple(move.split(":", 1)[1] for move in second))
            pairs.append((prefix, distance))
            comparisons.append({"group": key, "prefix": prefix, "edit_fraction": distance, "moves_only_edit_fraction": moves_only})
        summaries.append({
            "group": key,
            "distinct_first_4": len({moves[:4] for moves in tokens}),
            "distinct_first_8": len({moves[:8] for moves in tokens}),
            "distinct_first_12": len({moves[:12] for moves in tokens}),
            "median_common_prefix": statistics.median(pair[0] for pair in pairs),
            "median_edit_fraction": statistics.median(pair[1] for pair in pairs),
            "ply_range": [min(map(len, tokens)), max(map(len, tokens))],
            "outcomes": dict(Counter(row["outcome"] for row in games)),
        })
    return {
        "games": len(rows), "groups": len(summaries), "pair_comparisons": len(comparisons),
        "median_common_prefix": statistics.median(row["prefix"] for row in comparisons),
        "median_edit_fraction": statistics.median(row["edit_fraction"] for row in comparisons),
        "median_moves_only_edit_fraction": statistics.median(row["moves_only_edit_fraction"] for row in comparisons),
        "minimum_moves_only_edit_fraction": min(row["moves_only_edit_fraction"] for row in comparisons),
        "near_duplicates_under_10pct": sum(row["edit_fraction"] <= 0.1 for row in comparisons),
        "near_duplicates_under_20pct": sum(row["edit_fraction"] <= 0.2 for row in comparisons),
        "same_first_8_all_five": sum(row["distinct_first_8"] == 1 for row in summaries),
        "flagship": [row for row in summaries if row["group"][:3] == ("global", 1, 3)],
        "most_similar": sorted(summaries, key=lambda row: row["median_edit_fraction"])[:6],
        "by_group": summaries,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("journal", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    result = diversity(read_records(args.journal))
    args.output.write_text(f"{json.dumps(result, indent=2)}\n")
    print(json.dumps({key: value for key, value in result.items() if key != "by_group"}, indent=2))

"""Measure opening overlap and move-sequence variation within benchmark pilots."""

import argparse
import itertools
import json
import statistics
from collections import Counter, defaultdict
from pathlib import Path

from rapidfuzz.distance import Levenshtein

from benchmark_results import read_records

BOARD_PERMUTATIONS = tuple(itertools.permutations(range(4)))


def local_streams(row: dict) -> list[tuple[str, ...]]:
    """Return color-labeled move sequences for each local board."""
    streams = [[] for _ in range(4)]
    color = row["save"]["first_player"]
    for action in row["save"]["events"]:
        if action["kind"] == "move":
            streams[action["board"]].append(f"{color}:{action['uci']}")
            color = "black" if color == "white" else "white"
    return [tuple(stream) for stream in streams]


def local_similarity(first: list[tuple[str, ...]], second: list[tuple[str, ...]]) -> float:
    """Compare local sequences allowing board relabeling and global interleaving."""
    distances = [[Levenshtein.distance(a, b) for b in second] for a in first]
    sizes = [[max(len(a), len(b)) for b in second] for a in first]
    scores = []
    for permutation in BOARD_PERMUTATIONS:
        total = sum(sizes[a][b] for a, b in enumerate(permutation))
        scores.append(1 - sum(distances[a][b] for a, b in enumerate(permutation)) / total if total else 1.0)
    return max(scores)


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
        local_scores = []
        streams = [local_streams(row) for row in games]
        for first_index, second_index in itertools.combinations(range(len(tokens)), 2):
            first, second = tokens[first_index], tokens[second_index]
            prefix = next((i for i, (a, b) in enumerate(zip(first, second)) if a != b), min(len(first), len(second)))
            distance = Levenshtein.normalized_distance(first, second)
            moves_only = Levenshtein.normalized_distance(tuple(move.split(":", 1)[1] for move in first), tuple(move.split(":", 1)[1] for move in second))
            board_similarity = local_similarity(streams[first_index], streams[second_index])
            local_scores.append(board_similarity)
            pairs.append((prefix, distance))
            comparisons.append({"group": key, "prefix": prefix, "edit_fraction": distance, "moves_only_edit_fraction": moves_only, "local_similarity": board_similarity})
        summaries.append({
            "group": key,
            "distinct_first_4": len({moves[:4] for moves in tokens}),
            "distinct_first_8": len({moves[:8] for moves in tokens}),
            "distinct_first_12": len({moves[:12] for moves in tokens}),
            "median_common_prefix": statistics.median(pair[0] for pair in pairs),
            "median_edit_fraction": statistics.median(pair[1] for pair in pairs),
            "median_local_similarity": statistics.median(local_scores),
            "ply_range": [min(map(len, tokens)), max(map(len, tokens))],
            "outcomes": dict(Counter(row["outcome"] for row in games)),
        })
    return {
        "games": len(rows), "groups": len(summaries), "pair_comparisons": len(comparisons),
        "median_common_prefix": statistics.median(row["prefix"] for row in comparisons),
        "median_edit_fraction": statistics.median(row["edit_fraction"] for row in comparisons),
        "median_moves_only_edit_fraction": statistics.median(row["moves_only_edit_fraction"] for row in comparisons),
        "minimum_moves_only_edit_fraction": min(row["moves_only_edit_fraction"] for row in comparisons),
        "median_local_similarity": statistics.median(row["local_similarity"] for row in comparisons),
        "maximum_local_similarity": max(row["local_similarity"] for row in comparisons),
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

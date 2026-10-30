"""Summarize completed, color-paired classical benchmark games."""

import argparse
import gzip
import json
import math
from collections import Counter, defaultdict
from dataclasses import asdict, dataclass
from pathlib import Path

LEVELS = ("Beginner", "Casual", "Club", "Strong", "Expert", "Maximum")


@dataclass(frozen=True)
class Interval:
    low: float
    high: float


def binomial_interval(successes: int, trials: int, alpha: float) -> Interval:
    """Return a two-sided Clopper–Pearson interval."""
    if not 0 <= successes <= trials or trials == 0 or not 0 < alpha < 1:
        raise ValueError("invalid binomial counts or alpha")
    tail = alpha / 2

    def solve(cutoff: int, upper_tail: bool) -> float:
        lower, upper = 0.0, 1.0
        for _ in range(70):
            probability = (lower + upper) / 2
            terms = range(cutoff, trials + 1) if upper_tail else range(cutoff + 1)
            mass = sum(math.comb(trials, k) * probability**k * (1 - probability)**(trials - k) for k in terms)
            if (mass < tail) == upper_tail:
                lower = probability
            else:
                upper = probability
        return (lower + upper) / 2

    return Interval(0.0 if successes == 0 else solve(successes, True), 1.0 if successes == trials else solve(successes, False))


def paired_interval(values: list[int], maximum: int) -> Interval:
    """Bound a mean using simultaneous binomial intervals for paired outcomes."""
    if not values or any(not isinstance(value, int) or not 0 <= value <= maximum for value in values):
        raise ValueError("invalid paired outcomes")
    counts = Counter(values)
    intervals = [binomial_interval(counts[k], len(values), 0.05 / (maximum + 1)) for k in range(maximum + 1)]

    def extreme(reverse: bool) -> float:
        probabilities = [interval.low for interval in intervals]
        remaining = 1 - sum(probabilities)
        order = reversed(range(maximum + 1)) if reverse else range(maximum + 1)
        for k in order:
            added = min(remaining, intervals[k].high - probabilities[k])
            probabilities[k] += added
            remaining -= added
        if abs(remaining) > 1e-9:
            raise ValueError("infeasible simultaneous probability bounds")
        return sum(k / maximum * probability for k, probability in enumerate(probabilities))

    return Interval(max(0.0, extreme(False)), min(1.0, extreme(True)))


def read_records(path: Path) -> list[dict]:
    """Read newline-committed records from an active append-only journal."""
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines(keepends=True) if line.endswith("\n")]


def summarize(rows: list[dict]) -> list[dict]:
    """Summarize complete color pairs and their runtime metrics."""
    groups = defaultdict(list)
    ids = set()
    for row in rows:
        job = row["job"]
        if job["id"] in ids:
            raise ValueError(f"duplicate game {job['id']}")
        ids.add(job["id"])
        groups[(job.get("columns", 2), job.get("rows", 2), job["family"], job["a_level"], job["b_level"])].append(row)
    results = []
    for (columns, grid_rows, family, a_level, b_level), games in sorted(groups.items()):
        pairs = defaultdict(list)
        for game in games:
            pairs[game["job"]["pair"]].append(game)
        complete = []
        paired_wins = []
        paired_scores = []
        for pair in pairs.values():
            if len(pair) > 2 or len({game["job"]["a_color"] for game in pair}) != len(pair):
                raise ValueError("duplicate color in pair")
            if len(pair) != 2:
                continue
            if any(pair[0]["job"].get(key) != pair[1]["job"].get(key) for key in ("opening", "initial_fens")):
                raise ValueError("color-paired games have different openings")
            complete.extend(pair)
            paired_wins.append(sum(game["a_score"] == 1 for game in pair))
            paired_scores.append(int(sum(game["a_score"] for game in pair) * 2))
        if not complete:
            continue
        wins = sum(game["a_score"] == 1 for game in complete)
        draws = sum(game["a_score"] == 0.5 for game in complete)
        a_metrics = {key: sum(game["a"][key] for game in complete) for key in ("moves", "elapsed_ms", "engine_calls", "search_nodes", "cutoffs", "cache_hits", "completed_depth_sum", "fallback_moves")}
        a_metrics["material_evaluations"] = sum(game["a"].get("material_evaluations", 0) for game in complete)
        a_metrics["candidate_evaluations"] = sum(game["a"].get("candidate_evaluations", 0) for game in complete)
        b_metrics = {key: sum(game["b"][key] for game in complete) for key in ("moves", "elapsed_ms", "engine_calls")}
        results.append({
            "columns": columns, "rows": grid_rows,
            "family": family, "a_level": a_level, "b_level": b_level,
            "games": len(complete), "pairs": len(paired_wins), "wins": wins, "draws": draws, "losses": len(complete) - wins - draws,
            "win_rate": wins / len(complete), "score_rate": (wins + draws / 2) / len(complete),
            "win_ci95": asdict(paired_interval(paired_wins, 2)), "score_ci95": asdict(paired_interval(paired_scores, 4)),
            "white": {"games": sum(game["job"]["a_color"] == "white" for game in complete), "score": sum(game["a_score"] for game in complete if game["job"]["a_color"] == "white")},
            "black": {"games": sum(game["job"]["a_color"] == "black" for game in complete), "score": sum(game["a_score"] for game in complete if game["job"]["a_color"] == "black")},
            "mean_plies": sum(game["plies"] for game in complete) / len(complete),
            "a_metrics": a_metrics, "b_metrics": b_metrics,
        })
    return results


def write_report(directory: Path, output: Path, archive: Path | None) -> None:
    """Write summary data and optional complete-game archives."""
    records = read_records(directory / "results.jsonl")
    protocol = json.loads((directory / "protocol.json").read_text())
    summary = {"protocol": protocol, "levels": LEVELS, "completed_games": len(records), "target_games": protocol.get("target_games", 4200), "results": summarize(records)}
    finished = protocol.get("finished_utc")
    resources = [row for row in read_records(directory / "resources.jsonl") if finished is None or row["utc"] <= finished]
    telemetry = [row for row in resources if row["utc"] >= protocol["started_utc"]]
    if telemetry:
        summary["resources"] = {
            "samples": len(telemetry),
            "peak_system_cpu_percent": max(row["system_cpu_percent"] for row in telemetry),
            "mean_system_cpu_percent": sum(row["system_cpu_percent"] for row in telemetry) / len(telemetry),
            "peak_system_memory_percent": max(row["system_memory_percent"] for row in telemetry),
            "peak_benchmark_rss_bytes": max(row["benchmark_rss_bytes"] for row in telemetry),
        }
    output.write_text(f"{json.dumps(summary, indent=2)}\n")
    if archive:
        archive.mkdir(parents=True, exist_ok=True)
        grouped = defaultdict(list)
        for record in records:
            job = record["job"]
            prefix = f"{job['columns']}x{job['rows']}-" if "grids" in protocol else ""
            grouped[f"{prefix}{job['family']}-{job['a_level']}-{job['b_level']}.jsonl.gz"].append(record)
        for filename, games in grouped.items():
            with gzip.GzipFile(filename=str(archive / filename), mode="wb", mtime=0) as stream:
                for game in sorted(games, key=lambda row: row["job"]["id"]):
                    stream.write(f"{json.dumps(game, separators=(',', ':'))}\n".encode())
        with gzip.GzipFile(filename=str(archive / "resources.jsonl.gz"), mode="wb", mtime=0) as stream:
            for sample in resources:
                stream.write(f"{json.dumps(sample)}\n".encode())
        with gzip.GzipFile(filename=str(archive / "pilot.jsonl.gz"), mode="wb", mtime=0) as stream:
            stream.write((directory / "pilot-results.jsonl").read_bytes())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--archive", type=Path)
    args = parser.parse_args()
    write_report(args.directory, args.output, args.archive)

"""Complete the classical 2x2 benchmark from its pilot journal."""

import argparse
import hashlib
import json
import os
import platform
import subprocess
import time
from collections import defaultdict
from pathlib import Path

from benchmark_results import read_records, write_report

GLOBAL = (
    {"depth": 1, "milliseconds": 300, "analysis_nodes": 32},
    {"depth": 2, "milliseconds": 600, "analysis_nodes": 64},
    {"depth": 3, "milliseconds": 1000, "analysis_nodes": 128},
    {"depth": 4, "milliseconds": 1600, "analysis_nodes": 128},
    {"depth": 5, "milliseconds": 2400, "analysis_nodes": 256},
    {"depth": 6, "milliseconds": 4000, "analysis_nodes": 256},
)
LOCAL = (
    {"skill": 0, "milliseconds": 60},
    {"skill": 2, "milliseconds": 100},
    {"skill": 5, "milliseconds": 180},
    {"skill": 10, "milliseconds": 300},
    {"skill": 15, "milliseconds": 500},
    {"skill": 20, "milliseconds": 900},
)


def prepare_final(directory: Path, workers: int, pilot_workers: int) -> None:
    """Create the final sample after each pairing's determinism pilot completes."""
    pilot = read_records(directory / "pilot-results.jsonl")
    if len(pilot) != 420:
        raise ValueError(f"pilot requires 420 games, found {len(pilot)}")
    groups = defaultdict(list)
    for row in pilot:
        job = row["job"]
        groups[(job["family"], job["a_level"], job["b_level"])].append(row)
    if len(groups) != 42:
        raise ValueError("pilot must cover 42 pairings")
    opening_pool = json.loads(subprocess.check_output(["node", "scripts/benchmark-openings.mjs"], text=True))
    jobs = []
    conditions = []
    for (family, a_level, b_level), rows in sorted(groups.items()):
        traces = {}
        for color in ("white", "black"):
            games = [row for row in rows if row["job"]["a_color"] == color]
            if len(games) != 5:
                raise ValueError("pilot requires five games per color")
            traces[color] = len({json.dumps(row["save"]["events"], sort_keys=True) for row in games})
        deterministic = all(count == 1 for count in traces.values())
        conditions.append({"family": family, "a_level": a_level, "b_level": b_level, "pilot_unique_traces": traces, "forced_openings": deterministic})
        for pair in range(50):
            opening = opening_pool["openings"][pair] if deterministic else []
            for color in ("white", "black"):
                jobs.append({"id": f"main-{family}-{a_level}-{b_level}-{pair}-{color}", "family": family, "a_level": a_level, "b_level": b_level, "a_color": color, "pair": pair, "opening": opening})
    jobs.sort(key=lambda job: (job["pair"], job["family"], job["a_level"], job["b_level"], job["a_color"]))
    protocol = {
        "grid": 2, "start": "classical", "workers": workers, "games_per_pairing": 100, "colors_per_pairing": 50,
        "pilot_games": 420, "pilot_workers": pilot_workers, "timing_pilot_workers": 10,
        "pilot_policy": "Five games per color; force a two-ply classical opening only if each color's five complete traces are identical. All pilots are excluded from the final 100.",
        "global_presets": GLOBAL, "local_presets": LOCAL, "global_node_budget": 100000,
        "local_board_policy": "legal_boards[global_ply % legal_boards.len()]",
        "conditions": conditions, "opening_pool": opening_pool,
        "search_revision": "56ab470", "benchmark_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "binary_sha256": hashlib.sha256(Path("target/release/examples/benchmark").read_bytes()).hexdigest(),
        "platform": platform.platform(), "cpu": platform.processor(), "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "build_profile": "release", "logical_cpus": os.cpu_count(),
        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "node": subprocess.check_output(["node", "--version"], text=True).strip(),
        "fairy_stockfish": {"npm_version": "1.1.12", "source_revision": "b2e693ef1e111233ce3fb40685921708b3276ed6", "threads": 1, "nnue": False},
        "interval": "95% simultaneous Bonferroni-Clopper-Pearson bounds on the paired-outcome distribution, optimized over the probability bounds; one sample unit is a color-swapped pair.",
    }
    (directory / "jobs.json").write_text(f"{json.dumps(jobs)}\n")
    if Path("/proc/cpuinfo").exists():
        protocol["cpu"] = next(line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name"))
    (directory / "protocol.json").write_text(f"{json.dumps(protocol, indent=2)}\n")
    (directory / "results.jsonl").touch(exist_ok=False)


def run(directory: Path, workers: int, wait_pilot: bool, pilot_workers: int) -> None:
    """Run missing final jobs and write summary and archived game data."""
    if not (directory / "protocol.json").exists():
        directory.mkdir(parents=True, exist_ok=True)
        if wait_pilot:
            while len(read_records(directory / "pilot-results.jsonl")) < 420:
                time.sleep(5)
        else:
            jobs = [{"id": f"pilot-{family}-{a}-{b}-{pair}-{color}", "family": family, "a_level": a, "b_level": b, "a_color": color, "pair": pair, "opening": []}
                    for pair in range(5) for family in ("global", "local") for a in range(6) for b in range(a, 6) for color in ("white", "black")]
            manifest = directory / "all-pilot-jobs.json"
            manifest.write_text(f"{json.dumps(jobs)}\n")
            subprocess.run(["target/release/examples/benchmark", str(manifest), str(directory / "pilot-results.jsonl"), f"{pilot_workers}"], check=True)
        prepare_final(directory, workers, pilot_workers)
    if json.loads((directory / "protocol.json").read_text())["workers"] != workers:
        raise ValueError("cannot change concurrency within a benchmark run")
    log = (directory / "run.log").open("a")
    process = subprocess.Popen(["target/release/examples/benchmark", str(directory / "jobs.json"), str(directory / "results.jsonl"), f"{workers}"], stdout=log, stderr=log)
    output = Path("web/benchmark-results.json")
    while process.poll() is None:
        write_report(directory, output, None)
        time.sleep(10)
    log.close()
    write_report(directory, output, None)
    if process.returncode != 0:
        raise RuntimeError(f"benchmark failed with exit {process.returncode}; see {directory / 'run.log'}")
    rows = read_records(directory / "results.jsonl")
    groups = defaultdict(list)
    for row in rows:
        job = row["job"]
        groups[(job["family"], job["a_level"], job["b_level"])].append(row)
    if len(rows) != 4200 or len(groups) != 42 or any(len(games) != 100 or sum(game["job"]["a_color"] == "white" for game in games) != 50 for games in groups.values()):
        raise ValueError("final sample does not have 100 games / 50 per color in each pairing")
    protocol = json.loads((directory / "protocol.json").read_text())
    protocol["finished_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    (directory / "protocol.json").write_text(f"{json.dumps(protocol, indent=2)}\n")
    archive = Path("benchmarks/classical-2x2")
    write_report(directory, output, archive)
    (archive / "protocol.json").write_text(f"{json.dumps(protocol, indent=2)}\n")
    (archive / "summary.json").write_text(output.read_text())
    print(f"Completed {len(rows)} games; results: {output}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--workers", type=int, default=32)
    parser.add_argument("--wait-pilot", action="store_true")
    parser.add_argument("--pilot-workers", type=int, default=32)
    args = parser.parse_args()
    if not 1 <= args.workers <= 32 or not 1 <= args.pilot_workers <= 32:
        parser.error("concurrency must be between 1 and 32")
    run(args.directory, args.workers, args.wait_pilot, args.pilot_workers)

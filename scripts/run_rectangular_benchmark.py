"""Benchmark global and local policies on 4×2 and 4×4 grids."""

import argparse
import hashlib
import json
import os
import platform
import subprocess
import sys
import time
from collections import defaultdict
from pathlib import Path

from scripts.benchmark_results import read_records, write_report
from scripts.benchmark_presets import GLOBAL, LOCAL

GRIDS = ((4, 2), (4, 4))
PAIRINGS = ((1, 3), (3, 5), (5, 5))
FAMILIES = ("global", "local")
PILOT_PAIRS = 5
SCORED_PAIRS = 50


def utc() -> str:
    """Return the current UTC timestamp."""
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def jobs(pairs: int, phase: str, *, fallback: bool = False, material: bool = False, multipv: bool = False) -> list[dict]:
    """Return balanced jobs for the targeted policy comparisons."""
    return [
        {"id": f"{phase}-{columns}x{rows}-{family}-{a}-{b}-{pair}-{color}",
         "columns": columns, "rows": rows, "family": family, "a_level": a,
         "b_level": b, "pair": pair, "a_color": color, "opening": []}
        for pair in range(pairs) for columns, rows in GRIDS
        for family in (("fallback",) if fallback else ("material",) if material else ("multipv",) if multipv else FAMILIES)
        for a, b in (((0, 3), (0, 5)) if fallback else PAIRINGS)
        for color in ("white", "black")
    ]


def condition(job: dict) -> tuple:
    """Return the grid and policy pairing for a job."""
    return job["columns"], job["rows"], job["family"], job["a_level"], job["b_level"]


def prepare(directory: Path, binary: Path, workers: int, *, fallback: bool = False, material: bool = False, multipv: bool = False) -> None:
    """Prepare scored jobs after the determinism pilots finish."""
    pilot = read_records(directory / "pilot-results.jsonl")
    if len(pilot) != len(jobs(PILOT_PAIRS, "pilot", fallback=fallback, material=material, multipv=multipv)):
        raise ValueError("pilot sample is incomplete")
    groups = defaultdict(list)
    for row in pilot:
        groups[condition(row["job"])].append(row)
    decisions = {}
    conditions = []
    openings = {}
    for key, games in sorted(groups.items()):
        columns, rows, family, a, b = key
        traces = {}
        for color in ("white", "black"):
            colored = [game for game in games if game["job"]["a_color"] == color]
            if len(colored) != PILOT_PAIRS:
                raise ValueError("pilot requires five games per color")
            traces[color] = len({json.dumps(game["save"]["events"], sort_keys=True) for game in colored})
        decisions[key] = all(count == 1 for count in traces.values())
        conditions.append({"columns": columns, "rows": rows, "family": family,
                           "a_level": a, "b_level": b, "pilot_unique_traces": traces,
                           "forced_openings": decisions[key]})
        grid = f"{columns}x{rows}"
        if decisions[key] and grid not in openings:
            openings[grid] = json.loads(subprocess.check_output(
                ["node", "scripts/benchmark-openings.mjs", str(columns), str(rows)], text=True))
    scored = jobs(SCORED_PAIRS, "main", fallback=fallback, material=material, multipv=multipv)
    for job in scored:
        if decisions[condition(job)]:
            job["opening"] = openings[f"{job['columns']}x{job['rows']}"]["openings"][job["pair"]]
    (directory / "jobs.json").write_text(f"{json.dumps(scored)}\n")
    protocol = {
        "grids": [{"columns": columns, "rows": rows} for columns, rows in GRIDS],
        "start": "classical", "workers": workers, "pilot_workers": workers,
        "target_games": len(scored), "games_per_pairing": 100, "colors_per_pairing": 50,
        "pilot_games": len(pilot), "pilot_policy": "Five games per color; force a two-ply classical opening only if both colors repeat all five complete traces. Exclude pilots from scored results.",
        "conditions": conditions, "opening_pools": openings,
        "policy": "fallback-only" if fallback else "material-negamax" if material else "multipv-10" if multipv else "global-and-local",
        "global_presets": None if fallback else tuple({key: value for key, value in preset.items() if key != "analysis_nodes"} for preset in GLOBAL) if material else GLOBAL,
        "local_presets": LOCAL,
        "global_node_budget": None if fallback else 100000,
        "local_board_policy": "legal_boards[global_ply % legal_boards.len()]",
        "build_profile": "release", "started_utc": utc(), "logical_cpus": os.cpu_count(),
        "benchmark_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "search_sha256": hashlib.sha256(Path("crates/engine/src/search.rs").read_bytes()).hexdigest(),
        "platform": platform.platform(),
        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "node": subprocess.check_output(["node", "--version"], text=True).strip(),
        "fairy_stockfish": {"npm_version": "1.1.12", "source_revision": "b2e693ef1e111233ce3fb40685921708b3276ed6", "threads": 1, "nnue": False},
        "interval": "95% Bonferroni-Clopper-Pearson bounds on the color-paired outcome distribution, per condition.",
    }
    if fallback:
        protocol["fallback_policy"] = "Exact search::fallback_move: captured-piece value × 10 + promotion value; stable legal-action ties. King=1000, queen=9, rook=5, bishop/knight=3, pawn=1. No search or engine calls; a_level=0 is an ignored manifest placeholder, not a difficulty."
        protocol["reference_revision"] = "ad8aff7"
    if material:
        protocol["material_policy"] = "Existing global negamax/alpha-beta, move ordering, capture-priority fallback, CP-to-odds mapping and match-score DP. Leaf CP = White minus Black material: pawn 100, knight/bishop 300, rook 500, queen 900, king 0. No positional terms, local search or UCI engine for this player. Material calls are counted separately from engine calls."
        protocol["reference_revision"] = "95d9a2f"
    if multipv:
        protocol["candidate_count"] = 10
        protocol["candidate_policy"] = "Up to ten MultiPV moves per eligible board for both players, plus king captures and one capture-priority representative for missing routing regions. Native candidate analysis uses analysis_nodes × 10. All oracle calls count as engine_calls; candidate_evaluations is a subset. Existing DP, alpha-beta, deadlines and fallback unchanged."
        protocol["reference_revision"] = "01c3cef"
        protocol["uci_sha256"] = hashlib.sha256(Path("crates/engine/src/uci.rs").read_bytes()).hexdigest()
        protocol["uci_output_sha256"] = hashlib.sha256(Path("crates/engine/src/uci_output.rs").read_bytes()).hexdigest()
    (directory / "protocol.json").write_text(f"{json.dumps(protocol, indent=2)}\n")


def run(directory: Path, binary: Path, workers: int, *, fallback: bool = False, material: bool = False, multipv: bool = False) -> None:
    """Run resumable pilots and scored games, then archive complete results."""
    directory.mkdir(parents=True, exist_ok=True)
    study = "fallback" if fallback else "material" if material else "multipv" if multipv else "rectangular"
    policy = "fallback-only" if fallback else "material-negamax" if material else "multipv-10" if multipv else "global-and-local"
    output = Path(f"web/{study}-benchmark-results.json")
    archive = Path(f"benchmarks/{study}")
    monitor = subprocess.Popen([sys.executable, "scripts/monitor-benchmark.py", str(directory / "resources.jsonl"), str(directory)])
    try:
        if not (directory / "protocol.json").exists():
            manifest = directory / "pilot-jobs.json"
            manifest.write_text(f"{json.dumps(jobs(PILOT_PAIRS, 'pilot', fallback=fallback, material=material, multipv=multipv))}\n")
            with (directory / "pilot.log").open("a") as log:
                subprocess.run([str(binary), str(manifest), str(directory / "pilot-results.jsonl"), str(workers)], stdout=log, stderr=log, check=True)
            prepare(directory, binary, workers, fallback=fallback, material=material, multipv=multipv)
        protocol = json.loads((directory / "protocol.json").read_text())
        if protocol.get("policy", "global-and-local") != policy:
            raise ValueError("resume requires the same policy")
        if protocol["workers"] != workers or protocol["binary_sha256"] != hashlib.sha256(binary.read_bytes()).hexdigest():
            raise ValueError("resume requires the same release binary and concurrency")
        with (directory / "run.log").open("a") as log:
            process = subprocess.Popen([str(binary), str(directory / "jobs.json"), str(directory / "results.jsonl"), str(workers)], stdout=log, stderr=log)
            while process.poll() is None:
                write_report(directory, output, None)
                time.sleep(60)
        if process.returncode != 0:
            raise RuntimeError(f"benchmark exited {process.returncode}; see run.log")
        records = read_records(directory / "results.jsonl")
        expected = {job["id"] for job in jobs(SCORED_PAIRS, "main", fallback=fallback, material=material, multipv=multipv)}
        if {record["job"]["id"] for record in records} != expected or len(records) != len(expected):
            raise ValueError("scored sample is incomplete or duplicated")
        protocol["finished_utc"] = utc()
        (directory / "protocol.json").write_text(f"{json.dumps(protocol, indent=2)}\n")
        write_report(directory, output, archive)
        (archive / "protocol.json").write_text(f"{json.dumps(protocol, indent=2)}\n")
        (archive / "summary.json").write_text(output.read_text())
        print(f"Completed {len(records)} games: {output}", flush=True)
    finally:
        monitor.terminate()
        monitor.wait(timeout=10)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/benchmark"))
    parser.add_argument("--workers", type=int, default=24)
    policies = parser.add_mutually_exclusive_group()
    policies.add_argument("--fallback", action="store_true", help="Test the search fallback alone against Strong and Maximum local bots")
    policies.add_argument("--material", action="store_true", help="Test global negamax with material-only evaluation and no UCI engine")
    policies.add_argument("--multipv", action="store_true", help="Test ten-candidate selective search against local-only Fairy-Stockfish")
    args = parser.parse_args()
    if not 1 <= args.workers <= 32:
        parser.error("workers must be 1–32")
    run(args.directory.resolve(), args.binary.resolve(), args.workers, fallback=args.fallback, material=args.material, multipv=args.multipv)

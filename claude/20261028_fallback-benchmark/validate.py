"""Validate the complete fallback sample, archives, metrics, and build identity."""

import argparse
import gzip
import hashlib
import json
import math
from collections import Counter, defaultdict
from pathlib import Path


def validate(directory: Path, binary: Path) -> None:
    """Verify the complete 400-game sample against its manifests and archives."""
    root = Path(__file__).resolve().parents[2]
    protocol = json.loads((directory / "protocol.json").read_text())
    assert protocol["policy"] == "fallback-only"
    assert protocol["target_games"] == 400 and protocol["pilot_games"] == 40
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == protocol["binary_sha256"]
    assert hashlib.sha256((root / "crates/engine/src/search.rs").read_bytes()).hexdigest() == protocol["search_sha256"]
    for phase, manifest_name, journal_name, pairs_per_condition in (
        ("pilot", "pilot-jobs.json", "pilot-results.jsonl", 5),
        ("main", "jobs.json", "results.jsonl", 50),
    ):
        manifest = {job["id"]: job for job in json.loads((directory / manifest_name).read_text())}
        records = [json.loads(line) for line in (directory / journal_name).read_text().splitlines()]
        ids = Counter(record["job"]["id"] for record in records)
        assert set(ids) == set(manifest) and all(count == 1 for count in ids.values())
        assert len(records) == pairs_per_condition * 8
        colors = Counter()
        pairs = defaultdict(list)
        for record in records:
            job = record["job"]
            assert all(job[key] == value for key, value in manifest[job["id"]].items())
            assert job["family"] == "fallback" and job["a_level"] == 0
            assert job["columns"] == 4 and job["rows"] in (2, 4) and job["b_level"] in (3, 5)
            condition = job["rows"], job["b_level"]
            colors[(*condition, job["a_color"])] += 1
            pairs[(*condition, job["pair"])].append(record)
            assert record["outcome"] in ("white", "black", "draw")
            expected_score = .5 if record["outcome"] == "draw" else float(record["outcome"] == job["a_color"])
            assert record["a_score"] == expected_score
            for side in ("a", "b"):
                assert all(math.isfinite(value) and value >= 0 for key, value in record[side].items() if key != "depth_histogram")
            assert all(record["a"][key] == 0 for key in ("engine_calls", "search_nodes", "completed_depth_sum", "cutoffs", "cache_hits"))
        assert len(colors) == 8 and all(count == pairs_per_condition for count in colors.values())
        assert len(pairs) == pairs_per_condition * 4
        for pair in pairs.values():
            assert len(pair) == 2 and {record["job"]["a_color"] for record in pair} == {"white", "black"}
            assert pair[0]["job"]["opening"] == pair[1]["job"]["opening"]
        if phase == "main":
            archived = []
            for path in (root / "benchmarks/fallback").glob("4x[24]-*.jsonl.gz"):
                with gzip.open(path, "rt") as stream:
                    archived.extend(json.loads(line) for line in stream)
            assert sorted(archived, key=lambda row: row["job"]["id"]) == sorted(records, key=lambda row: row["job"]["id"])
        else:
            with gzip.open(root / "benchmarks/fallback/pilot.jsonl.gz", "rt") as stream:
                assert [json.loads(line) for line in stream] == records
    assert json.loads((root / "benchmarks/fallback/protocol.json").read_text()) == protocol
    assert (root / "benchmarks/fallback/summary.json").read_bytes() == (root / "web/fallback-benchmark-results.json").read_bytes()
    print("Validated 400 scored games, 40 pilots, balanced colors, archives, metrics, and release identity")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    validate(args.directory, args.binary)

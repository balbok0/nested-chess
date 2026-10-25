"""Record system and benchmark-process resource usage."""

import argparse
import json
import time
from pathlib import Path

import psutil


def monitor(output: Path, directory: str, interval: float) -> None:
    """Record resource samples for benchmark processes using the given directory."""
    previous_cpu = {}
    previous_time = time.monotonic()
    psutil.cpu_percent()
    with output.open("a") as stream:
        while True:
            processes = {}
            roots = []
            for process in psutil.process_iter(["pid", "name", "cmdline"]):
                if process.info["name"] == "benchmark" and any(directory in arg for arg in process.info["cmdline"] or []):
                    roots.append(process.pid)
                    try:
                        for child in [process, *process.children(recursive=True)]:
                            processes[child.pid] = child
                    except psutil.NoSuchProcess:
                        continue
            now = time.monotonic()
            cpu = {}
            rss = 0
            for pid, process in processes.items():
                try:
                    times = process.cpu_times()
                    cpu[pid] = times.user + times.system
                    rss += process.memory_info().rss
                except psutil.NoSuchProcess:
                    continue
            used_cpu = sum(max(0, value - previous_cpu.get(pid, 0)) for pid, value in cpu.items())
            memory = psutil.virtual_memory()
            row = {
                "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "system_cpu_percent": psutil.cpu_percent(),
                "system_memory_percent": memory.percent,
                "system_available_bytes": memory.available,
                "benchmark_pids": roots,
                "benchmark_processes": len(cpu),
                "benchmark_rss_bytes": rss,
                "benchmark_cpu_percent_of_machine": used_cpu / (now - previous_time) / psutil.cpu_count() * 100 if previous_cpu else None,
            }
            stream.write(f"{json.dumps(row)}\n")
            stream.flush()
            previous_cpu = cpu
            previous_time = now
            time.sleep(interval)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("directory")
    parser.add_argument("--interval", type=float, default=5)
    args = parser.parse_args()
    if args.interval <= 0:
        parser.error("interval must be positive")
    monitor(args.output, args.directory, args.interval)

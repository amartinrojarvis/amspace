#!/usr/bin/env python3
"""Read-only CPU/RSS sample; run with settings closed and the mouse at rest."""
import argparse
import json
import subprocess
import time


def sample(pid):
    line = subprocess.check_output(["ps", "-p", str(pid), "-o", "time=,rss="], text=True).strip()
    cpu, rss = line.split()
    parts = cpu.replace("-", ":").split(":")
    seconds = sum(float(p) * unit for p, unit in zip(reversed(parts), [1, 60, 3600, 86400]))
    return seconds, int(rss) / 1024


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("pid", type=int)
parser.add_argument("--seconds", type=int, default=20)
args = parser.parse_args()
if not 1 <= args.seconds <= 300:
    parser.error("--seconds must be between 1 and 300")
start_cpu, start_rss = sample(args.pid)
start = time.monotonic()
rss = [start_rss]
for _ in range(args.seconds):
    time.sleep(1)
    end_cpu, memory = sample(args.pid)
    rss.append(memory)
elapsed = time.monotonic() - start
print(json.dumps({"pid": args.pid, "seconds": round(elapsed, 2),
    "cpu_percent_one_core": round(100 * (end_cpu - start_cpu) / elapsed, 3),
    "rss_mib_min": round(min(rss), 2), "rss_mib_max": round(max(rss), 2)}, indent=2))

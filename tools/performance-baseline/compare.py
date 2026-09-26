#!/usr/bin/env python3
"""Compare JSONL baselines without hiding timeout, OOM, or failed cases."""

import json
import pathlib
import sys


def load(path: str):
    rows = {}
    for line in pathlib.Path(path).read_text(encoding="utf-8").splitlines():
        row = json.loads(line)
        if row.get("record_type") == "environment":
            continue
        key = (row.get("case"), row.get("order"), row.get("retention"))
        rows[key] = row
    return rows


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: compare.py BASELINE.jsonl CANDIDATE.jsonl", file=sys.stderr)
        return 2
    baseline, candidate = map(load, sys.argv[1:])
    print("case\torder\tretention\tbaseline\tcandidate\twall_ratio")
    for key in sorted(set(baseline) | set(candidate)):
        left = baseline.get(key, {"status": "missing"})
        right = candidate.get(key, {"status": "missing"})
        ratio = "n/a"
        if left.get("status") == right.get("status") == "ok":
            ratio = f"{right['wall_seconds'] / left['wall_seconds']:.4f}"
        print("\t".join((*map(str, key), left["status"], right["status"], ratio)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())


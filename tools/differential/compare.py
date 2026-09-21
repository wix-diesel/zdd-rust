#!/usr/bin/env python3
"""Compare canonical external-adapter output with the committed fixtures."""

from pathlib import Path
import sys


def expected(path: Path) -> dict[str, set[int]]:
    cases: dict[str, set[int]] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        cases[fields[0]] = {int(value, 16) for value in fields[-1].split(",") if value}
    return cases


def actual(path: Path) -> dict[str, set[int]]:
    cases: dict[str, set[int]] = {}
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 2:
            raise ValueError(f"{path}:{number}: expected CASE<TAB>HEX_MASKS")
        cases[fields[0]] = {int(value, 16) for value in fields[1].split(",") if value}
    return cases


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: compare.py ADAPTER_OUTPUT", file=sys.stderr)
        return 2
    fixture_path = Path(__file__).with_name("fixtures.tsv")
    wanted = expected(fixture_path)
    observed = actual(Path(sys.argv[1]))
    if wanted == observed:
        print(f"all {len(wanted)} cases match")
        return 0
    for name in sorted(wanted.keys() | observed.keys()):
        if wanted.get(name) != observed.get(name):
            print(f"{name}: expected={wanted.get(name)} actual={observed.get(name)}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Print the nextest filterset that names the quarantined tests (.config/quarantine.toml).

CI's test run excludes them and a later step runs only them, never failing the
job (ci.yml). With nothing quarantined it prints `none()`. Run locally with:

    cargo nextest run -p ferrite-cockpit -E "$(python3 scripts/quarantine.py)"
"""

import argparse
from pathlib import Path
import re
import sys
import tomllib
from typing import TextIO

ROOT = Path(__file__).resolve().parents[1]
LIST = ROOT / ".config" / "quarantine.toml"
FIELDS = {"binary", "test", "reason", "owner"}
# A test binary as nextest names it: `crate`, `crate::target`, or
# `crate::bin/target` (also `bench/`, `example/`).
BINARY = re.compile(r"[A-Za-z0-9_-]+(::((bin|bench|example)/)?[A-Za-z0-9_-]+)?")
TEST = re.compile(r"[A-Za-z0-9_]+(::[A-Za-z0-9_]+)*")


def quarantined(path: Path = LIST) -> list[tuple[str, str]]:
    """Each quarantined test as (binary, test), once every entry has both, a reason and an owner."""
    entries = tomllib.loads(path.read_text(encoding="utf-8")).get("test", [])
    tests = []
    for entry in entries:
        if set(entry) != FIELDS or not all(isinstance(value, str) and value.strip() for value in entry.values()):
            raise ValueError(f"{path.name}: each [[test]] has exactly a binary, a test, a reason and an owner: {entry}")
        if not BINARY.fullmatch(entry["binary"]):
            raise ValueError(f"{path.name}: not a nextest binary id: {entry['binary']!r}")
        if not TEST.fullmatch(entry["test"]):
            raise ValueError(f"{path.name}: not a test name: {entry['test']!r}")
        test = (entry["binary"], entry["test"])
        if test in tests:
            raise ValueError(f"{path.name}: {' '.join(test)} is listed twice")
        tests.append(test)
    return tests


def filterset(tests: list[tuple[str, str]]) -> str:
    return " | ".join(f"(binary_id(={binary}) & test(={test}))" for binary, test in tests) or "none()"


def main(argv: list[str] | None = None, out: TextIO = sys.stdout) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--list", type=Path, default=LIST)
    args = parser.parse_args(argv)
    try:
        tests = quarantined(args.list)
    except ValueError as invalid:
        print(invalid, file=sys.stderr)
        return 1
    print(filterset(tests), file=out)
    return 0


if __name__ == "__main__":
    sys.exit(main())

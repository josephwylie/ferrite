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
FIELDS = {"name", "reason", "owner"}
TEST_NAME = re.compile(r"[A-Za-z0-9_]+(::[A-Za-z0-9_]+)*")


def quarantined(path: Path = LIST) -> list[str]:
    """The quarantined tests' names, once each entry has a name, a reason and an owner."""
    entries = tomllib.loads(path.read_text(encoding="utf-8")).get("test", [])
    names = []
    for entry in entries:
        if set(entry) != FIELDS or not all(isinstance(value, str) and value.strip() for value in entry.values()):
            raise ValueError(f"{path.name}: each [[test]] has exactly a name, a reason and an owner: {entry}")
        if not TEST_NAME.fullmatch(entry["name"]):
            raise ValueError(f"{path.name}: not a test name: {entry['name']!r}")
        if entry["name"] in names:
            raise ValueError(f"{path.name}: {entry['name']} is listed twice")
        names.append(entry["name"])
    return names


def filterset(names: list[str]) -> str:
    return " | ".join(f"test(={name})" for name in names) or "none()"


def main(argv: list[str] | None = None, out: TextIO = sys.stdout) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--list", type=Path, default=LIST)
    args = parser.parse_args(argv)
    try:
        names = quarantined(args.list)
    except ValueError as invalid:
        print(invalid, file=sys.stderr)
        return 1
    print(filterset(names), file=out)
    return 0


if __name__ == "__main__":
    sys.exit(main())

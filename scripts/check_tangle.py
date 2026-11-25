#!/usr/bin/env python3
"""Hold the root view's tangle to a baseline that only goes down.

Counts how much of the window still runs through its root view,
`CockpitView`, and compares the counts with scripts/tangle.json. While the
window is untangled into feature crates they may only fall. Run locally with:

    python3 scripts/check_tangle.py

Add --write to record the counts as the new baseline.
"""

import argparse
import json
from pathlib import Path
import re
import sys
from typing import Callable, TextIO

from quarantine import quarantined

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "scripts" / "tangle.json"
ROOT_FILE = "crates/ferrite-cockpit/src/cockpit.rs"
COMMENT = re.compile(r"//[^\n]*")


def sources(root: Path) -> dict[str, str]:
    return {path.relative_to(root).as_posix(): path.read_text(encoding="utf-8")
            for path in sorted((root / "crates").glob("**/*.rs"))}


def files_matching(pattern: str) -> Callable[[Path, dict[str, str]], int]:
    regex = re.compile(pattern, re.MULTILINE)
    return lambda root, texts: sum(1 for text in texts.values() if regex.search(text))


def calls(pattern: str) -> Callable[[Path, dict[str, str]], int]:
    regex = re.compile(pattern)
    return lambda root, texts: sum(len(regex.findall(text)) for text in texts.values())


def root_fields(root: Path, texts: dict[str, str]) -> int:
    """`CockpitView`'s fields: its body's top-level items, comments and attributes aside."""
    text = COMMENT.sub("", texts[ROOT_FILE])
    start = re.search(r"\bstruct\s+CockpitView\s*\{", text).end()
    depth, item, fields = 0, "", 0
    for at, char in enumerate(text[start:], start):
        if char in "{([<":
            depth += 1
        elif char in "})]" or (char == ">" and text[at - 1] != "-"):
            if depth == 0:
                break
            depth -= 1
        if char == "," and depth == 0:
            fields += bool(item.strip())
            item = ""
        else:
            item += char
    return fields + bool(item.strip())


def root_lines(root: Path, texts: dict[str, str]) -> int:
    return texts[ROOT_FILE].count("\n")


def quarantined_tests(root: Path, texts: dict[str, str]) -> int:
    return len(quarantined(root / ".config" / "quarantine.toml"))


QUALIFIED = r"(?:\w+::)*CockpitView"
COUNTS: dict[str, Callable[[Path, dict[str, str]], int]] = {
    "files holding WeakEntity<CockpitView>": files_matching(rf"WeakEntity<\s*{QUALIFIED}\s*>"),
    "files with impl CockpitView": files_matching(rf"^\s*impl\s+{QUALIFIED}\s*\{{"),
    "CockpitView fields": root_fields,
    "cockpit.rs lines": root_lines,
    "add_cockpit_window calls": calls(r"(?<!fn )\badd_cockpit_window\s*\("),
    "quarantined tests": quarantined_tests,
}


def count(root: Path) -> dict[str, int]:
    texts = sources(root)
    return {name: counter(root, texts) for name, counter in COUNTS.items()}


def check(found: dict[str, int], baseline: dict[str, int], out: TextIO) -> bool:
    over = [name for name in COUNTS if found[name] > baseline.get(name, 0)]
    lower = [name for name in COUNTS if found[name] < baseline.get(name, 0)]
    for name in COUNTS:
        print(f"{name}: {found[name]}, baseline {baseline.get(name, 0)}.", file=out)
    if over:
        print(
            f"Rose: {', '.join(over)}. While the window is untangled these only fall: new code takes plain "
            "values in a view of its own, outside cockpit.rs, and a test that needs no window opens none. "
            "A rise that cannot be avoided raises the baseline: rerun with --write in this change and say why.",
            file=out,
        )
    if lower:
        print(f"Fell: {', '.join(lower)}. Lower the baseline with --write and commit scripts/tangle.json.", file=out)
    return not over


def main(argv: list[str] | None = None, out: TextIO = sys.stdout) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--baseline", type=Path, default=BASELINE)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--write", action="store_true", help="Record the counts as the baseline")
    args = parser.parse_args(argv)
    found = count(args.root.resolve())
    if args.write:
        args.baseline.write_text(json.dumps(found, indent=2) + "\n", newline="\n")
        print(f"Wrote {args.baseline.name}: {found}", file=out)
        return 0
    return 0 if check(found, json.loads(args.baseline.read_text()), out) else 1


if __name__ == "__main__":
    sys.exit(main())

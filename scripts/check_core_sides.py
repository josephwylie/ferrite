#!/usr/bin/env python3
"""Hold what the window's side names of the Engine's side to a baseline that only goes down (docs/adr/0049).

scripts/core-sides.json puts every top-level module of ferrite-core on one of
two sides: `shared`, what a window and a Server both speak, or `engine`, what
only a Server runs. A crossing is a path to an `engine` module from a `shared`
module, or from one of the window's crates. Each is counted per pair and
compared with the baseline. Tests and fixtures are not counted. Run with:

    python3 scripts/check_core_sides.py

Add --write to record the counts read as the new baseline.
"""

import argparse
from collections import Counter
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
SIDES = ROOT / "scripts" / "core-sides.json"
ADR = "docs/adr/0049-ferrite-is-a-protocol-an-engine-and-a-client.md"
CORE = "ferrite-core"
INLINE_TESTS = re.compile(r"^#\[cfg\(test\)\]\n(?:pub(?:\([a-z]+\))? )?mod \w+ \{.*?^\}\n", re.M | re.S)
COMMENT = re.compile(r"^\s*//.*$", re.M)
NAME = re.compile(r"\s*([a-z_][a-z0-9_]*)")


def is_test(path: Path) -> bool:
    return any(
        part == "tests" or part == "test_temp" or part.endswith("_tests") or "fixture" in part
        for part in (*path.parts[:-1], path.stem)
    )


def named(source: str, root: str) -> list[str]:
    """Each module of `root` a source names: `root::a`, and `a` and `b` of `root::{a::X, b}`."""
    found = []
    for match in re.finditer(rf"\b{root}::", source):
        at = match.end()
        if not source.startswith("{", at):
            if name := NAME.match(source, at):
                found.append(name.group(1))
            continue
        depth, entry = 0, at + 1
        for index in range(at, len(source)):
            char = source[index]
            if char in "{}":
                depth += 1 if char == "{" else -1
            if (char == "," and depth == 1) or depth == 0:
                if name := NAME.match(source, entry):
                    found.append(name.group(1))
                entry = index + 1
            if depth == 0:
                break
    return found


def crossings(root: Path, engine: set[str], window: list[str]) -> Counter:
    found = Counter()
    for crate in [CORE, *window]:
        src = root / "crates" / crate / "src"
        for path in sorted(src.rglob("*.rs")):
            relative = path.relative_to(src)
            if is_test(relative):
                continue
            owner = crate
            if crate == CORE:
                owner = relative.parts[0].removesuffix(".rs")
                if owner == "lib" or owner in engine:
                    continue
            source = COMMENT.sub("", INLINE_TESTS.sub("", path.read_text(encoding="utf-8")))
            for module in named(source, "crate" if crate == CORE else "ferrite_core"):
                if module in engine:
                    found[f"{owner} -> {module}"] += 1
    return found


def main(argv: list[str], root: Path = ROOT, sides_file: Path = SIDES, out=sys.stdout) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--write", action="store_true", help="record the counts read as the baseline")
    args = parser.parse_args(argv)

    config = json.loads(sides_file.read_text(encoding="utf-8"))
    shared, engine = set(config["shared"]), set(config["engine"])
    core = root / "crates" / CORE / "src"
    present = {entry.stem for entry in core.iterdir() if entry.stem != "lib" and (entry.is_dir() or entry.suffix == ".rs")}
    unplaced, gone = sorted(present - shared - engine), sorted((shared | engine) - present)
    for module in unplaced:
        print(f"{module}: is on no side. A window needs it: `shared`. Only a Server runs it: `engine` ({sides_file.name}, {ADR}).", file=out)
    for module in gone:
        print(f"{module}: is in {sides_file.name} but not in {CORE}. Remove it.", file=out)
    if unplaced or gone:
        return 1

    found = crossings(root, engine, config["window"])
    if args.write:
        config["crossings"] = dict(sorted(found.items()))
        sides_file.write_text(json.dumps(config, indent=1) + "\n", encoding="utf-8")
        print(f"Recorded {sum(found.values())} crossings over {len(found)} pairs.", file=out)
        return 0

    baseline = config["crossings"]
    worse = {pair: count for pair, count in found.items() if count > baseline.get(pair, 0)}
    stale = {pair: count for pair, count in baseline.items() if found.get(pair, 0) < count}
    for pair, count in sorted(worse.items()):
        owner, module = pair.split(" -> ")
        print(
            f"{pair}: {count} crossings, the baseline allows {baseline.get(pair, 0)}. `{module}` is the Engine's side "
            f"and `{owner}` is not. If `{owner}` needs a type from it, move the type to a shared module; "
            f"if it needs work done, ask the Server over the protocol ({ADR}).",
            file=out,
        )
    for pair, count in sorted(stale.items()):
        print(f"{pair}: down to {found.get(pair, 0)} from {count}. Record it with --write.", file=out)
    if worse or stale:
        return 1
    print(f"{sum(found.values())} crossings over {len(found)} pairs, none new.", file=out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

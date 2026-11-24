#!/usr/bin/env python3
"""Hold ferrite-core's upward references to a baseline that only goes down (docs/adr/0049).

scripts/core-layers.json puts every top-level module of ferrite-core in a
layer, lowest first. A module may name anything in its own layer or below. A
`crate::<module>` path that names a higher layer is an upward reference: each
one is counted per pair of modules and compared with the baseline. Tests are
not counted. Run with:

    python3 scripts/check_core_layers.py

Add --write to record the counts read as the new baseline.
"""

import argparse
from collections import Counter
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "crates" / "ferrite-core" / "src"
LAYERS = ROOT / "scripts" / "core-layers.json"
ADR = "docs/adr/0049-core-is-layers-and-the-window-model-is-its-own-crate.md"
PATH = re.compile(r"\bcrate::([a-z_][a-z0-9_]*)")
INLINE_TESTS = re.compile(r"^#\[cfg\(test\)\]\n(?:pub(?:\([a-z]+\))? )?mod \w+ \{.*?^\}\n", re.M | re.S)
COMMENT = re.compile(r"^\s*//.*$", re.M)


def is_test(path: Path) -> bool:
    return path.stem == "tests" or path.stem.endswith("_tests") or "tests" in path.parts or path.stem == "test_temp"


def modules(core: Path) -> set[str]:
    return {entry.stem for entry in core.iterdir() if entry.stem != "lib" and (entry.is_dir() or entry.suffix == ".rs")}


def upward(core: Path, layer_of: dict[str, int]) -> Counter:
    found = Counter()
    for path in sorted(core.rglob("*.rs")):
        relative = path.relative_to(core)
        owner = relative.parts[0].removesuffix(".rs")
        if owner == "lib" or is_test(relative):
            continue
        source = COMMENT.sub("", INLINE_TESTS.sub("", path.read_text(encoding="utf-8")))
        for named in PATH.findall(source):
            if named in layer_of and layer_of[named] > layer_of[owner]:
                found[f"{owner} -> {named}"] += 1
    return found


def main(argv: list[str], core: Path = CORE, layers_file: Path = LAYERS, out=sys.stdout) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--write", action="store_true", help="record the counts read as the baseline")
    args = parser.parse_args(argv)

    config = json.loads(layers_file.read_text(encoding="utf-8"))
    layer_of = {module: depth for depth, layer in enumerate(config["layers"]) for module in layer["modules"]}
    present = modules(core)
    unplaced = sorted(present - layer_of.keys())
    gone = sorted(layer_of.keys() - present)
    if unplaced or gone:
        for module in unplaced:
            print(f"{module}: has no layer. Put it in the lowest layer of {layers_file.name} it compiles in ({ADR}).", file=out)
        for module in gone:
            print(f"{module}: is in {layers_file.name} but not in ferrite-core. Remove it.", file=out)
        return 1

    found = upward(core, layer_of)
    if args.write:
        config["upward"] = dict(sorted(found.items()))
        layers_file.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
        print(f"Recorded {sum(found.values())} upward references over {len(found)} pairs.", file=out)
        return 0

    baseline = config["upward"]
    worse = {pair: count for pair, count in found.items() if count > baseline.get(pair, 0)}
    stale = {pair: count for pair, count in baseline.items() if found.get(pair, 0) < count}
    for pair, count in sorted(worse.items()):
        lower, higher = pair.split(" -> ")
        print(
            f"{pair}: {count} upward references, the baseline allows {baseline.get(pair, 0)}. "
            f"`{lower}` is in a lower layer than `{higher}`: move what it needs down, or take it as an argument ({ADR}).",
            file=out,
        )
    for pair, count in sorted(stale.items()):
        print(f"{pair}: down to {found.get(pair, 0)} from {count}. Record it with --write.", file=out)
    if worse or stale:
        return 1
    print(f"{sum(found.values())} upward references over {len(found)} pairs, none new.", file=out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

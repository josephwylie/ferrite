#!/usr/bin/env python3
"""Hold what the window's side names of the Engine's side to a baseline that only goes down (docs/adr/0049).

scripts/core-sides.json puts every top-level module of ferrite-core on one of
two sides: `shared`, what a window and a Server both speak, or `engine`, what
only a Server runs. A crossing is a path to an `engine` module from a `shared`
module, or from one of the window's crates. Each is counted per pair and
compared with the baseline. Run with:

    python3 scripts/check_core_sides.py

Add --write to record the counts read as the new baseline.

What a released app cannot compile is not counted: a module whose `mod` line,
or an ancestor's, has a `cfg` that fails with `test` off and only the
`shipped` features on, and an inline `#[cfg(test)] mod` block. A file's name
says nothing. A `pub use` counts once for each engine item it hands on, since
every crate above can then name that item without naming the engine.
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
MOD = re.compile(r"((?:^[ \t]*(?:#\[[^\n]*\]|//[^\n]*)\n)*)^[ \t]*(?:pub(?:\([^)\n]*\))?[ \t]+)?mod[ \t]+(\w+)[ \t]*;", re.M)
PATH = re.compile(r'#\[path\s*=\s*"([^"]+)"\]')
CFG = re.compile(r"#\[cfg\((.*)\)\]")
TOKEN = re.compile(r'\s*(?:(\w+)\s*=\s*"([^"]*)"|(\w+)|([(),]))')
PUB_USE = re.compile(r"^[ \t]*pub use ((?:crate|ferrite_core)::[^;]*);", re.M)
LEAF = re.compile(r"(\w+)\s*(?=[,}]|$)")


def holds(cfg: str, shipped: set[str]) -> bool | None:
    """Whether `cfg` holds in a released app: `test` off, only `shipped` features on. None where it
    rests on what this cannot tell, such as the platform."""
    tokens = [next(group for group in match.groups() if group is not None) if match.group(1) is None
              else (match.group(1), match.group(2)) for match in TOKEN.finditer(cfg)]
    at = 0

    def term() -> bool | None:
        nonlocal at
        token = tokens[at]
        at += 1
        if isinstance(token, tuple):
            return token[1] in shipped if token[0] == "feature" else None
        if token in ("any", "all", "not"):
            at += 1
            parts = []
            while tokens[at] != ")":
                parts.append(term())
                if tokens[at] == ",":
                    at += 1
            at += 1
            if token == "not":
                return None if parts[0] is None else not parts[0]
            decisive = token == "any"
            if decisive in parts:
                return decisive
            return None if None in parts else not decisive
        return False if token == "test" else None

    return term()


def declared(file: Path, name: str, attributes: str) -> Path | None:
    """The file `mod name;` in `file` brings in."""
    if path := PATH.search(attributes):
        return (file.parent / path.group(1)).resolve()
    base = file.parent if file.name in ("lib.rs", "main.rs", "mod.rs") else file.with_suffix("")
    for candidate in (base / f"{name}.rs", base / name / "mod.rs"):
        if candidate.exists():
            return candidate.resolve()
    return None


def unshipped(src: Path, shipped: set[str]) -> set[Path]:
    """The files a released app never compiles: reached from `src/lib.rs` only through a `mod`
    line whose `cfg` fails there. A file no `mod` line reaches is counted."""
    reached: dict[Path, bool] = {}

    def walk(file: Path, ships: bool):
        if reached.get(file) is True or (file in reached and not ships):
            return
        reached[file] = ships
        for attributes, name in MOD.findall(file.read_text(encoding="utf-8")):
            if child := declared(file, name, attributes):
                fails = any(holds(cfg, shipped) is False for cfg in CFG.findall(attributes))
                walk(child, ships and not fails)

    lib = src / "lib.rs"
    if lib.exists():
        walk(lib.resolve(), True)
    return {path for path, ships in reached.items() if not ships}


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


def handed_on(tree: str) -> list[tuple[str, int]]:
    """Each module of a `pub use` tree (`root::a::{X, Y}`, `root::{a::X, b::Y}`) with the items it hands on."""
    tree = tree.split("::", 1)[1].strip()
    entries = [tree]
    if tree.startswith("{"):
        entries, depth, entry = [], 0, ""
        for char in tree[1:-1]:
            depth += {"{": 1, "}": -1}.get(char, 0)
            if char == "," and depth == 0:
                entries.append(entry)
                entry = ""
            else:
                entry += char
        entries.append(entry)
    handed = []
    for entry in filter(str.strip, entries):
        module, _, rest = entry.strip().partition("::")
        handed.append((module, max(1, len(LEAF.findall(rest)))))
    return handed


def crossings(root: Path, engine: set[str], window: list[str], shipped: set[str]) -> Counter:
    found = Counter()
    for crate in [CORE, *window]:
        src = root / "crates" / crate / "src"
        skipped = unshipped(src, shipped)
        prefix = "crate" if crate == CORE else "ferrite_core"
        for path in sorted(src.rglob("*.rs")):
            relative = path.relative_to(src)
            if path.resolve() in skipped:
                continue
            owner = crate
            if crate == CORE:
                owner = relative.parts[0].removesuffix(".rs")
                if owner == "lib" or owner in engine:
                    continue
            source = COMMENT.sub("", INLINE_TESTS.sub("", path.read_text(encoding="utf-8")))
            for tree in PUB_USE.findall(source):
                for module, items in handed_on(tree):
                    if module in engine:
                        found[f"{owner} -> {module}"] += items
            for module in named(PUB_USE.sub("", source), prefix):
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

    found = crossings(root, engine, config["window"], set(config["shipped"]))
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

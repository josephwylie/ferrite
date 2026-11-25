#!/usr/bin/env python3
"""Keep each feature crate off the shell, on Cargo's resolved graph.

A feature crate builds and tests without the window: in its own build or its
own tests it may not reach the shell (`ferrite-cockpit`), the Engine, the
Server or another feature crate. Cargo decides what it reaches, as
`cargo tree -p <crate>` resolves it on every target with all its features:
the crate's own dev-dependencies count, its dependencies' do not. A feature
crate that does not exist yet passes. Run locally with:

    python3 scripts/check_crate_graph.py
"""

import argparse
import json
from pathlib import Path
import subprocess
import sys
from typing import TextIO

ROOT = Path(__file__).resolve().parents[1]
FEATURE_CRATES = ("ferrite-review", "ferrite-factory", "ferrite-tools", "ferrite-nav", "ferrite-settings", "ferrite-prs")
SHELL = ("ferrite-cockpit", "ferrite-engine", "ferrite-server")
TREE = ("tree", "--locked", "--edges", "normal,build,dev", "--target", "all", "--all-features")


def cargo(root: Path, *args: str) -> str:
    return subprocess.run(["cargo", *args], cwd=root, check=True, stdout=subprocess.PIPE, text=True).stdout


def reached(root: Path, crate: str) -> set[str]:
    listed = cargo(root, *TREE, "--package", crate, "--prefix", "none", "--format", "{p}")
    return {line.split()[0] for line in listed.splitlines() if line}


def check(root: Path, out: TextIO) -> bool:
    metadata = json.loads(cargo(root, "metadata", "--locked", "--no-deps", "--format-version", "1"))
    workspace = {package["name"] for package in metadata["packages"]}
    missing = [crate for crate in SHELL if crate not in workspace]
    if missing:
        print(f"Not in the workspace, so nothing would be checked against it: {', '.join(missing)}", file=out)
        return False
    # Walk from the members, but forbid every feature crate's name: one
    # outside the workspace is still not a crate a feature may reach.
    present = [crate for crate in FEATURE_CRATES if crate in workspace]
    forbidden = set(SHELL) | set(FEATURE_CRATES)
    found = [(crate, other) for crate in present for other in sorted(reached(root, crate) & (forbidden - {crate}))]
    for crate, other in found:
        print(f"{crate} reaches {other}, which a feature crate may not:", file=out)
        print(cargo(root, *TREE, "--package", crate, "--invert", other), file=out)
    if not found:
        print(f"Feature crates checked: {', '.join(present) or 'none yet'}.", file=out)
    return not found


def main(argv: list[str] | None = None, out: TextIO = sys.stdout) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    return 0 if check(args.root.resolve(), out) else 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Select verification from workspace dependencies, without resolving or building Cargo.

Run locally with `python3 scripts/ci_scope.py --base main` for changed packages,
or without --base to check the boundaries and select the whole workspace.
"""

import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import re
import subprocess
import tomllib


@dataclass(frozen=True)
class Package:
    directory: str
    dependencies: frozenset[str]


def workspace(root: Path) -> dict[str, Package]:
    root = root.resolve()
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    config = manifest["workspace"]
    inherited = config.get("dependencies", {})
    packages = {}
    excluded = {path.resolve() for pattern in config.get("exclude", []) for path in root.glob(pattern)}
    directories = set()
    for pattern in config["members"]:
        members = {path.resolve() for path in root.glob(pattern)} - excluded
        if not members:
            raise ValueError(f"Workspace member does not exist: {pattern}")
        directories.update(members)
    for directory in sorted(directories):
        data = tomllib.loads((directory / "Cargo.toml").read_text())
        name = data["package"]["name"]
        if not re.fullmatch(r"[A-Za-z0-9_-]+", name) or name in packages:
            raise ValueError(f"Invalid or duplicate workspace package: {name}")
        dependencies = set()
        for table in [data, *data.get("target", {}).values()]:
            for kind in ("dependencies", "dev-dependencies", "build-dependencies"):
                for alias, value in table.get(kind, {}).items():
                    dependency_root = directory
                    if isinstance(value, dict) and value.get("workspace"):
                        value = inherited[alias]
                        dependency_root = root
                    if isinstance(value, dict) and "path" in value:
                        dependency_directory = (dependency_root / value["path"]).resolve()
                        if dependency_directory.is_relative_to(root) and dependency_directory not in directories:
                            relative = dependency_directory.relative_to(root).as_posix()
                            raise ValueError(
                                f"Untracked in-tree path dependency: {name} -> {alias} ({relative}). "
                                "Add its directory to workspace.members and remove any workspace.exclude "
                                "entry, or extend the selector to track it before using this dependency."
                            )
                    dependencies.add(value.get("package", alias) if isinstance(value, dict) else alias)
        packages[name] = Package(directory.relative_to(root).as_posix(), frozenset(dependencies))
    if not packages:
        raise ValueError("Workspace has no packages")
    return packages


def check_boundaries(packages: dict[str, Package]) -> None:
    for source in ("ferrite-core", "ferrite-frame"):
        if source not in packages:
            raise ValueError(f"Required architectural boundary owner is missing: {source}")
        pending = [(source,)]
        visited = set()
        while pending:
            chain = pending.pop()
            name = chain[-1]
            forbidden = (name == "gpui" or name.startswith("gpui-")) if source == "ferrite-core" else name == "ferrite-cockpit"
            if forbidden:
                raise ValueError(f"Forbidden dependency: {' -> '.join(chain)}")
            if name in visited:
                continue
            visited.add(name)
            if name in packages:
                pending.extend((*chain, dep) for dep in sorted(packages[name].dependencies))


# These inputs affect packaging or dependency resolution outside workspace edges.
SHARED_INPUTS = ("vendor/", "scripts/", "assets/", "resources/", ".cargo/", ".config/", ".github/workflows/")
SHARED_FILES = {"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml"}


def select(packages: dict[str, Package], paths: list[str], full: bool = False) -> dict:
    check_boundaries(packages)
    selected = set()
    reasons = set()
    all_packages = full or not paths
    if all_packages:
        reasons.add("full run or unavailable change list")
    for path in paths:
        # Crate fixtures and vendored Markdown can be embedded at compile time.
        owners = {name for name, package in packages.items() if path.startswith(package.directory + "/")}
        if owners:
            selected.update(owners)
            continue
        if path in SHARED_FILES or path.startswith(SHARED_INPUTS):
            all_packages = True
            reasons.add(f"shared build or packaging input: {path}")
        elif path.startswith("docs/") or ("/" not in path and path.endswith(".md")):
            continue
        else:
            all_packages = True
            reasons.add(f"unclassified input: {path}")
    if all_packages:
        selected = set(packages)
    else:
        while True:
            dependants = {name for name, package in packages.items() if package.dependencies & selected}
            expanded = selected | dependants
            if expanded == selected:
                break
            selected = expanded
    return {
        "code": bool(selected),
        "browser": "ferrite" in selected or all_packages,
        "full": full,
        "packages": sorted(selected),
        "decisions": {name: name in selected for name in sorted(packages)},
        "reasons": sorted(reasons) or ["workspace dependency closure" if selected else "documentation only"],
    }


def changed_paths(root: Path, base: str, head: str) -> list[str]:
    result = subprocess.run(
        ["git", "diff", "--name-only", "--no-renames", "-z", f"{base}...{head}", "--"],
        cwd=root, check=True, stdout=subprocess.PIPE,
    )
    paths = result.stdout.decode().split("\0")
    return [path for path in paths if path]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--base", help="Compare against this Git revision's merge base")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--full", action="store_true", help="Select every package and Windows CI")
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    if (args.base and args.base.startswith("-")) or args.head.startswith("-"):
        parser.error("Git revisions cannot start with a dash")
    root = args.root.resolve()
    paths = changed_paths(root, args.base, args.head) if args.base else []
    result = select(workspace(root), paths, full=args.full or not args.base)
    print(json.dumps(result, indent=2))
    if args.github_output:
        with args.github_output.open("a") as output:
            for name in ("code", "browser", "full"):
                output.write(f"{name}={str(result[name]).lower()}\n")
            output.write(f"packages={' '.join(result['packages'])}\n")


if __name__ == "__main__":
    main()

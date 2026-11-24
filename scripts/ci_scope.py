#!/usr/bin/env python3
"""Select verification from workspace dependencies, without resolving or building Cargo.

Run locally with `python3 scripts/ci_scope.py --base main` for changed packages,
or without --base to check the boundaries and select the whole workspace.
"""

import argparse
from dataclasses import dataclass, field
import json
from pathlib import Path
import re
import subprocess
import tomllib


@dataclass(frozen=True)
class Edge:
    """A normal or build dependency, as its manifest names it."""
    package: str
    optional: bool = False
    default_features: bool = True
    features: frozenset[str] = frozenset()


# The features a release build turns on, where a package has them: its
# defaults, and the Browser's engine (scripts/install-app.sh,
# scripts/package-windows.ps1).
SHIPPED = frozenset({"default", "cef"})


@dataclass(frozen=True)
class Package:
    directory: str
    dependencies: frozenset[str]
    # What its own build links: every edge but a dev-dependency.
    runtime: frozenset[str] = frozenset()
    # Its normal and build edges by the name its manifest gives each, and its
    # features, for what a release build links (`shipped`).
    edges: dict[str, Edge] = field(default_factory=dict)
    features: dict[str, list[str]] = field(default_factory=dict)


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
        runtime = set()
        edges = {}
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
                    dependency = value.get("package", alias) if isinstance(value, dict) else alias
                    dependencies.add(dependency)
                    if kind != "dev-dependencies":
                        runtime.add(dependency)
                        spec = value if isinstance(value, dict) else {}
                        edge = edges.get(alias)
                        # A target table's edge and the plain one are one dependency.
                        edges[alias] = Edge(
                            dependency,
                            spec.get("optional", False) and (edge is None or edge.optional),
                            spec.get("default-features", True) or (edge is not None and edge.default_features),
                            frozenset(spec.get("features", [])) | (edge.features if edge else frozenset()),
                        )
        packages[name] = Package(directory.relative_to(root).as_posix(), frozenset(dependencies), frozenset(runtime),
                                 edges, data.get("features", {}))
    if not packages:
        raise ValueError("Workspace has no packages")
    return packages


# The window's crates. Each builds without the Server process, so an edit to
# the Server rebuilds no window code; their tests may still run a Server.
WINDOW_CHAIN = ("ferrite-client", "ferrite-ui", "ferrite-terminal", "ferrite-pane", "ferrite-frame", "ferrite-cockpit")


def check_boundaries(packages: dict[str, Package]) -> None:
    for source in ("ferrite-core", "ferrite-engine", "ferrite-frame"):
        if source not in packages:
            raise ValueError(f"Required architectural boundary owner is missing: {source}")
    # Core draws nothing (ADR 0001), and `ferrited` runs where no window's
    # libraries are (ADR 0026).
    for source in ("ferrite-core", "ferrite-engine", "ferrite-client", "ferrite-server"):
        if source in packages:
            forbid(packages, source, lambda name: name == "gpui" or name.startswith("gpui-"), "dependencies")
    # The Server holds no window model (ADR 0049); its tests drive one.
    if "ferrite-server" in packages:
        forbid(packages, "ferrite-server", lambda name: name == "ferrite-client", "runtime")
    forbid(packages, "ferrite-frame", lambda name: name == "ferrite-cockpit", "dependencies")
    for source in WINDOW_CHAIN:
        if source in packages:
            forbid(packages, source, lambda name: name == "ferrite-server", "runtime")
            # A released window links no Engine (ADR 0049). Its tests, the
            # demo and the in-process fixture behind a feature may.
            forbid(packages, source, lambda name: name == "ferrite-engine", "shipped")


def forbid(packages: dict[str, Package], source: str, forbidden, edges: str) -> None:
    """Fail when `source` reaches a `forbidden` package over `edges`: every
    edge (`dependencies`), every edge but a dev-dependency (`runtime`), or
    what a release build links (`shipped`)."""
    pending = [((source,), SHIPPED)]
    visited = set()
    while pending:
        chain, features = pending.pop()
        name = chain[-1]
        if forbidden(name):
            raise ValueError(f"Forbidden dependency: {' -> '.join(chain)}")
        if (name, features) in visited or name not in packages:
            continue
        visited.add((name, features))
        if edges == "shipped":
            pending.extend(((*chain, dep), wanted) for dep, wanted in shipped(packages[name], features))
        else:
            pending.extend(((*chain, dep), features) for dep in sorted(getattr(packages[name], edges)))


def shipped(package: Package, requested: frozenset[str]) -> list[tuple[str, frozenset[str]]]:
    """What `package` links when built with the `requested` features, each
    with the features it is built with: its normal and build edges that are
    not optional or that a feature turns on (`dep:x`, `x/f` or `x`)."""
    on, enabled, asked = set(), set(), {}
    pending = [name for name in requested if name in package.features]
    while pending:
        name = pending.pop()
        if name in on:
            continue
        on.add(name)
        for item in package.features[name]:
            if item.startswith("dep:"):
                enabled.add(item[4:])
            elif "/" in item:
                alias, feature = item.split("/", 1)
                if not alias.endswith("?"):
                    enabled.add(alias)
                asked.setdefault(alias.rstrip("?"), set()).add(feature)
            elif item in package.features:
                pending.append(item)
            else:
                enabled.add(item)
    links = []
    for alias, edge in sorted(package.edges.items()):
        if edge.optional and alias not in enabled:
            continue
        wanted = edge.features | asked.get(alias, set()) | ({"default"} if edge.default_features else set())
        links.append((edge.package, frozenset(wanted)))
    return links


# These inputs affect packaging or dependency resolution outside workspace edges.
# Vendored crates are tested on macOS only, in their own run with test features.
VENDOR = "vendor/"
SHARED_INPUTS = (VENDOR, "scripts/", "assets/", "resources/", ".cargo/", ".config/", ".github/workflows/")
SHARED_FILES = {"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml"}

# The packaged browser run (browser-window.yml) holds scarce macOS runners for
# most of an hour, so a PR runs it only for the inputs it alone sees whole:
# the Browser, the app crate, the GPUI stack, dependencies, the packaging
# scripts, and any line naming the `cef` or `browser-smoke` feature. ci.yml
# compiles both features on every PR, so a caller's change that breaks gated
# code fails there, and main runs the packaged browser after every merge.
BROWSER_FILES = {"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml", ".github/workflows/browser-window.yml",
                 "scripts/ci_scope.py", "scripts/install-app.sh", "scripts/stage-windows.ps1", "scripts/copy-chromium-windows.ps1"}
BROWSER_INPUTS = (VENDOR, ".cargo/", "scripts/patches/", "crates/ferrite/", "crates/ferrite-pane/src/visual/",
                  "crates/ferrite-pane/src/annotation_editor")
CEF_GATE = re.compile(r'feature\s*=\s*"(cef|browser-smoke)"')


def browser_input(path: str) -> bool:
    return (path in BROWSER_FILES or path.startswith(BROWSER_INPUTS) or path.endswith("/Cargo.toml")
            or (path.startswith(("crates/", "scripts/")) and "browser" in path))


def select(packages: dict[str, Package], paths: list[str], full: bool = False, browser: bool = False) -> dict:
    check_boundaries(packages)
    selected = set()
    reasons = set()
    all_packages = full or not paths
    browser = browser or not paths or any(map(browser_input, paths))
    if all_packages:
        reasons.add("full run or unavailable change list")
    for path in paths:
        # Before ownership: vendored members reach Ferrite through crates.io
        # packages such as gpui-kit, so their dependants are not workspace edges.
        if path in SHARED_FILES or path.startswith(SHARED_INPUTS):
            all_packages = True
            reasons.add(f"shared build or packaging input: {path}")
            continue
        # Crate fixtures, Markdown included, can be embedded at compile time.
        owners = {name for name, package in packages.items() if path.startswith(package.directory + "/")}
        if owners:
            selected.update(owners)
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
        "browser": browser,
        "full": full,
        "packages": sorted(name for name in selected if not packages[name].directory.startswith(VENDOR)),
        "vendored": sorted(name for name in selected if packages[name].directory.startswith(VENDOR)),
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


def changes_cef_gate(root: Path, base: str, head: str) -> bool:
    """Whether the diff adds or removes a line naming the `cef` or `browser-smoke` feature."""
    result = subprocess.run(
        ["git", "diff", "--unified=0", "--no-renames", "--no-color", f"{base}...{head}", "--"],
        cwd=root, check=True, stdout=subprocess.PIPE,
    )
    return any(line[:1] in "+-" and not line.startswith(("+++", "---")) and CEF_GATE.search(line)
               for line in result.stdout.decode(errors="replace").splitlines() if line)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--base", help="Compare against this Git revision's merge base")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--full", action="store_true", help="Select every package and Windows CI")
    parser.add_argument("--browser", action="store_true", help="Select the packaged browser checks")
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    if (args.base and args.base.startswith("-")) or args.head.startswith("-"):
        parser.error("Git revisions cannot start with a dash")
    root = args.root.resolve()
    paths = changed_paths(root, args.base, args.head) if args.base else []
    browser = args.browser or (bool(args.base) and changes_cef_gate(root, args.base, args.head))
    result = select(workspace(root), paths, full=args.full or not args.base, browser=browser)
    print(json.dumps(result, indent=2))
    if args.github_output:
        with args.github_output.open("a") as output:
            for name in ("code", "browser", "full"):
                output.write(f"{name}={str(result[name]).lower()}\n")
            for name in ("packages", "vendored"):
                output.write(f"{name}={' '.join(result[name])}\n")


if __name__ == "__main__":
    main()

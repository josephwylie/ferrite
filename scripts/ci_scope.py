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
    # Files outside its folder that its sources embed, from the root.
    embeds: tuple[str, ...] = ()

    def reads(self, path: str) -> bool:
        """Whether a change to `path` changes what this package builds or tests."""
        return path.startswith((self.directory + "/", *self.embeds))

    @property
    def run(self) -> str:
        """The test run its class puts it in."""
        return next(run for folder, run in CLASSES.items() if self.directory.startswith(folder))


# Where a member's tests run, by its folder: Ferrite's crates on every
# platform, the vendored framework crates on macOS only, in their own run
# with test features. A member in no class fails, so a new kind of crate
# is placed on purpose.
CLASSES = {"crates/": "packages", "vendor/": "vendored"}

# A file a Rust source embeds or takes as a module, by its literal path or a
# `concat!`'s first piece. Each is relative to the source's folder.
EMBED = re.compile(r'(?:\binclude(?:_str|_bytes)?!\(\s*(?:concat!\(\s*)?|#\[path\s*=\s*)"([^"]+)"')


def embedded(root: Path, directory: Path) -> tuple[str, ...]:
    """The files outside `directory` that its Rust sources embed."""
    found = set()
    for source in directory.rglob("*.rs"):
        for literal in EMBED.findall(source.read_text(errors="replace")):
            target = (source.parent / literal).resolve()
            if target.is_relative_to(root) and not target.is_relative_to(directory):
                found.add(target.relative_to(root).as_posix())
    return tuple(sorted(found))


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
        relative = directory.relative_to(root).as_posix()
        if not relative.startswith(tuple(CLASSES)):
            raise ValueError(f"Workspace member in no class: {name} ({relative}). Give its folder a class in CLASSES.")
        packages[name] = Package(relative, frozenset(dependencies), frozenset(runtime), edges, data.get("features", {}),
                                 embedded(root, directory))
    if not packages:
        raise ValueError("Workspace has no packages")
    return packages


# The window's crates. Each builds without the Server process, so an edit to
# the Server rebuilds no window code; their tests may still run a Server.
WINDOW_CHAIN = ("ferrite-client", "ferrite-ui", "ferrite-terminal", "ferrite-pane", "ferrite-frame", "ferrite-review",
                "ferrite-cockpit")


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
    # The frame and the Review build and test without the Cockpit.
    for source in ("ferrite-frame", "ferrite-review"):
        if source in packages:
            forbid(packages, source, lambda name: name == "ferrite-cockpit", "dependencies")
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


# What a path selects in ci.yml, beyond the members that read it.
SHARED = "shared input"  # every package and check, the vendored ones too
UNCLASSIFIED = "unclassified input"  # as a shared input
VENDORED = "read by a vendored crate"  # as vendored code: its dependants are not workspace edges
CHECKS = "macOS checks input"  # the app build, visual-reference, CEF and raw-press checks
WINDOWS = "Windows checks input"  # the Windows checks, which run on main and on a full-ci PR
NOTHING = "no check here"

# The first rule naming a path decides what it selects: a file, a folder
# (ending in "/"), or a file type at the root ("*.md"). A path no rule
# names and no member reads is unclassified.
RULES = (
    # Cargo's, the toolchain's and nextest's inputs, this workflow, and the
    # selector deciding what a change runs. Vendored members reach Ferrite
    # through crates.io packages such as gpui-kit, so their dependants are
    # not workspace edges.
    ("Cargo.toml", SHARED), ("Cargo.lock", SHARED), ("rust-toolchain", SHARED), ("rust-toolchain.toml", SHARED),
    (".cargo/", SHARED), (".config/", SHARED), ("vendor/", SHARED), (".github/workflows/ci.yml", SHARED),
    ("scripts/ci_scope.py", SHARED),
    # A script a check here runs, or one it reads or calls, selects that
    # check. Every script ci.yml names needs its own rule, since the folder
    # rule below selects nothing.
    ("scripts/check_raw_presses.py", CHECKS), ("scripts/raw-presses.json", CHECKS),
    ("scripts/tests/windows-stage.ps1", WINDOWS), ("scripts/stage-windows.ps1", WINDOWS),
    ("scripts/copy-chromium-windows.ps1", WINDOWS),
    # The scope job runs its own tests for every change. Other workflows
    # select themselves by their paths, the packaged browser by
    # `browser_input`, and the release by its tag.
    ("scripts/", NOTHING), (".github/workflows/", NOTHING),
    ("docs/", NOTHING), ("skills/", NOTHING), ("*.md", NOTHING),
)


def rule(path: str) -> str | None:
    for pattern, selects in RULES:
        if pattern.endswith("/"):
            named = path.startswith(pattern)
        elif pattern.startswith("*"):
            named = "/" not in path and path.endswith(pattern[1:])
        else:
            named = path == pattern
        if named:
            return selects
    return None


# The packaged browser run (browser-window.yml) holds scarce macOS runners for
# most of an hour, so a PR runs it only for the inputs it alone sees whole:
# the Browser, the app crate, the GPUI stack, dependencies, the packaging
# scripts, and any line naming the `cef` or `browser-smoke` feature. ci.yml
# compiles both features on every PR, so a caller's change that breaks gated
# code fails there, and main runs the packaged browser after every merge.
BROWSER_FILES = {"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml", ".github/workflows/browser-window.yml",
                 "scripts/ci_scope.py", "scripts/install-app.sh", "scripts/stage-windows.ps1", "scripts/copy-chromium-windows.ps1"}
BROWSER_INPUTS = ("vendor/", ".cargo/", "scripts/patches/", "crates/ferrite/", "crates/ferrite-pane/src/visual/",
                  "crates/ferrite-pane/src/annotation_editor")
CEF_GATE = re.compile(r'feature\s*=\s*"(cef|browser-smoke)"')


def browser_input(path: str) -> bool:
    return (path in BROWSER_FILES or path.startswith(BROWSER_INPUTS) or path.endswith("/Cargo.toml")
            or (path.startswith(("crates/", "scripts/")) and "browser" in path))


def select(packages: dict[str, Package], paths: list[str], full: bool = False, browser: bool = False,
           windows: bool = False) -> dict:
    """What ci.yml checks for a change to `paths`. `full` selects everything
    and Windows; `windows` runs the Windows checks for what is selected."""
    check_boundaries(packages)
    selected = set()
    inputs = {SHARED} if full or not paths else set()
    reasons = {"full run or unavailable change list"} if inputs else set()
    browser = browser or not paths or any(map(browser_input, paths))
    for path in paths:
        owners = {name for name, package in packages.items() if package.reads(path)}
        selects = rule(path) or (None if owners else UNCLASSIFIED)
        if selects in (None, NOTHING) and any(packages[name].run == "vendored" for name in owners):
            selects = VENDORED
        selected.update(owners)
        if selects not in (None, NOTHING):
            inputs.add(selects)
            reasons.add(f"{selects}: {path}")
    if inputs & {SHARED, UNCLASSIFIED, VENDORED}:
        selected = set(packages)
    else:
        while True:
            dependants = {name for name, package in packages.items() if package.dependencies & selected}
            expanded = selected | dependants
            if expanded == selected:
                break
            selected = expanded
    runs ={run: [] for run in CLASSES.values()}
    for name in sorted(selected):
        runs[packages[name].run].append(name)
    checks = bool(runs["packages"]) or CHECKS in inputs
    windows = (full or windows) and (bool(runs["packages"]) or WINDOWS in inputs)
    return {
        "code": bool(selected) or checks or windows,
        "browser": browser,
        "checks": checks,
        "windows": windows,
        **runs,
        "decisions": {name: name in selected for name in sorted(packages)},
        "reasons": sorted(reasons) or ["workspace dependency closure" if selected else "nothing this workflow checks changed"],
    }


def succeeds(root: Path, *args: str) -> bool:
    return subprocess.run(["git", *args], cwd=root, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0


def comparison(root: Path, head: str, base: str | None, checked: list[str]) -> str | None:
    """What to compare `head` with, or None to check everything. A PR's
    `base` must share history with `head`. A push to main takes the newest
    `checked` commit, whose run succeeded, that `head` descends from: a change
    whose run was cancelled or failed is checked again. A first push has none,
    and a forced one can leave them all behind."""
    if base:
        return base if succeeds(root, "merge-base", base, head) else None
    if not checked:
        return None
    tip = subprocess.run(["git", "rev-parse", f"{head}^{{commit}}"], cwd=root, check=True,
                         capture_output=True, text=True).stdout.strip()
    return next((commit for commit in checked
                 if commit != tip and succeeds(root, "merge-base", "--is-ancestor", commit, head)), None)


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
    since = parser.add_mutually_exclusive_group()
    since.add_argument("--base", help="Compare against this Git revision's merge base")
    since.add_argument("--checked", nargs="*", default=[], metavar="COMMIT",
                       help="Commits whose run on main succeeded, newest first: compare against the first HEAD descends from")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--full", action="store_true", help="Select every package and Windows CI")
    parser.add_argument("--windows", action="store_true", help="Run the Windows checks for what is selected")
    parser.add_argument("--browser", action="store_true", help="Select the packaged browser checks")
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    if any(revision.startswith("-") for revision in [args.base or "", args.head, *args.checked]):
        parser.error("Git revisions cannot start with a dash")
    root = args.root.resolve()
    base = comparison(root, args.head, args.base, args.checked)
    paths = changed_paths(root, base, args.head) if base else []
    browser = args.browser or (bool(base) and changes_cef_gate(root, base, args.head))
    result = select(workspace(root), paths, full=args.full or not base, browser=browser, windows=args.windows)
    print(json.dumps(result, indent=2))
    if args.github_output:
        with args.github_output.open("a") as output:
            for name in ("code", "browser", "checks", "windows"):
                output.write(f"{name}={str(result[name]).lower()}\n")
            for name in ("packages", "vendored"):
                output.write(f"{name}={' '.join(result[name])}\n")


if __name__ == "__main__":
    main()

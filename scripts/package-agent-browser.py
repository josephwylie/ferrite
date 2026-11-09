#!/usr/bin/env python3
"""Build and stage Ferrite's hardened native agent-browser 0.41.2 controller.

Build-time requirements: Python >=3.9, git and Rust. No Node/npm launcher.
Upstream source and the local hardening patch are pinned; a verified local
build is reused across app packaging runs. No daemon or browser is started.
"""
import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
from pathlib import Path

VERSION = "0.41.2"
SOURCE_COMMIT = "d52a5e90c372ec4710c69adc3837ac14bbd852aa"
SOURCE_URL = "https://github.com/vercel-labs/agent-browser.git"
ROOT = Path(__file__).resolve().parent.parent
PATCH = ROOT / "scripts/patches/agent-browser-0.41.2-ferrite.patch"


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            checksum.update(chunk)
    return checksum.hexdigest()


def run(*args, **kwargs):
    subprocess.run(args, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", required=True, type=Path)
    parser.add_argument("--offline", action="store_true", help="verify the staged build without downloading or rebuilding")
    args = parser.parse_args()
    name = "agent-browser.exe" if os.name == "nt" else "agent-browser"
    patch_digest = digest(PATCH)
    expected = {"version": VERSION, "source_commit": SOURCE_COMMIT,
                "patch_sha256": patch_digest, "platform": platform.system(), "arch": platform.machine()}
    artifact = args.destination / name
    provenance = args.destination / "agent-browser-build.json"
    if args.offline:
        try:
            existing = json.loads(provenance.read_text())
            if any(existing.get(key) != value for key, value in expected.items()) or existing.get("binary_sha256") != digest(artifact):
                parser.error("staged native controller provenance or SHA-256 mismatch")
        except (OSError, ValueError) as error:
            parser.error(f"staged native controller unavailable: {error}")
        print(json.dumps(existing))
        return
    cache = ROOT / "target" / "agent-browser-build" / patch_digest[:16]
    source = cache / "source"
    if not source.exists():
        cache.mkdir(parents=True, exist_ok=True)
        run("git", "-c", "core.autocrlf=false", "clone", "--depth", "1", "--branch", "v" + VERSION, SOURCE_URL, str(source))
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    if commit != SOURCE_COMMIT:
        parser.error(f"upstream tag/source mismatch: expected {SOURCE_COMMIT}, got {commit}")
    # The cache directory belongs to this exact patch hash. Detect partial or
    # mutated caches rather than silently build a different controller.
    if subprocess.run(["git", "apply", "--reverse", "--check", str(PATCH)], cwd=source, capture_output=True).returncode != 0:
        run("git", "apply", "--check", str(PATCH), cwd=source)
        run("git", "apply", str(PATCH), cwd=source)
    actual_patch = subprocess.check_output(["git", "diff", "--binary"], cwd=source)
    # git diff is normalized; compare it to a clean patch application, not a
    # stamp that could survive source edits.
    if actual_patch != PATCH.read_bytes():
        parser.error("cached upstream source differs from the retained hardening patch")
    build_target = ROOT / "target" / "agent-browser-cargo"
    run("cargo", "build", "--release", "--locked", "--target-dir", str(build_target), "--manifest-path", str(source / "cli/Cargo.toml"))
    built = build_target / "release" / name
    args.destination.mkdir(parents=True, exist_ok=True)
    shutil.copy2(built, artifact)
    licenses = args.destination / "agent-browser-licenses"
    licenses.mkdir(exist_ok=True)
    shutil.copy2(source / "LICENSE", licenses / "LICENSE")
    expected["binary_sha256"] = digest(artifact)
    expected["rustc"] = subprocess.check_output(["rustc", "--version"], text=True).strip()
    provenance.write_text(json.dumps(expected, indent=2) + "\n")
    print(json.dumps(expected))


if __name__ == "__main__":
    main()

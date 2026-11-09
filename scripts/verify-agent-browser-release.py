#!/usr/bin/env python3
"""Stage the native controller, without starting its daemon or a browser.

Only build-time Python is required. Hashes are GitHub release asset digests for
v0.41.2, independently pinned here rather than trusted from a download response.
"""
import argparse
import hashlib
import json
import os
import platform
import tempfile
import urllib.request
from pathlib import Path

VERSION = "0.41.2"
RELEASES = {
    ("darwin", "arm64"): ("agent-browser-darwin-arm64", "19680f06cf87d0095c318a2e80bea6b71523d95d3b651cd18d81885cd182477a"),
    ("darwin", "x64"): ("agent-browser-darwin-x64", "fc55608c2222650e6e0d0154f9edebd203b63bf853b85d22240bfbddbb006ea1"),
    ("windows", "x64"): ("agent-browser-win32-x64.exe", "c2eadee7b67e326dfff02d5dd51a16ce7673eb165699d134b3ea5f05b4a7a9d0"),
}


def digest(path):
    with path.open("rb") as source:
        checksum = hashlib.sha256()
        while chunk := source.read(1024 * 1024):
            checksum.update(chunk)
        return checksum.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=["darwin", "windows"], default=platform.system().lower())
    parser.add_argument("--arch", choices=["arm64", "x64"], default={"aarch64": "arm64", "AMD64": "x64", "x86_64": "x64"}.get(platform.machine(), platform.machine()))
    parser.add_argument("--destination", required=True, type=Path)
    parser.add_argument("--offline", action="store_true", help="verify an existing staged executable without downloading")
    args = parser.parse_args()
    release = RELEASES.get((args.platform, args.arch))
    if release is None:
        parser.error("no supported native release for this platform/architecture")
    asset, expected = release
    executable = args.destination / ("agent-browser.exe" if args.platform == "windows" else "agent-browser")
    if executable.exists():
        if digest(executable) != expected:
            parser.error(f"SHA-256 mismatch: {executable}; remove the untrusted file before retrying")
    else:
        if args.offline:
            parser.error(f"staged native executable not found: {executable}")
        args.destination.mkdir(parents=True, exist_ok=True)
        url = f"https://github.com/vercel-labs/agent-browser/releases/download/v{VERSION}/{asset}"
        # Never place a partial or unverified download at the executable path.
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=args.destination, delete=False) as output:
                temporary = Path(output.name)
                with urllib.request.urlopen(url, timeout=60) as response:
                    while chunk := response.read(1024 * 1024):
                        output.write(chunk)
            if digest(temporary) != expected:
                parser.error(f"SHA-256 mismatch downloading {asset}")
            temporary.chmod(0o755)
            os.replace(temporary, executable)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
    print(json.dumps({"version": VERSION, "asset": asset, "sha256": expected, "executable": str(executable.resolve())}))


if __name__ == "__main__":
    main()

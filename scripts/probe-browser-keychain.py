#!/usr/bin/env python3
"""Exercise production CEF encryption inside a disposable macOS CI keychain.

Never run this against an operator's keychain. The script refuses local use,
creates an empty keychain and isolated website profile, and restores the CI
runner's original keychain selection even when a browser assertion fails.
"""
import os
from pathlib import Path
import secrets
import shlex
import sqlite3
import subprocess
import sys
import tempfile


def security(*arguments):
    result = subprocess.run(
        ["security", *arguments], capture_output=True, text=True, check=False
    )
    if result.returncode:
        # Do not include the command argv: create/unlock contains an ephemeral
        # password that has no place in CI logs.
        raise RuntimeError(f"security {arguments[0]} failed: {result.stderr.strip()}")
    return result.stdout


def main():
    if sys.platform != "darwin" or os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("This probe requires an isolated macOS GitHub Actions runner")
    if len(sys.argv) != 2:
        raise RuntimeError("Pass the packaged Ferrite executable")
    executable = str(Path(sys.argv[1]).resolve(strict=True))
    previous_search = shlex.split(security("list-keychains", "-d", "user"))
    previous_default = shlex.split(security("default-keychain", "-d", "user"))
    if len(previous_default) != 1:
        raise RuntimeError("Cannot safely restore the runner's default keychain")

    with tempfile.TemporaryDirectory(
        prefix="ferrite-os-keychain-", dir=os.environ.get("RUNNER_TEMP")
    ) as directory:
        keychain = str(Path(directory) / "browser-ci.keychain-db")
        profile = Path(directory) / "website-profile"
        password = secrets.token_hex(32)
        created = False
        try:
            security("create-keychain", "-p", password, keychain)
            created = True
            security("set-keychain-settings", "-lut", "3600", keychain)
            security("unlock-keychain", "-p", password, keychain)
            security("list-keychains", "-d", "user", "-s", keychain)
            security("default-keychain", "-d", "user", "-s", keychain)
            environment = dict(os.environ)
            environment["FERRITE_BROWSER_PROFILE"] = str(profile)
            environment["FERRITE_BROWSER_SMOKE_MOCK_KEYCHAIN"] = "0"
            for stage in ("write", "read"):
                environment["FERRITE_BROWSER_PROFILE_SMOKE"] = stage
                result = subprocess.run(
                    [executable, "--visual-smoke"], env=environment, check=False
                )
                if result.returncode:
                    raise RuntimeError(f"Production keychain profile {stage} failed")

            found = 0
            for database in profile.rglob("Cookies"):
                with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True) as connection:
                    rows = connection.execute(
                        "SELECT value, encrypted_value FROM cookies WHERE name = ?",
                        ("ferrite_profile",),
                    ).fetchall()
                for value, encrypted in rows:
                    if value or not encrypted or b"authenticated" in bytes(encrypted):
                        raise RuntimeError("Persistent fixture cookie was not encrypted at rest")
                    found += 1
            if not found:
                raise RuntimeError("No persistent fixture cookie found for independent inspection")
            # A mock implementation would never create this entry in our new,
            # initially empty keychain. Dump attributes only, never secret data.
            if "Safe Storage" not in security("dump-keychain", keychain):
                raise RuntimeError("Chromium did not create an OS Safe Storage keychain entry")
            print("PASS production macOS keychain: encrypted cookie, OS keychain entry, authenticated restart")
        finally:
            try:
                security("default-keychain", "-d", "user", "-s", previous_default[0])
            finally:
                try:
                    security("list-keychains", "-d", "user", "-s", *previous_search)
                finally:
                    if created:
                        security("delete-keychain", keychain)


if __name__ == "__main__":
    main()

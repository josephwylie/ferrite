#!/usr/bin/env python3
"""Hold feature code's raw presses to a baseline that only goes down (docs/adr/0044).

Reads `cargo clippy --message-format=json` on stdin, counts the
`clippy::disallowed_methods` diagnostics in each source file and compares them
with scripts/raw-presses.json. Run locally with:

    cargo clippy --locked -p ferrite-cockpit -p ferrite-pane -p ferrite-frame \\
        -p ferrite -p ferrite-terminal --message-format=json \\
        -- --cap-lints warn -A clippy::all -W clippy::disallowed_methods \\
        | python3 scripts/check_raw_presses.py

Add --write to record the counts read as the new baseline.
"""

import argparse
from collections import defaultdict
import json
from pathlib import Path
import sys
from typing import Iterable, TextIO

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "scripts" / "raw-presses.json"
ADR = "docs/adr/0044-feature-code-presses-through-shared-controls.md"
LINT = "clippy::disallowed_methods"


class ClippyFailed(Exception):
    pass


def presses(messages: Iterable[str], root: Path) -> dict[str, list[int]]:
    """Each file's flagged lines, from Cargo's JSON messages."""
    seen = set()
    found = defaultdict(list)
    finished = None
    for raw in messages:
        if not raw.startswith("{"):
            continue
        message = json.loads(raw)
        if message.get("reason") == "build-finished":
            finished = message.get("success")
        if message.get("reason") != "compiler-message":
            continue
        diagnostic = message["message"]
        primary = next((span for span in diagnostic["spans"] if span["is_primary"]), None)
        if primary is None:
            continue
        file = Path(primary["file_name"])
        if file.is_absolute() and file.is_relative_to(root):
            file = file.relative_to(root)
        # A path in a clippy.toml that names no method drops that rule silently.
        if file.name == "clippy.toml":
            raise ClippyFailed(f"{file.as_posix()}:{primary['line_start']}: {diagnostic['message']}")
        if (diagnostic.get("code") or {}).get("code") != LINT:
            continue
        key = (file.as_posix(), primary["line_start"], primary["column_start"])
        if key in seen:
            continue
        seen.add(key)
        found[key[0]].append(key[1])
    if finished is not True:
        raise ClippyFailed("clippy did not finish successfully, so its count is incomplete")
    return {file: sorted(lines) for file, lines in sorted(found.items())}


def counted(presses: int) -> str:
    return f"{presses} raw press" if presses == 1 else f"{presses} raw presses"


def check(found: dict[str, list[int]], baseline: dict[str, int], out: TextIO) -> bool:
    over = [(file, lines) for file, lines in found.items() if len(lines) > baseline.get(file, 0)]
    for file, lines in over:
        allowed = baseline.get(file, 0)
        at = ", ".join(str(line) for line in lines)
        print(f"{file}: {counted(len(lines))}, baseline {allowed} (lines {at}).", file=out)
    if over:
        print(
            "Feature code presses through a shared ferrite-ui control, not a raw element's "
            f"on_click or on_mouse_down. Use one for the new press; see {ADR}.",
            file=out,
        )
    lower = [(file, allowed, len(found.get(file, []))) for file, allowed in baseline.items()
             if len(found.get(file, [])) < allowed]
    for file, allowed, count in lower:
        print(f"{file}: baseline {allowed} can be lowered to {count}.", file=out)
    if lower:
        print("Lower it with --write and commit scripts/raw-presses.json.", file=out)
    total = sum(len(lines) for lines in found.values())
    print(f"{counted(total)} in {len(found)} files; baseline {sum(baseline.values())}.", file=out)
    return not over


def main(argv: list[str] | None = None, stdin: TextIO = sys.stdin, out: TextIO = sys.stdout) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--baseline", type=Path, default=BASELINE)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--write", action="store_true", help="Record the counts read as the baseline")
    args = parser.parse_args(argv)
    try:
        found = presses(stdin, args.root.resolve())
    except ClippyFailed as failure:
        print(failure, file=out)
        return 1
    if args.write:
        counts = {file: len(lines) for file, lines in found.items()}
        args.baseline.write_text(json.dumps(counts, indent=2) + "\n")
        print(f"Wrote {counted(sum(counts.values()))} in {len(counts)} files to {args.baseline}.", file=out)
        return 0
    baseline = json.loads(args.baseline.read_text())
    return 0 if check(found, baseline, out) else 1


if __name__ == "__main__":
    sys.exit(main())

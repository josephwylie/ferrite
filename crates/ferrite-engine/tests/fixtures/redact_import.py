"""Redact a vendor session file into a committable import fixture.

The import fixtures next to this script are real on-disk session files —
what `~/.claude/projects/<slug>/<session>.jsonl` and
`~/.codex/sessions/<date>/rollout-*.jsonl` actually hold — captured from
fresh throwaway sessions made for this purpose and nothing else. They are the
import contract: the parser in `src/import.rs` is proven against them, so a
vendor release that changes the on-disk shape breaks the build loudly.

  usage: python3 redact_import.py <source-file> <fixture-name>
  env:   CAPTURE_CWD   the throwaway session's working directory, masked to
                       /workspace in the fixture

How the committed sources were generated (all throwaway, in /tmp cwds):
  claude  — `claude -p --model haiku '<prompt>'` for the first turn, then
            `claude -p --model haiku --resume <session-id> '<prompt>'` twice
            more (one turn with --allowedTools 'Bash(echo *)' running a tool);
            the file appears under ~/.claude/projects/<cwd-slug>/.
  codex   — `codex app-server` driven over stdio (initialize, thread/start,
            two turn/starts); the rollout path is in the thread/start
            response. One source keeps a turn the shutdown aborted — real
            evidence of a file that ends mid-turn.

Privacy: these fixtures ship in a public repository. Masks match the wire
fixtures' conventions — home → /home/operator, the session cwd → /workspace,
email/organization → operator@example.invalid, hostname → operator.local,
installation ids zeroed — plus two the session files add: the recording
machine's IANA timezone (a location) → UTC, and the operator's own agents-md
instructions text → a placeholder. Everything else stays byte-for-byte what
the vendor wrote.
"""

import json
import os
import re
import socket
import sys

HOME = os.path.expanduser("~")
CWD = os.environ.get("CAPTURE_CWD", "")

IDENTIFYING = re.compile(r'"(email|organization)":"[^"]*"')
INSTALLATION = re.compile(r'"installationId":"[0-9a-f-]{36}"')
# The macOS per-user temp directory: its middle component is a stable hash of
# the account, so any spelling of it identifies the operator.
USER_TMPDIR = re.compile(r"(?:/private)?/var/folders/[^/\"\\]+/[^/\"\\]+/[TC]\b")

HOSTS = {socket.gethostname(), socket.gethostname().split(".")[0] + ".local"}
# Longest spelling first: on macOS the realpath of a /tmp cwd is the /tmp
# path under /private, and masking the shorter spelling first would leave a
# /private prefix stranded in front of the mask.
PATHS = sorted(
    (
        (path, mask)
        # CAPTURE_CWD may name several colon-separated directories: a session
        # resumed from more than one place recorded each of them.
        for directory, mask in [(cwd, "/workspace") for cwd in CWD.split(":") if cwd]
        + [(HOME, "/home/operator")]
        for path in {directory, os.path.realpath(directory)}
    ),
    key=lambda pair: len(pair[0]),
    reverse=True,
)


def machine_timezone():
    """The recording machine's IANA timezone name, from /etc/localtime — the
    rollout repeats it both as a JSON field and inside prompt-scaffolding
    text, so it is masked as a plain string wherever it appears."""
    target = os.path.realpath("/etc/localtime")
    _, _, name = target.partition("zoneinfo/")
    return name


def operator_instructions():
    """The operator's own ~/.codex/AGENTS.md text, JSON-escaped the way the
    rollout embeds it (inside world_state and again inside developer
    messages). Their writing is theirs; no import parser reads it."""
    path = os.path.join(HOME, ".codex", "AGENTS.md")
    if not os.path.exists(path):
        return None
    text = open(path).read().strip()
    return json.dumps(text)[1:-1] if text else None


TIMEZONE = machine_timezone()
INSTRUCTIONS = operator_instructions()


def redact(line):
    for path, mask in PATHS:
        line = line.replace(path, mask)
    for host in HOSTS:
        line = line.replace(host, "operator.local")
    if TIMEZONE:
        line = line.replace(TIMEZONE, "UTC")
    if INSTRUCTIONS:
        line = line.replace(INSTRUCTIONS, "(operator instructions redacted)")
    line = INSTALLATION.sub('"installationId":"00000000-0000-0000-0000-000000000000"', line)
    line = USER_TMPDIR.sub("/tmp/operator", line)
    return IDENTIFYING.sub(r'"\1":"operator@example.invalid"', line)


def main():
    source, fixture = sys.argv[1], sys.argv[2]
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), fixture)
    with open(source) as src, open(out, "w") as dst:
        for line in src:
            dst.write(redact(line))
    # The gate the fixtures must pass before they land: nothing identifying
    # survives redaction.
    text = open(out).read()
    for needle in [HOME, os.path.basename(HOME)] + sorted(HOSTS):
        if needle and needle in text:
            sys.exit(f"redact_import.py: {needle!r} survived redaction in {fixture}")
    for line in text.splitlines():
        json.loads(line)  # every fixture line must stay valid JSON
    print(f"    {fixture}")


if __name__ == "__main__":
    main()

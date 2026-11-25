"""Snapshot a temporary home's setup paths, and its Ferrite trash.

  snap.py <home> <settings dir> <out.json>

Records path -> [kind, sha256 of bytes, sha256 with CRLF as LF, link target]
for the table's paths under <home> and everything under <settings
dir>/sync-trash and <settings dir>/sync-conflicts. Links are recorded as
links, never followed. Prints counts only, never contents.
"""
import hashlib
import json
import os
import stat
import sys

PATHS = [
    ".agents/skills", ".claude/skills", ".claude/commands", ".claude/agents", ".claude/rules",
    ".claude/hooks", ".claude/output-styles", ".claude/CLAUDE.md", ".claude/keybindings.json",
    ".claude/statusline.sh", ".claude/settings.json", ".claude.json", ".codex/skills",
    ".codex/prompts", ".codex/rules", ".codex/AGENTS.md", ".codex/config.toml",
]


def is_link(full):
    return os.path.islink(full) or (hasattr(os.path, "isjunction") and os.path.isjunction(full))


def entry(full):
    if is_link(full):
        return ["link", "", "", os.readlink(full)]
    if stat.S_ISDIR(os.lstat(full).st_mode):
        return ["dir", "", "", ""]
    data = open(full, "rb").read()
    return ["file", hashlib.sha256(data).hexdigest(), hashlib.sha256(data.replace(b"\r\n", b"\n")).hexdigest(), ""]


def walk(base, rel, out, prefix=""):
    full = os.path.join(base, *rel.split("/")) if rel else base
    if not os.path.lexists(full):
        return
    out[prefix + rel] = entry(full)
    if out[prefix + rel][0] != "dir":
        return
    for name in sorted(os.listdir(full)):
        walk(base, f"{rel}/{name}" if rel else name, out, prefix)


home, settings, dest = sys.argv[1], sys.argv[2], sys.argv[3]
out = {}
for rel in PATHS:
    walk(home, rel, out)
for kept in ("sync-trash", "sync-conflicts"):
    walk(os.path.join(settings, kept), "", out, prefix=f"@{kept}/")
json.dump(out, open(dest, "w", encoding="utf-8"), indent=0, sort_keys=True)
kinds = {}
for kind, *_ in out.values():
    kinds[kind] = kinds.get(kind, 0) + 1
print(dest, len(out), kinds)

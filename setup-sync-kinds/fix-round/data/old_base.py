"""Put a base shaped like #494's (the first table's, unstamped) where the
hub keeps its base for the Machine `ferrite-wsl`.

  old_base.py <hub home> <machine snapshot.json> <hub settings dir>

The first table followed `.claude/skills` links, so it listed each linked
skill as a folder of files; it synced whole `settings.json` and
`config.toml` and `.claude/plugins`. It also lists the Machine's
`linux-skill`, which the hub does not have: a base that says both had it
reads as the hub having removed it, which the first table's sync then
removed on the Machine.
"""
import hashlib
import json
import os
import sys

hub, snapshot, settings = sys.argv[1], sys.argv[2], sys.argv[3]
machine = json.load(open(snapshot, encoding="utf-8"))
entries = {}


def file_entry(sha, size=0):
    return {"kind": "file", "size": size, "sha256": sha, "modified": 1763900000000}


for name in ("reui", "ask-matt"):
    root = os.path.join(hub, ".agents", "skills", name)
    entries[f".claude/skills/{name}"] = {"kind": "dir"}
    for folder, dirs, files in os.walk(root):
        rel = os.path.relpath(folder, root).replace("\\", "/")
        prefix = f".claude/skills/{name}" + ("" if rel == "." else f"/{rel}")
        if rel != ".":
            entries[prefix] = {"kind": "dir"}
        for file in files:
            data = open(os.path.join(folder, file), "rb").read()
            entries[f"{prefix}/{file}"] = file_entry(hashlib.sha256(data).hexdigest(), len(data))
for path, (kind, sha, _text, _target) in machine.items():
    if path.startswith(".claude/skills/linux-skill") or path in (".claude/settings.json", ".codex/config.toml"):
        entries[path] = {"kind": "dir"} if kind == "dir" else file_entry(sha)
entries[".claude/plugins"] = {"kind": "dir"}
entries[".claude/plugins/installed_plugins.json"] = file_entry("0" * 64)

target = "ferrite-wsl"
value = 0xCBF29CE484222325
for byte in target.encode():
    value = ((value ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
folder = os.path.join(settings, "machines", f"{value:016x}"[:12])
os.makedirs(folder, exist_ok=True)
path = os.path.join(folder, "setup-sync.json")
json.dump({"entries": entries}, open(path, "w", encoding="utf-8"))
print(f"old base with {len(entries)} entries, no table, at {path}")

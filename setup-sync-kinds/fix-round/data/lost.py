"""What a sync lost on one side: each file or link of <before> that is not at
its path in <after> with the same bytes, and is not kept in that side's
trash or conflicts in <after> either.

  lost.py <before.json> <after.json>

Prints what changed at each path, where each changed file's old bytes are
kept, and LOST for any that are kept nowhere.
"""
import json
import sys

before = json.load(open(sys.argv[1], encoding="utf-8"))
after = json.load(open(sys.argv[2], encoding="utf-8"))
kept = {}
for path, (kind, sha, text, target) in after.items():
    if path.startswith("@") and kind == "file":
        kept.setdefault(sha, []).append(path)
    if path.startswith("@") and kind == "link":
        kept.setdefault("link:" + target, []).append(path)

lost = changed = 0
for path, (kind, sha, text, target) in sorted(before.items()):
    if path.startswith("@") or kind == "dir":
        continue
    now = after.get(path)
    if now == [kind, sha, text, target]:
        continue
    changed += 1
    where = kept.get(sha if kind == "file" else "link:" + target, [])
    state = f"now {now[0]}" if now else "gone"
    if where:
        print(f"{path}: {state}; was kept at {where[0]}")
    else:
        lost += 1
        print(f"LOST {path}: {state}; its old {kind} is kept nowhere")
added = [path for path in after if not path.startswith("@") and path not in before and after[path][0] != "dir"]
print(f"{changed} changed, {lost} lost, {len(added)} added: {', '.join(sorted(added))}")

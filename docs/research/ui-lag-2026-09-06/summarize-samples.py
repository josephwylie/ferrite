#!/usr/bin/env python3
"""Summarize the first (main) thread in these saved macOS sample profiles.

Counts inclusive samples once per stack. Exact demangled method names avoid
mistaking a generic selection wrapper for time spent doing selection work.
These percentages are sampled stacks, not frame timings or GPU utilization.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

demangler = subprocess.check_output(
    ["xcrun", "--find", "llvm-cxxfilt"], text=True
).strip()
targets = {
    "draw": "<gpui::window::Window>::draw",
    "layout": "<gpui::taffy::TaffyLayoutEngine>::compute_layout",
    "register": "<gpui_base::text_selection::WindowSelectionState>::register_participant",
    "publish": "<gpui_base::text_selection::WindowSelectionState>::publish_snapshots",
    "metal_submission": "<gpui_apple::metal_renderer::MetalRenderer>::draw",
}
reports = []
for filename in sys.argv[1:]:
    path = Path(filename)
    source = path.read_text()
    symbols = list(dict.fromkeys(re.findall(r"_R[A-Za-z0-9_$]+", source)))
    decoded = subprocess.run(
        [demangler], input="\n".join(symbols) + "\n",
        text=True, capture_output=True, check=True,
    ).stdout.splitlines()
    names = dict(zip(symbols, decoded))
    source = re.sub(r"_R[A-Za-z0-9_$]+", lambda match: names[match[0]], source)
    nodes, stack = [], []
    for line in source.splitlines():
        if "Thread_" in line and nodes:
            break
        match = re.match(r"^([ +!:\|]*)(\d+) (.*)$", line)
        if not match:
            continue
        depth = len(match[1])
        while stack and stack[-1]["depth"] >= depth:
            stack.pop()
        node = {
            "depth": depth, "count": int(match[2]), "children": 0,
            "symbol": match[3].split("  (in ")[0],
        }
        if stack:
            stack[-1]["children"] += node["count"]
        node["stack"] = [parent["symbol"] for parent in stack] + [node["symbol"]]
        stack.append(node)
        nodes.append(node)
    if not nodes:
        raise SystemExit(f"No sampled thread in {path}")
    total = nodes[0]["count"]
    if any(node["children"] > node["count"] for node in nodes):
        raise SystemExit(f"Invalid sample tree in {path}")
    report = {"file": path.name, "main_samples": total, "hotspots": {}}
    for label, target in targets.items():
        count = sum(
            node["count"] - node["children"]
            for node in nodes if target in node["stack"]
        )
        report["hotspots"][label] = {"samples": count, "pct": round(count / total * 100, 2)}
    reports.append(report)
print(json.dumps(reports, indent=2))

#!/usr/bin/env python3
"""Diagnostic: run the native selection probe and reject quadratic scaling.

Usage: python3 check-selection.py /path/to/selection_probe
The binary is built from selection_probe.rs with GPUI test-support libraries.
Absolute debug timings are not release-app frame times.
"""

import re
import subprocess
import sys

result = subprocess.run(
    [sys.argv[1], "--nocapture", "--test-threads=1"],
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
    text=True,
    check=False,
)
print(result.stdout, end="")
if result.returncode:
    raise SystemExit(result.returncode)
medians = {
    int(count): float(milliseconds)
    for count, milliseconds in re.findall(
        r"selection_probe participants=(\d+) median_ms=([\d.]+)", result.stdout
    )
}
if not {64, 256} <= medians.keys() or medians[64] <= 0:
    raise SystemExit("Missing or invalid measurement")
ratio = medians[256] / medians[64]
print(f"4x participants: {ratio:.2f}x cost; linear budget: 6.00x")
if ratio > 6:
    print("FAIL: selection registration exceeds the linear scaling budget")
    raise SystemExit(1)
print("PASS: selection registration stays within the linear scaling budget")

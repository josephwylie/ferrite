#!/usr/bin/env python3
"""Summarize nextest's JUnit reports as Markdown for a GitHub job summary.

For each report: the tests that failed, the flaky ones (a try failed, a
retry passed, so the job stayed green), and the slowest, timed over all
their tries. CI runs it after the tests, whether they passed or not:

    python3 scripts/junit_summary.py target/nextest/*/junit.xml >> "$GITHUB_STEP_SUMMARY"
"""

import argparse
from dataclasses import dataclass
from pathlib import Path
import sys
from typing import TextIO
import xml.etree.ElementTree as ElementTree

SLOWEST = 10
# A test that never passed has a `failure` for its first try and a
# `rerunFailure` for each retry; one that passed on a retry has a
# `flakyFailure` for each try that failed. The case's own time is one try's.
NEVER_PASSED = {"failure", "error"}
FAILED_TRY = NEVER_PASSED | {"rerunFailure", "rerunError", "flakyFailure", "flakyError"}


@dataclass(frozen=True)
class Case:
    test: str
    passed: bool
    tries: int
    seconds: float
    timed_out: bool


def cases(report: ElementTree.Element) -> list[Case]:
    found = []
    for suite in report.iter("testsuite"):
        for case in suite.iter("testcase"):
            if case.find("skipped") is not None:
                continue
            failed = [child for child in case if child.tag in FAILED_TRY]
            passed = not any(child.tag in NEVER_PASSED for child in failed)
            found.append(Case(
                test=f"{suite.get('name')} {case.get('name')}",
                passed=passed,
                tries=len(failed) + (1 if passed else 0),
                seconds=float(case.get("time", 0)) + sum(float(child.get("time", 0)) for child in failed),
                timed_out=any(child.get("type") == "test timeout" for child in failed),
            ))
    return found


def summary(name: str, found: list[Case]) -> str:
    failed = [case for case in found if not case.passed]
    flaky = [case for case in found if case.passed and case.tries > 1]
    slowest = sorted(found, key=lambda case: case.seconds, reverse=True)[:SLOWEST]
    parts = [f"### {name}: {len(found)} run, {len(failed)} failed, {len(flaky)} flaky"]
    if failed:
        parts.append("\n".join(["| Failed | Tries | Timed out |", "|---|---:|---|", *(
            f"| `{case.test}` | {case.tries} | {'yes' if case.timed_out else ''} |" for case in failed)]))
    if flaky:
        parts.append("\n".join(["| Flaky: failed, then passed on a retry | Tries |", "|---|---:|", *(
            f"| `{case.test}` | {case.tries} |" for case in flaky)]))
    if slowest:
        parts.append("\n".join(["| Slowest | Seconds, all tries | Tries |", "|---|---:|---:|", *(
            f"| `{case.test}` | {case.seconds:.1f} | {case.tries} |" for case in slowest)]))
    return "\n\n".join(parts) + "\n"


def main(argv: list[str] | None = None, out: TextIO = sys.stdout) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("reports", nargs="+", type=Path)
    args = parser.parse_args(argv)
    for path in args.reports:
        # A build that failed writes no report; the test step says why.
        if not path.exists():
            print(f"No test report at `{path.as_posix()}`: the tests did not run.\n", file=out)
            continue
        report = ElementTree.parse(path).getroot()
        print(summary(report.get("name", path.as_posix()), cases(report)), file=out)
    return 0


if __name__ == "__main__":
    sys.exit(main())

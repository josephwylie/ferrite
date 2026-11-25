import io
from pathlib import Path
import tempfile
import unittest

from junit_summary import main


# The shape nextest 0.9.152 writes with --retries 2 and a slow-timeout, from
# a real run, with the output trimmed.
REPORT = '''<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="tests" tests="6" skipped="0" failures="2" errors="0" time="9.1">
    <testsuite name="ferrite-core" tests="4" skipped="0" errors="0" failures="1">
        <testcase name="tests::passes" classname="ferrite-core" time="0.011"/>
        <testcase name="tests::flaky" classname="ferrite-core" time="0.072">
            <flakyFailure time="0.015" message="panicked" type="test failure with exit code 101">first try</flakyFailure>
        </testcase>
        <testcase name="tests::fails" classname="ferrite-core" time="0.021">
            <failure message="panicked" type="test failure with exit code 101">always fails</failure>
            <rerunFailure time="0.076" message="panicked" type="test failure with exit code 101">again</rerunFailure>
            <rerunFailure time="0.010" message="panicked" type="test failure with exit code 101">again</rerunFailure>
        </testcase>
        <testcase name="tests::passes_slowly" classname="ferrite-core" time="1.518"/>
    </testsuite>
    <testsuite name="ferrite-cockpit::journeys" tests="2" skipped="0" errors="0" failures="1">
        <testcase name="hangs" classname="ferrite-cockpit::journeys" time="2.034">
            <failure type="test timeout"/>
            <rerunFailure time="2.016" type="test timeout"/>
            <rerunFailure time="2.014" type="test timeout"/>
        </testcase>
        <testcase name="skipped" classname="ferrite-cockpit::journeys" time="0"><skipped/></testcase>
    </testsuite>
</testsuites>
'''


def rows(out: str, header: str) -> list[str]:
    """The rows of the table whose header starts with `header`."""
    table = out[out.index(f"| {header}"):].split("\n\n")[0]
    return table.splitlines()[2:]


class SummaryTests(unittest.TestCase):
    def summarize(self, *reports: str | None) -> str:
        with tempfile.TemporaryDirectory() as directory:
            paths = []
            for index, report in enumerate(reports):
                path = Path(directory) / f"{index}.xml"
                if report is not None:
                    path.write_text(report)
                paths.append(str(path))
            out = io.StringIO()
            self.assertEqual(main(paths, out), 0)
        return out.getvalue()

    def test_heads_each_report_with_its_counts(self):
        self.assertIn("### tests: 5 run, 2 failed, 1 flaky\n", self.summarize(REPORT))

    def test_lists_failures_with_their_tries_and_marks_timeouts(self):
        self.assertEqual(rows(self.summarize(REPORT), "Failed"), [
            "| `ferrite-core tests::fails` | 3 |  |",
            "| `ferrite-cockpit::journeys hangs` | 3 | yes |",
        ])

    def test_lists_only_the_tests_a_retry_passed_as_flaky(self):
        self.assertEqual(rows(self.summarize(REPORT), "Flaky"), ["| `ferrite-core tests::flaky` | 2 |"])

    def test_ranks_the_slowest_by_their_time_over_all_tries(self):
        self.assertEqual(rows(self.summarize(REPORT), "Slowest")[:2], [
            "| `ferrite-cockpit::journeys hangs` | 6.1 | 3 |",
            "| `ferrite-core tests::passes_slowly` | 1.5 | 1 |",
        ])

    def test_shows_at_most_ten_slowest(self):
        cases = "".join(f'<testcase name="t{n}" time="{n}"/>' for n in range(12))
        report = f'<testsuites name="tests"><testsuite name="x">{cases}</testsuite></testsuites>'
        slowest = rows(self.summarize(report), "Slowest")
        self.assertEqual([row.split("`")[1] for row in slowest], [f"x t{n}" for n in range(11, 1, -1)])

    def test_a_green_run_lists_only_the_slowest(self):
        out = self.summarize('<testsuites name="tests"><testsuite name="x"><testcase name="a" time="1"/></testsuite></testsuites>')
        self.assertIn("1 run, 0 failed, 0 flaky", out)
        self.assertNotIn("| Failed", out)
        self.assertNotIn("| Flaky", out)

    def test_a_missing_report_says_the_tests_did_not_run(self):
        out = self.summarize(None, REPORT)
        self.assertIn("the tests did not run", out)
        self.assertIn("### tests:", out)


if __name__ == "__main__":
    unittest.main()

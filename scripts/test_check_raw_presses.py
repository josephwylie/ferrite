import io
import json
from pathlib import Path
import tempfile
import unittest

from check_raw_presses import main


ROOT = Path(__file__).resolve().parents[1]
FEATURE_CRATES = ["ferrite", "ferrite-cockpit", "ferrite-frame", "ferrite-pane", "ferrite-review", "ferrite-factory",
                  "ferrite-tools", "ferrite-settings", "ferrite-terminal"]


def press(file: str, line: int, column: int = 10) -> str:
    return json.dumps({
        "reason": "compiler-message",
        "message": {
            "message": "use of a disallowed method `gpui::StatefulInteractiveElement::on_click`",
            "code": {"code": "clippy::disallowed_methods", "explanation": None},
            "spans": [{"file_name": file, "line_start": line, "column_start": column, "is_primary": True}],
        },
    })


def other_lint(file: str, line: int) -> str:
    return json.dumps({
        "reason": "compiler-message",
        "message": {
            "message": "unused variable",
            "code": {"code": "unused_variables", "explanation": None},
            "spans": [{"file_name": file, "line_start": line, "column_start": 1, "is_primary": True}],
        },
    })


FINISHED = json.dumps({"reason": "build-finished", "success": True})


class RatchetTests(unittest.TestCase):
    def run_check(self, lines: list[str], baseline: dict[str, int], *flags: str) -> tuple[int, str, Path]:
        directory = Path(tempfile.mkdtemp())
        path = directory / "raw-presses.json"
        path.write_text(json.dumps(baseline))
        out = io.StringIO()
        code = main(["--baseline", str(path), "--root", str(directory), *flags],
                    io.StringIO("\n".join(lines) + "\n"), out)
        return code, out.getvalue(), path

    def test_passes_at_the_baseline(self):
        code, out, _ = self.run_check([press("a.rs", 3), press("a.rs", 9), FINISHED], {"a.rs": 2})
        self.assertEqual(code, 0, out)

    def test_fails_when_a_file_goes_over_its_baseline(self):
        code, out, _ = self.run_check([press("a.rs", 3), press("a.rs", 9), FINISHED], {"a.rs": 1})
        self.assertEqual(code, 1)
        self.assertIn("a.rs: 2 raw presses, baseline 1 (lines 3, 9).", out)
        self.assertIn("shared ferrite-ui control", out)
        self.assertIn("docs/adr/0044", out)

    def test_fails_on_a_file_missing_from_the_baseline(self):
        code, out, _ = self.run_check([press("new.rs", 5), FINISHED], {"a.rs": 1})
        self.assertEqual(code, 1)
        self.assertIn("new.rs: 1 raw press, baseline 0 (lines 5).", out)

    def test_names_entries_that_can_be_lowered_without_failing(self):
        code, out, _ = self.run_check([press("a.rs", 3), FINISHED], {"a.rs": 2, "gone.rs": 4})
        self.assertEqual(code, 0, out)
        self.assertIn("a.rs: baseline 2 can be lowered to 1.", out)
        self.assertIn("gone.rs: baseline 4 can be lowered to 0.", out)

    def test_counts_only_the_disallowed_methods_lint(self):
        code, out, _ = self.run_check([other_lint("a.rs", 1), press("a.rs", 3), FINISHED], {"a.rs": 1})
        self.assertEqual(code, 0, out)

    def test_counts_a_diagnostic_reported_twice_once(self):
        code, out, _ = self.run_check([press("a.rs", 3), press("a.rs", 3), FINISHED], {"a.rs": 1})
        self.assertEqual(code, 0, out)

    def test_fails_when_clippy_did_not_finish(self):
        failed = json.dumps({"reason": "build-finished", "success": False})
        for lines in ([press("a.rs", 3), failed], [press("a.rs", 3)]):
            with self.subTest(lines=lines):
                code, out, _ = self.run_check(lines, {"a.rs": 1})
                self.assertEqual(code, 1)
                self.assertIn("clippy did not finish", out)

    def test_fails_when_a_configured_method_does_not_resolve(self):
        unresolved = json.dumps({
            "reason": "compiler-message",
            "message": {
                "message": "`gpui::Nothing::on_click` does not refer to a reachable function",
                "code": None,
                "spans": [{"file_name": "/elsewhere/crates/x/clippy.toml", "line_start": 4,
                           "column_start": 14, "is_primary": True}],
            },
        })
        code, out, _ = self.run_check([unresolved, FINISHED], {})
        self.assertEqual(code, 1)
        self.assertIn("does not refer to a reachable function", out)

    def test_write_records_the_counts_read(self):
        code, _, path = self.run_check([press("b.rs", 1), press("a.rs", 7), press("a.rs", 2), FINISHED],
                                       {"a.rs": 9, "gone.rs": 1}, "--write")
        self.assertEqual(code, 0)
        self.assertEqual(path.read_text(), '{\n  "a.rs": 2,\n  "b.rs": 1\n}\n')

    def test_feature_crates_share_one_rule(self):
        configs = {crate: (ROOT / "crates" / crate / "clippy.toml").read_text() for crate in FEATURE_CRATES}
        self.assertEqual(len(set(configs.values())), 1, "the feature crates' clippy.toml files differ")
        self.assertFalse((ROOT / "crates" / "ferrite-ui" / "clippy.toml").exists())

    def test_committed_baseline_is_sorted_with_no_empty_entries(self):
        baseline = json.loads((ROOT / "scripts" / "raw-presses.json").read_text())
        self.assertEqual(list(baseline), sorted(baseline))
        self.assertTrue(all(count > 0 for count in baseline.values()))


if __name__ == "__main__":
    unittest.main()

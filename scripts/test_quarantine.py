import contextlib
import io
from pathlib import Path
import tempfile
import unittest

from quarantine import LIST, main, quarantined


def entry(binary: str, test: str, reason: str = "fails on a loaded runner", owner: str = "someone") -> str:
    return f'[[test]]\nbinary = "{binary}"\ntest = "{test}"\nreason = "{reason}"\nowner = "{owner}"\n'


class QuarantineTests(unittest.TestCase):
    def run_list(self, text: str) -> tuple[int, str, str]:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "quarantine.toml"
            path.write_text(text)
            out, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stderr(err):
                code = main(["--list", str(path)], out)
        return code, out.getvalue(), err.getvalue()

    def test_nothing_quarantined_selects_no_test(self):
        self.assertEqual(self.run_list("# only a comment\n")[:2], (0, "none()\n"))

    def test_each_entry_selects_one_test_in_one_binary(self):
        # The same test name lives in several crates; an entry names one of them.
        code, out, _ = self.run_list(entry("ferrite-pane", "tests::render_code_draws_only_covered_glyphs")
                                     + entry("ferrite-reviewr::git_repo", "a_binary_change_lists_with_zero_stats"))
        self.assertEqual((code, out), (0, "(binary_id(=ferrite-pane) & test(=tests::render_code_draws_only_covered_glyphs))"
                                          " | (binary_id(=ferrite-reviewr::git_repo) & test(=a_binary_change_lists_with_zero_stats))\n"))

    def test_one_name_in_two_binaries_is_two_entries(self):
        code, out, _ = self.run_list(entry("ferrite", "tests::draws") + entry("ferrite-frame", "tests::draws"))
        self.assertEqual(code, 0)
        self.assertEqual(out.count("test(=tests::draws)"), 2)

    def test_an_entry_needs_a_binary_a_test_a_reason_and_an_owner(self):
        for text in (
            '[[test]]\ntest = "tests::a"\nreason = "flaky"\nowner = "someone"\n',
            '[[test]]\nbinary = "ferrite-core"\ntest = "tests::a"\nreason = "flaky"\n',
            entry("ferrite-core", "tests::a", owner=" "),
            entry("ferrite-core", "tests::a") + 'since = "today"\n',
        ):
            with self.subTest(text=text):
                code, out, err = self.run_list(text)
                self.assertEqual((code, out), (1, ""))
                self.assertIn("a binary, a test, a reason and an owner", err)

    def test_a_binary_or_test_that_would_change_the_filter_is_refused(self):
        for binary, test, said in (
            ("ferrite-core) | all(", "tests::a", "not a nextest binary id"),
            ("ferrite-core", "tests::a) | all(", "not a test name"),
            ("ferrite-core tests::a", "tests::a", "not a nextest binary id"),
        ):
            with self.subTest(binary=binary, test=test):
                code, out, err = self.run_list(entry(binary, test))
                self.assertEqual((code, out), (1, ""))
                self.assertIn(said, err)

    def test_a_test_listed_twice_is_refused(self):
        code, _, err = self.run_list(entry("ferrite-core", "tests::a") + entry("ferrite-core", "tests::a"))
        self.assertEqual(code, 1)
        self.assertIn("listed twice", err)

    def test_committed_list_is_valid(self):
        quarantined(LIST)


if __name__ == "__main__":
    unittest.main()

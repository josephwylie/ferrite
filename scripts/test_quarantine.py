import contextlib
import io
from pathlib import Path
import tempfile
import unittest

from quarantine import LIST, main, quarantined


def entry(name: str, reason: str = "fails on a loaded runner", owner: str = "someone") -> str:
    return f'[[test]]\nname = "{name}"\nreason = "{reason}"\nowner = "{owner}"\n'


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

    def test_each_entry_is_selected_by_its_exact_name(self):
        code, out, _ = self.run_list(entry("cockpit::tests::a::waits") + entry("tests::stalls"))
        self.assertEqual((code, out), (0, "test(=cockpit::tests::a::waits) | test(=tests::stalls)\n"))

    def test_an_entry_needs_a_name_a_reason_and_an_owner(self):
        for text in (
            '[[test]]\nname = "tests::a"\nreason = "flaky"\n',
            entry("tests::a", owner=" "),
            entry("tests::a") + 'since = "today"\n',
        ):
            with self.subTest(text=text):
                code, out, err = self.run_list(text)
                self.assertEqual((code, out), (1, ""))
                self.assertIn("a name, a reason and an owner", err)

    def test_a_name_that_is_not_a_test_path_is_refused(self):
        code, out, err = self.run_list(entry("tests::a) | all("))
        self.assertEqual((code, out), (1, ""))
        self.assertIn("not a test name", err)

    def test_a_test_listed_twice_is_refused(self):
        code, _, err = self.run_list(entry("tests::a") + entry("tests::a"))
        self.assertEqual(code, 1)
        self.assertIn("listed twice", err)

    def test_committed_list_is_valid(self):
        quarantined(LIST)


if __name__ == "__main__":
    unittest.main()

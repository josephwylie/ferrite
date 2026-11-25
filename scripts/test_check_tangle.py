import io
import json
from pathlib import Path
import tempfile
import unittest

from check_tangle import main


ROOT_VIEW = '''pub struct CockpitView {
    /// The client's model, read by the nav (`a, b`) {
    pub(crate) cockpit: Entity<Cockpit>,
    held: std::collections::HashMap<ThreadKey, (u64, Vec<u8>)>,
    // ---- the frame's, a comment with a comma, and a brace }
    on_pick: Box<dyn Fn(&mut App, usize) -> Option<Vec<u8>>>,
    #[cfg(test)]
    pub(crate) chimed: Vec<Sound>,
    machines: Option<crate::machines::Machines>
}

impl CockpitView {
    fn new() -> Self { todo!() }
}
'''
HOLDER = "pub struct Nav { root: gpui::WeakEntity<CockpitView> }\nimpl CockpitView {}\n"
FIXTURE = '''fn add_cockpit_window(cx: &mut TestAppContext) -> WindowHandle<CockpitView> { todo!() }
#[test]
fn opens() { add_cockpit_window(cx); add_cockpit_window (cx); }
'''
ENTRY = ('[[test]]\nbinary = "ferrite-cockpit"\ntest = "cockpit::tests::{}"\n'
         'reason = "fails on a loaded runner"\nowner = "someone"\n')


class TangleTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.write("crates/ferrite-cockpit/src/cockpit.rs", ROOT_VIEW)
        self.write("crates/ferrite-cockpit/src/cockpit/nav.rs", HOLDER)
        self.write("crates/ferrite-cockpit/src/cockpit/tests.rs", FIXTURE)
        self.write(".config/quarantine.toml", "")
        self.baseline = self.root / "tangle.json"

    def write(self, relative: str, text: str):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def run_check(self, *flags: str) -> tuple[int, str]:
        out = io.StringIO()
        code = main(["--root", str(self.root), "--baseline", str(self.baseline), *flags], out)
        return code, out.getvalue()

    def test_write_records_each_count(self):
        self.assertEqual(self.run_check("--write")[0], 0)
        self.assertEqual(json.loads(self.baseline.read_text()), {
            "files holding WeakEntity<CockpitView>": 1,
            "files with impl CockpitView": 2,
            "CockpitView fields": 5,
            "cockpit.rs lines": 14,
            "add_cockpit_window calls": 2,
            "quarantined tests": 0,
        })

    def test_passes_at_the_baseline(self):
        self.run_check("--write")
        code, out = self.run_check()
        self.assertEqual(code, 0, out)
        self.assertNotIn("Rose", out)

    def test_each_count_that_rises_fails(self):
        rises = {
            "files holding WeakEntity<CockpitView>":
                ("crates/ferrite-cockpit/src/cockpit/new.rs", "struct X(WeakEntity<crate::cockpit::CockpitView>);\n"),
            "files with impl CockpitView": ("crates/ferrite-cockpit/src/cockpit/new.rs", "impl CockpitView\n{\n}\n"),
            "CockpitView fields": ("crates/ferrite-cockpit/src/cockpit.rs",
                                   ROOT_VIEW.replace("    machines:", "    extra: bool, machines:")),
            "cockpit.rs lines": ("crates/ferrite-cockpit/src/cockpit.rs", ROOT_VIEW + "\n"),
            "add_cockpit_window calls": ("crates/ferrite-cockpit/src/cockpit/tests.rs",
                                         FIXTURE + "fn more() { add_cockpit_window(cx); }\n"),
            "quarantined tests": (".config/quarantine.toml", ENTRY.format("opens")),
        }
        for name, (relative, text) in rises.items():
            with self.subTest(count=name):
                self.setUp()
                self.run_check("--write")
                before = json.loads(self.baseline.read_text())[name]
                self.write(relative, text)
                code, out = self.run_check()
                self.assertEqual(code, 1, out)
                self.assertIn(f"{name}: {before + 1}, baseline {before}.", out)
                self.assertIn(f"Rose: {name}.", out)

    def test_a_fall_fails_until_it_is_recorded(self):
        self.run_check("--write")
        (self.root / "crates/ferrite-cockpit/src/cockpit/nav.rs").unlink()
        code, out = self.run_check()
        self.assertEqual(code, 1, out)
        self.assertIn("files holding WeakEntity<CockpitView>: 0, baseline 1.", out)
        self.assertIn("Fell: files holding WeakEntity<CockpitView>, files with impl CockpitView.", out)
        self.assertIn("run `python3 scripts/check_tangle.py --write` and commit scripts/tangle.json", out)
        self.run_check("--write")
        self.assertEqual(self.run_check()[0], 0)

    def test_comments_and_literals_are_not_counted(self):
        self.write("crates/ferrite-cockpit/src/cockpit.rs", "struct CockpitView { /* } */ first: bool, second: bool, }\n")
        self.write("crates/ferrite-cockpit/src/cockpit/nav.rs", "")
        self.write("crates/ferrite-cockpit/src/cockpit/tests.rs", '''
// add_cockpit_window(cx)
/* outer /* nested */ add_cockpit_window(cx); impl CockpitView { */
/// A Nav holds a WeakEntity<CockpitView>.
const SAID: &str = "add_cockpit_window(cx) WeakEntity<CockpitView>";
const RAW: &str = r#"add_cockpit_window(" "#;
fn quote() -> char { '"' }
fn opens<'a>(cx: &'a mut Cx) { add_cockpit_window(cx); }
''')
        self.run_check("--write")
        counts = json.loads(self.baseline.read_text())
        self.assertEqual((counts["CockpitView fields"], counts["add_cockpit_window calls"],
                          counts["files holding WeakEntity<CockpitView>"], counts["files with impl CockpitView"]),
                         (2, 1, 0, 0))

    def test_a_count_missing_from_the_baseline_fails(self):
        self.baseline.write_text("{}")
        code, out = self.run_check()
        self.assertEqual(code, 1)
        self.assertIn("files holding WeakEntity<CockpitView>: 1, baseline 0.", out)


if __name__ == "__main__":
    unittest.main()

import io
import json
from pathlib import Path
import tempfile
import unittest

from check_core_sides import main, named


class NamedTests(unittest.TestCase):
    def test_reads_plain_and_nested_paths(self):
        source = "use ferrite_core::store::Store;\nuse ferrite_core::{\n    activity::{A, B},\n    engine,\n    thread::ThreadId,\n};"
        self.assertEqual(named(source, "ferrite_core"), ["store", "activity", "engine", "thread"])


class SideTests(unittest.TestCase):
    def run_check(self, sources: dict[str, str], crossings: dict[str, int], *flags: str) -> tuple[int, str, Path]:
        root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        for name, text in {"ferrite-core/src/thread.rs": "", "ferrite-core/src/engine.rs": "", **sources}.items():
            path = root / "crates" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        sides = root / "core-sides.json"
        sides.write_text(json.dumps({
            "shared": ["thread"], "engine": ["engine"], "window": ["ferrite-client"], "crossings": crossings,
        }), encoding="utf-8")
        out = io.StringIO()
        return main(list(flags), root=root, sides_file=sides, out=out), out.getvalue(), sides

    def test_the_engine_may_name_what_is_shared(self):
        code, _, _ = self.run_check({"ferrite-core/src/engine.rs": "use crate::thread::ThreadId;"}, {})
        self.assertEqual(code, 0)

    def test_a_shared_module_naming_the_engine_fails(self):
        code, said, _ = self.run_check({"ferrite-core/src/thread.rs": "use crate::engine::Engine;"}, {})
        self.assertEqual(code, 1)
        self.assertIn("thread -> engine: 1 crossings, the baseline allows 0", said)

    def test_a_window_crate_naming_the_engine_fails(self):
        code, said, _ = self.run_check({"ferrite-client/src/lib.rs": "use ferrite_core::{thread::ThreadId, engine::Engine};"}, {})
        self.assertEqual(code, 1)
        self.assertIn("ferrite-client -> engine: 1 crossings", said)

    def test_a_baselined_crossing_passes_and_one_more_fails(self):
        once = "use crate::engine::Engine;\n"
        self.assertEqual(self.run_check({"ferrite-core/src/thread.rs": once}, {"thread -> engine": 1})[0], 0)
        self.assertEqual(self.run_check({"ferrite-core/src/thread.rs": once * 2}, {"thread -> engine": 1})[0], 1)

    def test_a_removed_crossing_must_be_recorded(self):
        code, said, _ = self.run_check({}, {"thread -> engine": 1})
        self.assertEqual(code, 1)
        self.assertIn("Record it with --write", said)

    def test_tests_fixtures_and_comments_are_not_counted(self):
        sources = {
            "ferrite-core/src/thread.rs": "// crate::engine\n#[cfg(test)]\nmod tests {\n    use crate::engine::Engine;\n}\n",
            "ferrite-client/src/cockpit/tests.rs": "use ferrite_core::engine::Engine;",
            "ferrite-client/src/cockpit/factory_fixture.rs": "use ferrite_core::engine::Engine;",
        }
        self.assertEqual(self.run_check(sources, {})[0], 0)

    def test_a_module_on_no_side_fails(self):
        code, said, _ = self.run_check({"ferrite-core/src/novel.rs": ""}, {})
        self.assertEqual(code, 1)
        self.assertIn("novel: is on no side", said)

    def test_write_records_what_was_read(self):
        code, _, sides = self.run_check({"ferrite-core/src/thread.rs": "use crate::engine::Engine;"}, {}, "--write")
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(sides.read_text(encoding="utf-8"))["crossings"], {"thread -> engine": 1})


if __name__ == "__main__":
    unittest.main()

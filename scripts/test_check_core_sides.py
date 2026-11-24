import io
import json
from pathlib import Path
import tempfile
import unittest

from check_core_sides import handed_on, holds, main, named


class NamedTests(unittest.TestCase):
    def test_reads_plain_and_nested_paths(self):
        source = "use ferrite_core::store::Store;\nuse ferrite_core::{\n    activity::{A, B},\n    engine,\n    thread::ThreadId,\n};"
        self.assertEqual(named(source, "ferrite_core"), ["store", "activity", "engine", "thread"])

    def test_reads_what_a_pub_use_hands_on(self):
        self.assertEqual(handed_on("ferrite_core::engine::{Engine, Spawner as Spawn, x::{A, B}}"), [("engine", 4)])
        self.assertEqual(handed_on("ferrite_core::{engine::Engine, store::{self, Store}}"), [("engine", 1), ("store", 2)])


class CfgTests(unittest.TestCase):
    def test_a_cfg_fails_without_test_or_an_unshipped_feature(self):
        shipped = {"cef"}
        self.assertIs(holds("test", shipped), False)
        self.assertIs(holds('any(test, feature = "test-support")', shipped), False)
        self.assertIs(holds('all(feature = "cef", not(test))', shipped), True)
        self.assertIs(holds('feature = "cef"', shipped), True)
        self.assertIsNone(holds("windows", shipped))
        self.assertIsNone(holds('any(windows, feature = "headless")', shipped))
        self.assertIs(holds('all(windows, feature = "headless")', shipped), False)


class SideTests(unittest.TestCase):
    def run_check(self, sources: dict[str, str], crossings: dict[str, int], *flags: str) -> tuple[int, str, Path]:
        root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        for name, text in {"ferrite-core/src/thread.rs": "", "ferrite-core/src/engine.rs": "", **sources}.items():
            path = root / "crates" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        sides = root / "core-sides.json"
        sides.write_text(json.dumps({
            "shared": ["thread"], "engine": ["engine"], "window": ["ferrite-client"], "shipped": ["cef"],
            "crossings": crossings,
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

    def test_what_a_released_app_never_compiles_is_not_counted(self):
        engine = "use ferrite_core::engine::Engine;\n"
        sources = {
            "ferrite-core/src/thread.rs": "// crate::engine\n#[cfg(test)]\nmod tests {\n    use crate::engine::Engine;\n}\n",
            "ferrite-client/src/lib.rs": (
                '#[cfg(test)]\nmod tests;\n#[cfg(any(test, feature = "test-support"))]\n/// The fixture.\npub mod fixture;\n'
                '#[cfg(feature = "demo")]\nmod demo;\n'
            ),
            "ferrite-client/src/tests.rs": engine,
            "ferrite-client/src/fixture.rs": f'#[path = "far/world.rs"]\nmod world;\n{engine}',
            "ferrite-client/src/far/world.rs": engine,
            "ferrite-client/src/demo.rs": engine,
        }
        self.assertEqual(self.run_check(sources, {})[0], 0)

    def test_a_name_or_a_shipped_feature_skips_nothing(self):
        engine = "use ferrite_core::engine::Engine;\n"
        sources = {
            "ferrite-client/src/lib.rs": '#[cfg(feature = "cef")]\nmod visual;\nmod world_fixture;\n#[cfg(windows)]\nmod native;\n',
            "ferrite-client/src/visual.rs": engine,
            "ferrite-client/src/world_fixture.rs": engine,
            "ferrite-client/src/native.rs": engine,
            "ferrite-client/src/unreached.rs": engine,
        }
        code, said, _ = self.run_check(sources, {})
        self.assertEqual(code, 1)
        self.assertIn("ferrite-client -> engine: 4 crossings", said)

    def test_a_pub_use_counts_each_item_it_hands_on(self):
        sources = {"ferrite-client/src/lib.rs": "pub use ferrite_core::engine::{Engine, Spawner};\nuse ferrite_core::engine::Launch;\n"}
        code, said, _ = self.run_check(sources, {})
        self.assertEqual(code, 1)
        self.assertIn("ferrite-client -> engine: 3 crossings", said)

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

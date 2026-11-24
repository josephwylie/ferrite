import io
import json
from pathlib import Path
import tempfile
import unittest

from check_core_layers import main


class LayerTests(unittest.TestCase):
    def run_check(self, sources: dict[str, str], upward: dict[str, int], *flags: str) -> tuple[int, str, Path]:
        root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        core = root / "src"
        for name, text in sources.items():
            path = core / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        layers = root / "core-layers.json"
        layers.write_text(json.dumps({
            "layers": [{"name": "low", "modules": ["clock"]}, {"name": "high", "modules": ["engine"]}],
            "upward": upward,
        }), encoding="utf-8")
        out = io.StringIO()
        return main(list(flags), core=core, layers_file=layers, out=out), out.getvalue(), layers

    def test_a_downward_reference_passes(self):
        code, _, _ = self.run_check({"clock.rs": "", "engine.rs": "use crate::clock::Clock;"}, {})
        self.assertEqual(code, 0)

    def test_a_new_upward_reference_fails_and_names_both_modules(self):
        code, said, _ = self.run_check({"clock.rs": "use crate::engine::Engine;", "engine.rs": ""}, {})
        self.assertEqual(code, 1)
        self.assertIn("clock -> engine: 1 upward references, the baseline allows 0", said)

    def test_a_baselined_reference_passes_and_one_more_fails(self):
        once = "use crate::engine::Engine;\n"
        self.assertEqual(self.run_check({"clock.rs": once, "engine.rs": ""}, {"clock -> engine": 1})[0], 0)
        self.assertEqual(self.run_check({"clock.rs": once * 2, "engine.rs": ""}, {"clock -> engine": 1})[0], 1)

    def test_a_removed_reference_must_be_recorded(self):
        code, said, _ = self.run_check({"clock.rs": "", "engine.rs": ""}, {"clock -> engine": 1})
        self.assertEqual(code, 1)
        self.assertIn("Record it with --write", said)

    def test_tests_and_comments_are_not_counted(self):
        sources = {
            "clock.rs": "// crate::engine\n#[cfg(test)]\nmod tests {\n    use crate::engine::Engine;\n}\n",
            "clock/tests.rs": "use crate::engine::Engine;",
            "engine.rs": "",
        }
        self.assertEqual(self.run_check(sources, {})[0], 0)

    def test_a_module_with_no_layer_fails(self):
        code, said, _ = self.run_check({"clock.rs": "", "engine.rs": "", "novel.rs": ""}, {})
        self.assertEqual(code, 1)
        self.assertIn("novel: has no layer", said)

    def test_write_records_what_was_read(self):
        code, _, layers = self.run_check({"clock.rs": "use crate::engine::Engine;", "engine.rs": ""}, {}, "--write")
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(layers.read_text(encoding="utf-8"))["upward"], {"clock -> engine": 1})


if __name__ == "__main__":
    unittest.main()

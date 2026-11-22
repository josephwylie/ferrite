"""The boundaries probe's held-gesture check, without a window or a provider."""
import base64
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "browser_window_boundaries", Path(__file__).with_name("browser-window-boundaries.py"))
boundaries = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(boundaries)

PNG = base64.b64encode(b"\x89PNG\r\n\x1a\n").decode()


def screenshot(protected):
    state = {"state": {"active": 1, "tabs": [
        {"id": 2, "protected": True},
        {"id": 1, "protected": protected},
    ]}, "revision": 5}
    return {"isError": False, "content": [
        {"type": "text", "text": json.dumps(state)},
        {"type": "image", "data": PNG},
    ]}


class HeldScreenshot(unittest.TestCase):
    def run_held_screenshot(self, protected):
        with tempfile.TemporaryDirectory() as output:
            calls, finished = [], []
            boundary = boundaries.Boundary(
                lambda name, operation: calls.append((name, operation)),
                finished.append, Path(output))
            boundary.a = {"state": {"active": 1, "tabs": [{"id": 1, "generation": 2}]},
                          "revision": 3, "observation": "seen"}
            boundary.target = "e1"
            boundary.send({"kind": "control", "control": {"kind": "screenshot"}}, boundary.held_image)
            boundary.receive(calls[-1][0], screenshot(protected))
            results = json.loads((Path(output) / "boundary-results.json").read_text()) if finished else None
            return calls, results

    def test_a_held_page_goes_on_to_the_old_agent_write(self):
        calls, results = self.run_held_screenshot(True)
        self.assertIsNone(results)
        self.assertEqual(calls[-1][1]["control"], {"kind": "click", "reference": "e1"})

    def test_a_dropped_hold_fails_the_run_at_the_held_screenshot(self):
        calls, results = self.run_held_screenshot(False)
        self.assertEqual(len(calls), 1, "no click is sent after the hold dropped")
        self.assertFalse(results["success"])
        self.assertIn("lost its protection", results["error"])


if __name__ == "__main__":
    unittest.main()

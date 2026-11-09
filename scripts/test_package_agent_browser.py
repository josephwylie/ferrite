"""Exercise the artifact packager through its build-time command line."""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("verify-agent-browser-release.py")


class PackageAgentBrowser(unittest.TestCase):
    def test_corrupt_cached_release_is_rejected_without_network(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary)
            executable = destination / "agent-browser.exe"
            executable.write_bytes(b"untrusted release")
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--platform", "windows", "--arch", "x64",
                 "--destination", str(destination), "--offline"],
                capture_output=True, text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("SHA-256 mismatch", result.stderr)
            self.assertEqual(executable.read_bytes(), b"untrusted release")

    def test_missing_offline_release_is_not_downloaded(self):
        with tempfile.TemporaryDirectory() as destination:
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--platform", "darwin", "--arch", "arm64",
                 "--destination", destination, "--offline"],
                capture_output=True, text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("staged native executable not found", result.stderr)
            self.assertEqual(list(Path(destination).iterdir()), [])


if __name__ == "__main__":
    unittest.main()

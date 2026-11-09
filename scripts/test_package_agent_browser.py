"""Exercise the shipped packager's offline integrity boundary without a build."""
import hashlib
import json
import os
import platform
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("package-agent-browser.py")
PATCH = Path(__file__).parent / "patches/agent-browser-0.41.2-ferrite.patch"
FIXTURE_HASH = "44426d43bf5e69c1bd0ee2d8bb7837ce25d54ab2dd7737eeae1a990a07483f7e"


class PackageAgentBrowser(unittest.TestCase):
    def stage(self, destination, metadata=None):
        executable = destination / ("agent-browser.exe" if os.name == "nt" else "agent-browser")
        executable.write_bytes(b"verified native fixture")
        metadata = metadata or destination
        metadata.mkdir(exist_ok=True)
        provenance = metadata / "agent-browser-build.json"
        provenance.write_text(json.dumps({
            "version": "0.41.2",
            "source_commit": "d52a5e90c372ec4710c69adc3837ac14bbd852aa",
            "patch_sha256": hashlib.sha256(PATCH.read_bytes()).hexdigest(),
            "platform": platform.system(),
            "arch": platform.machine(),
            "binary_sha256": FIXTURE_HASH,
        }))
        licenses = metadata / "agent-browser-licenses"
        licenses.mkdir(exist_ok=True)
        (licenses / "LICENSE").write_text("Inert test license fixture")
        return executable, provenance

    def verify(self, destination, metadata=None):
        command = [sys.executable, str(SCRIPT), "--destination", str(destination), "--offline"]
        if metadata is not None:
            command.extend(["--metadata-directory", str(metadata)])
        return subprocess.run(command, capture_output=True, text=True)

    def test_corrupt_staged_controller_is_rejected_without_overwriting_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary)
            executable, _ = self.stage(destination)
            executable.write_bytes(b"tampered native fixture")
            result = self.verify(destination)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("SHA-256 mismatch", result.stderr)
            self.assertEqual(executable.read_bytes(), b"tampered native fixture")

    def test_wrong_source_or_hardening_patch_provenance_is_rejected(self):
        for field, value in (("source_commit", "0" * 40), ("patch_sha256", "0" * 64)):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                destination = Path(temporary)
                _, provenance = self.stage(destination)
                data = json.loads(provenance.read_text())
                data[field] = value
                provenance.write_text(json.dumps(data))
                result = self.verify(destination)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("provenance or SHA-256 mismatch", result.stderr)

    def test_separate_bundle_metadata_directory_verifies_the_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary)
            metadata = destination / "Resources"
            self.stage(destination, metadata)
            result = self.verify(destination, metadata)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["binary_sha256"], FIXTURE_HASH)
            self.assertFalse((destination / "agent-browser-build.json").exists())

    def copy_stage(self, source, destination, metadata):
        # The staging path must work with no git, Rust compiler or native
        # executable available. Python itself is invoked by absolute path.
        environment = {key: value for key, value in os.environ.items() if key.casefold() != "path"}
        environment["PATH"] = str(destination / "no-build-tools")
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--destination", str(destination),
             "--metadata-directory", str(metadata), "--from-stage", str(source)],
            capture_output=True, text=True, env=environment,
        )

    def test_verified_build_stages_without_compiler_and_preserves_provenance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "built"
            source.mkdir()
            executable, provenance = self.stage(source)
            destination = root / "bundle" / "MacOS"
            metadata = root / "bundle" / "Resources"
            result = self.copy_stage(source, destination, metadata)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((destination / executable.name).read_bytes(), executable.read_bytes())
            self.assertEqual((metadata / provenance.name).read_bytes(), provenance.read_bytes())
            self.assertEqual((metadata / "agent-browser-licenses" / "LICENSE").read_text(), "Inert test license fixture")
            self.assertFalse((destination / provenance.name).exists())

    def test_staging_rejects_wrong_source_patch_platform_architecture_or_binary(self):
        cases = (("source_commit", "0" * 40), ("patch_sha256", "0" * 64),
                 ("platform", "unsupported fixture platform"), ("arch", "unsupported fixture arch"),
                 ("binary_sha256", "0" * 64))
        for field, value in cases:
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source = root / "built"
                source.mkdir()
                _, provenance = self.stage(source)
                data = json.loads(provenance.read_text())
                data[field] = value
                provenance.write_text(json.dumps(data))
                destination = root / "bundle"
                result = self.copy_stage(source, destination, root / "Resources")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("provenance or SHA-256 mismatch", result.stderr)
                self.assertFalse(destination.exists())

    def test_missing_offline_build_is_not_downloaded_or_built(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary)
            result = self.verify(destination)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("staged native controller unavailable", result.stderr)
            self.assertEqual(list(destination.iterdir()), [])


if __name__ == "__main__":
    unittest.main()

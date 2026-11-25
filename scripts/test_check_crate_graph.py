import io
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from check_crate_graph import main


def path(crate: str) -> str:
    return f'{crate} = {{ path = "../{crate}" }}'


class CrateGraphTests(unittest.TestCase):
    """Each test resolves a small workspace with Cargo, as the check does."""

    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        (self.root / "Cargo.toml").write_text('[workspace]\nresolver = "2"\nmembers = ["crates/*"]\n')
        for crate in ("ferrite-cockpit", "ferrite-engine", "ferrite-server", "ferrite-ui"):
            self.write_crate(crate)
        # Its own tests run the Engine; a crate built on it does not.
        self.write_crate("ferrite-pane", f"[dependencies]\n{path('ferrite-ui')}\n[dev-dependencies]\n{path('ferrite-engine')}\n")

    def write_crate(self, name: str, manifest: str = ""):
        directory = self.root / "crates" / name
        (directory / "src").mkdir(parents=True, exist_ok=True)
        (directory / "src" / "lib.rs").write_text("")
        (directory / "Cargo.toml").write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\n{manifest}')

    def run_check(self) -> tuple[int, str]:
        subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=self.root, check=True, capture_output=True)
        out = io.StringIO()
        return main(["--root", str(self.root)], out), out.getvalue()

    def test_passes_before_any_feature_crate_exists(self):
        self.assertEqual(self.run_check(), (0, "Feature crates checked: none yet.\n"))

    def test_a_feature_crate_may_build_on_a_crate_whose_own_tests_run_the_engine(self):
        self.write_crate("ferrite-review", f"[dependencies]\n{path('ferrite-pane')}\n[dev-dependencies]\n{path('ferrite-ui')}\n")
        self.assertEqual(self.run_check(), (0, "Feature crates checked: ferrite-review.\n"))

    def test_a_feature_crate_reaching_the_shell_by_any_edge_fails(self):
        self.write_crate("helper", f"[dependencies]\n{path('ferrite-engine')}\n")
        reaches = {
            "a dependency": (f"[dependencies]\n{path('ferrite-cockpit')}\n", "ferrite-cockpit"),
            "its own tests": (f"[dev-dependencies]\n{path('ferrite-engine')}\n", "ferrite-engine"),
            "a build script": (f"[build-dependencies]\n{path('ferrite-server')}\n", "ferrite-server"),
            # Not the host's, whichever machine runs this.
            "another platform": (f"[target.'cfg(target_os = \"android\")'.dependencies]\n{path('ferrite-server')}\n",
                                 "ferrite-server"),
            "a feature": ('[features]\nwindow = ["dep:ferrite-cockpit"]\n[dependencies]\n'
                          'ferrite-cockpit = { path = "../ferrite-cockpit", optional = true }\n', "ferrite-cockpit"),
            "another crate": (f"[dependencies]\n{path('helper')}\n", "ferrite-engine"),
        }
        for edge, (manifest, shell) in reaches.items():
            with self.subTest(edge=edge):
                self.write_crate("ferrite-factory", manifest)
                code, out = self.run_check()
                self.assertEqual(code, 1, out)
                self.assertIn(f"ferrite-factory reaches {shell}, which a feature crate may not:", out)
                self.assertIn(f"{shell} v0.1.0", out)

    def test_a_feature_crate_may_not_reach_another(self):
        self.write_crate("ferrite-review")
        self.write_crate("ferrite-prs", f"[dependencies]\n{path('ferrite-review')}\n")
        code, out = self.run_check()
        self.assertEqual(code, 1, out)
        self.assertIn("ferrite-prs reaches ferrite-review", out)
        self.assertNotIn("ferrite-review reaches", out)

    def test_a_feature_crate_outside_the_workspace_is_still_forbidden(self):
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nresolver = "2"\nmembers = ["crates/*"]\nexclude = ["crates/ferrite-prs"]\n')
        self.write_crate("ferrite-prs")
        self.write_crate("ferrite-review", f"[dependencies]\n{path('ferrite-prs')}\n")
        code, out = self.run_check()
        self.assertEqual(code, 1, out)
        self.assertIn("ferrite-review reaches ferrite-prs", out)

    def test_a_missing_shell_crate_fails_instead_of_checking_nothing(self):
        shutil.rmtree(self.root / "crates" / "ferrite-server")
        code, out = self.run_check()
        self.assertEqual(code, 1)
        self.assertIn("Not in the workspace, so nothing would be checked against it: ferrite-server", out)


if __name__ == "__main__":
    unittest.main()

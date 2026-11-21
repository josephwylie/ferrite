import contextlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from ci_scope import Package, changed_paths, changes_cef_gate, check_boundaries, main, select, workspace


ROOT = Path(__file__).resolve().parents[1]


class SelectionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.packages = workspace(ROOT)
        cls.vendored = sorted(name for name, package in cls.packages.items() if package.directory.startswith("vendor/"))
        cls.ferrite = sorted(set(cls.packages) - set(cls.vendored))

    def test_every_workspace_member_selects_its_tests(self):
        for name, package in self.packages.items():
            with self.subTest(package=name):
                result = select(self.packages, [f"{package.directory}/src/lib.rs"])
                self.assertIn(name, result["packages"] + result["vendored"])
                self.assertEqual(set(result["decisions"]), set(self.packages))

    def test_app_change_does_not_select_dependencies(self):
        self.assertEqual(select(self.packages, ["crates/ferrite/src/lib.rs"])["packages"], ["ferrite"])

    def test_terminal_selects_reverse_dependencies_only(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-terminal/src/lib.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-terminal"])

    def test_focus_notify_is_tested_with_its_consumers(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-focus-notify/src/lib.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-focus-notify"])

    def test_frame_selects_cockpit_dev_dependency(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-frame/src/lib.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-frame"])

    def test_core_selects_transitive_consumers(self):
        result = select(self.packages, ["crates/ferrite-core/src/server/browser.rs"])
        self.assertIn("ferrite-terminal", result["packages"])
        self.assertIn("ferrite-frame", result["packages"])
        self.assertNotIn("ferrite-focus-notify", result["packages"])
        self.assertNotIn("ferrite-reviewr", result["packages"])
        self.assertTrue(result["browser"])

    def test_browser_inputs_missing_from_old_filter_select_browser(self):
        paths = ["Cargo.lock", "crates/ferrite-pane/Cargo.toml", "crates/ferrite-cockpit/Cargo.toml",
                 "crates/ferrite-core/src/server/browser.rs", "vendor/gpui-base/src/lib.rs"]
        for path in paths:
            with self.subTest(path=path):
                self.assertTrue(select(self.packages, [path])["browser"])

    def test_shared_and_unknown_inputs_fail_closed(self):
        paths = ["Cargo.lock", "Cargo.toml", "vendor/new-package/src/lib.rs", "scripts/install-app.sh",
                 "scripts/stage-windows.ps1", "scripts/package-agent-browser.py", ".cargo/config.toml",
                 ".config/nextest.toml", ".github/workflows/ci.yml", ".github/workflows/browser-window.yml",
                 "resources/icon.ico", "new-build-input", "crates/removed/src/lib.rs"]
        for path in paths:
            with self.subTest(path=path):
                result = select(self.packages, [path])
                self.assertEqual(result["packages"], self.ferrite)
                self.assertEqual(result["vendored"], self.vendored)

    def test_packaged_browser_runs_for_what_only_it_sees_whole(self):
        paths = ["crates/ferrite-cockpit/src/cockpit/browser/controls.rs", "crates/ferrite-cockpit/src/cockpit/browser_smoke.rs",
                 "crates/ferrite-core/src/browser_annotations.rs", "crates/ferrite-pane/src/visual/engine.rs",
                 "crates/ferrite-pane/src/annotation_editor.rs", "crates/ferrite/src/lib.rs", "crates/ferrite-ui/Cargo.toml",
                 "Cargo.lock", "vendor/gpui-pre/src/window.rs", "scripts/install-app.sh", "scripts/probe-browser-window.py",
                 "scripts/patches/agent-browser-0.41.2-ferrite.patch", ".github/workflows/browser-window.yml"]
        for path in paths:
            with self.subTest(path=path):
                self.assertTrue(select(self.packages, [path])["browser"])

    def test_other_changes_leave_the_packaged_browser_to_main(self):
        # These still select the ferrite package, so ci.yml checks the CEF builds.
        paths = ["crates/ferrite-cockpit/src/cockpit.rs", "crates/ferrite-cockpit/src/cockpit/decisions.rs",
                 "crates/ferrite-core/src/server.rs", "crates/ferrite-ui/src/lib.rs", "scripts/raw-presses.json",
                 ".github/workflows/ci.yml", ".config/nextest.toml", "skills/verify-ferrite/features/decisions.md"]
        for path in paths:
            with self.subTest(path=path):
                result = select(self.packages, [path])
                self.assertIn("ferrite", result["packages"])
                self.assertFalse(result["browser"])

    def test_full_ci_selects_windows_and_the_label_selects_the_packaged_browser(self):
        path = ["crates/ferrite-ui/src/lib.rs"]
        full = select(self.packages, path, full=True)
        self.assertEqual(full["packages"], self.ferrite)
        self.assertFalse(full["browser"])
        self.assertTrue(select(self.packages, path, browser=True)["browser"])

    def test_vendored_crates_are_members_kept_out_of_package_runs(self):
        self.assertEqual(self.vendored, ["gpui-base", "gpui-component", "gpui-pre", "gpui-pre-macos", "taffy"])
        self.assertFalse(set(self.vendored) & set(self.ferrite))

    def test_vendored_source_selects_every_package(self):
        # Ferrite reaches the vendored crates through crates.io gpui-kit, not a workspace edge.
        result = select(self.packages, ["vendor/gpui-pre/src/window.rs"])
        self.assertEqual(result["packages"], self.ferrite)
        self.assertEqual(result["vendored"], self.vendored)

    def test_crate_change_does_not_select_vendored_tests(self):
        result = select(self.packages, ["crates/ferrite-ui/src/lib.rs"])
        self.assertEqual(result["vendored"], [])
        self.assertIn("ferrite-ui", result["packages"])

    def test_documentation_only_explicitly_skips_checks(self):
        result = select(self.packages, ["README.md", "AGENTS.md", "docs/audit.md", "docs/image.png"])
        self.assertFalse(result["code"])
        self.assertFalse(result["browser"])
        self.assertEqual(result["packages"], [])
        self.assertEqual(result["vendored"], [])
        self.assertFalse(any(result["decisions"].values()))
        self.assertEqual(result["reasons"], ["documentation only"])

    def test_markdown_inside_packages_and_vendor_is_not_assumed_documentation(self):
        for path in ["crates/ferrite-core/tests/fixtures/message.md", "vendor/gpui-base/README.md"]:
            with self.subTest(path=path):
                self.assertTrue(select(self.packages, [path])["code"])

    def test_empty_diff_and_explicit_full_cannot_skip(self):
        for paths, full in [([], False), (["README.md"], True)]:
            with self.subTest(paths=paths, full=full):
                result = select(self.packages, paths, full)
                self.assertEqual(result["packages"], self.ferrite)
                self.assertEqual(result["vendored"], self.vendored)
        self.assertTrue(select(self.packages, [])["browser"])

    def test_new_member_receives_a_decision_without_selector_changes(self):
        packages = dict(self.packages, helper=Package("crates/helper", frozenset()))
        result = select(packages, ["crates/helper/src/lib.rs"])
        self.assertEqual(result["packages"], ["helper"])
        self.assertFalse(result["browser"])
        self.assertTrue(result["decisions"]["helper"])


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "Cargo.toml").write_text('''[workspace]
members = ["crates/*"]
[workspace.dependencies]
renamed = { package = "gpui-kit", version = "1" }
''')
        self.write_package("ferrite-core")
        self.write_package("ferrite-frame")
        self.write_package("ferrite-cockpit")

    def write_package(self, name, dependencies=""):
        directory = self.root / "crates" / name
        directory.mkdir(parents=True, exist_ok=True)
        (directory / "Cargo.toml").write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\n{dependencies}')

    def test_real_workspace_obeys_boundaries(self):
        check_boundaries(workspace(ROOT))

    def test_gpui_alias_target_optional_and_build_edges_are_forbidden(self):
        for table in ["dependencies", "dev-dependencies", "build-dependencies", "target.'cfg(windows)'.dependencies"]:
            with self.subTest(table=table):
                self.write_package("ferrite-core", f'[{table}]\nrenamed = {{ workspace = true, optional = true }}\n')
                with self.assertRaisesRegex(ValueError, "ferrite-core -> gpui-kit"):
                    check_boundaries(workspace(self.root))

    def test_transitive_core_gpui_edge_is_forbidden(self):
        self.write_package("ferrite-core", '[dependencies]\nhelper = { path = "../helper" }\n')
        self.write_package("helper", '[dependencies]\nui = { package = "gpui-base", version = "1" }\n')
        with self.assertRaisesRegex(ValueError, "ferrite-core -> helper -> gpui-base"):
            select(workspace(self.root), ["README.md"])

    def test_implicit_path_member_fails_closed_then_checks_boundary_when_registered(self):
        for table in ["dependencies", "dev-dependencies", "build-dependencies", "target.'cfg(windows)'.dependencies"]:
            for inherited in [False, True]:
                with self.subTest(table=table, inherited=inherited):
                    manifest = '''[workspace]
members = ["crates/ferrite-core", "crates/ferrite-frame", "crates/ferrite-cockpit"]
[workspace.dependencies]
renamed = { package = "helper", path = "crates/helper" }
'''
                    (self.root / "Cargo.toml").write_text(manifest)
                    value = "workspace = true" if inherited else 'package = "helper", path = "../helper"'
                    self.write_package("ferrite-core", f'[{table}]\nrenamed = {{ {value} }}\n')
                    self.write_package("helper", '[dependencies]\nui = { package = "gpui-base", version = "1" }\n')
                    with self.assertRaisesRegex(ValueError, r"Untracked in-tree path dependency: ferrite-core -> renamed \(crates/helper\).*workspace.members"):
                        select(workspace(self.root), ["README.md"])
                    (self.root / "Cargo.toml").write_text(manifest.replace('"crates/ferrite-core",', '"crates/helper", "crates/ferrite-core",'))
                    with self.assertRaisesRegex(ValueError, "ferrite-core -> helper -> gpui-base"):
                        select(workspace(self.root), ["README.md"])
                    self.write_package("helper")
                    self.assertEqual(select(workspace(self.root), ["crates/helper/src/lib.rs"])["packages"],
                                     ["ferrite-core", "helper"])

    def test_excluded_path_dependency_cannot_bypass_boundary_checks(self):
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/*"]\nexclude = ["crates/helper"]\n')
        self.write_package("ferrite-core", '[dependencies]\nhelper = { path = "../helper" }\n')
        self.write_package("helper", '[dependencies]\ngpui = "1"\n')
        with self.assertRaisesRegex(ValueError, "Untracked in-tree path dependency.*workspace.exclude"):
            select(workspace(self.root), ["README.md"])

    def test_transitive_frame_cockpit_edge_is_forbidden(self):
        self.write_package("ferrite-frame", '[dependencies]\nhelper = { path = "../helper" }\n')
        self.write_package("helper", '[dev-dependencies]\nwindow = { package = "ferrite-cockpit", path = "../ferrite-cockpit" }\n')
        with self.assertRaisesRegex(ValueError, "ferrite-frame -> helper -> ferrite-cockpit"):
            check_boundaries(workspace(self.root))

    def test_cycles_terminate_and_preserve_reverse_closure(self):
        self.write_package("a", '[dependencies]\nb = { path = "../b" }\n')
        self.write_package("b", '[dependencies]\na = { path = "../a" }\n')
        self.assertEqual(select(workspace(self.root), ["crates/a/src/lib.rs"])["packages"], ["a", "b"])

    def test_missing_member_fails_instead_of_skipping(self):
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/missing"]\n')
        with self.assertRaisesRegex(ValueError, "does not exist"):
            workspace(self.root)

    def test_github_output_matches_cli_decisions(self):
        output = self.root / "outputs"
        stdout = io.StringIO()
        with patch.object(sys, "argv", ["ci_scope.py", "--root", str(self.root), "--github-output", str(output)]), contextlib.redirect_stdout(stdout):
            main()
        result = json.loads(stdout.getvalue())
        self.assertEqual(result["packages"], ["ferrite-cockpit", "ferrite-core", "ferrite-frame"])
        self.assertEqual(output.read_text(), "code=true\nbrowser=true\nfull=true\npackages=ferrite-cockpit ferrite-core ferrite-frame\n"
                         "vendored=\n")

    def test_invalid_boundary_writes_no_skip_outputs(self):
        self.write_package("ferrite-core", '[dependencies]\ngpui = "1"\n')
        output = self.root / "outputs"
        with patch.object(sys, "argv", ["ci_scope.py", "--root", str(self.root), "--github-output", str(output)]):
            with self.assertRaisesRegex(ValueError, "Forbidden dependency"):
                main()
        self.assertFalse(output.exists())

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, check=True, capture_output=True, text=True).stdout.strip()

    def commit(self, message):
        if not (self.root / ".git").exists():
            self.git("init", "-q")
            self.git("config", "user.email", "ci-scope@example.invalid")
            self.git("config", "user.name", "CI scope test")
        self.git("add", ".")
        self.git("-c", "commit.gpgsign=false", "commit", "-qm", message)
        return self.git("rev-parse", "HEAD")

    def test_renamed_source_to_docs_keeps_deleted_source_in_diff(self):
        source = self.root / "crates/ferrite-core/source.rs"
        source.write_text("fixture")
        base = self.commit("base")
        (self.root / "docs").mkdir()
        source.rename(self.root / "docs/source.md")
        self.commit("rename")
        paths = changed_paths(self.root, base, "HEAD")
        self.assertIn("crates/ferrite-core/source.rs", paths)
        self.assertTrue(select(workspace(self.root), paths)["code"])

    def test_a_changed_cef_gate_selects_the_packaged_browser(self):
        source = self.root / "crates/ferrite-cockpit/src/lib.rs"
        source.parent.mkdir()
        source.write_text('fn quit() {}\n#[cfg(feature = "cef")]\nfn shutdown() {}\n')
        base = self.commit("base")
        source.write_text('fn quit() { save(); }\n#[cfg(feature = "cef")]\nfn shutdown() {}\n')
        edited = self.commit("edit beside the gate")
        self.assertFalse(changes_cef_gate(self.root, base, edited))
        source.write_text('fn quit() { save(); }\n#[cfg(feature = "cef")]\nfn shutdown() {}\n#[cfg(feature = "browser-smoke")]\nfn probe() {}\n')
        self.commit("add gated code")
        self.assertTrue(changes_cef_gate(self.root, edited, "HEAD"))
        output = self.root / "outputs"
        with patch.object(sys, "argv", ["ci_scope.py", "--root", str(self.root), "--base", edited, "--github-output", str(output)]), \
                contextlib.redirect_stdout(io.StringIO()):
            main()
        self.assertIn("browser=true\n", output.read_text())


if __name__ == "__main__":
    unittest.main()

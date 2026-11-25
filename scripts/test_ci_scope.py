import contextlib
import io
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from ci_scope import RULES, Package, changed_paths, changes_cef_gate, check_boundaries, main, select, workspace


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
                         ["ferrite", "ferrite-cockpit", "ferrite-terminal", "ferrite-tools"])

    def test_focus_notify_is_tested_with_its_consumers(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-focus-notify/src/lib.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-focus-notify"])

    def test_frame_selects_cockpit_dev_dependency(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-frame/src/lib.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-frame", "ferrite-settings"])

    def test_review_selects_the_window_that_lays_it_out(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-review/src/lib.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-review"])

    def test_factory_selects_the_window_that_makes_its_views(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-factory/src/archive.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-factory"])

    def test_tools_select_the_window_that_makes_them(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-tools/src/remote_files.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-tools"])

    def test_nav_selects_the_window_that_makes_it(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-nav/src/empty_board.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-nav"])

    def test_settings_selects_the_window_that_makes_its_views(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-settings/src/folder_browser.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-settings"])

    def test_prs_selects_the_window_that_makes_its_views(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-prs/src/side_chat.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-prs"])

    def test_core_selects_transitive_consumers(self):
        result = select(self.packages, ["crates/ferrite-core/src/browser.rs"])
        self.assertIn("ferrite-terminal", result["packages"])
        self.assertIn("ferrite-frame", result["packages"])
        self.assertNotIn("ferrite-focus-notify", result["packages"])
        self.assertNotIn("ferrite-reviewr", result["packages"])
        self.assertTrue(result["browser"])

    def test_engine_selects_the_server_the_app_and_the_tests_that_run_it(self):
        result = select(self.packages, ["crates/ferrite-engine/src/engine.rs"])["packages"]
        self.assertEqual(result, ["ferrite", "ferrite-client", "ferrite-cockpit", "ferrite-engine", "ferrite-factory",
                                  "ferrite-frame", "ferrite-nav", "ferrite-pane", "ferrite-prs", "ferrite-review",
                                  "ferrite-server", "ferrite-settings", "ferrite-tools"])

    def test_server_selects_the_app_and_the_tests_that_run_it(self):
        self.assertEqual(select(self.packages, ["crates/ferrite-server/src/calls.rs"])["packages"],
                         ["ferrite", "ferrite-cockpit", "ferrite-server"])

    def test_browser_inputs_missing_from_old_filter_select_browser(self):
        paths = ["Cargo.lock", "crates/ferrite-pane/Cargo.toml", "crates/ferrite-cockpit/Cargo.toml",
                 "crates/ferrite-server/src/browser.rs", "vendor/gpui-base/src/lib.rs"]
        for path in paths:
            with self.subTest(path=path):
                self.assertTrue(select(self.packages, [path])["browser"])

    def test_shared_and_unknown_inputs_fail_closed(self):
        paths = ["Cargo.lock", "Cargo.toml", "rust-toolchain.toml", "vendor/new-package/src/lib.rs", ".cargo/config.toml",
                 ".config/nextest.toml", ".github/workflows/ci.yml", "scripts/ci_scope.py", ".github/actions/setup/action.yml",
                 "resources/icon.ico", "assets/logo.png", ".gitattributes", "new-build-input", "crates/removed/src/lib.rs"]
        for path in paths:
            with self.subTest(path=path):
                result = select(self.packages, [path])
                self.assertEqual(result["packages"], self.ferrite)
                self.assertEqual(result["vendored"], self.vendored)
                self.assertTrue(result["checks"])
        # An input no rule knows may be one only the packaged run reads.
        for path in [".github/actions/setup/action.yml", "resources/icon.ico", "assets/logo.png", ".gitattributes",
                     "new-build-input", "crates/removed/src/lib.rs", "packaging/Ferrite.entitlements"]:
            with self.subTest(path=path):
                self.assertTrue(select(self.packages, [path])["browser"])

    def test_scripts_no_check_here_runs_select_nothing(self):
        # The scope job runs its own tests on every event; other workflows
        # and the release run the rest, or only people do.
        paths = ["scripts/test_ci_scope.py", "scripts/test_check_raw_presses.py", "scripts/test_check_tangle.py",
                 "scripts/test_check_crate_graph.py", "scripts/test_junit_summary.py", "scripts/test_quarantine.py",
                 "scripts/install-app.sh",
                 "scripts/package-agent-browser.py", "scripts/package-windows.ps1", "scripts/package-server.sh",
                 "scripts/tests/package-server.sh", "scripts/patches/agent-browser-0.41.2-ferrite.patch",
                 "scripts/probe-native-codex-queue.py", "scripts/warm-main.sh", "scripts/wsl-machine.ps1",
                 "scripts/git-hooks/post-commit", "scripts/bench/edit-loop.sh", "scripts/core-sides.json"]
        for path in paths:
            with self.subTest(path=path):
                result = select(self.packages, [path], windows=True)
                self.assertFalse(result["code"])
                self.assertEqual(result["packages"] + result["vendored"], [])

    def test_what_every_test_run_reads_selects_every_package(self):
        for path in [".config/quarantine.toml", "scripts/quarantine.py", "scripts/junit_summary.py"]:
            with self.subTest(path=path):
                result = select(self.packages, [path], windows=True)
                self.assertEqual(result["packages"], self.ferrite)
                self.assertEqual(result["vendored"], self.vendored)
                self.assertTrue(result["checks"])
                self.assertTrue(result["windows"])

    def test_the_untangling_guards_select_the_macos_checks_only(self):
        # No step names the tangle's baseline: check_tangle.py reads it.
        for path in ["scripts/check_tangle.py", "scripts/tangle.json", "scripts/check_crate_graph.py"]:
            with self.subTest(path=path):
                result = select(self.packages, [path], windows=True)
                self.assertTrue(result["code"])
                self.assertTrue(result["checks"])
                self.assertFalse(result["windows"])
                self.assertEqual(result["packages"] + result["vendored"], [])

    def test_the_raw_press_baseline_selects_the_macos_checks_only(self):
        for path in ["scripts/check_raw_presses.py", "scripts/raw-presses.json"]:
            with self.subTest(path=path):
                result = select(self.packages, [path], windows=True)
                self.assertTrue(result["code"])
                self.assertTrue(result["checks"])
                self.assertFalse(result["windows"])
                self.assertEqual(result["packages"] + result["vendored"], [])

    def test_the_windows_staging_scripts_select_the_windows_checks_only(self):
        for path in ["scripts/stage-windows.ps1", "scripts/copy-chromium-windows.ps1", "scripts/tests/windows-stage.ps1"]:
            with self.subTest(path=path):
                main = select(self.packages, [path], windows=True)
                self.assertTrue(main["windows"])
                self.assertFalse(main["checks"])
                self.assertEqual(main["packages"] + main["vendored"], [])
                # A PR leaves them to main, as it does every Windows check.
                self.assertFalse(select(self.packages, [path])["code"])

    def test_every_script_this_workflow_runs_has_its_own_rule(self):
        # The scripts/ rule selects nothing, so a script a new check runs
        # must say what it selects.
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        named = {path.rstrip(".") for path in re.findall(r"scripts/[\w./-]+", workflow)}
        self.assertIn("scripts/check_raw_presses.py", named)
        for path in sorted(named):
            with self.subTest(path=path):
                self.assertIn(path, {pattern for pattern, _ in RULES})

    def test_other_workflows_select_nothing_here(self):
        for path in [".github/workflows/release.yml", ".github/workflows/browser-controller.yml",
                     ".github/workflows/browser-window.yml"]:
            with self.subTest(path=path):
                self.assertFalse(select(self.packages, [path], windows=True)["code"])
        self.assertTrue(select(self.packages, [".github/workflows/browser-window.yml"])["browser"])

    def test_vendored_tests_run_only_for_vendored_code_and_what_builds_it(self):
        for path in ["vendor/gpui-pre/src/window.rs", "Cargo.toml", "Cargo.lock", ".cargo/config.toml",
                     ".config/nextest.toml", ".github/workflows/ci.yml", "scripts/ci_scope.py"]:
            with self.subTest(path=path):
                self.assertEqual(select(self.packages, [path])["vendored"], self.vendored)
        for path in ["crates/ferrite-core/src/lib.rs", "scripts/install-app.sh", "scripts/raw-presses.json",
                     ".github/workflows/browser-window.yml", "skills/ferrite/SKILL.md", "CONTEXT.md"]:
            with self.subTest(path=path):
                self.assertEqual(select(self.packages, [path])["vendored"], [])

    def test_a_file_a_member_embeds_selects_it(self):
        # The `ferrite skill` text, an engine fixture ferrite-core's tests
        # read, the README gpui-base's markdown test renders, and the test
        # programs' temporary folder, a module of every test program.
        for path, reader in [("skills/ferrite/SKILL.md", "ferrite-engine"),
                             ("crates/ferrite-engine/tests/fixtures/claude-report-findings-2.1.318.jsonl", "ferrite-core"),
                             ("README.md", "gpui-base"),
                             ("crates/ferrite-core/src/test_temp.rs", "ferrite-focus-notify")]:
            with self.subTest(path=path):
                result = select(self.packages, [path], windows=True)
                self.assertIn(reader, result["packages"] + result["vendored"])
        self.assertFalse(select(self.packages, ["skills/verify-ferrite/SKILL.md"])["code"])

    def test_vendored_crates_are_tested_together(self):
        # Their run names gpui-pre-macos's test-support feature, which Cargo
        # refuses unless gpui-pre-macos is selected. A file a vendored crate
        # reads is vendored code: its dependants reach it through crates.io.
        result = select(self.packages, ["README.md"])
        self.assertEqual(result["vendored"], self.vendored)
        self.assertEqual(result["packages"], self.ferrite)
        for name, package in self.packages.items():
            with self.subTest(package=name):
                self.assertIn(select(self.packages, [f"{package.directory}/src/lib.rs"])["vendored"], ([], self.vendored))

    def test_windows_runs_for_what_a_push_to_main_changed(self):
        crate = select(self.packages, ["crates/ferrite-server/src/calls.rs"], windows=True)
        self.assertTrue(crate["windows"])
        self.assertEqual(crate["packages"], ["ferrite", "ferrite-cockpit", "ferrite-server"])
        self.assertEqual(crate["vendored"], [])
        self.assertFalse(select(self.packages, ["docs/audit.md"], windows=True)["code"])
        self.assertFalse(select(self.packages, ["crates/ferrite-server/src/calls.rs"])["windows"])

    def test_packaged_browser_runs_for_what_only_it_sees_whole(self):
        paths = ["crates/ferrite-cockpit/src/cockpit/browser/controls.rs", "crates/ferrite-cockpit/src/cockpit/browser_smoke.rs",
                 "crates/ferrite-core/src/browser_annotations.rs", "crates/ferrite-pane/src/visual/engine.rs",
                 "crates/ferrite-pane/src/annotation_editor.rs", "crates/ferrite/src/lib.rs", "crates/ferrite-ui/Cargo.toml",
                 "Cargo.lock", "vendor/gpui-pre/src/window.rs", "scripts/install-app.sh", "scripts/probe-browser-window.py",
                 "scripts/patches/agent-browser-0.41.2-ferrite.patch", ".github/workflows/browser-window.yml",
                 # Each can break the macOS pass alone.
                 "crates/ferrite/assets/app-icon.png", "Cargo.toml", "crates/ferrite-pane/src/visual/web/cef/input.rs",
                 "crates/ferrite-pane/src/visual/web/cef/handlers.rs", "crates/ferrite-pane/src/visual/browser_control/native.rs",
                 "crates/ferrite-pane/src/visual/web/cef/profile.rs", "scripts/browser_resources.py",
                 "vendor/gpui-base/src/macos_accessibility.rs"]
        for path in paths:
            with self.subTest(path=path):
                self.assertTrue(select(self.packages, [path])["browser"])

    def test_other_changes_leave_the_packaged_browser_to_main(self):
        # These still run the macOS checks, so ci.yml checks the CEF builds.
        paths = ["crates/ferrite-cockpit/src/cockpit.rs", "crates/ferrite-cockpit/src/cockpit/decisions.rs",
                 "crates/ferrite-server/src/lib.rs", "crates/ferrite-ui/src/lib.rs", "scripts/raw-presses.json",
                 ".github/workflows/ci.yml", ".config/nextest.toml"]
        for path in paths:
            with self.subTest(path=path):
                result = select(self.packages, [path])
                self.assertTrue(result["checks"])
                self.assertFalse(result["browser"])

    def test_full_ci_selects_windows_and_the_label_selects_the_packaged_browser(self):
        path = ["crates/ferrite-ui/src/lib.rs"]
        full = select(self.packages, path, full=True)
        self.assertEqual(full["packages"], self.ferrite)
        self.assertTrue(full["windows"])
        self.assertFalse(full["browser"])
        self.assertFalse(select(self.packages, path)["windows"])
        self.assertTrue(select(self.packages, path, browser=True)["browser"])

    def test_vendored_crates_are_members_kept_out_of_package_runs(self):
        self.assertEqual(
            self.vendored,
            ["gpui-base", "gpui-component", "gpui-pre", "gpui-pre-macos", "gpui-pre-platform", "gpui-pre-windows", "taffy"],
        )
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
        result = select(self.packages, ["CONTEXT.md", "AGENTS.md", "docs/audit.md", "docs/image.png", "skills/verify-ferrite/SKILL.md"],
                        windows=True)
        self.assertFalse(result["code"])
        self.assertFalse(result["browser"])
        self.assertFalse(result["checks"])
        self.assertFalse(result["windows"])
        self.assertEqual(result["packages"], [])
        self.assertEqual(result["vendored"], [])
        self.assertFalse(any(result["decisions"].values()))
        self.assertEqual(result["reasons"], ["nothing this workflow checks changed"])

    def test_markdown_inside_packages_and_vendor_is_not_assumed_documentation(self):
        for path in ["crates/ferrite-engine/tests/fixtures/message.md", "vendor/gpui-base/README.md"]:
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
        self.write_package("ferrite-engine")
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
members = ["crates/ferrite-core", "crates/ferrite-engine", "crates/ferrite-frame", "crates/ferrite-cockpit"]
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

    def test_transitive_feature_crate_cockpit_edges_are_forbidden(self):
        self.write_package("helper", '[dev-dependencies]\nwindow = { package = "ferrite-cockpit", path = "../ferrite-cockpit" }\n')
        for source in ["ferrite-frame", "ferrite-review", "ferrite-factory", "ferrite-tools", "ferrite-nav",
                       "ferrite-settings", "ferrite-prs"]:
            with self.subTest(source=source):
                self.write_package(source, '[dependencies]\nhelper = { path = "../helper" }\n')
                with self.assertRaisesRegex(ValueError, f"{source} -> helper -> ferrite-cockpit"):
                    check_boundaries(workspace(self.root))
                self.write_package(source)

    def test_window_chain_takes_the_server_only_for_its_tests(self):
        self.write_package("ferrite-server")
        self.write_package("ferrite-cockpit", '[dev-dependencies]\nferrite-server = { path = "../ferrite-server" }\n')
        check_boundaries(workspace(self.root))
        for table in ["dependencies", "build-dependencies", "target.'cfg(windows)'.dependencies"]:
            with self.subTest(table=table):
                self.write_package("ferrite-pane", f'[{table}]\nhelper = {{ path = "../helper" }}\n')
                self.write_package("helper", '[dependencies]\nserver = { package = "ferrite-server", path = "../ferrite-server", optional = true }\n')
                with self.assertRaisesRegex(ValueError, "ferrite-pane -> helper -> ferrite-server"):
                    check_boundaries(workspace(self.root))

    def test_window_crates_link_the_engine_only_behind_a_feature(self):
        engine = 'ferrite-engine = { path = "../ferrite-engine"'
        self.write_package("ferrite-client", f'[features]\ntest-support = ["dep:ferrite-engine"]\n'
                                             f'[dependencies]\n{engine}, optional = true }}\n'
                                             f'[dev-dependencies]\n{engine} }}\n')
        self.write_package("ferrite-cockpit", '[features]\ndemo = ["ferrite-client/test-support"]\n'
                                              '[dependencies]\nferrite-client = { path = "../ferrite-client" }\n')
        check_boundaries(workspace(self.root))
        for features, dependency in [("", f"{engine} }}"), ("default = []", f"{engine}, optional = false }}"),
                                     ('default = ["ferrite-engine"]', f"{engine}, optional = true }}"),
                                     ('cef = ["ferrite-engine/x"]', f"{engine}, optional = true }}")]:
            with self.subTest(features=features, dependency=dependency):
                self.write_package("ferrite-client", f"[features]\n{features}\n[dependencies]\n{dependency}\n")
                with self.assertRaisesRegex(ValueError, "ferrite-client -> ferrite-engine"):
                    check_boundaries(workspace(self.root))

    def test_a_window_crate_turning_on_a_feature_that_links_the_engine_is_forbidden(self):
        self.write_package("ferrite-client", '[features]\ntest-support = ["dep:ferrite-engine"]\n'
                                             '[dependencies]\nferrite-engine = { path = "../ferrite-engine", optional = true }\n')
        for table in ["dependencies", "build-dependencies", "target.'cfg(windows)'.dependencies"]:
            with self.subTest(table=table):
                self.write_package("ferrite-pane", f'[{table}]\nhelper = {{ path = "../helper", features = ["fixture"] }}\n')
                self.write_package("helper", '[features]\nfixture = ["client/test-support"]\n[dependencies]\n'
                                             'client = { package = "ferrite-client", path = "../ferrite-client" }\n')
                with self.assertRaisesRegex(ValueError, "ferrite-pane -> helper -> ferrite-client -> ferrite-engine"):
                    check_boundaries(workspace(self.root))

    def test_engine_builds_without_gpui(self):
        self.write_package("ferrite-engine", '[dev-dependencies]\nhelper = { path = "../helper" }\n')
        self.write_package("helper", '[dependencies]\nui = { package = "gpui-base", version = "1" }\n')
        with self.assertRaisesRegex(ValueError, "ferrite-engine -> helper -> gpui-base"):
            check_boundaries(workspace(self.root))

    def test_server_builds_without_gpui(self):
        self.write_package("ferrite-server", '[dependencies]\nhelper = { path = "../helper" }\n')
        self.write_package("helper", '[dependencies]\nui = { package = "gpui-base", version = "1" }\n')
        with self.assertRaisesRegex(ValueError, "ferrite-server -> helper -> gpui-base"):
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
        self.assertEqual(result["packages"], ["ferrite-cockpit", "ferrite-core", "ferrite-engine", "ferrite-frame"])
        self.assertEqual(output.read_text(), "code=true\nbrowser=true\nchecks=true\nwindows=true\npackages=ferrite-cockpit "
                         "ferrite-core ferrite-engine ferrite-frame\nvendored=\n")

    def test_a_member_in_no_class_fails(self):
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/*", "tools/helper"]\n')
        (self.root / "tools/helper").mkdir(parents=True)
        (self.root / "tools/helper/Cargo.toml").write_text('[package]\nname = "helper"\nversion = "0.1.0"\n')
        with self.assertRaisesRegex(ValueError, r"in no class: helper \(tools/helper\)"):
            workspace(self.root)

    def test_a_file_embedded_from_outside_its_folder_selects_the_member(self):
        source = self.root / "crates/ferrite-core/src/lib.rs"
        source.parent.mkdir()
        source.write_text('const SKILL: &str = include_str!("../../../skills/core/SKILL.md");\n'
                          'const ICON: &[u8] = include_bytes!(concat!("../../ferrite-engine/icons/", "x.svg"));\n'
                          'const OWN: &str = include_str!("../README.md");\n'
                          '#[cfg(test)]\n#[path = "../../ferrite-engine/src/shared.rs"]\nmod shared;\n')
        packages = workspace(self.root)
        self.assertEqual(packages["ferrite-core"].embeds,
                         ("crates/ferrite-engine/icons", "crates/ferrite-engine/src/shared.rs", "skills/core/SKILL.md"))
        for path in ["skills/core/SKILL.md", "crates/ferrite-engine/icons/x.svg", "crates/ferrite-engine/src/shared.rs"]:
            with self.subTest(path=path):
                self.assertIn("ferrite-core", select(packages, [path])["packages"])
        self.assertFalse(select(packages, ["skills/other/SKILL.md"])["code"])

    def scope(self, *args):
        output = self.root / "outputs"
        output.unlink(missing_ok=True)
        with patch.object(sys, "argv", ["ci_scope.py", "--root", str(self.root), *args, "--github-output", str(output)]), \
                contextlib.redirect_stdout(io.StringIO()):
            main()
        return dict(line.split("=", 1) for line in output.read_text().splitlines())

    EVERYTHING = "ferrite-cockpit ferrite-core ferrite-engine ferrite-frame"

    def test_a_push_checks_everything_since_the_newest_commit_whose_run_succeeded(self):
        frame = self.root / "crates/ferrite-frame/src/lib.rs"
        frame.parent.mkdir()
        frame.write_text("fn frame() {}\n")
        checked = self.commit("base")
        frame.write_text("fn frame() { draw(); }\n")
        self.commit("change the frame; its run was cancelled")
        (self.root / "README.md").write_text("Ferrite\n")
        head = self.commit("document")
        # Newest first; the head's own run and commits this clone lacks are passed over.
        outputs = self.scope("--checked", head, "1234567890abcdef1234567890abcdef12345678", checked, "--windows")
        self.assertEqual(outputs["packages"], "ferrite-frame")
        self.assertEqual(outputs["windows"], "true")

    def test_a_push_with_no_checked_commit_it_descends_from_checks_everything(self):
        base = self.commit("base")
        self.git("checkout", "-q", "--orphan", "rewritten")
        self.commit("rewritten history")
        # A first push, an unknown commit, and one a forced push left behind.
        for checked in [[], ["0" * 40], ["1234567890abcdef1234567890abcdef12345678"], [base]]:
            with self.subTest(checked=checked):
                outputs = self.scope("--checked", *checked, "--windows")
                self.assertEqual(outputs["packages"], self.EVERYTHING)
                self.assertEqual(outputs["windows"], "true")

    def test_a_base_with_no_common_history_checks_everything(self):
        base = self.commit("base")
        self.git("checkout", "-q", "--orphan", "unrelated")
        self.commit("unrelated history")
        for revision in [base, "0" * 40]:
            with self.subTest(base=revision):
                self.assertEqual(self.scope("--base", revision)["packages"], self.EVERYTHING)

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

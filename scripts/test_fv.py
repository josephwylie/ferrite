import contextlib
import importlib.machinery
import importlib.util
import io
import os
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

FV = Path(__file__).resolve().parents[1] / "skills" / "verify-ferrite" / "scripts" / "fv"
loader = importlib.machinery.SourceFileLoader("fv", str(FV))
fv = importlib.util.module_from_spec(importlib.util.spec_from_loader("fv", loader))
loader.exec_module(fv)


class SafetyTests(unittest.TestCase):
    def test_a_run_id_cannot_leave_the_temp_directory(self):
        for run_id in ("../home", "a/b", "", ".hidden", "x" * 60):
            with self.subTest(run_id=run_id), self.assertRaises(fv.Refused):
                fv.Run(run_id)

    def test_every_call_drops_the_operators_server_and_names_the_runs_store(self):
        run = fv.Run("unit")
        operator = {"FERRITE_SOCKET": "/x/api.sock", "FERRITE_THREAD_ID": "7", "FERRITE_BIN": "/x/ferrite",
                    "FERRITE_STORE": "/x/threads"}
        with patch.dict(os.environ, operator):
            env = run.env()
        self.assertFalse({"FERRITE_SOCKET", "FERRITE_THREAD_ID", "FERRITE_BIN"} & set(env))
        self.assertEqual(env["FERRITE_STORE"], str(fv.TEMP / "fv-unit" / "threads"))

    def test_a_run_has_a_home_of_its_own_and_keeps_the_toolchains(self):
        operator = {"HOME": "/home/me", "USERPROFILE": r"C:\Users\me", "CODEX_HOME": "/home/me/codex",
                    "CLAUDE_CONFIG_DIR": "/home/me/claude", "CARGO_HOME": "/opt/cargo"}
        with patch.dict(os.environ, operator):
            env = fv.Run("unit").env()
            rustup = str(Path.home() / ".rustup")
            signed_in = fv.Run("unit", operator_home=True).env()
        home = str(fv.TEMP / "fv-unit" / "home")
        self.assertEqual((env["HOME"], env["USERPROFILE"]), (home, home))
        self.assertFalse({"CODEX_HOME", "CLAUDE_CONFIG_DIR"} & set(env))
        self.assertEqual((env["CARGO_HOME"], env["RUSTUP_HOME"]), ("/opt/cargo", rustup))
        self.assertEqual((signed_in["HOME"], signed_in["CODEX_HOME"]), ("/home/me", "/home/me/codex"))

    def test_a_process_not_running_this_checkouts_binary_is_never_ours(self):
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"],
                                 env={**os.environ, "FERRITE_STORE": str(fv.TEMP / "fv-unit" / "threads")})
        try:
            self.assertIn("not", fv.disowned(fv.Run("unit"), "server", child.pid))
        finally:
            child.kill()
            child.wait()
        self.assertIn("not running", fv.disowned(fv.Run("unit"), "server", child.pid))

    def test_commands_other_than_launch_need_a_run(self):
        with patch.dict(os.environ, {}, clear=True), contextlib.redirect_stderr(io.StringIO()) as err:
            self.assertEqual(fv.main(["doctor"]), 1)
        self.assertIn("FV_RUN is not set", err.getvalue())


if __name__ == "__main__":
    unittest.main()

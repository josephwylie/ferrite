"""Private native service client for packaged feasibility probes (not app code)."""
import json
import os
import secrets
import socket
import subprocess
import tempfile
import threading
import time
from pathlib import Path


class NativeService:
    def __init__(self, executable):
        self.executable = str(Path(executable).resolve())
        self.token = secrets.token_hex(32)
        self.temporary = tempfile.TemporaryDirectory(prefix="ferrite-ab-")
        self.directory = Path(self.temporary.name)
        self.directory.chmod(0o700)
        self.lock = threading.RLock()
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith(("AGENT_BROWSER_", "FERRITE_AGENT_BROWSER"))}
        self.env.update(AGENT_BROWSER_DAEMON="1", AGENT_BROWSER_SESSION="ferrite-probe",
                        AGENT_BROWSER_SOCKET_DIR=str(self.directory), AGENT_BROWSER_IDLE_TIMEOUT_MS="60000",
                        FERRITE_AGENT_BROWSER="1", FERRITE_AGENT_BROWSER_TOKEN=self.token)
        self.process = None
        self.endpoint = self.directory / ("ferrite-probe.port" if os.name == "nt" else "ferrite-probe.sock")

    def __enter__(self):
        self.start()
        return self

    def __exit__(self, *unused):
        self.stop()
        self.temporary.cleanup()

    def start(self):
        self.token = secrets.token_hex(32)
        self.env["FERRITE_AGENT_BROWSER_TOKEN"] = self.token
        self.process = subprocess.Popen([self.executable], env=self.env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        deadline = time.monotonic() + 15
        while True:
            if self.process.poll() is not None:
                raise RuntimeError("controller exited before its transport was ready")
            try:
                with self.connect():
                    return
            except (OSError, ValueError):
                if time.monotonic() > deadline:
                    self.stop()
                    raise RuntimeError("controller transport readiness timed out")
                time.sleep(0.02)

    def stop(self):
        if self.process is None:
            return
        if self.process.poll() is None:
            self.process.terminate()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)
        self.process.stderr.close()
        self.process = None

    def connect(self):
        if os.name == "nt":
            return socket.create_connection(("127.0.0.1", int(self.endpoint.read_text())), timeout=60)
        connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        connection.settimeout(60)
        try:
            connection.connect(str(self.endpoint))
        except OSError:
            connection.close()
            raise
        return connection

    def raw(self, value):
        with self.connect() as connection:
            connection.sendall(json.dumps(value).encode() + b"\n")
            with connection.makefile("rb") as response:
                return json.loads(response.readline())

    def command(self, action, **arguments):
        with self.lock:
            result = self.raw({"action": action, "ferriteToken": self.token, **arguments})
            if result.get("success") is not True:
                raise RuntimeError(f"native {action} failed: {result.get('error')}")
            return result.get("data", {})

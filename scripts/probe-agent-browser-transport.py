#!/usr/bin/env python3
"""Verify the packaged native service rejects unauthorized commands.

Starts only the controller (never launches or attaches to a browser). All
state lives in a private temporary directory, with a new launch capability.
The same running daemon answers repeated authorized requests; each request
uses a new connection, exercising reconnect without another service process.
"""
import argparse
import json
import os
import secrets
import socket
import subprocess
import tempfile
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    args = parser.parse_args()
    executable = args.executable.resolve()
    token = secrets.token_hex(32)
    with tempfile.TemporaryDirectory(prefix="ferrite-ab-") as temporary:
        directory = Path(temporary)
        directory.chmod(0o700)
        env = {key: value for key, value in os.environ.items()
               if not key.startswith(("AGENT_BROWSER_", "FERRITE_AGENT_BROWSER"))}
        env.update(AGENT_BROWSER_DAEMON="1", AGENT_BROWSER_SESSION="ferrite-probe",
                   AGENT_BROWSER_SOCKET_DIR=temporary, AGENT_BROWSER_IDLE_TIMEOUT_MS="30000",
                   FERRITE_AGENT_BROWSER="1", FERRITE_AGENT_BROWSER_TOKEN=token)
        # Ferrite mode without a capability must fail before opening listeners.
        missing = dict(env)
        del missing["FERRITE_AGENT_BROWSER_TOKEN"]
        rejected = subprocess.run([str(executable)], env=missing, capture_output=True, timeout=10)
        assert rejected.returncode != 0, "daemon accepted missing launch capability"
        assert not list(directory.glob("*.pid")), "unauthorized startup created daemon files"
        daemon = subprocess.Popen([str(executable)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            endpoint = directory / ("ferrite-probe.port" if os.name == "nt" else "ferrite-probe.sock")
            deadline = time.monotonic() + 10
            while not endpoint.exists():
                if daemon.poll() is not None:
                    raise RuntimeError("controller exited before its transport was ready")
                if time.monotonic() > deadline:
                    raise RuntimeError("controller transport readiness timed out")
                time.sleep(0.02)

            def command(value):
                if os.name == "nt":
                    connection = socket.create_connection(("127.0.0.1", int(endpoint.read_text())), timeout=5)
                else:
                    connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                    connection.settimeout(5)
                    connection.connect(str(endpoint))
                with connection:
                    connection.sendall(json.dumps(value).encode() + b"\n")
                    with connection.makefile("rb") as response:
                        return json.loads(response.readline())

            unauthenticated = command({"action": "session_info"})
            wrong = command({"action": "session_info", "ferriteToken": "wrong"})
            assert unauthenticated.get("success") is False, unauthenticated
            assert wrong.get("success") is False, wrong
            first = command({"action": "session_info", "ferriteToken": token})
            second = command({"action": "session_info", "ferriteToken": token})
            launch = command({"action": "launch", "ferriteToken": token})
            streaming = command({"action": "stream_enable", "ferriteToken": token})
            assert launch.get("success") is False, "controller launched an unrelated browser"
            assert "explicit CDP connect required" in launch.get("error", ""), launch
            assert streaming.get("success") is False, "controller enabled an unauthenticated input path"
            assert "disabled in Ferrite" in streaming.get("error", ""), streaming
            assert first.get("success") is True, first
            assert second.get("success") is True, second
            assert not list(directory.glob("*.stream")), "unauthenticated stream server is running"
            assert daemon.poll() is None, "service was not reused"
            assert int((directory / "ferrite-probe.pid").read_text()) == daemon.pid
            print(json.dumps({"version": "0.41.2", "missing_launch_capability_rejected": True,
                              "missing_command_capability_rejected": True,
                              "wrong_command_capability_rejected": True,
                              "authorized_reconnected_requests": 2, "service_processes": 1,
                              "stream_listener_disabled": True, "external_browser_launch_rejected": True, "browser_attached": False}))
        finally:
            daemon.terminate()
            try:
                daemon.wait(timeout=5)
            except subprocess.TimeoutExpired:
                daemon.kill()
                daemon.wait(timeout=5)


if __name__ == "__main__":
    main()

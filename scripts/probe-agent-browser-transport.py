#!/usr/bin/env python3
"""Verify the packaged native service rejects unauthorized commands.

Starts only the controller, with isolated state; never attaches or launches a
browser. Repeated commands use new connections to the same native daemon.
"""
import argparse
import json
import subprocess
from pathlib import Path

from native_browser_service import NativeService


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    args = parser.parse_args()
    service = NativeService(args.executable)
    try:
        missing = dict(service.env)
        del missing["FERRITE_AGENT_BROWSER_TOKEN"]
        rejected = subprocess.run([service.executable], env=missing, capture_output=True, timeout=10)
        assert rejected.returncode != 0, "daemon accepted missing launch capability"
        assert not list(service.directory.glob("*.pid")), "unauthorized startup created daemon files"
        with service:
            for action in ["session_info", "close", "__agent_browser_internal_shutdown"]:
                for credential in [{}, {"ferriteToken": "wrong"}]:
                    result = service.raw({"action": action, **credential})
                    assert result.get("success") is False, result
            service.command("session_info")
            service.command("session_info")
            launch = service.raw({"action": "launch", "ferriteToken": service.token})
            streaming = service.raw({"action": "stream_enable", "ferriteToken": service.token})
            assert launch.get("success") is False and "explicit CDP connect required" in launch.get("error", ""), launch
            assert streaming.get("success") is False and "disabled in Ferrite" in streaming.get("error", ""), streaming
            assert not list(service.directory.glob("*.stream")), "unauthenticated stream server is running"
            assert service.process.poll() is None, "service was not reused"
            assert int((service.directory / "ferrite-probe.pid").read_text()) == service.process.pid
            print(json.dumps({"version": "0.41.2", "missing_launch_capability_rejected": True,
                              "missing_command_capability_rejected": True,
                              "wrong_command_capability_rejected": True,
                              "authorized_reconnected_requests": 2, "service_processes": 1,
                              "stream_listener_disabled": True, "external_browser_launch_rejected": True,
                              "browser_attached": False}))
    finally:
        service.stop()
        service.temporary.cleanup()


if __name__ == "__main__":
    main()

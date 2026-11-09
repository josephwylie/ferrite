#!/usr/bin/env python3
"""Drive the exact packaged CEF smoke pages through the native controller.

The Rust smoke owns/pumps the live views and independently checks their pixels
when this driver finishes. This driver never opens another browser or target.
"""
import base64
import concurrent.futures
import json
import os
import socket
import shutil
import struct
import tempfile
import urllib.parse
import zlib
from pathlib import Path

from native_browser_service import NativeService


def screenshot_color(path):
    """Decode a known Chromium RGB/RGBA PNG and sample its center independently."""
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "native screenshot is not PNG"
    offset, compressed = 8, bytearray()
    while offset < len(data):
        size = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4:offset + 8]
        chunk = data[offset + 8:offset + 8 + size]
        if kind == b"IHDR":
            width, height, depth, color, compression, filtering, interlace = struct.unpack(">IIBBBBB", chunk)
            assert depth == 8 and color in (2, 6) and interlace == 0
            channels = 3 if color == 2 else 4
        elif kind == b"IDAT":
            compressed.extend(chunk)
        offset += size + 12
    raw, stride = zlib.decompress(compressed), width * channels
    previous = bytearray(stride)
    for y in range(height // 2 + 1):
        start = y * (stride + 1)
        method, row = raw[start], bytearray(raw[start + 1:start + 1 + stride])
        for index in range(stride):
            left = row[index - channels] if index >= channels else 0
            above = previous[index]
            upper_left = previous[index - channels] if index >= channels else 0
            if method == 1:
                predictor = left
            elif method == 2:
                predictor = above
            elif method == 3:
                predictor = (left + above) // 2
            elif method == 4:
                estimate = left + above - upper_left
                distances = (abs(estimate - left), abs(estimate - above), abs(estimate - upper_left))
                predictor = (left, above, upper_left)[distances.index(min(distances))]
            else:
                assert method == 0, f"unknown PNG filter {method}"
                predictor = 0
            row[index] = (row[index] + predictor) & 255
        previous = row
    pixel = width // 2 * channels
    return tuple(previous[pixel:pixel + 3])


def forbidden_handshake(endpoint, authorization=None, origin=None):
    url = urllib.parse.urlsplit(endpoint)
    headers = [f"GET {url.path or '/'} HTTP/1.1", f"Host: {url.netloc}",
               "Upgrade: websocket", "Connection: Upgrade", "Sec-WebSocket-Version: 13",
               "Sec-WebSocket-Key: " + base64.b64encode(os.urandom(16)).decode()]
    if authorization is not None:
        headers.append("Authorization: " + authorization)
    if origin is not None:
        headers.append("Origin: " + origin)
    with socket.create_connection((url.hostname, url.port), timeout=5) as connection:
        connection.sendall(("\r\n".join(headers) + "\r\n\r\n").encode())
        status = connection.recv(1024).split(b"\r\n", 1)[0]
        assert b" 403 " in status, f"unauthorized CDP handshake accepted: {status!r}"


def main():
    endpoint = os.environ["FERRITE_CDP_URL"]
    authorization = os.environ["FERRITE_CDP_AUTHORIZATION"]
    executable = os.environ["FERRITE_BROWSER_CONTROLLER"]
    forbidden_handshake(endpoint)
    forbidden_handshake(endpoint, "Bearer wrong")
    forbidden_handshake(endpoint, authorization, "http://127.0.0.1:3000")
    expected = {"/one": ((0, 0, 255), "rgb(0, 0, 255)"),
                "/two": ((0, 255, 0), "rgb(0, 255, 0)")}
    with tempfile.TemporaryDirectory(prefix="ferrite-cef-captures-") as captures:
        with NativeService(executable) as service:
            def attach():
                service.command("launch", cdpUrl=endpoint, cdpHeaders={"Authorization": authorization})
                tabs = service.command("tab_list")["tabs"]
                assert len(tabs) == 2, f"unexpected targets (sealed visuals must be hidden): {tabs}"
                selected = {urllib.parse.urlsplit(tab["url"]).path: tab for tab in tabs}
                assert set(selected) == set(expected), selected
                assert len({tab["targetId"] for tab in tabs}) == 2
                assert all(tab["targetId"].startswith("ferrite-browser-") for tab in tabs)
                return selected

            tabs = attach()
            first_pid = service.process.pid

            def work(path):
                # The native controller has mutable active-tab state. Selecting
                # and operating a target is one serialized service operation.
                with service.lock:
                    service.command("tab_switch", tabId=tabs[path]["tabId"])
                    observed = service.command("evaluate", script="location.pathname")["result"]
                    assert observed == path, (path, observed)
                    service.command("click", selector="#paint")
                    color = service.command("evaluate", script="getComputedStyle(document.body).backgroundColor")["result"]
                    assert color == expected[path][1], (path, color)
                    capture = Path(captures) / (path[1:] + ".png")
                    service.command("screenshot", path=str(capture))
                    assert screenshot_color(capture) == expected[path][0], path

            with concurrent.futures.ThreadPoolExecutor(max_workers=2) as agents:
                list(agents.map(work, expected))
            assert service.process.pid == first_pid, "agents created another controller"
            old_targets = {path: tab["targetId"] for path, tab in tabs.items()}
            # Restart only the controller. CEF pages must stay live and retain
            # their identity and visible state; no tab/browser close is sent.
            service.stop()
            service.start()
            tabs = attach()
            assert old_targets == {path: tab["targetId"] for path, tab in tabs.items()}
            for path in expected:
                service.command("tab_switch", tabId=tabs[path]["tabId"])
                color = service.command("evaluate", script="getComputedStyle(document.body).backgroundColor")["result"]
                assert color == expected[path][1], "controller reconnect lost visible page state"
            evidence = {"embedded_targets": 2, "concurrent_agents": 2,
                              "controllers_per_active_service": 1, "screenshots_match_live_pages": True,
                              "native_clicks": 2, "controller_restart_retains_targets": True,
                              "unauthorized_cdp_handshakes_rejected": 3,
                              "sealed_visual_discovery": False}
            if os.environ.get("FERRITE_BROWSER_EVIDENCE"):
                destination = Path(os.environ["FERRITE_BROWSER_EVIDENCE"])
                destination.mkdir(parents=True, exist_ok=True)
                for screenshot in Path(captures).glob("*.png"):
                    shutil.copy2(screenshot, destination / screenshot.name)
                (destination / "native-results.json").write_text(json.dumps(evidence, indent=2) + "\n")
            print(json.dumps(evidence))


if __name__ == "__main__":
    main()

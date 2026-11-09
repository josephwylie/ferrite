#!/usr/bin/env python3
"""Drive the exact packaged CEF smoke pages through the native controller.

The Rust smoke owns/pumps the live views and independently checks their pixels
when this driver finishes. This driver never opens another browser or target.
"""
import base64
from concurrent.futures import ThreadPoolExecutor
import json
import os
import socket
import shutil
import struct
import tempfile
import threading
import time
import urllib.parse
import zlib
from pathlib import Path

from native_browser_service import NativeService
from browser_processes import sample_browser_processes


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
        try:
            connection.sendall(("\r\n".join(headers) + "\r\n\r\n").encode())
            status = connection.recv(1024).split(b"\r\n", 1)[0]
        except (ConnectionAbortedError, ConnectionResetError, BrokenPipeError):
            # Windows may abort the rejected upgrade before HTTP403 is read.
            # The subsequent authorized native attachment must still succeed.
            return "connection closed"
        if not status:
            return "connection closed"
        assert b" 403 " in status, f"unauthorized CDP handshake accepted: {status!r}"
        return "HTTP403"


def assert_page_isolation(service, path, endpoint, private_file):
    """Attack from page JS, preserving the earlier screenshot as evidence."""
    targets = [private_file, "ferrite-view://page/foreign-document", "ferrite://browser-smoke-denied"]
    for target in targets:
        encoded = json.dumps(target)
        readable = service.command("evaluate", script=f"""(async () => {{
            try {{ await (await fetch({encoded})).text(); return true; }}
            catch (_) {{ return false; }}
        }})()""")["result"]
        assert readable is False, f"external page could fetch privileged URL: {target}"
        # Fetch CORS failures alone do not prove navigation policy. A full-page
        # navigation must also be refused without losing the original document.
        observed = service.command("evaluate", script=f"""(async () => {{
            try {{ location.href = {encoded}; }} catch (_) {{}}
            await new Promise(resolve => setTimeout(resolve, 100));
            return location.pathname;
        }})()""")["result"]
        assert observed == path, f"external page navigated to privileged URL: {target}"
    opened = service.command("evaluate", script=f"""new Promise(resolve => {{
        const socket = new WebSocket({json.dumps(endpoint)});
        const timer = setTimeout(() => {{ socket.close(); resolve('timeout'); }}, 3000);
        socket.onopen = () => {{ clearTimeout(timer); socket.close(); resolve(true); }};
        socket.onerror = () => {{ clearTimeout(timer); resolve(false); }};
    }})""")["result"]
    assert opened is False, "page JavaScript reached private controller transport"


def main():
    endpoint = os.environ["FERRITE_CDP_URL"]
    authorization = os.environ["FERRITE_CDP_AUTHORIZATION"]
    executable = os.environ["FERRITE_BROWSER_CONTROLLER"]
    private_file = os.environ["FERRITE_BROWSER_PRIVATE_FILE"]
    handshake_denials = [
        {"case": "missing capability", "denial": forbidden_handshake(endpoint)},
        {"case": "wrong capability", "denial": forbidden_handshake(endpoint, "Bearer wrong")},
        {"case": "page origin", "denial": forbidden_handshake(endpoint, authorization, "http://127.0.0.1:3000")},
    ]
    browser_pid = os.getppid()
    process_samples = {"before_controller": sample_browser_processes(browser_pid, executable)}
    destination = Path(os.environ["FERRITE_BROWSER_EVIDENCE"]) if os.environ.get("FERRITE_BROWSER_EVIDENCE") else None
    if destination is not None:
        destination.mkdir(parents=True, exist_ok=True)

    def retain_process_samples():
        if destination is not None:
            (destination / "process-samples.json").write_text(json.dumps(process_samples, indent=2) + "\n")

    retain_process_samples()
    operations = {"native_clicks": 0}
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
            fixed_targets = {tab["targetId"] for tab in tabs.values()}
            process_samples["fixed_target_ids"] = sorted(fixed_targets)

            def work(path):
                # The native controller has mutable active-tab state. Selecting
                # and operating a target is one serialized service operation.
                with service.lock:
                    service.command("tab_switch", tabId=tabs[path]["tabId"])
                    observed = service.command("evaluate", script="location.pathname")["result"]
                    assert observed == path, (path, observed)
                    snapshot = service.command("snapshot", interactive=True)
                    assert "Paint" in snapshot["snapshot"], snapshot
                    buttons = [ref for ref, node in snapshot["refs"].items()
                               if node["role"] == "button" and node["name"] in ("Paint", "Painted")]
                    assert len(buttons) == 1, snapshot
                    service.command("click", selector="@" + buttons[0])
                    operations["native_clicks"] += 1
                    color = service.command("evaluate", script="getComputedStyle(document.body).backgroundColor")["result"]
                    assert color == expected[path][1], (path, color)
                    capture = Path(captures) / (path[1:] + ".png")
                    service.command("screenshot", path=str(capture))
                    assert screenshot_color(capture) == expected[path][0], path
                    if destination is not None:
                        shutil.copy2(capture, destination / capture.name)

            def measure_agents(paths):
                # Keep real native work active until at least one OS process
                # query completes, including slower Windows CIM enumeration.
                sampled = threading.Event()
                barrier = threading.Barrier(len(paths) + 1)
                guard = threading.Lock()
                activity = {"active": 0, "peak": 0}
                samples = []

                def agent(path):
                    with guard:
                        activity["active"] += 1
                        activity["peak"] = max(activity["peak"], activity["active"])
                    barrier.wait(timeout=30)
                    try:
                        rounds = 0
                        while rounds < 3 or not sampled.is_set():
                            work(path)
                            rounds += 1
                    finally:
                        with guard:
                            activity["active"] -= 1

                with ThreadPoolExecutor(max_workers=len(paths)) as agents:
                    futures = [agents.submit(agent, path) for path in paths]
                    barrier.wait(timeout=30)
                    try:
                        while not all(future.done() for future in futures):
                            with guard:
                                active = activity["active"]
                            sample = sample_browser_processes(browser_pid, executable)
                            sample["active_agent_workers_at_sample_start"] = active
                            samples.append(sample)
                            if len(samples) >= 3:
                                sampled.set()
                            time.sleep(0.05)
                    finally:
                        sampled.set()
                    for future in futures:
                        future.result()
                assert len(samples) >= 3 and any(sample["active_agent_workers_at_sample_start"] for sample in samples)
                observed_targets = {tab["targetId"] for tab in service.command("tab_list")["tabs"]}
                assert observed_targets == fixed_targets, "Process comparison changed the target set"
                return {"peak_active_agent_workers": activity["peak"], "samples": samples, "target_ids": sorted(observed_targets),
                        "after": sample_browser_processes(browser_pid, executable)}

            process_samples["single_agent"] = measure_agents(["/one"])
            process_samples["concurrent_agents"] = measure_agents(list(expected))
            retain_process_samples()
            single_samples = process_samples["single_agent"]["samples"] + [process_samples["single_agent"]["after"]]
            concurrent_samples = process_samples["concurrent_agents"]["samples"] + [process_samples["concurrent_agents"]["after"]]
            single_browsers = max(sample["browser_process_count"] for sample in single_samples)
            concurrent_browsers = max(sample["browser_process_count"] for sample in concurrent_samples)
            controllers = [sample["controller_process_count"] for sample in single_samples + concurrent_samples]
            assert all(count == 1 for count in controllers), process_samples
            assert concurrent_browsers <= single_browsers, f"Browser processes scaled with agent count: single={single_browsers}, concurrent={concurrent_browsers}"
            assert process_samples["concurrent_agents"]["peak_active_agent_workers"] == len(expected)
            assert service.process.pid == first_pid, "agents created another controller"
            # Both known-good screenshots already exist before hostile attempts.
            # Recheck location and state after every refusal, then let Rust
            # independently verify that all three live views kept their pixels.
            for path in expected:
                service.command("tab_switch", tabId=tabs[path]["tabId"])
                assert_page_isolation(service, path, endpoint, private_file)
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
            process_samples["after_controller_reconnect"] = sample_browser_processes(browser_pid, executable)
            retain_process_samples()
            evidence = {"embedded_targets": len(tabs),
                              "concurrent_agents": process_samples["concurrent_agents"]["peak_active_agent_workers"],
                              "controllers_per_active_service": max(controllers),
                              "browser_process_count_single_agent": single_browsers,
                              "browser_process_count_concurrent_agents": concurrent_browsers,
                              "process_samples": process_samples, "screenshots_match_live_pages": True,
                              "native_clicks": operations["native_clicks"], "snapshot_reference_clicks": operations["native_clicks"], "controller_restart_retains_targets": True,
                              "unauthorized_cdp_handshakes_rejected": len(handshake_denials), "handshake_denials": handshake_denials,
                              "sealed_visual_discovery": False, "page_privileged_fetches_and_navigations_rejected": 12,
                              "page_controller_websockets_rejected": 2}
            if destination is not None:
                (destination / "native-results.json").write_text(json.dumps(evidence, indent=2) + "\n")
            print(json.dumps(evidence))


if __name__ == "__main__":
    main()

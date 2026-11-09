#!/usr/bin/env python3
"""Exercise Ferrite's packaged native command boundary against an isolated Chrome.

Chrome is a fixture dependency supplied by the runner, never an app fallback.
No browser download or operator profile is used. The controller attaches once
and all observations/actions use its authenticated persistent JSONL service.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from native_browser_service import NativeService


def chrome_executable():
    candidates = [os.environ.get("CHROME_PATH"), shutil.which("google-chrome"),
                  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                  r"C:\Program Files\Google\Chrome\Application\chrome.exe",
                  r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return candidate
    raise RuntimeError("the native behavior fixture requires runner-installed Chrome")


def evaluate(service, script):
    return service.command("evaluate", script=script)["result"]


def reference(service, role, name):
    observed = service.command("snapshot", interactive=True)
    refs = [key for key, node in observed["refs"].items()
            if node["role"] == role and node["name"] == name]
    assert len(refs) == 1, observed
    return "@" + refs[0]


def stale_replacement(service):
    html = '''<button id="save" onclick="window.writes=(window.writes||0)+1">Save</button>
<label>Name<input id="name"></label><script>window.writes=0</script>'''
    service.command("navigate", url="data:text/html;base64," + base64.b64encode(html.encode()).decode())
    old_button = reference(service, "button", "Save")
    evaluate(service, "document.querySelector('#save').outerHTML='<button id=save onclick=\"window.writes++\">Save</button>'")
    result = service.raw({"action": "click", "selector": old_button, "ferriteToken": service.token})
    assert result.get("success") is False, f"a stale reference clicked its same-label replacement: {result}"
    assert evaluate(service, "window.writes") == 0, "stale click changed the replacement"
    old_input = reference(service, "textbox", "Name")
    evaluate(service, "document.querySelector('#name').outerHTML='<input id=name>'")
    result = service.raw({"action": "fill", "selector": old_input, "value": "wrong node", "ferriteToken": service.token})
    assert result.get("success") is False, f"a stale reference filled its same-label replacement: {result}"
    assert evaluate(service, "document.querySelector('#name').value") == "", "stale fill changed the replacement"
    # A fresh observation must still make the replacement usable.
    fresh = reference(service, "button", "Save")
    service.command("click", selector=fresh)
    assert evaluate(service, "window.writes") == 1
    return {"stale_click_rejected": True, "stale_fill_rejected": True, "fresh_reference_works": True}


def scoped_diagnostics(service):
    service.command("navigate", url="data:text/html,<title>Thread A</title><p>A</p>")
    first = [tab for tab in service.command("tab_list")["tabs"] if "Thread A" == tab.get("title")][0]["tabId"]
    service.command("console", clear=True)
    service.command("errors", clear=True)
    service.command("requests", clear=True)
    evaluate(service, "console.log('ONLY_THREAD_A'); setTimeout(()=>console.log('BACKGROUND_THREAD_A'),100)")
    service.command("tab_new", url="data:text/html,<title>Thread B</title><p>B</p>")
    second = [tab for tab in service.command("tab_list")["tabs"] if "Thread B" == tab.get("title")][0]["tabId"]
    evaluate(service, "console.log('ONLY_THREAD_B')")
    time.sleep(0.2)
    messages = service.command("console")["messages"]
    text = json.dumps(messages)
    assert "ONLY_THREAD_B" in text and "THREAD_A" not in text, f"Thread B received another Thread's console: {messages}"
    service.command("console", clear=True)
    service.command("tab_switch", tabId=first)
    text = json.dumps(service.command("console")["messages"])
    assert "ONLY_THREAD_A" in text and "BACKGROUND_THREAD_A" in text and "THREAD_B" not in text, text
    # Clearing B must not erase A; returning to B must not recover A's log.
    service.command("tab_switch", tabId=second)
    assert service.command("console")["messages"] == []
    return {"console_target_isolation": True, "background_console_attributed": True, "clear_is_target_scoped": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="ferrite-native-fixture-") as temporary:
        profile = Path(temporary) / "profile"
        browser = subprocess.Popen([chrome_executable(), "--headless=new", "--disable-gpu",
                                    "--no-first-run", "--no-default-browser-check", "--remote-debugging-port=0",
                                    "--user-data-dir=" + str(profile), "about:blank"],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            port_file = profile / "DevToolsActivePort"
            deadline = time.monotonic() + 20
            while not port_file.is_file():
                if browser.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("isolated Chrome fixture did not become ready")
                time.sleep(0.05)
            port, endpoint = port_file.read_text().splitlines()[:2]
            with NativeService(args.executable) as service:
                service.command("launch", cdpUrl=f"ws://127.0.0.1:{port}{endpoint}")
                print(json.dumps(stale_replacement(service)), flush=True)
                print(json.dumps(scoped_diagnostics(service)), flush=True)
        finally:
            browser.terminate()
            try:
                browser.wait(timeout=5)
            except subprocess.TimeoutExpired:
                browser.kill()
                browser.wait(timeout=5)


if __name__ == "__main__":
    main()

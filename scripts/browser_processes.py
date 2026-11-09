"""Sample actual controller/CEF descendants of the packaged smoke process."""
import json
import os
import subprocess
import time
from pathlib import Path


def sample_browser_processes(browser_pid, controller_executable):
    started = time.monotonic()
    if os.name == "nt":
        script = "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name | ConvertTo-Json -Compress"
        output = subprocess.check_output(
            ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script],
            text=True, timeout=15,
        )
        data = json.loads(output.lstrip("\ufeff"))
        if isinstance(data, dict):
            data = [data]
        processes = [{"pid": int(item["ProcessId"]), "parent_pid": int(item["ParentProcessId"]),
                      "executable": item["Name"]} for item in data]
    else:
        output = subprocess.check_output(["ps", "-axww", "-o", "pid=,ppid=,comm="], text=True, timeout=15)
        processes = []
        for line in output.splitlines():
            pid, parent, executable = line.strip().split(None, 2)
            processes.append({"pid": int(pid), "parent_pid": int(parent),
                              "executable": Path(executable).name})
    root = next((item for item in processes if item["pid"] == browser_pid), None)
    if root is None:
        raise RuntimeError("Packaged browser process was absent from the OS process sample")
    descendants = {browser_pid}
    while True:
        expanded = descendants | {item["pid"] for item in processes if item["parent_pid"] in descendants}
        if expanded == descendants:
            break
        descendants = expanded
    root_name = root["executable"].casefold()
    helper_prefix = Path(root_name).stem + " helper"
    controller_name = Path(controller_executable).name.casefold()
    browsers, controllers = [], []
    for item in processes:
        if item["pid"] not in descendants:
            continue
        name = item["executable"].casefold()
        if name == controller_name:
            controllers.append(item)
        elif (item["pid"] == browser_pid or name == root_name or name.startswith(helper_prefix)
              or any(brand in name for brand in ("chrome", "chromium", "crashpad", "msedge", "firefox", "safari"))):
            browsers.append(item)
    return {"sample_started_monotonic": started,
            "sampling_duration_ms": round((time.monotonic() - started) * 1000, 2),
            "browser_process_count": len(browsers), "controller_process_count": len(controllers),
            "browser_processes": sorted(browsers, key=lambda item: item["pid"]),
            "controller_processes": sorted(controllers, key=lambda item: item["pid"])}

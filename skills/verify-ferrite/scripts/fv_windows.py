#!/usr/bin/env python3
"""fv on Windows: the same commands as `fv`, with the platform parts swapped.

`fv` reads processes through ps or /proc and the Server's socket through lsof
or /proc/net/unix; Windows has none of them. This loads `fv` unchanged and
replaces those parts: a run's processes are told apart by image path and
creation time, and the Server's API is a named pipe (`crates/ferrite-core/src/
link/windows.rs`). See ../SKILL.md, "Windows".
"""

import ctypes
import os
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from ctypes import wintypes
from importlib.machinery import SourceFileLoader
from importlib.util import module_from_spec, spec_from_loader
from pathlib import Path

if sys.platform != "win32":
    sys.exit("fv_windows.py is for Windows; use fv")

_loader = SourceFileLoader("fv", str(Path(__file__).resolve().with_name("fv")))
fv = module_from_spec(spec_from_loader("fv", _loader))
_loader.exec_module(fv)
fv.BINARY = fv.CHECKOUT / "target" / "debug" / "ferrite.exe"
# `ferrite --headless` takes only a Store under std::env::temp_dir(), %TEMP%.
fv.TEMP = Path(tempfile.gettempdir()).resolve()

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
H, P = wintypes.HANDLE, ctypes.c_void_p
for _name, _result, _args in (
    ("OpenProcess", H, (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)),
    ("CloseHandle", wintypes.BOOL, (H,)),
    ("GetExitCodeProcess", wintypes.BOOL, (H, P)),
    ("GetProcessTimes", wintypes.BOOL, (H, P, P, P, P)),
    ("QueryFullProcessImageNameW", wintypes.BOOL, (H, wintypes.DWORD, wintypes.LPWSTR, P)),
    ("CreateFileW", H, (wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, P, wintypes.DWORD, wintypes.DWORD, H)),
    ("GetNamedPipeServerProcessId", wintypes.BOOL, (H, P)),
    ("CreateToolhelp32Snapshot", H, (wintypes.DWORD, wintypes.DWORD)),
    ("Process32FirstW", wintypes.BOOL, (H, P)),
    ("Process32NextW", wintypes.BOOL, (H, P)),
):
    getattr(kernel32, _name).restype = _result
    getattr(kernel32, _name).argtypes = _args
QUERY_LIMITED = 0x1000
STILL_ACTIVE = 259
INVALID_HANDLE = wintypes.HANDLE(-1).value
CREATE_NEW_PROCESS_GROUP = 0x0000_0200
CREATE_NO_WINDOW = 0x0800_0000


class Entry(ctypes.Structure):
    _fields_ = [("size", wintypes.DWORD), ("usage", wintypes.DWORD), ("pid", wintypes.DWORD),
                ("heap", ctypes.c_size_t), ("module", wintypes.DWORD), ("threads", wintypes.DWORD),
                ("parent", wintypes.DWORD), ("priority", ctypes.c_long), ("flags", wintypes.DWORD),
                ("exe", ctypes.c_wchar * 260)]


def facts(pid):
    """A live pid's image path and creation time, or None."""
    handle = kernel32.OpenProcess(QUERY_LIMITED, False, pid) if pid else None
    if not handle:
        return None
    try:
        code = wintypes.DWORD()
        if not kernel32.GetExitCodeProcess(handle, ctypes.byref(code)) or code.value != STILL_ACTIVE:
            return None
        created, ended, kernel, user = (ctypes.c_ulonglong() for _ in range(4))
        kernel32.GetProcessTimes(handle, *map(ctypes.byref, (created, ended, kernel, user)))
        name, size = ctypes.create_unicode_buffer(32768), wintypes.DWORD(32768)
        image = name.value if kernel32.QueryFullProcessImageNameW(handle, 0, name, ctypes.byref(size)) else "?"
        return image, created.value
    finally:
        kernel32.CloseHandle(handle)


def disowned(run, role, pid):
    """Why pid is not the process this run started as `role`, or None when it is.

    The image path and the creation time `start` kept: a pid Windows reuses
    has another creation time, so the check needs neither argv nor the
    environment (which Windows hands out to no other process cheaply).
    """
    found = facts(pid)
    if found is None:
        return f"pid {pid} is not running"
    image, created = found
    if Path(image).resolve() != fv.BINARY.resolve():
        return f"pid {pid} runs {image}, not {fv.BINARY}"
    if str(created) != run.saved().get(f"{role.upper()}_STARTED"):
        return f"pid {pid} is not the {role} this run started (another creation time)"
    return None


def pipe_server(path):
    """The pid serving the named pipe Ferrite names for `path`, or None."""
    wide = str(path).encode("utf-16-le")
    digest = 0xCBF2_9CE4_8422_2325
    for byte in wide:
        digest = ((digest ^ byte) * 0x0000_0100_0000_01B3) & 0xFFFF_FFFF_FFFF_FFFF
    suffix = f"-{digest:016x}"
    names = [n for n in os.listdir("\\\\.\\pipe\\") if n.startswith("ferrite-server-") and n.endswith(suffix)]
    if len(names) != 1:
        return None
    handle = kernel32.CreateFileW(f"\\\\.\\pipe\\{names[0]}", 0xC000_0000, 0, None, 3, 0, None)
    if handle in (None, INVALID_HANDLE):
        return None
    try:
        pid = wintypes.ULONG()
        return pid.value if kernel32.GetNamedPipeServerProcessId(handle, ctypes.byref(pid)) else None
    finally:
        kernel32.CloseHandle(handle)


def listens_on(pid, socket):
    return pipe_server(socket) == pid


def start(run, role, log):
    out = open(run.root / log, "w")
    env = {**run.env(), "TERM": "dumb"}
    child = subprocess.Popen([str(fv.BINARY), *run.role_args(role)], env=env, cwd=run.repo,
                             stdin=subprocess.DEVNULL, stdout=out, stderr=out,
                             creationflags=CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
    run.remember(f"{role.upper()}_PID", child.pid)
    run.remember(f"{role.upper()}_STARTED", facts(child.pid)[1])
    return child


def descendants(roots):
    """Every live process under `roots`, as {pid: creation time}."""
    snapshot = kernel32.CreateToolhelp32Snapshot(2, 0)
    children = {}
    entry = Entry(size=ctypes.sizeof(Entry))
    more = kernel32.Process32FirstW(snapshot, ctypes.byref(entry))
    while more:
        children.setdefault(entry.parent, []).append(entry.pid)
        more = kernel32.Process32NextW(snapshot, ctypes.byref(entry))
    kernel32.CloseHandle(snapshot)
    born = {pid: alive[1] for pid in roots if (alive := facts(pid))}
    found, todo = {}, list(born)
    while todo:
        parent = todo.pop()
        for child in children.get(parent, []):
            alive = facts(child)
            # A parent pid outlives its process: a child older than the parent
            # names an earlier process that had the same pid.
            if child not in found and child not in born and alive and alive[1] >= born[parent]:
                found[child] = born[child] = alive[1]
                todo.append(child)
    return found


def stop(_args):
    run = fv.Run.current()
    failed = False
    owned = {role: run.pid(role) for role in ("window", "server") if disowned(run, role, run.pid(role)) is None}
    # Agent CLIs sit in kill-on-close jobs the Server holds; these are checked after.
    spawned = descendants(set(owned.values()))
    for role in ("window", "server"):
        pid = run.pid(role)
        if role not in owned:
            print(f"skip {role}: {disowned(run, role, pid)}")
            continue
        os.kill(pid, signal.SIGTERM)  # TerminateProcess: Windows has no SIGTERM to catch
        deadline = time.monotonic() + 15
        while facts(pid) is not None and time.monotonic() < deadline:
            time.sleep(0.2)
        gone = facts(pid) is None
        failed |= not gone
        print(f"{'ok  ' if gone else 'FAIL'} {role} pid {pid} {'stopped' if gone else 'did not exit; inspect it before killing harder'}")
    deadline = time.monotonic() + 10
    while any((facts(p) or (0, None))[1] == t for p, t in spawned.items()) and time.monotonic() < deadline:
        time.sleep(0.2)
    for pid, created in spawned.items():
        alive = facts(pid)
        if alive and alive[1] == created:
            failed = True
            print(f"FAIL pid {pid} the run spawned still runs: {alive[0]}")
    logs = run.evidence / "logs"
    if (run.root / "logs").is_dir():
        shutil.copytree(run.root / "logs", logs, dirs_exist_ok=True)
    logs.mkdir(parents=True, exist_ok=True)
    for name in ("server.out", "window.out", "settings.json", "run.env"):
        if (run.root / name).exists():
            shutil.copy2(run.root / name, logs / name)
    if failed:
        print(f"FAIL kept {run.root}: a process of this run still runs")
        return 1
    # git writes its objects read-only, and Windows will not delete a read-only file.
    shutil.rmtree(run.root, onexc=lambda remove, path, _: (os.chmod(path, stat.S_IWRITE), remove(path)))
    print(f"stopped {run.id}; evidence kept in {run.evidence}")
    return 0


fv.disowned = disowned
fv.listens_on = listens_on
fv.start = start
fv.COMMANDS["stop"] = stop

if __name__ == "__main__":
    sys.exit(fv.main(sys.argv[1:]))

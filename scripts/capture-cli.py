#!/usr/bin/env python3
"""Isolated tmux CLI evidence recorder. No GUI automation; no app runtime changes.

Uses tmux's terminal parser, records raw output before startup, samples ANSI grids,
and records every input/resize. Run --help. Python stdlib + tmux; render uses Pillow.
"""
import argparse
import base64
import datetime
import hashlib
import json
import os
import fcntl
from pathlib import Path
import pty
import select
import signal
import shlex
import subprocess
import sys
import tempfile
import termios
import time
import tty

SCRIPT = Path(__file__).resolve()


def write_json(path, data):
    Path(path).write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")


def append(path, data):
    with Path(path).open("a") as f:
        f.write(json.dumps(data, ensure_ascii=False) + "\n")


def tmux(root, *args, check=True):
    meta = json.loads((root / "manifest.json").read_text())
    return subprocess.run(["tmux", "-L", meta["socket"], *args],
                          capture_output=True, check=check)


def event(root, **data):
    append(root / "actions.jsonl", {"time": time.time(), **data})


def capture(root, name, history=False):
    fmt = "#{pane_width},#{pane_height},#{cursor_x},#{cursor_y},#{cursor_flag},#{alternate_on},#{history_size},#{pane_dead}"
    before = tmux(root, "display-message", "-p", "-t", name, fmt).stdout.decode().strip()
    args = ["capture-pane", "-p", "-e", "-N", "-t", name]
    if history:
        args += ["-S", "-"]
    grid = tmux(root, *args).stdout.decode("utf-8", "replace")
    info = tmux(root, "display-message", "-p", "-t", name, fmt).stdout.decode().strip()
    if before.split(",")[:2] != info.split(",")[:2]:
        return capture(root, name, history)
    return {"time": time.time(), "provider": name, "meta": info, "ansi": grid}


def start(args):
    root = args.root.resolve()
    root.mkdir(parents=True, exist_ok=False)
    fixture = Path(tempfile.mkdtemp(prefix="ferrite-cli-fixture-"))
    (fixture / "sample.txt").write_text(
        "# Terminal transcript fixture\n\nAlpha beta gamma.\n"
        "one  two   three    four\n\tTabbed input\n"
        "Unicode: café naïve → ✓ 界 e\u0301\n"
        "Long line: " + "word " * 42 + "END\n")
    (fixture / "sample.py").write_text("def greet(name):\n    return f'Hello, {name}!'\n\nprint(greet('world'))\n")
    commands = {
        "claude": ["claude", "--strict-mcp-config", "--permission-mode", "manual",
                   "--tools", "Read,Glob,Grep,Bash,AskUserQuestion", "--settings",
                   '{"disableAllHooks":true}', "--append-system-prompt",
                   "This is a harmless terminal presentation capture. Only read synthetic files in the current directory. Never modify files, use network tools, inspect secrets, or delegate to agents. Execute only explicitly requested harmless shell commands."],
        "codex": ["codex", "-s", "read-only", "-a", "on-request",
                  "-c", 'web_search="disabled"', "-c", 'tui.notifications=false']
    }
    meta = {"created_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "socket": "ferrite-capture-" + str(os.getpid()), "columns": 120, "rows": 44,
            "sample_interval_s": 0.1, "terminal": "tmux-256color", "term_program": "tmux",
            "default_foreground": "#d8dee9", "default_background": "#17191f",
            "commands": commands, "fixture": str(fixture),
            "notes": ["Inherited authentication and user configuration; no credentials copied into evidence.",
                      "Claude hooks disabled for this invocation; only named built-in tools enabled.",
                      "Codex shell sandbox read-only; no web search; external tools not requested.",
                      "Captured grids sampled, raw PTY output continuous. Rendered images are reconstructions, not native screenshots."]}
    meta["versions"] = {tool: subprocess.check_output([tool, flag], text=True).strip()
                        for tool, flag in [("claude", "--version"), ("codex", "--version"), ("tmux", "-V")]}
    write_json(root / "manifest.json", meta)
    for name in commands:
        (root / name).mkdir()
        launch = shlex.join([sys.executable, str(SCRIPT), "launch", str(root), name])
        tmux(root, "-f", "/dev/null", "new-session", "-d", "-s", name,
             "-x", "120", "-y", "44", "-c", str(fixture), launch)
        tmux(root, "set-option", "-t", name, "status", "off")
        tmux(root, "set-window-option", "-t", name, "remain-on-exit", "on")
        tmux(root, "set-window-option", "-t", name, "history-limit", "100000")
        tmux(root, "set-window-option", "-t", name, "window-size", "manual")
        tmux(root, "set-window-option", "-t", name, "window-style", "fg=#d8dee9,bg=#17191f")
    log = (root / "recorder.log").open("ab")
    proc = subprocess.Popen([sys.executable, str(SCRIPT), "watch", str(root)],
                            stdin=subprocess.DEVNULL, stdout=log, stderr=log, start_new_session=True)
    write_json(root / "recorder.json", {"pid": proc.pid})
    for name in commands:
        (root / name / "go").touch()
    print(root)


def launch(args):
    while not (args.root / args.name / "go").exists():
        time.sleep(0.05)
    meta = json.loads((args.root / "manifest.json").read_text())
    env = os.environ.copy()
    env.update(TERM="tmux-256color", COLORTERM="truecolor")
    env.pop("CLAUDECODE", None)
    env.pop("NO_COLOR", None)
    command = meta["commands"][args.name]
    outer_settings = termios.tcgetattr(0)
    size = fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8)
    child, master = pty.fork()
    if child == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, size)
        os.execvpe(command[0], command, env)
    def resize(*_):
        fcntl.ioctl(master, termios.TIOCSWINSZ, fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8))
    signal.signal(signal.SIGWINCH, resize)
    tty.setraw(0)
    offsets = {"input": 0, "output": 0}
    files = {name: (args.root / args.name / (name + ".bin")).open("wb", buffering=0)
             for name in offsets}
    try:
        while True:
            ready, _, _ = select.select([0, master], [], [])
            for fd in ready:
                try:
                    data = os.read(fd, 65536)
                except OSError:
                    return
                if not data:
                    return
                direction = "input" if fd == 0 else "output"
                files[direction].write(data)
                append(args.root / args.name / (direction + "-index.jsonl"),
                       {"time": time.time(), "offset": offsets[direction], "length": len(data)})
                offsets[direction] += len(data)
                dest = master if fd == 0 else 1
                while data:
                    data = data[os.write(dest, data):]
    finally:
        termios.tcsetattr(0, termios.TCSADRAIN, outer_settings)
        for f in files.values():
            f.close()


def raw(args):
    with (args.root / args.name / "output.bin").open("ab", buffering=0) as out:
        offset = 0
        while True:
            data = os.read(0, 65536)
            if not data:
                break
            out.write(data)
            append(args.root / args.name / "output-index.jsonl",
                   {"time": time.time(), "offset": offset, "length": len(data)})
            offset += len(data)


def watch(args):
    last = {}
    sequence = {"claude": 0, "codex": 0}
    while not (args.root / "stop").exists():
        tick = time.monotonic()
        for name in sequence:
            try:
                frame = capture(args.root, name)
            except subprocess.CalledProcessError:
                continue
            signature = hashlib.sha256((frame["ansi"] + frame["meta"]).encode()).hexdigest()
            if signature != last.get(name):
                frame["index"] = sequence[name]
                append(args.root / name / "frames.jsonl", frame)
                sequence[name] += 1
                last[name] = signature
        time.sleep(max(0, 0.1 - (time.monotonic() - tick)))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    subs = p.add_subparsers(dest="cmd", required=True)
    for cmd in ["start", "watch", "stop", "status"]:
        s = subs.add_parser(cmd)
        s.add_argument("root", type=Path)
    for cmd in ["launch", "raw", "send", "key", "mark", "resize"]:
        s = subs.add_parser(cmd)
        s.add_argument("root", type=Path)
        s.add_argument("name", choices=["claude", "codex"])
        if cmd in ["send", "key", "mark"]:
            s.add_argument("value")
        if cmd == "send":
            s.add_argument("--enter", action="store_true")
        if cmd == "resize":
            s.add_argument("columns", type=int)
            s.add_argument("rows", type=int)
    args = p.parse_args()
    args.root = args.root.resolve()
    if args.cmd in ["start", "launch", "raw", "watch"]:
        globals()[args.cmd](args)
    elif args.cmd == "send":
        event(args.root, action="text", provider=args.name, text=args.value, enter=args.enter)
        tmux(args.root, "set-buffer", "--", args.value)
        tmux(args.root, "paste-buffer", "-p", "-t", args.name)
        if args.enter:
            time.sleep(0.25)
            tmux(args.root, "send-keys", "-t", args.name, "Enter")
    elif args.cmd == "key":
        event(args.root, action="key", provider=args.name, key=args.value)
        tmux(args.root, "send-keys", "-t", args.name, args.value)
    elif args.cmd == "resize":
        event(args.root, action="resize", provider=args.name, columns=args.columns, rows=args.rows)
        tmux(args.root, "resize-window", "-t", args.name, "-x", str(args.columns), "-y", str(args.rows))
    elif args.cmd == "mark":
        # send-keys returns before the application paints. Allow the asynchronous
        # redraw to settle; transient states remain in the sampled frame stream.
        time.sleep(0.35)
        frame = capture(args.root, args.name)
        frame["label"] = args.value
        frame["history_ansi"] = capture(args.root, args.name, history=True)["ansi"]
        path = args.root / args.name / (args.value + ".json")
        write_json(path, frame)
        event(args.root, action="mark", provider=args.name, label=args.value)
        print(path)
    elif args.cmd == "status":
        for name in ["claude", "codex"]:
            f = capture(args.root, name)
            print(name, f["meta"], "\n", f["ansi"])
    elif args.cmd == "stop":
        (args.root / "stop").touch()
        for name in ["claude", "codex"]:
            frame = capture(args.root, name, history=True)
            write_json(args.root / name / "final-history.json", frame)
        tmux(args.root, "kill-server")
        event(args.root, action="stop")


if __name__ == "__main__":
    main()

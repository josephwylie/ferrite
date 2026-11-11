#!/usr/bin/env python3
"""reclaim-worktree-builds.py — when the disk runs low, give idle worktrees
the main checkout's build back in place of their own.

Each worktree builds into its own `target/`, which starts as a clone of the
main checkout's and costs no disk until the worktree builds (see
scripts/warm-worktree.sh). What it builds is its own, and nothing else takes
it away, so many worktrees fill a disk. Below MIN_FREE_GB free, the idle
worktree that built longest ago has its build replaced by a fresh clone, then
the next, until there is room. A worktree so treated loses only what it
compiled for its own branch, and compiles that again on its next build.

A worktree is left alone while cargo or rustc runs in it, when it built in the
last IDLE_MINUTES, and when it has built nothing since it was warmed (there is
nothing to take). The main checkout is never touched, and neither is any file
outside a `target/`.

The git hooks run it on every commit and checkout (scripts/git-hooks), which
is as often as agents are at work; a launchd timer cannot, since macOS keeps
background jobs out of a repo under ~/Desktop. It does nothing while there is
room, and one run at a time. With no worktree left to take from, it says so
in a notification, at most once an hour.

Usage: reclaim-worktree-builds.py [--dry-run]
"""

import fcntl
import os
import subprocess
import sys
import time
from pathlib import Path

MIN_FREE_GB = 15
IDLE_MINUTES = 30

SCRIPTS = Path(__file__).resolve().parent
DRY_RUN = "--dry-run" in sys.argv


def run(*command, cwd=None):
    return subprocess.run(command, cwd=cwd, capture_output=True, text=True).stdout


def log(message):
    print(time.strftime("%F %T"), message, flush=True)


def free_gb(path):
    disk = os.statvfs(path)
    return disk.f_bavail * disk.f_frsize / 2**30


def last_built(debug):
    """When anything was last compiled into `debug`: its newest artifact."""
    try:
        return max(
            (entry.stat(follow_symlinks=False).st_mtime for entry in os.scandir(debug / "deps")),
            default=0,
        )
    except OSError:
        return 0


def building_in():
    """The directories cargo and rustc are running in."""
    lines = run("lsof", "-a", "-c", "cargo", "-c", "rustc", "-d", "cwd", "-Fn").splitlines()
    return [line[1:] for line in lines if line.startswith("n/")]


def idle_builds(worktrees):
    """Each idle worktree's build directory with when it last built, oldest first."""
    busy = building_in()
    recent = time.time() - IDLE_MINUTES * 60
    idle = []
    for worktree in worktrees:
        debug = worktree / "target" / "debug"
        if not debug.is_dir():
            continue
        if any(cwd == str(worktree) or cwd.startswith(f"{worktree}/") for cwd in busy):
            continue
        built = last_built(debug)
        warmed = debug / ".warmed"
        if built >= recent or (warmed.exists() and built <= warmed.stat().st_mtime):
            continue
        idle.append((built, worktree, debug))
    return sorted(idle)


def reclaim(worktree, debug):
    """Swap the build for a clone of the main checkout's, then drop the old one."""
    old = debug.with_name("debug.reclaimed")
    subprocess.run(["rm", "-rf", str(old)])
    debug.rename(old)
    subprocess.run([str(SCRIPTS / "warm-worktree.sh")], cwd=worktree, capture_output=True)
    subprocess.run(["rm", "-rf", str(old)])


def notify_once_an_hour(main, message):
    stamp = main / "target" / ".reclaim-notified"
    if stamp.exists() and time.time() - stamp.stat().st_mtime < 3600:
        return
    stamp.touch()
    subprocess.run(
        ["osascript", "-e", f'display notification "{message}" with title "Ferrite builds"'],
        capture_output=True,
    )


def main():
    listed = run("git", "worktree", "list", "--porcelain", cwd=SCRIPTS).splitlines()
    main, *worktrees = [Path(line[9:]) for line in listed if line.startswith("worktree ")]
    free = free_gb(main)
    if free >= MIN_FREE_GB:
        return

    if DRY_RUN:
        for built, worktree, _ in idle_builds(worktrees):
            log(f"would reclaim {worktree.name} (idle {(time.time() - built) / 3600:.1f} h)")
        return

    (main / "target").mkdir(exist_ok=True)
    lock = open(main / "target" / ".reclaim.lock", "w")
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        return

    for built, worktree, debug in idle_builds(worktrees):
        idle_hours = (time.time() - built) / 3600
        reclaim(worktree, debug)
        freed, free = free_gb(main) - free, free_gb(main)
        log(f"reclaimed {worktree.name} (idle {idle_hours:.1f} h): {freed:+.1f} GB, {free:.1f} GB free")
        if free >= MIN_FREE_GB:
            return

    busy = len(building_in())
    log(f"{free:.1f} GB free, under {MIN_FREE_GB}; nothing idle left to reclaim, {busy} compiling")
    notify_once_an_hour(main, f"{free:.0f} GB free and no idle worktree build to reclaim.")


if __name__ == "__main__":
    main()

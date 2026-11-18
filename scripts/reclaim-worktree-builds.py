#!/usr/bin/env python3
"""reclaim-worktree-builds.py — when the disk runs low, give idle worktrees
the main checkout's build back in place of their own, then drop what the main
checkout's own crates last built long ago.

Each worktree builds into its own `target/`, which starts as a clone of the
main checkout's and costs no disk until either of the two builds (see
scripts/warm-worktree.sh). What a worktree builds is its own, and its clone
keeps what the main checkout has rebuilt since, so many worktrees fill a disk. Below MIN_FREE_GB free, the idle
worktree that built longest ago has its build replaced by a fresh clone, then
the next, until there is room. A worktree so treated loses only what it
compiled for its own branch, and compiles that again on its next build.

A worktree is left alone while cargo or rustc runs in it, when it built in the
last IDLE_MINUTES, and when neither it nor the main checkout has built since
it was warmed (its clone still costs nothing).

The main checkout's build is taken from last, and only where it is dead
weight. Cargo keeps one unit per feature set, toolchain and dependency
version, and a crate under `crates/` leaves the old ones behind each time one
of those moves: 169 units of `ferrite-core` on the day this was written. A
unit of such a crate not built in STALE_HOURS is dropped, while nothing
compiles there. A unit still wanted compiles again on the next build; a
dependency's units are kept, since those stay good for as long as the lock
file does. No file outside a `target/` is touched.

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
STALE_HOURS = 24

SCRIPTS = Path(__file__).resolve().parent
DRY_RUN = "--dry-run" in sys.argv


def run(*command, cwd=None):
    return subprocess.run(command, cwd=cwd, capture_output=True, text=True).stdout


def log(message):
    print(time.strftime("%F %T"), message, flush=True)


def free_gb(path):
    disk = os.statvfs(path)
    return disk.f_bavail * disk.f_frsize / 2**30


def last_built(debug, units="*/*"):
    """When anything was last compiled into `debug`: its newest fingerprint.

    Cargo keeps each compiled unit under `build/<package>/<hash>/`, and
    rewrites that unit's `fingerprint/` each time it builds it.
    """
    try:
        return max(
            (stamp.stat().st_mtime for stamp in debug.glob(f"build/{units}/fingerprint/*")),
            default=0,
        )
    except OSError:
        return 0


def stale_units(main):
    """The main checkout's units of its own crates not built in STALE_HOURS."""
    debug = main / "target" / "debug"
    old = time.time() - STALE_HOURS * 3600
    stale = []
    for crate in sorted((main / "crates").iterdir()):
        for unit in (debug / "build" / crate.name).glob("*/"):
            if last_built(debug, f"{crate.name}/{unit.name}") < old:
                stale.append(unit)
    return stale


def building_in():
    """The directories cargo and rustc are running in."""
    lines = run("lsof", "-a", "-c", "cargo", "-c", "rustc", "-d", "cwd", "-Fn").splitlines()
    return [line[1:] for line in lines if line.startswith("n/")]


def idle_builds(main, worktrees):
    """Each idle worktree's build directory with when it last built, oldest first."""
    busy = building_in()
    recent = time.time() - IDLE_MINUTES * 60
    main_built = last_built(main / "target" / "debug")
    idle = []
    for worktree in worktrees:
        debug = worktree / "target" / "debug"
        if not debug.is_dir():
            continue
        if any(cwd == str(worktree) or cwd.startswith(f"{worktree}/") for cwd in busy):
            continue
        built = last_built(debug)
        warmed = debug / ".warmed"
        costs_nothing = warmed.exists() and max(built, main_built) <= warmed.stat().st_mtime
        if built >= recent or costs_nothing:
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
        for built, worktree, _ in idle_builds(main, worktrees):
            log(f"would reclaim {worktree.name} (idle {(time.time() - built) / 3600:.1f} h)")
        log(f"would drop {len(stale_units(main))} stale units of the main checkout's own crates")
        return

    (main / "target").mkdir(exist_ok=True)
    lock = open(main / "target" / ".reclaim.lock", "w")
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        return

    for built, worktree, debug in idle_builds(main, worktrees):
        idle_hours = (time.time() - built) / 3600
        reclaim(worktree, debug)
        freed, free = free_gb(main) - free, free_gb(main)
        log(f"reclaimed {worktree.name} (idle {idle_hours:.1f} h): {freed:+.1f} GB, {free:.1f} GB free")
        if free >= MIN_FREE_GB:
            return

    busy = building_in()
    if not any(cwd == str(main) or cwd.startswith(f"{main}/") for cwd in busy):
        stale = stale_units(main)
        for unit in stale:
            subprocess.run(["rm", "-rf", str(unit)])
        if stale:
            freed, free = free_gb(main) - free, free_gb(main)
            log(f"dropped {len(stale)} stale units of the main checkout: {freed:+.1f} GB, {free:.1f} GB free")
            if free >= MIN_FREE_GB:
                return

    busy = len(busy)
    log(f"{free:.1f} GB free, under {MIN_FREE_GB}; nothing idle left to reclaim, {busy} compiling")
    notify_once_an_hour(main, f"{free:.0f} GB free and no idle worktree build to reclaim.")


if __name__ == "__main__":
    main()

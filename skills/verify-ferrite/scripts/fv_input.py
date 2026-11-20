"""Keyboard and mouse input posted to one process (fv's window pid).

Events go to that pid only (CGEventPostToPid), so the operator's frontmost
app never receives them and focus is not stolen. Needs Accessibility for
the app that runs this (Ferrite.app when an agent runs it).

usage: fv_input.py <pid> key <combo>...      gpui syntax: cmd-, escape cmd-shift-n enter
       fv_input.py <pid> type <text>
       fv_input.py <pid> click --x <x> --y <y> [--double] [--right]
x, y are window points: pixels of an `fv shot` divided by the shot's scale
(a shot of a 1440-point window is 1440 px wide on a 1x display, 2880 on 2x).
"""
import sys
import time

import Quartz

KEYS = {
    **{c: k for c, k in zip("asdfhgzxcv", [0, 1, 2, 3, 4, 5, 6, 7, 8, 9])},
    "b": 11, "q": 12, "w": 13, "e": 14, "r": 15, "y": 16, "t": 17,
    "1": 18, "2": 19, "3": 20, "4": 21, "6": 22, "5": 23, "=": 24, "9": 25,
    "7": 26, "-": 27, "8": 28, "0": 29, "]": 30, "o": 31, "u": 32, "[": 33,
    "i": 34, "p": 35, "l": 37, "j": 38, "'": 39, "k": 40, ";": 41, "\\": 42,
    ",": 43, "/": 44, "n": 45, "m": 46, ".": 47, "`": 50,
    "enter": 36, "return": 36, "tab": 48, "space": 49, "backspace": 51,
    "escape": 53, "esc": 53, "delete": 117, "left": 123, "right": 124,
    "down": 125, "up": 126, "home": 115, "end": 119,
}
MODS = {
    "cmd": Quartz.kCGEventFlagMaskCommand,
    "shift": Quartz.kCGEventFlagMaskShift,
    "alt": Quartz.kCGEventFlagMaskAlternate,
    "ctrl": Quartz.kCGEventFlagMaskControl,
}
GAP = 0.04


def post(pid, event):
    Quartz.CGEventPostToPid(pid, event)
    time.sleep(GAP)


def key(pid, combo):
    mods, name = [], combo.lower()
    while "-" in name[:-1] and name.split("-", 1)[0] in MODS:
        mod, name = name.split("-", 1)
        mods.append(mod)
    if name not in KEYS:
        sys.exit(f"fv_input: unknown key {name!r}")
    flags = 0
    for mod in mods:
        if mod not in MODS:
            sys.exit(f"fv_input: unknown modifier {mod!r}")
        flags |= MODS[mod]
    for down in (True, False):
        event = Quartz.CGEventCreateKeyboardEvent(None, KEYS[name], down)
        Quartz.CGEventSetFlags(event, flags)
        post(pid, event)


def type_text(pid, text):
    for ch in text:
        for down in (True, False):
            event = Quartz.CGEventCreateKeyboardEvent(None, 0, down)
            Quartz.CGEventKeyboardSetUnicodeString(event, len(ch), ch)
            post(pid, event)


def window_origin(pid):
    for w in Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID
    ):
        if w.get("kCGWindowOwnerPID") == pid and w.get("kCGWindowName") == "ferrite":
            bounds = w["kCGWindowBounds"]
            return bounds["X"], bounds["Y"]
    sys.exit(f"fv_input: no on-screen window for pid {pid}")


def click(pid, x, y, double=False, right=False):
    ox, oy = window_origin(pid)
    at = Quartz.CGPointMake(ox + x, oy + y)
    down, up, button = (
        (Quartz.kCGEventRightMouseDown, Quartz.kCGEventRightMouseUp, Quartz.kCGMouseButtonRight)
        if right
        else (Quartz.kCGEventLeftMouseDown, Quartz.kCGEventLeftMouseUp, Quartz.kCGMouseButtonLeft)
    )
    post(pid, Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventMouseMoved, at, button))
    for count in (1, 2) if double else (1,):
        for kind in (down, up):
            event = Quartz.CGEventCreateMouseEvent(None, kind, at, button)
            Quartz.CGEventSetIntegerValueField(event, Quartz.kCGMouseEventClickState, count)
            post(pid, event)


def main(argv):
    if len(argv) < 3:
        sys.exit(__doc__)
    pid, verb, rest = int(argv[0]), argv[1], argv[2:]
    if verb == "key":
        for combo in rest:
            key(pid, combo)
    elif verb == "type":
        type_text(pid, " ".join(rest))
    elif verb == "click":
        if "--x" not in rest or "--y" not in rest:
            sys.exit("fv_input: the visible window clicks at --x and --y only")
        x, y = (float(rest[rest.index(flag) + 1]) for flag in ("--x", "--y"))
        click(pid, x, y, "--double" in rest, "--right" in rest)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])

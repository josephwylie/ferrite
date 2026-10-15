---
status: accepted
date: 2026-10-15
---

# Alerts: sounds, system notifications, and toasts that wait and expire

ADR 0005 decides *when* a Thread wants the operator: a Notice when its Main
finished for good, a live request when a Decision waits. This decision is
about how the window tells them, following Herdr's notifications
(docs/research/herdr-benchmark-2026-10-15.md, #4).

**Seen means looked at.** A Notice or request is read when its Thread is
on screen *while the window has the system's focus*. The window tells the
core Cockpit when it gains or loses focus (`Cockpit::set_window_active`);
while it is away nothing is acknowledged, so a finish on a Pane left on
screen is still news, and coming back reads what is on screen. This amends ADR 0005, where the
focused Pane was read on every pump whether anyone was looking or not.

**One alert per event, after a second.** The bell hears each new Notice
and request, and the event waits `ALERT_DELAY` (1 s). Then it is checked
again: one already read, answered, cancelled or dismissed is dropped — a
permission the operator answers at once, or a finish they were watching,
never interrupts. A survivor rings, unless the operator is looking at it:
the window has focus and its Thread is on the board, whether or not its
Pane is the focused one — a request shows where it is asked. This drops
the toast rule of FL-18, which toasted a finish only while the nav was
folded and its Thread off the board: a finish off the board now toasts
with the nav open too, as the plan's one suppression rule asks. Ringing
means:

- a **toast**, which replaces any toast its Thread had (a Thread has one
  voice) and goes on its own after 8 s for a request, 5 s for a finish —
  or sooner, once read, resolved, or in view. The bell's list keeps the
  record either way;
- a **sound**: a built-in chime for done and one for needs you,
  synthesised as WAV in `ferrite_core::sound` (no audio assets, no audio
  crate) and played by the platform's own player — `afplay` on macOS,
  PowerShell's `SoundPlayer` on Windows (WAV only). The operator may choose
  a file for either (one that will not play falls back to the built-in),
  turn sounds off, or mute a provider; a provider's mute is its own switch,
  kept while sounds are off. Muting is about noise, as the plan lists it
  under sounds: a muted provider's Threads still toast and still post
  system notifications, since a request still waits on the operator. Alerts that ring together chime once,
  needs-you over done, so Threads finishing at once never pile up chimes;
- a **system notification** (superseded in part by ADR 0005's
  2026-10-15 update: herdr-focus-notify now decides whether one is posted,
  also while the window is frontmost and for what is in view on a Pane
  that is not focused, and delivers it on macOS) through
  GPUI's `show_system_notification` (`UNUserNotificationCenter` on
  macOS, which needs the app bundle, so a dev build posts none; toast
  notifications on Windows). Its tag is the Thread's, so a newer one
  replaces it in the notification centre; clicking it brings the window
  forward and opens what it was about, and once read it is taken back out.
  Only the test platform has checked this so far. A packaged Windows build
  is unchecked, and an unpackaged AppUserModelID may need a Start-menu
  shortcut before Windows shows its toasts. The package script makes no
  shortcut yet.

**Machines off screen** (ADR 0026) ring through the same bell. Each
one's status link hears its Server's Notices and requests as well as its
Threads' states, and the bell hears every machine apart, keyed by machine
(`MachineKey`, its ssh target): ids are per Store, so Thread 3 here and
Thread 3 on box are two Threads, each with its own toast and its own
system notification tag. What a machine held when the window first heard
it is not news — the window has just left it, or met it already waiting
— so only what it says afterwards rings. After that it is the same as
here: the second's wait and the check again (against that machine's
Notices), one toast per Thread, 8 s and 5 s, one chime with needs-you
over done and the provider's mute read from the machine's Thread state,
and a system notification while the window is in the background. Nobody
looks at a machine off screen, so none of its alerts is ever in view.
Its rows and toasts name the machine (`box › Release`), its toasts offer
`⏎ open` alone (a status link answers nothing), and ⌫ does not dismiss
its rows (only the machine's own windows change its Notices). Opening
one — a toast, a row, a system notification's click — puts the window
on that machine (`CockpitWindow::go_to`), and the Cockpit built there
opens it once attached: the request or Notice if it still stands, else
its Thread, revived if it was parked since. A machine that does not
answer within 10 s opens nothing: the operator has moved on, and focus
is not taken from them minutes later. The badge counts the unread
requests off screen too, so `mark all read` asks every machine off
screen to read its Notices as well — otherwise the badge could only be
cleared by visiting each. Finished turns list newest first across
machines.

The bell moves with the window. Going to another machine, all it holds
is re-keyed (`Bell::moved`): what it heard of each machine, the events
waiting out their second, the toasts standing and the system
notifications posted. The one left is now off screen and what it held
is heard already; the one arrived at is now on screen and what rang of
it off screen does not ring again. So an alert rings once, wherever the
window is when it does, and an event heard just before the move still
rings after it. A toast about a third machine stands its full time, and
a system notification posted before the move still opens on its own
machine and is still taken back once read: its tag names the machine
absolutely (this computer's tags have no `@target`), not as on or off
screen, so it is the same tag after the move.

A machine not yet heard from — the one arrived at, until its Server
answers, and the one left, until its status link does — cannot say
whether an alert about it still stands; until then its Notices are none,
not news. Its toasts stand on, its system notifications stay posted, and
its waiting events wait for it, but no longer than their toast would
have stood: a machine that never answers drops them unrung.

The timing lives in the window's `Bell` (fed `now` from the pump, so tests
drive it with the executor's clock); the core keeps deciding what is
unread. Tests and visual captures never play a sound: the Cockpit records
the chime it would have played.

Settings has a Notifications page for all of it: sounds on or off, each
chime's file (play, choose…, built-in), per-provider sounds, and system
notifications on or off. Toast timing has no setting.

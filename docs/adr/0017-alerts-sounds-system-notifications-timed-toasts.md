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
- while the window is in the background, a **system notification**
  through GPUI's `show_system_notification` (`UNUserNotificationCenter` on
  macOS, which needs the app bundle, so a dev build posts none; toast
  notifications on Windows). Its tag is the Thread's, so a newer one
  replaces it in the notification centre; clicking it brings the window
  forward and opens what it was about, and once read it is taken back out.
  Only the test platform has checked this so far. A packaged Windows build
  is unchecked, and an unpackaged AppUserModelID may need a Start-menu
  shortcut before Windows shows its toasts. The package script makes no
  shortcut yet.

The timing lives in the window's `Bell` (fed `now` from the pump, so tests
drive it with the executor's clock); the core keeps deciding what is
unread. Tests and visual captures never play a sound: the Cockpit records
the chime it would have played.

Settings has a Notifications page for all of it: sounds on or off, each
chime's file (play, choose…, built-in), per-provider sounds, and system
notifications on or off. Toast timing has no setting.

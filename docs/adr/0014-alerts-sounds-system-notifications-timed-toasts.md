---
status: accepted
date: 2026-10-15
---

# Alerts: sounds, system notifications, and toasts that wait and expire

ADR 0005 decides *when* a Thread wants the operator: a Notice when its Main
finished for good, a live request when a Decision waits. This decision is
about how the window tells them, following Herdr's notifications
(docs/research/herdr-benchmark-2026-10-15.md, #4).

**Seen means looked at.** A Notice or request is read when the operator
lands on its Thread *while the window has the system's focus*. The window
tells the core Cockpit when it gains or loses focus
(`Cockpit::set_present`); while it is away nothing is acknowledged, so a
finish on the very Pane left focused is still news, and coming back reads
the focused Pane as landing on it does. This amends ADR 0005, where the
focused Pane was read on every pump whether anyone was looking or not.

**One alert per event, after a second.** The bell hears each new Notice
and request, and the event waits `ALERT_DELAY` (1 s). Then it is checked
again: one already read, answered, cancelled or dismissed is dropped — a
permission the operator answers at once, or a finish they were watching,
never interrupts. A survivor rings, unless the operator is looking at it:
the window has focus and the event is in view — a request on the focused
Pane (its keys answer it there), a finish on the board. Ringing means:

- a **toast**, which replaces any toast its Thread had (a Thread has one
  voice) and goes on its own after 8 s for a request, 5 s for a finish —
  or sooner, once read, resolved, or in view. The bell's list keeps the
  record either way;
- a **sound**: a built-in chime for done and one for needs you,
  synthesised as WAV in `ferrite_core::sound` (no audio assets, no audio
  crate) and played by the platform's own player — `afplay` on macOS,
  PowerShell's `SoundPlayer` on Windows (WAV only). The operator may choose
  a file for either, turn sounds off, or mute a provider;
- while the window is in the background, a **system notification**
  through GPUI's `show_system_notification` (`UNUserNotificationCenter` on
  macOS, which needs the app bundle; a dev build posts none). Its tag is
  the Thread's, so a newer one replaces it in the notification centre;
  clicking it brings the window forward and opens what it was about, and
  once read it is taken back out.

The timing lives in the window's `Bell` (fed `now` from the pump, so tests
drive it with the executor's clock); the core keeps deciding what is
unread. Tests and visual captures never play a sound: the Cockpit records
the chime it would have played.

Settings has a Notifications page for all of it: sounds on or off, each
chime's file (play, choose…, built-in), per-provider sounds, and system
notifications on or off. Toast timing has no setting.

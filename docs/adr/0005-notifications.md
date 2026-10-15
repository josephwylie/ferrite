---
status: accepted, amended by 0017 and the 2026-10-15 update below
date: 2026-09-06
---

# Notifications: one headless module says when a Thread finished

A Thread's Main has finished for good when the provider's own turn end
arrives and no descendant is still working, pending, or awaiting a Decision.
That judgement lives in one deep module, `ferrite_core::notifications`,
behind a small interface:
`observe` folds one Thread's frame, `attention` says whether its Pane wants
the operator's eye, `open`/`acknowledge`/`dismiss`/`clear` are the verbs.
It reads only what the adapters already normalised into `Activity` — Main's
turn ends (`ActivityUpdate::main_settled`, root or autonomous), Main's busy
state, the operator-turn flag, and every child's status — and takes `now`
from the Cockpit's pump. No provider JSON, no clock, no window.

Subagents never notify. A Main whose turn ended while children still work
is deferred: Claude resumes such a Main from each background agent's
`task_notification` (a per-turn `init`, then a `result` whose `origin` is
`task-notification`), so the finish is the resumed turn's; Codex's parent
waits on its children inside its own turn, so its `turn/completed` already
comes last. A provider that never resumes an idle Main is given a short
grace after its last child settles, then the deferral concludes. The
adapters supply the earliest "Main is at work" fact each provider has —
Codex's root `turn/started`, and Claude's repeated `init` at the head of
every turn, added by this decision — so a Main between its own result and
its resume never reads as finished. An interrupt is the operator's own act
and never notifies; a held prompt going out at turn end is more work the
operator queued, not a finish they wait on.

A Notice is unread until the operator lands on its Thread or opens it from
the bell. The Cockpit acknowledges the focused Thread on every pump and at
its focus doors, so no window bookkeeping decides what has been seen.
Parking ends live completion tracking and discards any deferral; existing
Notices remain available to reopen the Thread. Replaying its history never
counts as a new live finish.

The window shows Notices three ways, all through GPUI Kit: the bell in the
nav's chrome band with a `Badge` count and a `Popover` panel; a toast per
finish in the kit's `Notification` stack at the board's top-right, which
auto-hides and lands the operator on the Pane when clicked; and the Pane's
existing focus ring, which pulses on an unread Notice until focus lands.
The ring is the same ring: motion, not a second colour, says which Pane
wants the operator. ADR 0017 adds sounds and system notifications, makes
toasts wait a second and expire, and reads what is on screen only while
the window has focus.

Both adapters are proven by replaying the committed captures through the
real Sessions into the Cockpit: the Claude overlap capture finishes exactly
twice and never while its agents run, the nested and both Codex captures
finish once. The rule's own cases run headless with an injected clock.

## Update 2026-10-15: system notifications are herdr-focus-notify's

The system notification is no longer Ferrite's own code. Following the
user's decision to copy community plugins 1:1, Ferrite vendors
[herdr-focus-notify](https://github.com/yankewei/herdr-focus-notify)
(MIT, yankewei) as `crates/ferrite-focus-notify`, with its source verbatim
apart from a Ferrite adapter in place of its Herdr adapter (the crate's
README lists every change and the upstream commit).

The split: this ADR and ADR 0017 still decide **what** a Notice is, when it
is read, and when the bell rings (the second's wait, one voice per Thread,
nothing for what is in view). For each alert that rings, focus-notify
decides **whether** it becomes a system notification, **how** it is
delivered, and **how its click lands**:

- **Whether.** Upstream skips a notification only when the pane is the
  focused one *and* its terminal is frontmost; otherwise it sends. Ferrite is
  its own terminal, so the Thread's Pane being the focused one and the
  window having the system's focus stand for those
  (`ferrite_focus_notify::notification_decision`). Where this overlapped
  ADR 0017's rule (a system notification only while the window is in the
  background), focus-notify's wins: a Thread off the board that rings while
  the window is frontmost now posts a system notification beside its toast.
  Where it overlaps Ferrite's rules for what is in view, Ferrite's win.
  Upstream sends for any pane but the focused one. Ferrite reads every
  Notice and request of a Pane's shown Subject while the window has focus
  (**seen**), on its next pump, well inside the second's wait; and the bell
  keeps quiet about whatever still stands on a Thread on the board (ADR
  0017), such as a Subagent's request while its Pane shows Main. So while
  the window has focus nothing on the board posts, focused Pane or not: a
  system notification is for what the operator is not looking at, and
  Ferrite decides what that is. focus-notify's skip (the focused Pane in
  the frontmost window) therefore repeats what the board already kept
  quiet; it stays as the vendored decision rather than being special-cased
  away.
  Upstream's other overlap, notifying only `blocked` and `done`, already
  holds: a live request is blocked, a finished turn done; a failed turn is a
  Notice and goes as done, its words saying it failed.
- **How.** On macOS, a clickable `alerter` notification, laid out as
  upstream lays out its rows: `Blocked · {Thread}` or `Done · {Thread}` as
  the title (the Thread named by its machine when off screen), the provider
  and the git state of the Thread's directory (`main* · +120/-45`, upstream's
  2-second budget) as the subtitle, the row's detail as the message (with
  upstream's copy when it has none), and the provider's bundled icon. Its tag
  is the Thread's, so a newer one replaces it. Upstream's visibility monitor
  is not used: a notification whose Notice or request is read, answered or
  gone is taken back by the bell as before, through upstream's withdraw
  (`alerter --remove` and the cleared marker, so one still queued never
  shows). Without `alerter` installed, on Windows and Linux (upstream is
  macOS only), and in tests and captures, GPUI's notification centre carries
  the same posting, under the same tag, with the same click.
- **Click.** Upstream exec'd itself to focus the Herdr pane over Herdr's
  socket and raise the terminal window running it. Ferrite waits on the
  notification itself, and a click comes back in-process: the window comes
  forward (the click handler activates the app, since alerter no longer
  does) and lands on the Thread as ADR 0017 describes, on its machine
  when that is off screen. Upstream's terminal learning (which app shows a
  workspace) has nothing to learn here and is not used. The price of an
  in-process click: a notification still up once Ferrite is gone would print
  its click to a closed pipe and land nowhere, where upstream's exec'd
  helper still worked. So Ferrite takes back every notification still up as
  it quits (`Notifier::withdraw_all`); after a crash the stragglers stay
  until dismissed or alerter's hour runs out, and a click on one does
  nothing. Each posting also holds one OS thread, mostly asleep, while
  alerter waits on the operator.
- **What is not carried over.** Upstream's visibility monitor
  (`SendWithVisibilityMonitor`) watched, from a separate process, for the
  pane becoming visible and took the notification back; Ferrite's bell
  already takes back every posting whose Notice or request is read on its
  next pump, in-process, so the decision is treated as a plain send.
  Upstream found alerter afresh on every event, being a fresh process per
  event; Ferrite looks once per run, so alerter installed while Ferrite
  runs is used from the next launch. Upstream is macOS only, so there is no
  upstream delivery to adopt on Windows and Linux: GPUI's notification
  centre stays the carrier there rather than none at all.

---
status: accepted
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
wants the operator. Toasts are in-app only and silent; system delivery is
a later choice.

Both adapters are proven by replaying the committed captures through the
real Sessions into the Cockpit: the Claude overlap capture finishes exactly
twice and never while its agents run, the nested and both Codex captures
finish once. The rule's own cases run headless with an injected clock.

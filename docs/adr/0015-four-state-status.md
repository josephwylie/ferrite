---
status: accepted
date: 2026-10-15
---

# Four-state status: blocked, done, working, idle

Every list that ranks Threads — the nav, its headings, the rail, the
palette, the bottom bar's tabs — reads one of four states, Herdr's model
in priority order: **blocked** (a Decision waits, or the turn or Session
failed), **done** (a turn finished that the operator has not seen),
**working** (Main works, or only its Subagents or background tasks do),
**idle**. Herdr infers these from the screen; Ferrite computes them from
what it already knows exactly — the transcript's status, Activity's pending
Decisions and working descendants, the transcript's background tasks, and
the unread Notices of `ferrite_core::notifications`. `cockpit::Attention`
names the four; `nav::RowStatus` keeps Ferrite's finer states (failing,
failed, background, parked) as faces within them, and its `Ord` agrees with
`Attention`, so a heading's roll-up is the `max` of its rows and the
attention sort is a stable sort on it.

**Seen** means on screen while the window has focus. The Cockpit follows
the window's activation (`Cockpit::set_window_active`, fed by an activation
observer) and acknowledges every Pane on screen — a Group's members as well
as the focused Pane, the one fullscreen Pane when there is one — on every
pump, at its focus doors, and the moment the window comes back. A finish on
screen while the window is away stays done until the operator returns. The
Pane's pulsing ring no longer has a case to draw: a Pane on screen is seen.

**Faces.** Ferrite keeps its own inks rather than Herdr's: working green,
a Decision yellow, a failure red — red already means failed here, and
yellow already means the operator is wanted. Done is new: a teal `✓`
(`theme::FINISHED`), distinct from every dot. Idle stays the muted solid
dot, because the hollow ring already means "not Main's live work": a faint
ring is parked, a green ring is background work behind an idle Main.

**What it replaced.** The Needs-you strip duplicated rows that the yellow
mark, the waiting reason word (`approval`, `plan`, `question`) and the
attention sort already carry, so it is gone; ⌘D still walks
`Cockpit::needs_you` oldest request first. Nav headings are one style — a
lowercase label in the metadata ink with the section's roll-up mark — and
the folded nav is a 6ch rail of numbered rows with their marks rather than
nothing.

**Palette filters.** On the palette's empty line `b` `w` `i` `d` keep one
state's rows (a Project or Group by its roll-up) and `a` all of them; once
the line holds text, every key types. Commands have no state and stay, so
a command's first letter lost to a filter still matches on the rest.

Herdr's own "idle ○" and its yellow working dot were considered and
declined for the reasons above; its screen detection was never an option.

---
status: accepted
date: 2026-10-15
---

# Four-state status: blocked, done, working, idle

Every list that ranks Threads — the nav, its headings, the rail, the
palette, the bottom bar's tabs — reads one of four states, Herdr's model
in priority order: **blocked** (a Decision waits), **done** (a turn finished that the operator has not seen),
**working** (Main works, or only its Subagents or background tasks do),
**idle**. Herdr infers these from the screen; Ferrite computes them from
what it already knows exactly — the transcript's status, Activity's pending
Decisions and working descendants, the transcript's background tasks, and
the unread Notices of `ferrite_core::notifications`. `cockpit::Attention`
names the four; `nav::RowStatus` keeps Ferrite's finer states (failing,
failed, background, parked) as faces within them — failing and background
are working, failed and parked are idle — and its `Ord` agrees with
`Attention`, so a heading's roll-up is the `max` of its rows and the
attention sort is a stable sort on it — of the loose rows, of each Group's
members, of the Groups in a section and of the sections, each by its
roll-up. Like `sort: recent`, it holds still under the pointer: the ranks
the tree was sorted by are kept with the drawn order until the pointer
leaves.

**Seen** means on screen while the window has focus. The Cockpit follows
the window's activation (`Cockpit::set_window_active`, read when the first
frame has a window and then fed by an activation observer, so a window that
opens unfocused sees nothing) and acknowledges every Pane on screen — a Group's members as well
as the focused Pane, the one fullscreen Pane when there is one — on every
pump, at its focus doors, and the moment the window comes back. A finish on
screen while the window is away stays done until the operator returns. The
Pane's pulsing ring no longer has a case to draw: a Pane on screen is seen.
Every word follows the same rule: the Pane head and the wall tile say
`done` only while the finish is unseen, and say nothing once it is seen,
as the nav row then shows its age.

**Failed is a secondary state.** A failed turn or a closed Session keeps
its red mark and `failed` word on its own row, but ranks as idle: it does
not roll up into its Project's or Group's heading, sorts with the idle
rows under `sort: attention`, and answers the `i` filter, not `b`.
Ranking it blocked would have pinned a dead Session to the top and turned
its heading red until the operator parked it, though nothing on it waits
for an answer; blocked is reserved for a Decision.

**Faces.** Ferrite keeps its own inks rather than Herdr's: working green,
a Decision yellow, a failure red — red already means failed here, and
yellow already means the operator is wanted. Done is new: a teal `✓`
(`theme::FINISHED`), distinct from every dot. Idle stays the muted solid
dot, because the hollow ring already means "not Main's live work": a faint
ring is parked, a green ring is background work behind an idle Main.

**What it replaced.** The Needs-you strip duplicated rows that the yellow
mark, the waiting reason word (`approval`, `plan`, `question`) and the
attention sort already carry, so it is gone; ⌘D still walks
`Cockpit::needs_you` oldest request first. The reason `plan` is Claude's
alone: it is read off the `ExitPlanMode` tool, and Codex has no plan
approval to wait on, so a Codex request reads `approval` or `question`.
Nav headings are one style — a lowercase label in the metadata ink with
the section's roll-up mark — and the folded nav is a 6ch rail of rows with
their marks rather than nothing. The rail's rows start under the titlebar band, so
the host traffic lights, which ADR 0014 left on the plane when folded, now
overhang the rail's chrome onto the plane with nothing under them.

**Rail digits are keys.** In Herdr a rail number is the key that jumps
there. Ferrite's ⌘1–9 already mean the shown board's Panes in head order
(R5), so the rail keeps the tree's order and shows a row's digit only
where ⌘ and that digit focus it; a row off the board shows its mark alone.
Numbering the tree instead would have made line 3 and ⌘3 two different
Threads.

**Palette filters.** A line of one letter is a filter, not a search: `b`
`w` `i` `d` keep one state's rows (a Project or Group by its roll-up), `a`
all of them, and the state's name shows at the input's right. The next
letter turns the line into a search, so no key is lost to a filter — `d`
alone keeps the done rows, `debug` finds the idle Thread named so.
Binding the letters as actions on the empty line was tried and dropped:
the first letter of every search starting with one of them became a
sticky filter that hid the row being searched for. Commands have no state
and stay under a filter.

Herdr's own "idle ○" and its yellow working dot were considered and
declined for the reasons above; its screen detection was never an option.

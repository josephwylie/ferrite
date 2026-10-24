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

## Amendment (2026-10-21): the done face, the rail and the Needs-you view

The UI refresh ([ADR 0033](0033-sans-chrome-and-card-panes.md), SPEC §5.1
and §6) changes three things here.

- **Done is a shape, not a hue.** Its mark is a `check-circle` in
  `TEXT_DIM`; the teal `theme::FINISHED` no longer colours it. An unseen
  finish's title stays `TEXT_STRONG`. Idle and parked lose the muted dot
  too: a `terminal` icon, at 50% when parked. Background keeps its green
  ring.
- **The rail is 52px**, not 6ch: one 32px cell per open Thread, holding its
  mark and, under it, its ⌘ digit where that key focuses it.
- **The Needs-you queue is a view.** The nav's `Needs you` row (with its
  count, hidden at 0), the bar's `N need you` and ⌘I show, in the tree's
  place, the blocked Threads oldest-first and then the Notices under
  `Recent`. It replaces the bell's list. It is not the strip this ADR
  removed: the tree still carries no queue, and ⌘D still walks
  `Cockpit::needs_you`.

## Amendment (2026-10-24): the Attention list, herdr's Agents panel

herdr's sidebar has two panels, and both are always on screen. Spaces
lists the projects with their rolled-up state. Agents lists every agent
with its own state. Ferrite copied the four states, the roll-up and the
attention sort, but not the Agents panel. Smart filtering then stood in
for it, first as `done` and `working` sections over the tree (2026-10-06)
and later as two views behind a setting that was off by default (SPEC
§5.1.2, 2026-10-21). Neither left a list of what is in flight on screen.

The nav now carries one on purpose. The **Attention list** sits over
`Projects`, always on, with no setting. It has three sections: `Needs
you` (blocked, oldest request first, ⌘D's order), `Done` (unseen, newest
first) and `Working` (creation order, so nothing shuffles under the
pointer). Each row is one line, and each section folds to its head and
count. Smart filtering, its setting, its views and its palette commands
are gone. A settings file that still names `smart_filtering` loads, and
the key is ignored.

- **The tree still carries no queue.** The list sits over the tree, not in
  it. Every Thread keeps its own row in its Project, in the tree's order.
  This is not the Needs-you strip this ADR removed: that strip was a
  second copy of the tree's yellow rows. The list is the one place where
  done and working Threads from every Project stand together.
- **No idle section.** herdr's Agents panel lists idle agents too.
  Ferrite's tree already lists every Thread, so repeating the idle ones
  would double the column.
- **No failed section.** Failed still ranks as idle, as above. It rarely
  happens, and its row in the tree keeps its red mark.
- **Board groups do not rank.** A task's own words (`ready to approve`)
  stay on its row's line 2. The list ranks Threads by their Attention
  alone.
- **A finish never leaves from under the operator.** The focused Thread
  is held under `Done` in two cases. Opening a done Thread reads its
  Notice, yet its row stays under `Done`. A Thread that finishes while the
  operator watches it is seen at once, and it moves from `Working` to
  `Done` and stays there. Either way the row leaves once focus moves to
  another Thread. Nothing else is held: an interrupt raises no Notice, so
  that row leaves, and a row never stands in a section that no longer says
  what happened. This rule lives in the nav, not in core: the Notice is
  read when it is seen, as ever, so the Bar, the Pane head, the toasts and
  ⌘D are unchanged.

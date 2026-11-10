---
status: accepted
date: 2026-11-10
---

# The Panel is one tabbed host for every side surface

The Panel was built after Claude's: one scroll of sections with no tabs,
which a preview, a side chat, the tools' dock (Files, Changes, Terminal),
the browser, an agent's drill-in and background tasks each took over or
sat beside in their own way. That grew to about twelve side-surface
ideas, four tab strips, six expand buttons, about nine close buttons and
five width rules, with a priority order deciding which body wins. The
operator compared it with T3 Code's right panel, "one thing with tabs",
and chose T3's. The measured spec is
`docs/design/t3-reference/panel-composer.md`.

## Decision

The Panel holds **Surfaces as tabs**, one list per Thread: an Overview
(environment, context, connectors), Changes, Files, a File (a preview),
a Browser tab, a Terminal, a side chat, an agent, background work, a
pull request. A Surface's identity is its kind plus what it shows, so
opening something already open activates its tab rather than adding one.
Every way in (a file link, a changed file, ⇧⌘D, a side chat, an agent
row) goes through that one door.

One head row: the tabs, a **+** menu listing every Surface with a
one-letter key (unavailable ones dimmed, with the reason), and maximize.
An empty Panel shows the same list as "Open a surface". Each Thread's
tabs, its active tab, maximized and width persist across restarts; an
agent may reveal a Surface but not take the Panel from a tab the operator
just chose. The title bar keeps only the Panel toggle; the per-surface
tab strips, expand and close buttons and the dock's columns go.

The Review slot and a Thread beside another stay where they are: they
are the board's layout, not side surfaces.

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
a Browser tab, a Terminal, a side chat, background work, a pull
request. A Surface's identity is its kind plus what it shows, so
opening something already open activates its tab rather than adding one.
Every way in (a file link, a changed file, ⇧⌘D, a side chat) goes
through that one door.

One head row: the tabs, a **+** menu listing every Surface with a
one-letter key (unavailable ones dimmed, with the reason), and maximize.
An empty Panel shows the same list as "Open a surface". Each Thread's
tabs, its active tab, maximized and width persist across restarts; an
agent may reveal a Surface but not take the Panel from a tab the operator
just chose. The title bar keeps only the Panel toggle; the per-surface
tab strips, expand and close buttons and the dock's columns go.

The Review slot and a Thread beside another stay where they are: they
are the board's layout, not side surfaces.

## Amendment (2026-11-21): a Subagent opens in the Pane, not the Panel

The agent Surface is gone. A Subagent had two views, its full chat in the
Pane (the Pane switching its Subject to it) and a compact one held in the
Panel, and the operator wants only the full chat. Every way to a Subagent
(its `Ran agent` row, Background work's `Open` and `Open thread`, the
details card's Lineage rows) now switches the Pane's Subject. A
`panel.json` written before this still loads: a tab of a kind this build
no longer has is dropped, and every other tab is kept.

The Subagent's chat has no head over it either. The strip that stood
there (back, `Main ›` crumbs, the agent's mark, name and state word,
what its transcript covers, `Stop`) said again what lives elsewhere: the
title bar's back arrow (⌘[) and Escape on a finished Subagent go back,
the details card's Lineage names each agent and its state, and Background
work's `Stop agent` (or Escape on a running Subagent) stops it. Its
`Retry` for a saved history that failed to load went with it; opening the
Subagent again asks again.

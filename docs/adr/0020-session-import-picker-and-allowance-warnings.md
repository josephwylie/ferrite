---
status: proposed
date: 2026-10-15
---

# Session import is a palette scope; allowance warnings are crossings

From the Herdr benchmark (docs/research/herdr-benchmark-2026-10-15.md):
search past Claude/Codex sessions and resume one, and warn when the
5-hour or weekly allowance runs low.

## Decision

1. **One picker, in the palette.** Importing a session opens the ⌘K
   palette scoped to sessions (`PaletteScope::Sessions`), reached from
   its `import a session` command and from `/import`. The Composer's
   popover picker it replaces listed eight files and could not be
   searched; the palette already has a query line, fuzzy matching over a
   row's name and context, sections, a veil and its keys. A row is the
   session's title (or first prompt, cut as a Title is) with its project,
   provider and age as context, so the one query searches all of them.
2. **Listed lazily, off the UI thread.** The palette opens at once and
   fills when the Spawner's listing answers (`discover_sessions`); a
   picker closed or reopened meanwhile drops the late answer. Claude's
   store is read newest-first by mtime and stops at the cap (200), so a
   long history costs a stat per file rather than a read of each.
3. **Where an import lands.** Enter adopts through the core's
   `adopt_into` — `import.rs`, then a revive that resumes the file's own
   session. It takes the place of a blank Pane the picker was opened for
   (a draft with nothing typed, or a Thread that still offers import, as
   `/import` always is) and otherwise opens alongside, focused. A refusal
   speaks in that Pane, or as a toast when there is none.
4. **A warning is a crossing, not a level.** When a provider reports
   its windows, the reading is compared with the one remembered before
   it (`providers/limits.rs`); a window that rises past one of
   `Settings::allowance_warnings` (default 80 and 95 percent; empty is
   off) toasts once, naming the highest threshold passed. Because the
   remembered reading survives launches, a relaunch over a high window
   does not warn again; a window that rolled over counts from nothing.
   The warning is a toast, not a Notice: it belongs to the account, not
   to a Thread.

---
status: accepted
date: 2026-10-27
---

# The app owns the window's frame; the Cockpit fills it

Follows the crate split (#162, #168, #176).

## Context

The Cockpit crate (`ferrite-cockpit`) drew the titlebar, the bottom bar
and the Settings sheet itself, and held the key table's binding and the
menu bar, because `CockpitView` renders the whole window. An edit to any
of them rebuilt the largest crate in the workspace, and the crate owned
more than the Cockpit.

Two ways out: the app owns a root view that hosts the Cockpit as a child,
or the Cockpit keeps rendering and takes the frame as slots it fills.
A root view would put a second view above the Cockpit's cached parts
(ADR 0012): every part's notify would re-render it, and the frame would
need the Cockpit's state (views, vitals, every setting) made public.

## Decision

- **Slots, not a host.** `ferrite_ui::frame::Frame` is the seam: the
  titlebar strip, the nav head's drag stretch, the bottom bar, the
  Settings sheet and binding the key table. The app installs one
  (`ferrite_ui::frame::install`) at launch, process-wide like the key
  table (ADR 0024). The Cockpit still draws the frame in its own render,
  so ADR 0012's caching, `notify_frame` and the render counts are as
  they were.
- **The Cockpit says what; the frame draws.** It fills the bar with data
  (`Bar`: tabs, the need-you count, usage, free disk, the clock, and the
  presses) and Settings with pages of rows (`frame::settings`): each row
  a control as data and the closure that changes the setting. Values and
  persistence stay the Cockpit's (`change_settings`).
- **`ferrite-frame`** draws them and binds the keys; it sits beside the
  Cockpit, on `ferrite-ui` and the `ferrite-core` types the bar's data
  names (`Provider`, `RateLimits`), never on the Cockpit. The app (`ferrite`) installs it with
  its menu bar. The Cockpit's own tests install it too, as a
  dev-dependency, so they keep drawing the real frame.
- The sheet recipe (`ferrite_ui::sheet`) is shared by Settings and the
  Cockpit's own sheets; `Quit` lives with the key table that names it.

## Consequences

- An edit to the titlebar, the bar's drawing or the Settings sheet
  rebuilds `ferrite-frame` and the app, not the Cockpit.
- A new kind of Settings control is a `Control` variant and its drawing;
  a new setting is a row in the Cockpit, as before.
- A window opened without an installed frame panics; the app installs it
  before `open_window`, the Cockpit's test build on first use.

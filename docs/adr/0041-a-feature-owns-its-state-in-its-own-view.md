---
status: accepted
date: 2026-11-18
---

# A feature owns its state in its own view

The Cockpit's root view (`CockpitView`) held 140 fields, and every feature
read and wrote any of them. A change to one feature's state broke another
that happened to touch it, and nothing in the code said whose a field was.

## Decision

- **A feature owns its state.** A feature that keeps state of its own lives
  in a view of its own (an `Entity`), and the fields live on that view. The
  root holds the entity, never the fields; where it still reads one, it
  reads it through the entity.
- **The window hears it through events.** The feature tells the root what
  the operator asked for by emitting one event enum. The root subscribes
  once and acts on it. A feature never updates the root from its own
  listeners, so neither is ever borrowed while the other runs.
- **Inputs are plain values.** What the feature draws from the root's state
  it reads in its render through one root method that returns plain
  values, through a weak handle to the root. Reading the root in render
  keeps today's redraw rule: a root notify still redraws the feature
  (vendored gpui, `tracking_reads`). Root code that runs while the root is
  borrowed passes those values in instead of having the feature read them.
- **The nav is the first slice.** `NavView` (`cockpit/nav_view.rs`) owns
  the nav's own state: its motion, scroll and viewport, keyboard focus and
  cursor, hover, search, picks and Park sweep, and Undo. It emits
  `NavEvent`. What the rest of the window also decides stays on the root:
  whether the nav is collapsed or forced open, its width measurements, the
  Project filter, its notice line. The other features follow one at a
  time, each in its own change.
- **Settings is the second.** `SettingsView` (`cockpit/settings_view.rs`)
  owns whether the sheet is up, its exit, its page, its search field and
  its focus. It emits `SettingsEvent`. Rows of a feature that has no view
  of its own yet (Connections, Permissions, Machine defaults, Server,
  Ferrite updates) the root still builds, with its own listeners, and
  hands over with the inputs. Each moves when its feature does.
- **The Project card is the third.** `ProjectEditorView`
  (`cockpit/project_editor_view.rs`) owns the open card, the one fading
  out, its focus, and on a machine its path field and the listings it
  completes from. It emits `ProjectEditorEvent`. What changes the Projects
  (create, rename, add or remove a directory, remove a Project) stays the
  root's, as do the closes that make the card modal, since they close other
  features' state.
- **ADR 0042 moves part of the last two.** Whether Settings or the
  Project card is up, its exit and its veil are now the sheet host's, and
  each view exists only while its sheet is up.
- **Connections is the fourth.** `ConnectionsView`
  (`cockpit/connections_view.rs`) owns the environment editor and the
  attempt it runs, a Remove waiting for its confirm, and the last error
  and warning. It emits `ConnectionsEvent`; the root saves and answers by
  a method on the view. What it asks of the sheet it is a page of (show
  the page, take the keyboard back) it emits as `PageEvent`, which the
  sheet hears: it holds no handle to the sheet. The Settings sheet makes and holds it, so none of
  it outlives the sheet; the root still closes the editor at the close,
  since the view lives on through the sheet's fade. Its save stays its
  own, not `change_settings`: it keeps nothing it could not write, and
  will not leave an environment whose files are unsaved.
- **PRs is the fifth.** `PrsView` (`cockpit/prs_view.rs`) owns the PRs
  destination's scope and filter, the lists it has read and the reads still
  out, the marked row and whether its detail shows, the PR view chosen, its
  scroll and its focus; its keys answer on its own element. It reads the
  Projects and an open PR through `prs_inputs`. The root holds it for the
  window's life, so the list is as it was left at the next visit. It emits
  `PrsEvent`: the root asks the forge for a list and hands the answer to the
  view to wait on, reads an opened PR, gives the board back, or opens the
  Project card. Whether the PRs stand in the board's place stays the root's
  `destination`, shared with the Factory. The Panel's PR reader has no view
  of its own yet, so the root draws an open PR's body with its own listeners
  and hands it over with the inputs, as Settings takes other features' rows.
- **The Side chat is the sixth.** `SideChatView`
  (`cockpit/side_chat_view.rs`) owns every Thread's side chats in one
  view: each tab's field, title and fork once it lands, the tab each
  Panel last showed, the forks still on their way, the field's wait for
  the keyboard, and a Trash waiting for its second press. One view, not
  one per Thread: the forks land in the order they were asked, whichever
  Thread asked, so one queue has to see them all. The Panel draws it as
  the body of whichever side chat it shows, and the view reads which
  that is with its inputs, so the root no longer tends it every frame.
  It emits `SideChatEvent`: a question for a fork, Bring back, Trash.
  The fork is a Thread of the engine, so asking for it, binding it when
  it lands and retiring it with its tab stay the root's, as does which
  Surface the Panel shows.

## Precedent

Zed builds its workspace this way: each panel is an entity that owns its
state, emits events, and is subscribed to by the workspace. We take the
shape only. We did not adopt Zed's `Panel` and `Dock` traits, its action
registry or its settings store. Ferrite has one nav and one Panel (ADR
0039), and nothing yet needs a general host for more. No Zed code is
copied.

## Words

The word is **nav**, and the nav is the Inbox (ADR 0040). _Avoid_ sidebar.

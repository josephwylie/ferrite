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
  `destination`, shared with the Factory. An open PR's body is the PR
  reader's (the eighth, below), which the root has it draw and hands over
  with the inputs.
- **The Side chat is the sixth.** `SideChatView`
  (`cockpit/side_chat_view.rs`) owns every Thread's side chats in one
  view: each tab's field, title and fork once it lands, the tab each
  Panel last showed, the forks still on their way, the field's wait for
  the keyboard, and a Trash waiting for its second press. One view, not
  one per Thread: a landed fork names the Thread it was made from
  (`fork_of` on the window's focus ask), and the view holds the one tab
  each Thread's fork is on its way to, kept after the tab closes so the
  fork is retired when it lands. A Server from before the `fork_of`
  capability names none, and then the one Thread waiting takes the
  fork; with two waiting, neither does. The Panel draws it as
  the body of whichever side chat it shows, and the view reads which
  that is with its inputs, so the root no longer tends it every frame.
  It emits `SideChatEvent`: a question for a fork, Bring back, Trash.
  The fork is a Thread of the engine, so asking for it, binding it when
  it lands and retiring it with its tab stay the root's, as does which
  Surface the Panel shows.
- **The Terminal is the seventh.** `TerminalsView`
  (`cockpit/tools/terminal.rs`) owns every Thread's shells in one view:
  each tab's number and its shell or why it would not start, what the
  `⋯` menu set, the shown shell's wait for the keyboard, and whether a
  right-click found a selection. One view, not one per Thread: what the
  `⋯` menu sets is the window's, not a Thread's. Each shell is
  already an entity of its own, `ferrite-terminal`'s, which the view
  hears. The Panel draws it as the body of whichever shell it shows, and
  the view reads which that is with its inputs, so the root no longer
  tends the shell's keyboard every frame. It emits `TerminalsEvent`:
  Attach, a menu or the `⋯` Menu button, an address opened, Try again, a
  shell ended. A shell starts in the Thread's folder, on the window's
  machine, so the root hands the view both to start it with, and again
  for Try again. Which Surface the Panel shows, its tabs and the
  context menu stay the root's; the menu's rows it reads from the view.
- **The PR reader is the eighth.** `PullRequestView`
  (`cockpit/panel/pull_request.rs`) owns every PR it has read, a Thread's
  tab's or a Project's, with the request still out for each; what each
  action on a PR became, from its hand-off to the PR read back; the Merge
  waiting for its second press; when a view was last chosen; and the
  cache its prose renders through. One view for the window: the Panel's
  PR Surface is its render, reading `pr_inputs`, and the PRs' detail is
  drawn by it as the root hands the open PR over, since the two can stand
  in one frame while the Panel slides shut. It emits `PrEvent`: read a PR
  again, a view chosen, Code's diff, a hand-off, the `⋯`. The forge and
  the Thread's agent are the root's, so it asks them and hands the
  answers to the view, and every frame it disarms a Merge whose Surface
  is no longer shown, the Surface drawn or not. Which view of a PR is
  chosen stays where it is kept (the Surface's, the PRs'), and Code's
  body stays Changes', which the root draws into the inputs.
- **Changes is the ninth.** `ChangesView` (`cockpit/tools/changes.rs`)
  owns every diff the window has read, a Thread's Changes and each pull
  request's Code: its scope, the files read and the read still out, the
  folds and reviewed marks, the wrap, whitespace and test-file toggles,
  the filter field, the list's scroll, and the last stage git refused.
  One view for the window, with no render of its own: the root draws a
  diff wherever it shows (`changes_pane`), as the Panel's Changes Surface,
  a PR's Code in the Panel or the PRs' detail, since two of them can
  stand in one frame while the Panel slides shut. The branch names and
  the `⋯` Menu button come with `ChangesInputs`. It emits `ChangesEvent`:
  read a diff again, stage a file, open one in Files, a menu, the `⋯`.
  Git and the forge are the root's, so it asks them and hands each answer
  to the view, which numbers every read and stage and drops an answer
  from another checkout or machine; a stage's answer asks for the read
  after it. Which diff a Surface shows and making it as it opens stay the
  root's, as does the context menu; the menu's rows it reads from the
  view. A Thread's diffs, its pull requests' included, go with the
  Thread, as a closed tab keeps its state. The root reads the diff of
  the PRs' open PR as soon as Code shows, so a PR opened on Code never
  draws an empty view. A Project's pull request diffs are not freed yet.
- **Files is the tenth.** `FilesView` (`cockpit/tools/files.rs`) owns
  every Thread's Files: the tree with the folders it listed, git's colours
  and on a machine its last listing, the filter field, the open tabs with
  their editors, readings and find bars, the editor settings the `⋯` menu
  sets, the row a right-click is on, and the number of each read still
  out. It keeps a gone Thread's Files while a buffer holds an edit the
  disk does not have, so auto save still writes it, a quit flushes it and
  leaving names it. One view for the window, like the Terminal's. The
  Panel draws it as the Files Surface's body, and the file tabs are stops
  it registers on the Panel's row. It reads the Thread, whether the tree
  fits beside the editor, the `⋯` Menu button and the diffs read (for
  `Show changes`) through `files_inputs`. It emits `FilesEvent`: a file
  opened from the tree, `Show changes`, Attach, ⌘P, a menu, the `⋯`.
  A file opened from the tree goes through the root's `open_in_files`,
  the one way into a tab, since opening shows the Surface; a local image
  goes to the Pane's lightbox instead. The root makes a Thread's Files
  as its Surface opens, since that needs the checkout, the machine and
  the saved tabs. It saves them (`WorkingSets`, files.json) and tells the
  view each frame to settle a rendered file's find. A remote read or
  write still asks the root's Cockpit, after the frame, through the weak
  handle `RemoteFiles` holds. A Pane's file preview stays the root's and
  hands a checkout's text file to `open_in_files`.
- **Background work is the eleventh.** `BackgroundView`
  (`cockpit/background_work.rs`) owns every Thread's background work as
  the window keeps it: each stop sent, by Thread, Session generation and
  what it ends, where it stands and the count that tells one send's timer
  from another's, and the Background Surface's cards turned from how they
  start. One view for the window, with no render of its own: the root
  draws it wherever it shows, as the line under Main's transcript, the
  Overview's `Background tasks` section and the Background Surface's body,
  and the `Background work` dialog reads its `Stop` from it. Each reads
  the Thread's running and finished work, the refusals of its stops and
  whether the Session can stop a command through `background_inputs`. It
  emits `BackgroundEvent`: `Details`, a row of the section, an agent's
  `Open`, the line's `Stop`, a stop square. A stop is a Server call, so
  the root checks the Session still runs the work, tells the view it was
  sent, sends it and tells the view of a refusal; the view's own timer
  offers `Retry`. Each frame the root's pump has the view forget the
  stops of work that ended. A gone Thread's stops and turned cards go
  with it.
- **The agent Surface is the twelfth.** `AgentDrillsView`
  (`cockpit/agent_pane.rs`) owns every agent held open in a Panel, by its
  Thread and the agent's key: the Subject it opens on, why its saved
  history could not load, and the Pane's pieces its transcript is drawn
  with, whose tool rows it folds. One view for the window, like the side
  chat's. The Panel draws it as the body of whichever agent it shows, and
  the view reads which that is through the root as it draws; there the
  root tells that agent's transcript what it holds and builds the line it
  heads with (`agent_drill_history`), which the Pane's head says too and
  whose `Retry` is the root's. So the root no longer tends every agent
  held every frame, only the one shown. It emits `AgentDrillEvent`:
  `Close tab` on an agent the Thread no longer knows. Asking for the
  agent's saved history is a Server call, so holding an agent stays the
  root's, as it opens and each frame for a tab a Panel shows that no
  drill holds (one restored or reopened); a tab's close frees its agent,
  as does a gone Thread.
- **The Overview is the thirteenth.** `OverviewView`
  (`cockpit/panel/overview.rs`) owns what the Panel's Overview keeps for
  every Thread: the Changes section's unfolded files, the hunks each read
  with the diffstat it was read at, and the sections' scroll, which the
  Background Surface scrolls with too. One view for the window, with no
  render of its own: the root has it draw the Overview's body, reading the
  machine, the folder and branch, the Thread's facts and pull request, its
  context reading and MCP servers through `overview_inputs`, with the
  Background section drawn by the background work's view. It emits
  `OverviewEvent`: a file's hunks to read, `Create PR`, a file's name,
  `Review changes`, the pull request's row. Reading the diff is a Server
  call, so the root asks it and hands the answer to the view, which drops
  one read at an older diffstat; sending `Create PR`'s prompt and asking
  the Session for a fresh context reading stay the root's too. Its hunks
  are not the Changes' diff: they are always the uncommitted diff, read
  again when the agent edits the file, while the Changes read in the
  scope the operator picks, when the Surface opens or Refresh is pressed.
  A gone Thread's folds, hunks and scroll go with it.

## What does not get a view

A feature earns a view by state the root kept and listeners that wrote
it. Code whose state already has an owner off the root, and whose root
code is the root's own work, does not: it keeps a plain struct beside
what it hangs from, and the root builds what it draws in one method.
The Thread details card is the example. What it keeps is each Pane's
`DetailsCard` (`ferrite-pane`, `details.rs`): whether it is open and its
exit, which only its own methods write, its focus and scroll, its density
measurements, and where its canvas, card and opener were laid out. Whether
it stands beside the transcript is the Thread's saved Surfaces
(`details_inline_open`). Its rows the root builds in `details_content`
into plain values that draw themselves (`cockpit/details/content.rs`).
What stays on the root is the root's: moving the keyboard between the card
and the Composer, fitting the card to the Pane's width as the columns
move, and closing a card whose Pane lost the focus. A view would move
state that already has an owner and add events for calls the root makes
anyway. Its `Show in thread details` menu is a context menu in the popup
slot (ADR 0043).

## Precedent

Zed builds its workspace this way: each panel is an entity that owns its
state, emits events, and is subscribed to by the workspace. We take the
shape only. We did not adopt Zed's `Panel` and `Dock` traits, its action
registry or its settings store. Ferrite has one nav and one Panel (ADR
0039), and nothing yet needs a general host for more. No Zed code is
copied.

## Words

The word is **nav**, and the nav is the Inbox (ADR 0040). _Avoid_ sidebar.

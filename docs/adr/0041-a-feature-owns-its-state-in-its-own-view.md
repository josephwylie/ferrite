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
  time, each in its own change. The Snooze field (a Thread's Custom…
  time, drawn over the list) moved in later: `NavView` keeps the Thread it
  snoozes, its line and why the last ⏎ did not take, and reads the
  Thread's name as it draws. The root opens it from the Snooze menu
  (`edit_snooze`). Its ⏎ and esc stay the root's Submit and Interrupt,
  which reach the root wherever the keyboard is, and hand them to the
  view while the field is open. A time it reads it says as
  `NavEvent::Snooze`; snoozing is a Server ask, so the root asks and
  answers the view (`snooze_answered`), which closes the field or keeps
  the refusal in it. A press in the nav lets it go on the view's own; the
  nav folding, the keys going to the list and a notice opened let it go
  through the root (`close_snooze`). The focus pump, the paste and the
  Composer's keys ask the view for its line. The inline title rename
  gets no view (below).
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
  handle `RemoteFiles` holds. A Pane's file preview hands a checkout's
  text file to `open_in_files` (`redirect_previews_to_files`).
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
- **The toasts are the fourteenth.** `ToastsView` (`cockpit/toasts.rs`)
  owns the Inbox's alerts (`NeedsYou`): what it heard of each machine,
  what waits out its delay, the toasts standing and the system
  notifications posted, and the toast fading out. Its render is the toast
  float, reading the newest toast's row, whether the nav is folded and
  the widths it hangs beside through `toast_inputs`. It emits
  `ToastEvent`: a quick answer, or what a toast opens, each taking the
  toast down at once. Hearing the machines and ringing stay the root
  pump's (`ring_alerts`), which updates the view and then plays the chime
  and posts the system notifications, as does taking a toast down when
  the list opens what it was about. The window carries the view's alerts
  to the next machine's root (`with_needs_you`). The kit's own toasts
  stay the root's (`toast_margins`).
- **The command palette is the fifteenth.** `Palette` (`palette.rs`) was
  a view already, keeping its line, its rows, its type tab, its cursor
  and its scroll; now it answers its own keys too (↑ ↓ ⇥, ⏎, esc), and
  as the import picker it keeps the session listing it waits on, so a
  picker closed or reopened since drops the answer with itself. The root
  makes one each time it opens, with its rows from `palette_inputs`, and
  hears it from then on, as it hears the Project card. It emits
  `PaletteEvent`: run a row, or dismiss. Running a row stays the root's
  (`run_palette_act`, `run_command`), since a command can touch anything
  in the window, as do ⌘K and the palette's two keyed commands, its veil
  and its exit, which it shares with the shortcuts sheet, and where the
  sessions are looked for. The root's pump asks the picker for the
  listing's answer and fills it.
- **The File Surface is the sixteenth.** `FileSurfaceView`
  (`cockpit/panel/preview.rs`) owns what the Panel's File Surface keeps
  of every Pane's preview, by Pane: each File tab's document as last read,
  the open last seen and the fade it brings, the numbered lines' and
  rendered document's scrolls, the find bar, a Browser annotation's
  capture still being read, and the Pane whose preview took the last
  press, which ⌘F finds in. One view for the window, by Pane, not by
  Thread: a draft has a preview too, and its first send hands what it
  kept to the Thread's Pane (`adopt`). It has no render of its own: the
  root has it draw the card, as the File and Browser Surfaces' body and a
  draft's Panel alone, reading the Pane's preview, the Thread's checkout
  and the live browser's body through `file_surface_inputs`. It emits
  `FileSurfaceEvent`: escape closes the card, a press gives its Pane the
  keyboard, a page's title opens its menu. The document is the Pane's
  (`Preview`, `ferrite-pane`), which every entry point opens through. Each
  frame the root's `tend_previews` opens a remote file in Files, reads a
  capture through the Server and has the view tend each preview, then
  opens the Surface of each document that opened and has each preview
  follow the File tab its Panel shows, since which Surface shows is the
  root's. What a Pane's preview kept goes with its Pane (`sync_panes`),
  as it did when it hung from the Pane; a gone Thread's Pane goes there
  too. The lightbox gets no view (below).
- **The Browser's chrome is the seventeenth, and the Browser's first
  slice.** `BrowserChromeView` (`cockpit/browser/chrome.rs`) owns what each
  Thread's Browser keeps above its page: the address field, the page
  address it last took from the Server, which tells an edit from it, and
  whether a new tab is being entered, while no page's controls apply. One
  view for the window, by Thread, with no render of its own: the root has
  it draw the tab strip and the toolbar above the page (`browser_body`),
  reading the Thread's tabs, the active one, whether a live page stands for
  it and whether an annotation is open through `browser_chrome_inputs`. Go,
  escape in the field, New tab and Cancel new tab are its own. It emits
  `BrowserChromeEvent`: an action on the pages (a tab, Back, Forward,
  Reload, Go, a tab's close), the annotate button, and New tab, which stops
  a capture under way. An action is a Server call, so the root's
  `browser_action` sends it and tells the view it landed, and the root's
  pump has the field follow the active page's address. The page's dialog,
  its menu and an annotation read the new tab through the view. A gone
  Thread's chrome goes with its Browser (`forget_gone_threads`). The rest
  of the Browser stays on the root's `Browsers` and moves in slices of its
  own: what a page asks of the window (its right-click menu and its script
  dialog), annotations and their review, the provider's control and capture
  queue, and last the pages and the engine's lifetime. The Panel host, the
  popup slot, the sheet host and the focus pump stay the root's (ADRs
  0042–0044).
- **A page's script dialog is the eighteenth, and the Browser's second
  slice.** `BrowserDialogView` (`cockpit/browser/dialog.rs`) owns what each
  Thread's Browser keeps of the dialog its page raised: the request the
  card was set up for, the prompt's field, the card's focus, and whether
  it has yet had the keyboard. One view for the window, by Thread, with no
  render of its own: the root has it draw the card over the page
  (`browser_body`), and its pump hands it the dialog the shown page has up
  through `browser_dialog_inputs`, none while a new tab is being entered.
  OK, Cancel, enter, escape and a press on the card's backdrop are its
  own. It emits `BrowserDialogEvent::Answer`, which the root hands the
  shown page's native handle. The focus pump stays the root's
  (`focus_shown_dialogs`, `dialog_holds_focus`) and asks the view to take
  the keyboard and whether it holds it; whether the card was drawn and is
  on screen the pump alone writes, so those stay on the root's `Browser`.
  The popup slot stays the root's too: a dialog that arrives under an open
  menu closes it (ADR 0043). A gone Thread's dialog goes with its Browser
  (`forget_gone_threads`). The page's right-click menu gets no view
  (below). What remains on `Browsers` for slices of their own:
  annotations and their review, the provider's control and capture queue,
  and last the pages and the engine's lifetime.
- **A retained annotation's review is the nineteenth, and the Browser's
  third slice.** `BrowserReviewView` (`cockpit/browser/review.rs`) owns
  what each Thread's Browser keeps while a retained annotation is edited:
  its capture's image being read, with the keyboard's place and the time
  it started, then the editor that adds notes and drawings to it. One view
  for the window, by Thread, with no render of its own: the root draws the
  editor in the page's place (`annotation_browser_body`). It needs nothing
  of the root's to draw, so it has no inputs method. Its editor's Cancel
  is its own. It emits `BrowserReviewEvent::Add` with the annotation the
  notes and drawings make, or why it could not be made; the root hands it
  to the Thread's Composer and closes the review. Reading the capture is a
  Server call, so the root's pump takes each Pane's request to edit one,
  asks the Server and hands the view the answer to wait on (`load`), then
  has it tend each read (`tend`), which opens the editor or gives back why
  the read failed or ran past its time for the Browser's notice. One
  annotation is open at a time: the root refuses a live one while a review
  is open and a review while a live one is, and the annotate button closes
  either. A gone Thread's review goes in `forget_gone_threads`. The live
  annotation stays on the root's `Browser` (`annotation`): its Server
  lease and pause protection, the in-page editor's events, the capture
  through the engine's queue, the encode and the hand-off to the Composer,
  whose Send calls the root's `submit`; the pump writes nearly all of it,
  and what listeners write (begin, cancel, close, an action deferred until
  the page is restored) is Server and engine calls the root makes anyway.
  What remains on `Browsers` for slices of their own: the live
  annotation, the provider's control and capture queue, and last the pages
  and the engine's lifetime. Neither of the first two earned a view, and
  the pages stay the root's (below, What does not get a view).
- **The Decisions are the twentieth.** `DecisionsView`
  (`cockpit/decisions.rs`) owns the thread rules option 2 made where the
  provider offered no standing answer, and each Pane's `RequestForms`, by
  Pane: the cursor, the amend note, the split button's menu, a question's
  or form's answers. One view for the window, by Pane, like the File
  Surface's: the entry is made with its Pane, so a card always has one to
  draw from, goes with it (`sync_panes`), and a draft's first send hands
  it to the Thread's Pane (`adopt`). It has no render of its own: the root
  draws the cards into each Pane's transcript and the compact transcripts
  from a snapshot it takes in `decision_inputs` (`Cards`), and a card
  drawn inside a cached transcript presses through the view. It answers
  the Decision keys (↑ ↓ ⏎ esc ⇥, `1`–`4`): they stay actions on the
  root's element, since the view has no element in the focus chain, and
  each hands the view the request the Pane shows with what only the root
  knows (which options can act, where the keyboard goes as the note
  closes; `KeyTarget`). Whatever a card's press or a key asks of the
  Server it says as a `DecisionEvent`: an approval's option (with `Allow
  and switch to …`'s mode), an answer as it stands (Cancel, Skip,
  Complete), a question's answers with the note taken with them, a
  question's or a form's Send, a picked option, ⏎ or esc in the amend
  note. Answering is a Server call, so the root asks the view for the
  note, tells the Server and, for option 2, hands the view the rule to
  keep; each frame it answers the approvals a rule covers. The chords
  (⌘⏎, ⇧⌘⏎, ⌘.) and ↵ on an empty line stay the root's: they read none of
  the view's state but a ready question's answers, and answer the Server,
  ⌘⏎ from across the room too. The open Decision is core's, and its reply
  error the Pane's. A gone Thread's rules go with it.
- **The empty board is the twenty-first.** `EmptyBoardView`
  (`cockpit/empty_board.rs`) owns the home's selected row and its list's
  focus; its keys (↑ ↓ ⏎ under `EmptyBoard`) answer on its own element,
  as the PRs' do. The root holds it for the window's life, so the row
  stays where it was left at the next visit. It reads the CLIs' versions,
  the ways to start that would do something now and the Threads used last
  through `empty_board_inputs`. It emits `EmptyBoardEvent`: a way to
  start's action, which the root dispatches, or a recent Thread, which the
  root focuses when a Pane shows it and brings back when none does.
  Whether it shows (no Pane on the board), the probe of the CLIs' versions,
  which Settings reads too, and the focus pump that hands it the keyboard
  stay the root's. It keeps nothing by Thread or Pane, so a gone Thread or
  a dropped Pane leaves it nothing to free.
- **The Archive is the twenty-second, and the Factory's first slice.**
  `ArchiveView` (`cockpit/archive.rs`) owns a Factory's Archive search
  field and the earlier Worker being read, with whether this window
  reopened it to park it again. The Factory (`FactoryView`) holds it,
  made as the Factory opens on a Project, so it goes when the Factory
  leaves the board's place, as before. Its render is the Archive's list
  beside the Task panel, reading the archived Tasks that match its search
  and the open Task through `archive_inputs`. It emits `ArchiveEvent`: a
  row opens its Task, Board gives the board back. Whether the Archive
  shows stays the Factory's (`archive_shown`), as whether the PRs show is
  the root's `destination`: opening a Task, the outline and the board
  head's button decide it. Reopening and parking an earlier Worker are
  Server calls, so `read_worker` stays the root's and hands the view the
  Worker it opened; the Task panel that draws its transcript and the
  Workers a Task had, Clear completed and the store's trouble stay the
  root's drawing. A gone Thread's Worker being read goes with it
  (`forget_gone_threads`). The rest of the Factory moves leaves first, so
  the shell's own move is small: Direction (the per-Task answer boxes,
  whether the open one holds the keyboard, and the Rework and Direct this
  task sheets as last closed), then Starts, the Workflows editor, the
  Coordinator surface, and last the shell, `FactoryView` itself, with
  the Task panel and the board.
- **Direction is the twenty-third, the Factory's second slice.**
  `DirectionView` (`cockpit/direction.rs`) owns what a Factory's Tasks
  are being told: each Task's answer box, made once, whether the open
  one holds the keyboard, and each Task's Rework and Direct this task
  sheets as last closed (`KeptDirection`), which a sheet takes as it
  opens and gives back as it goes. The Factory holds it, as it holds the
  Archive's. Its render is the open Task's say in the Overview (its own
  direction, the question and its answer box, a permission it waits on,
  Ready to approve with its evidence, Direct this task), reading the Task
  as the board says it now through `direction_inputs`; the root adds it
  only when the Task has something to say (`says`), so an empty say adds
  no gap. It emits `DirectionEvent`: a press on the answer box, which the
  focus pump then hands the keyboard, Send answer, Approve, a sheet to
  open, and Answer in the Worker tab. Sending is a Server call, so the
  root sends and then clears the box or spends what the sheet kept; the
  sheets themselves are `DirectView`s the sheet host shows. It keeps
  nothing by Thread, so a gone Thread leaves it nothing to free. Left of
  the Factory: Starts, the Workflows editor, the Coordinator surface and
  the shell.
- **Starts is the twenty-fourth, the Factory's third slice.**
  `StartsView` (`cockpit/starts.rs`) owns what a Factory keeps of how a
  Workflow's Tasks start (#225): the repositories and each repository's
  labels the GitHub login listed for a GitHub start's pickers, with the
  lists still out and why one could not be read. The Factory holds it, as
  it holds the Archive's, so the lists are asked again only when the
  Factory opens anew. Its render is the inspector showing the start (the
  source picker, the chosen source's settings, the automatic starts'
  switch, a backlog's question and a failed check), reading the shown
  Workflow's start, its automatic starts and the start's two fields
  through `starts_inputs`. It emits `StartsEvent`: a picker's rows to
  open, a segment chosen, a press on a field, the switch, the backlog's
  answer. The start is the draft's and every change to it is a Server
  ask, so `edit_start` and the switch stay the root's, as does asking the
  login for a list: the root asks and hands the view the answer to wait
  on, and the root's pump has the view take each answer, as the palette's
  import listing does. Whether the start shows stays the Workflows
  editor's (`Inspected::Start`), as does its outline row, which opens it;
  the two fields are the Workflows editor's, which the view draws through the
  one `field_input` the editor's own fields use. It keeps nothing by
  Thread. Left of the Factory: the Workflows editor, the Coordinator
  surface and the shell.
- **The Workflows editor is the twenty-fifth, the Factory's fourth
  slice.** `WorkflowsView` (`cockpit/workflows/editor.rs`) owns
  what the editor has chosen and what is being typed in it: the Workflow
  and Step shown, what the inspector shows (`Inspected`: a Step, the
  approval settings or the start, one at a time, so only its outline row
  draws chosen; choosing a Workflow shows its first Step), whether the
  skills search is revealed, the outline's focus and scroll, each text
  field's Composer, made once for as long as the Factory is open (the
  start's two included), and the field that holds the keyboard. The
  Factory holds it, as it holds the Archive's. Its render is the
  Workflows surface, which the root only hosts: the head (the Workflow
  picker, the version, New workflow, Run task, Save), the draft's
  problems, the outline and the inspector, or the empty state while the
  Project has no Workflow, with what they read from `workflows_inputs`:
  the shown Workflow's name, its latest version and whether the draft
  differs from it, the draft, the Step shown, the automatic starts'
  switch, the draft's problems and the catalog they were found against,
  each Provider's models, the shown Step's Provider's installed skills,
  and the starts' view, which draws the start. The outline's keys read the same inputs
  through a weak handle to the root, so a second key before the next
  frame reads the draft the first one left. The focus pump hands the
  keyboard to the chosen field, else the outline. It emits
  `WorkflowsEvent`: a field typed in, whose text the root sends to the
  draft; a press on a field or the outline; a Step's row, ↑ ↓, the
  approval's, the start's or a problem's row, which the root shows
  (`select_step`, `select_approval`, `open_start`) and fills the fields of
  (`ensure_fields`, which reads the draft); ⌥↑ ⌥↓, a drop, Add agent, a
  Step's switch, a skill added or removed and Delete agent; a route's
  picker, a row's `⋯`, an inspector chooser, the head's Workflow picker
  and New workflow, whose rows the root opens; a segment's choice; Run
  task, which opens the New task sheet on the Workflow shown; and Save.
  Every edit and Save is a Server ask (`workflow_ask`,
  `save_workflow`), so editing stays the root's. Each Provider's
  installed skills get no view, by the rule below: only the root asks
  for them and only its pump takes the answers, so they are a plain
  struct on the Factory (`Skills`), in the shape of the import picker's
  listing (`listen`, `listed`). The open picker's rows stay the Factory's
  (`picks`), shared with the New task sheet, a Task's sheets and the
  Coordinator, and the popup slot and the focus pump stay the root's, as
  do the Factory head's surface tabs, which the shell draws. Left of the
  Factory: the Coordinator surface, then the shell.
- **The Coordinator surface is the twenty-sixth, the Factory's fifth
  slice.** `CoordinatorView` (`cockpit/coordinator.rs`) owns the one
  thing the window keeps of a Factory's Coordinator (#224): the opening
  field whose Send starts it, made once as the Factory opens. The Factory
  holds it, as it holds the Archive's. The rest is the Server's: the
  Coordinator's Thread, whether it is starting and why it could not, the
  Provider, model and effort chosen for it, and its proposals, all on
  the board the Server publishes. It has no render of its own:
  the Coordinator's conversation, once it runs, is its ordinary Pane,
  which only the root can draw (`pane_slot`, with the parts' cache), so
  the root draws the Pane and has the view draw the surface round it
  (`coordinator_surface`): the choosers for what it runs on, the empty
  conversation with the field and Send until it runs, and the proposals'
  cards, reading through `coordinator_inputs` what it runs on and that
  Provider's models, whether it is starting and why it could not, and
  each proposal as its card says it, with the Task an accepted one
  started while the board has it. It emits `CoordinatorEvent`: Send, a
  chooser's rows, which the root opens, a proposal's accept and `Not
  now`, and `Open task`, which gives the board back and opens the Task's
  panel. Starting it, choosing what it runs on, accepting and dismissing
  are Server asks (`coordinator_ask`), so they stay the root's, which
  clears the field once the Server took its words; a refusal is said in
  the Factory's `note`. The focus pump hands the keyboard to its Pane's
  Composer once it runs, else the field. It keeps nothing by Thread.
  Left of the Factory: the shell, `FactoryView` itself, with what it keeps
  for every surface: which surface shows (`surface`) and its tabs, the
  last command's `note`, the open picker's rows (`picks`), shared by the
  Workflows editor, the New task sheet, a Task's sheets and the
  Coordinator, the New task sheet as last closed, opening and closing the
  Factory, and the board with its columns and the Task panel.
- **The Factory's shell is the twenty-seventh, its last slice.**
  `FactoryView` (`cockpit/factory.rs`) was a struct the root held and
  drew; now it is a view the root holds in the board's place, made as the
  Factory opens on a Project and heard from its making (`new_factory`),
  so it goes when the Factory leaves the board's place, as before. Which
  Project's Factory shows stays the root's `destination`
  (`Shows::Factory(project, view)`), as whether the PRs show does. It
  owns what the Factory keeps for every surface: which surface shows
  (`surface`) and whether the Archive stands in the Board's place, the
  open Task and its panel's tab, the last command's `note`, the New task
  sheet as last closed (`kept`), the columns' scroll and the column it
  last brought into view, the skills found, the open picker's rows
  (`picks`) and the five leaf views. It has no render of its own: the
  Coordinator's Pane and a Worker's are drawn only by the root
  (`pane_slot`), so the root has it draw the board (`board`), reading
  through `factory_inputs` the Project's title, each Task on the board as
  its card says it (`TaskCard`), how many the Archive holds, the tabs'
  counts, the board's width and the store's trouble, and hands in what
  only it draws: the Coordinator's surface round its Pane, and for the
  open Task's panel the Task as the board says it now with, on the Worker
  tab, the Worker's Pane, an earlier Worker read or the Workers it had
  (`TaskPanel`, `task_panel_inputs`). The view draws the head (the title
  and count, the surface tabs, the note, Clear completed, the Archive's
  button, New task), the trouble, and the surface shown: the Workflows
  editor, the Archive's list, the Board's six columns of cards, each with
  the Task panel (its status, Stop, Resume, Delete, Close, the tabs, the
  Overview with the Task's say and pull request, Activity, Context).
  Dismissing the note, opening the Archive, a card pressed again and
  Close are its own. It emits `FactoryEvent`: a surface tab, which the
  root shows (`factory_surface`, the Board's closing the Archive first),
  Clear completed and Stop, Resume or Delete (`Command`), Server asks
  sent through `task_command`, New task, whose sheet the sheet host
  shows, a card (`OpenTask`), which `open_task` opens, and a panel's tab
  (`Tab`), as `task_tab`, which hands the Worker's Pane the keyboard.
  `picks` stays the Factory's: a picker's rows are `Pick`s, the
  Factory's own type, filled by every picker it has (the Workflows
  editor's, the New task sheet's, a Task's sheets', the Coordinator's),
  read by the popup slot through `MenuTarget::Factory`, and gone with the
  Factory, as they were; putting them on the slot would make every
  target carry a Factory's rows. What stays the root's is the root's:
  every Server ask and the `note` each answer leaves (`workflow_ask`,
  `coordinator_ask`, `task_command`, `read_worker`, starting a Task),
  opening the Factory and a Task, the sheet host, the popup slot, the
  focus pump (`factory_focus`), the Panes, a Task's toasts and
  notifications (`task_toasts`, which the window raises whether or not
  the Factory shows), and the Factory's rows in the nav and the palette.
  Nothing the Factory keeps is left on the root.

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

The image lightbox is the second. What it keeps is each Pane's preview's
`Lightbox` (`ferrite-pane`, `preview.rs`): the image viewer, the gallery
and the image shown in it, where it grew from and whether its zoom menu
is open, which only the preview's own methods write (`open`,
`step_image`, `set_zoom_menu`, `close_lightbox`); the viewer is an entity
of its own, which keeps the zoom and the pan. The root draws it over the
window in `lightbox_element` (`cockpit/lightbox.rs`) for the focused Pane,
and what stays on the root is the root's: handing the keyboard back to
the Pane as it closes, and ⌘W closing it before anything else.

A page's right-click menu is the third. What it keeps on the root's
`Browser` is the right-click the page reported (`context`), which only
the pump writes as it reads the engine (`serve_page_requests`), and where
the pointer was (`pointer`), a cell the page area's mouse-down sets
without reaching either view. The menu is a context menu in the popup
slot (ADR 0043), whose rows and verbs the root builds and runs
(`cockpit/browser/page_menu.rs`). No listener writes its state, so a view
would add an event for a popup the root opens anyway.

The provider's control of the Browser's pages is the fourth. `Controls`
(`cockpit/browser/controls.rs`) keeps the controls the Server asked for and
the one running, the live annotation's captures waiting and the one
running, and the engine's control worker the two share. The root holds it
beside `Browsers`. Only the pump writes it: the root hands it each Server
request (`push`), the live annotation asks it for a capture (`capture`),
and each frame the pump has it start and finish them (`pump`), handing in
the pages, the Server and the App it reaches the engine through. No root
code reads its fields, except the quit, which shuts the worker down. A
gone Thread's requests are answered as failed in their turn, as before,
since its pages are gone.

A Browser's live annotation is the fifth. `LiveAnnotation`
(`cockpit/browser/live_annotation.rs`) stays on each Thread's `Browser`
(`annotation`), so it goes with the Thread's Browser. It keeps the
Server's lease and pause protection from Attach through capture and
retention, the in-page editor's events, the capture, the encode and the
annotation the Server retains. Each frame the pump has it step (`tend`),
handing in the Browser as it stands, the Server, the capture queue,
whether the Thread's Composer could send and whether the Thread has one.
The step says what is left to the root: the annotation for the Composer,
which may then send it, or the action taken on the page while it was open.
What listeners ask (`begin`, `cancel`, `close`, `defer`) are its own
methods, which the root calls with the Server and the page it reads. A
view would give it events for calls the root makes on it anyway. Whether
the Composer could send, handing an annotation to it and its Send stay
the root's (`cockpit/browser/annotations.rs`).

The pages and the engine's lifetime are the sixth, and stay the root's
`Browsers` (`cockpit/browser.rs`): each Thread's tabs as the Server last
said (`state`), which owner it said they are for (`owner`), the live pages
realized in the engine (`pages`), the tabs the engine refused (`failed`),
the pages that pause together (`native_peers`), the notice (`error`), the
Browsers to reveal (`reveal`), the revisions sent for an echo (`sent`) and
what the pages said since the last frame (`events`). The pump writes
nearly all of it as it reads the Server and the engine. The rest comes
from `browser_action`, which a press reaches through the chrome's events
and which is a Server call that ends in revealing a Surface. A view would
pass that through events and still need the root for the Server, the
Surfaces and the Panes. A gone Thread's Browser goes in
`forget_gone_threads`.

The Review beside a Pane is the seventh. What the Review keeps is its
`ReviewView`'s (`review.rs`), an entity already: the scope, the files read
or why they could not be, a file opened whole, the pull request, the
commits listed and the bases, the search, the pick, the unchanged runs it
opened, the comment field, its scroll and its focus. It asks the root for
each read by its own events (`ScopePicked`, `CommitPicked`, `Searched`,
`FileOpened`, `PrAsked`, `FilesAsked`, `BasesAsked`, `BasePicked`,
`MenuAsked`), heard from the moment the root makes it (`open_review`).
What the root keeps is each Pane's `ReviewSlot` (`cockpit/beside.rs`), a
plain struct beside the Pane (`CockpitPane`): the open review and its
head's title, the one sliding shut, and the slot's focus, which only its
own methods write (`show`, `close`, `let_go`). It goes with its Pane
(`sync_panes`), a gone Thread's too. Each Thread's slide (`ReaderSlides`)
only the pump writes (`tend_reader_slides`, and `land_reader_slides` as a
seam is grabbed), and it is board layout: the reader leaf `board_tree`
splices beside the Pane, its share and its reveal. What else stays on the
root is the root's: reading each scope where the checkout is, opening the
Review from the command, the palette and the Overview's `Review changes`
(`open_beside`), handing the keyboard back to the Pane as it closes (its
×, escape, ⌘W), and the slot's context menu (ADR 0043). A view would move
a struct that already has an owner and add events (close, a press) for
calls the root makes anyway. Its diff is its own read, not the Changes'
or the Overview's.

The inline title rename is the eighth. It is not the nav's alone: the
same field stands in for a Thread's title in its nav row or in the view
head over its Pane (`RenameTarget`), one at a time, so `NavView` would
hold the Pane head's field. Nor does it earn a view of its own: every
press that opens it is the root's (a menu's Rename, the view head's
double-click, and F2 and a row's second click, which the nav says as
`NavEvent::Rename`), as is every one that closes it (⏎ and esc as the
root's Submit and Interrupt, a press anywhere else, a popup going up),
and saving is the store's (`rename_thread`). So it is a plain struct on
the root, `Rename` (`cockpit/rename.rs`), which only its own methods
write (`open`, `close`, `take`, which drops a blank line); the nav reads
its row's field through `NavInputs` and the view head reads its own. The
root keeps the commit and where its refusal is said, the nav's notice,
and the focus pump, which holds the keyboard on the field. A view would
add events for calls the root makes on it anyway. A Project is renamed
in the Project card, not here.

A draft's command listing is the ninth, and the Composer's first slice.
Before a draft has a Session, its `/` and `$` menus list what its
Provider finds in its folder. `DraftCommands`
(`cockpit/draft_commands.rs`) keeps that listing: the Provider and folder
it is for, the discovery still out, and the commands it found or why it
could not. Only the root writes it, in the shape of the import picker's
listing (`listen`, `listed`): deriving a menu asks afresh when the draft's
Provider or folder changed and takes an answer that came
(`draft_commands`), which then tells the draft's line which tokens are
commands; the pump takes an answer that lands between keystrokes and
derives the menu again; an edit with no `/` command at the caret, and a
draft whose folder is gone, drop it (`forget`). A Thread's commands are
its Session's, in core. A view would add events for asks the root makes
anyway, and the menu that reads it is still the root's.

## Precedent

Zed builds its workspace this way: each panel is an entity that owns its
state, emits events, and is subscribed to by the workspace. We take the
shape only. We did not adopt Zed's `Panel` and `Dock` traits, its action
registry or its settings store. Ferrite has one nav and one Panel (ADR
0039), and nothing yet needs a general host for more. No Zed code is
copied.

## Words

The word is **nav**, and the nav is the Inbox (ADR 0040). _Avoid_ sidebar.

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
  The nav and the toast float read their own instead, and hear the
  model (below).
- **The nav is the first slice.** `NavView` (`ferrite_nav::view` since
  the 2026-11-25 amendment; the root's side is `cockpit/nav_view.rs`) owns
  the nav's own state: its motion, scroll and viewport, keyboard focus and
  cursor, hover, search, picks and Park sweep, and Undo. It emits
  `NavEvent`. What the rest of the window also decides stays on the root:
  whether the nav is collapsed or forced open, its width measurements, the
  Project filter, its notice line. The other features follow one at a
  time, each in its own change. The Snooze field (a Thread's Custom…
  time, drawn over the list) moved in later: `NavView` keeps the Thread it
  snoozes, its line and why the last ⏎ did not take, and draws the
  Thread's name from its row. The root opens it from the Snooze menu
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
  out, its focus, and on a machine the browser of its folders
  (`FolderBrowser`, ADR 0048). It emits `ProjectEditorEvent`. What changes the Projects
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
- **PRs is the fifth.** `PrsView` (`ferrite_prs::prs` since the
  2026-11-26 amendment; the root's side is `cockpit/prs_view.rs`) owns the
  PRs destination's scope and filter, the lists it has read and the reads still
  out, the marked row and whether its detail shows, the PR view chosen, its
  scroll and its focus; its keys answer on its own element. It reads the
  Projects and an open PR through `prs_inputs`. The root holds it for the
  window's life, so the list is as it was left at the next visit. It emits
  `PrsEvent`: the root asks the forge for a list and hands the answer to the
  view to wait on, reads an opened PR, gives the board back, or opens the
  Project card. Whether the PRs stand in the board's place stays the root's
  `destination`, shared with the Factory. An open PR's body is the PR
  reader's (the eighth, below), which the view holds as its child and
  draws.
- **The Side chat is the sixth.** `SideChatView` (`ferrite_prs::side_chat`
  since the 2026-11-26 amendment; the root's side is
  `cockpit/side_chat_view.rs`) owns every Thread's side chats in one
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
  (`ferrite_prs::pull_request` since the 2026-11-26 amendment; the root's
  side is `cockpit/panel/pull_request.rs`) owns every PR it has read, a Thread's
  tab's or a Project's, with the request still out for each; what each
  action on a PR became, from its hand-off to the PR read back; the Merge
  waiting for its second press; when a view was last chosen; and the
  cache its prose renders through. One view for the window: the Panel's
  PR Surface is its render, reading `pr_inputs`, and the PRs' detail
  holds it as a child and draws it, since the two can stand in one frame
  while the Panel slides shut. It emits `PrEvent`: read a PR
  again, a view chosen, Code's diff, a hand-off, the `⋯`. The forge and
  the Thread's agent are the root's, so it asks them and hands the
  answers to the view, and every frame it disarms a Merge whose Surface
  is no longer shown, the Surface drawn or not. Which view of a PR is
  chosen stays where it is kept (the Surface's, the PRs'), and Code's
  body stays Changes', a view the root hands the reader and the PRs.
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
- **Files is the tenth.** `FilesView` (`ferrite_tools::files` since the
  2026-11-25 amendment; the root's side is `cockpit/tools/files.rs`) owns
  every Thread's Files: the tree with the folders it listed, git's colours
  and on a machine its last listing, the filter field, the open tabs with
  their editors, readings and find bars, the editor settings the `⋯` menu
  sets, the row a right-click is on, and the number of each read still
  out. It keeps a gone Thread's Files while a buffer holds an edit the
  disk does not have, so auto save still writes it, a quit flushes it and
  leaving names it. One view for the window, like the Terminal's. The
  Panel draws it as the Files Surface's body. The file tabs are stops of
  the Panel's row that the view declares (`tab_stops`) and the root
  enters as it draws the body, handing each tab its handle back. It reads
  the Thread, whether the tree fits beside the editor, the `⋯` Menu
  button, the tabs' handles and the diffs read (for `Show changes`)
  through a `Source` the root makes over `files_inputs`. It emits
  `FilesEvent`: a file
  opened from the tree, `Show changes`, Attach, ⌘P, a menu, the `⋯`.
  A file opened from the tree goes through the root's `open_in_files`,
  the one way into a tab, since opening shows the Surface; a local image
  goes to the Pane's lightbox instead. The root makes a Thread's Files
  as its Surface opens, since that needs the checkout, the machine and
  the saved tabs. It saves them (`WorkingSets`, files.json) and tells the
  view each frame to settle a rendered file's find. A remote read or
  write goes through the window's `Submit` that `RemoteFiles` holds. A
  Pane's file preview hands a checkout's
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
  emits `BackgroundEvent`: `Details`, a row of the section, an agent
  card's `Open`, the line's `Stop`, a stop square; the dialog's `Open
  subagent` is its own `BackgroundDialogEvent::OpenThread`. A stop is a Server call, so
  the root checks the Session still runs the work, tells the view it was
  sent, sends it and tells the view of a refusal; the view's own timer
  offers `Retry`. Each frame the root's pump has the view forget the
  stops of work that ended. A gone Thread's stops and turned cards go
  with it.
- **The agent Surface was the twelfth, and is gone** (ADR 0039's
  amendment of 2026-11-21). `AgentDrillsView` (`cockpit/agent_pane.rs`)
  held agents open in the Panel; a Subagent now has one view, its full
  chat in the Pane, so the view went. The compact transcript it shared
  with the side chat (`sync_compact_transcript`) is the side chat's.
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
- **The toasts are the fourteenth.** `ToastsView` (`ferrite_nav::toasts`
  since the 2026-11-25 amendment; the root's side is `cockpit/toasts.rs`)
  owns the Inbox's alerts (`NeedsYou`): what it heard of each machine,
  what waits out its delay, the toasts standing and the system
  notifications posted, and the toast fading out. Its render is the toast
  float, drawing the newest toast's row as it read it and where the
  window placed it (the amendment below). It emits
  `ToastEvent`: a quick answer, or what a toast opens, each taking the
  toast down at once. It hears the Notices and the machines and rings
  them itself, and hands the chime and the system notifications to the
  root (the amendment below, the toasts hear for themselves). Taking a
  toast down when the list opens what it was about stays the root's. The window carries the view's alerts
  to the next machine's root (`with_needs_you`). The kit's own toasts
  stay the root's (`place_toasts` keeps their margin).
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
  (`ferrite_pane::decision` since the 2026-11-26 amendment, beside the
  cards that press through it; the root's side is `cockpit/decisions.rs`)
  owns the thread rules option 2 made where the
  provider offered no standing answer, and each Pane's `RequestForms`, by
  Pane: the cursor, the amend note, the split button's menu, a question's
  or form's answers. One view for the window, by Pane, like the File
  Surface's: the entry is made with its Pane, so a card always has one to
  draw from, goes with it (`sync_panes`), and a draft's first send hands
  it to the Thread's Pane (`adopt`). It has no render of its own: the
  cards draw from a snapshot the root takes in `decision_inputs`
  (`Cards`, a value), which the cell draws on its plane and the
  transcript as its last rows; a card presses through the view, which it
  holds weakly, so a card drawn inside a cached transcript reads nothing
  of the root; no compact transcript draws one. Until that amendment the
  cards were the Cockpit's and the root handed the cell a draw of its own
  for its plane's. The verbs a card's note answers (`Submit`,
  `Interrupt`, `PickOption1`-`4`) are declared beside the Composer and
  the cards under the `cockpit` names the keymap binds. It answers
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
  (`ferrite_nav::empty_board` since the 2026-11-25 amendment; the root's
  side is `cockpit/empty_board.rs`) owns the home's selected row and its list's
  focus; its keys (↑ ↓ ⏎ under `EmptyBoard`) answer on its own element,
  as the PRs' do. The root holds it for the window's life, so the row
  stays where it was left at the next visit. It reads the CLIs' versions,
  the ways to start that would do something now and the Threads used last
  (each as its nav row names it) through `empty_board_inputs`. It emits `EmptyBoardEvent`: a way to
  start's action, which the root dispatches, or a recent Thread, which the
  root focuses when a Pane shows it and brings back when none does.
  Whether it shows (no Pane on the board), the probe of the CLIs' versions,
  which Settings reads too, and the focus pump that hands it the keyboard
  stay the root's. It keeps nothing by Thread or Pane, so a gone Thread or
  a dropped Pane leaves it nothing to free.
- **The Archive is the twenty-second, and the Factory's first slice.**
  `ArchiveView` (`ferrite_factory::archive` since the 2026-11-25
  amendment; the root's side is `cockpit/archive.rs`) owns a Factory's
  Archive search field and the earlier Worker being read, with whether
  this window reopened it to park it again. The Factory (`FactoryView`) holds it,
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
  `DirectionView` (`ferrite_factory::direction` since the 2026-11-25
  amendment; the root's side is `cockpit/direction.rs`) owns what a Factory's Tasks
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
  `StartsView` (`ferrite_factory::starts` since the 2026-11-25 amendment;
  the root's side is `cockpit/starts.rs`) owns what a Factory keeps of how a
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
  slice.** `WorkflowsView` (`ferrite_factory::workflows` since the
  2026-11-25 amendment; the root's side is `cockpit/workflows/editor.rs`) owns
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
  through the same read of the root, so a second key before the next
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
  slice.** `CoordinatorView` (`ferrite_factory::coordinator` since the
  2026-11-26 amendment; the root's side is `cockpit/coordinator.rs`) owns the one
  thing the window keeps of a Factory's Coordinator (#224): the opening
  field whose Send starts it, made once as the Factory opens. The Factory
  holds it, as it holds the Archive's. The rest is the Server's: the
  Coordinator's Thread, whether it is starting and why it could not, the
  Provider, model and effort chosen for it, and its proposals, all on
  the board the Server publishes. Until that amendment it had no render
  of its own:
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
  `FactoryView` (`ferrite_factory::board` since the 2026-11-26
  amendment; the root's side is `cockpit/factory.rs`) was a struct the root held and
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
  (`picks`) and the five leaf views. Until that amendment it had no
  render of its own: the
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
- **The Composer menu is the twenty-eighth, and the Composer's first
  view.** `ComposerMenuView` (`cockpit/composer_menu.rs`) owns the `/`,
  `@` and `$` menu: the open menu and the line it stands on, the mute an
  escape or a press elsewhere leaves until the text moves, the one edit a
  recalled prompt makes without opening it, and the file search a
  Thread's Session runs for `@`. What the caret opens it reads from the
  line itself (`Wanted`); what that menu lists the root reads for it
  through `composer_menu_inputs`, asking only for what it lists: the
  focused Pane and its Composer, the Thread's commands or the draft's
  listing (`DraftCommands`), or the Pane's root, the Session's file search
  and whether the window is a machine's. The root still derives it, on an
  edit and when a draft's listing or a search answer lands, since only
  the root knows which Pane is focused. A command, a skill or a file the
  view splices into the line; every other pick it says as
  `ComposerMenuEvent::Pick` (a picker to open, `/import`, a Provider, an
  effort, a band choice), which the root makes as it makes the band
  menu's. Its keys (`MenuNext`, `MenuPrevious`, `MenuPick`,
  `MenuDismiss`, and ⇥ on a `/` row) stay the root's actions, since the
  keyboard is in the Composer, and the root hands them to the view, as it
  does the Decision keys. What closes it from outside calls the view: a
  sheet, a popup or the palette opening, the Pane going or losing the
  focus, a drop on a Pane, and a press outside, which mutes it too. The
  root draws it as the focused Pane's drawer, and only that Pane reads
  it as it draws. The rows it is made of (`menu_rows.rs`) are shared with
  the band menu and the model picker, which are popups in the root's slot
  (ADR 0043).
- **The model picker is the twenty-ninth, and the Composer's second
  view.** `ModelPickerView` (`cockpit/model_picker.rs`) owns what the
  provider, model and effort picker keeps while it is up: every Provider's
  model rows, each carrying its pick, and which of them the tab or the
  search lists, the Reasoning column's rungs, the tab and the rail pill's
  slide, the column the arrows act in and each column's cursor, the list's
  scroll, the search line, and the favourites it ranks by. The popup slot
  holds it (`Popup::ModelPicker`), made as the picker opens, so it goes
  as the popup does, as the floats hold the palette. What it lists the
  root reads through `model_picker_inputs`, as it opens and again after a
  pick, an announcement, or a turn starting or ending
  (`rebuild_model_picker`): the rows for the Thread or the draft's
  binding from each Provider's catalog, the rungs, the model's name, the
  Provider a pick hands over to, why every row is held (`tuning_hold`),
  the Providers whose CLI is missing and the favourites. Its keys (↑ ↓ ← →
  ⏎ and ⌘1–⌘9) answer on its own element, where its search line holds the
  keyboard, as the palette's do; esc goes on to the slot. A tab, the
  arrows and the search are its own. It emits `ModelPickerEvent`: a model
  row or a rung picked (`Pick`), which the root makes, re-aiming the
  Thread or making the draft's band choice, unless a turn began since the
  picker drew, and a row's star (`Favourite`), which the root saves in
  the window's settings and hands back. A pick from a picker fading out
  is dropped. The slot keeps what is the slot's (ADR 0043): the Pane the
  picker hangs over, opening it, closing it on esc or a press outside,
  where the keyboard goes back, and the permission mode menu nested in
  it, which only the root opens and closes. The footer stays the root's
  drawing, since the permission mode is the Session's: the root hangs the
  view and the footer in one surface over the Composer (`model_picker_element`).
  It keeps nothing by Thread, so a gone Thread leaves it nothing to free.

## What does not get a view

A feature earns a view by state the root kept and listeners that wrote
it. Code whose state already has an owner off the root, and whose root
code is the root's own work, does not: it keeps a plain struct beside
what it hangs from, and the root builds what it draws in one method.
The Thread details card is the example. What it keeps is each Pane's
`DetailsCard` (`ferrite-pane`, `details.rs`): whether it is open and its
exit, and how many `Show more` presses its `Previous agents` has had
(`previous_pages`, back to none as it opens), which only its own methods
write, its focus and scroll, its density measurements, and where its
canvas, card and opener were laid out. Whether it stands beside the
transcript, and whether its `Previous agents` is unfolded, are the
Thread's saved Surfaces (`details_inline_open`, `previous_agents_open`). Its rows the root builds in `details_content`
into plain values that draw themselves (`ferrite_pane::details::content`); a
watched PR's eye is one more value on its `PrBlock`, the watch's URL from
the Thread's digest (`Facts::watched_pr`), and its press calls the root's
`watch_pr` (`cockpit/pr_watching.rs`), as the palette and the PR's context
menu do. Main's Lineage rows for Delegates and for the Thread that
delegated it are plain values from the digest too (`ThreadFacts::delegating`
and `delegated_from`, ADR 0047); a row's press calls the root's
`open_elsewhere` and its stop `cancel_delegate` (`cockpit/delegates.rs`).
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
derives the menu again; an edit with neither a `/` command nor a `$`
skill at the caret, and a draft whose folder is gone, drop it (`forget`). A Thread's commands are
its Session's, in core. A view would add events for asks the root makes
anyway; the menu that reads it (`ComposerMenuView`) is handed what it
lists.

A draft's checkouts are the tenth, and the Composer's second slice.
`Checkouts` (`cockpit/checkouts.rs`) keeps what a draft's workspace chip
and its menu read of the Project's repo: the branch each Project's
checkout is on, the Projects being asked about, what the repo lists (its
other worktrees and its local branches) and, on a machine, the listing
its Server is reading. Only the root asks, off the UI thread
(`look_up_checkouts`, `refresh_repo_listing`) or of the machine's Server
(`ask_machine_repo`), and only the answers and the pump write it
(`listed`, `set_listing`, `machine_listed`); the chip and the menu's rows
read it as they draw. A view would add events for asks the root makes
anyway. The Project and Workspace menus, their rows and their picks stay
the root's, in the popup slot (ADR 0043). A Thread's checkout labels are
the Facts' (`refresh_branches`).

The PR reads are the eleventh. `PrReads` (`cockpit/pr_reads.rs`) keeps
what the window remembers to ask `gh` for a Thread's PR on its own: the
Thread the pump last saw focused, and each Thread's
`Transcript::published` as last seen, so a commit or a push reads the PR
again at once (HD27). Only the pump writes it (`pace_pr_reads`), and a gone
Thread's count goes in `forget_gone_threads`. The ask (`check_pr`), which
the palette, the context menu and opening a Thread make too, and its
answer (`pr_landed`) stay the root's, and the reading is the Facts'
(`ask_pr`, `pr_answered`). A view would add events for asks the root makes
anyway. It stays beside the Server's watch (ADR 0046) because the two read
different PRs: the card and the nav show a Thread's branch PR whether or
not its agent watches it, and the Server reads only watched PRs, on its
own two-minute clock. Serving every Thread's PR from the Server would be a
`gh` read per open Thread on a clock, where `PrReads` reads on focus and
on a push. A watched Thread's card follows the watcher without a second
clock: when the Thread's digest says its watch started, ended or stopped
holding (`Facts::streamed`), the pump reads the PR again (`check_pr`).

The titler's names on their way are the twelfth. `Titling`
(`cockpit/titling.rs`) keeps the Threads the titler is naming from their
first prompt, whose nav cards draw the title faded until the answer
lands. Only the root asks, off the UI thread (`start_titling`), and only
the answer writes it (`adopt_title`), which an operator's rename in the
meantime outranks; the nav's row reads it as the root builds it
(`naming`). Renaming is the store's (`rename_thread`). It is not the
Facts': they are what the Server and git say of a Thread, and this is
an ask of the window's own.

A refused Session control is the thirteenth. `SessionControlError`
(`cockpit/session_control.rs`) keeps why the last control the operator
asked of a running Session (a permission mode, a model, an effort, a
stop) was refused, with the Thread and the Session's generation it was
asked of, so a restarted Session says nothing of it. Only the send writes
it (`run_session_control`), which the Session controls card, the
palette, the Decisions, the Panel and the background work's stop all
make; the card, the Overview and the stop read it for their own Session
(`of`). The Session controls card stays the root's drawing, and the
Session is core's. A view would add events for a call the root makes
anyway.

The plan card and the CI checks card are the fourteenth. Each is a popup
that drops from a Thread's Pane head (`Popup::Plan`, `Popup::Checks`), and
the slot holds all the state either has: which Thread it is for. What they show is the agent's
plan in the Thread's transcript and the checks of its pull request (the
Facts'). The root reads them and hangs the card under the head
(`plan_card_element`, `context_checks_element`); the card is drawn from
what was read, the plan as plain values (`PlanCard::of`, `render`) and the
checks by a free function whose rows open a run's log through the root
(`checks_card`), both in `cockpit/head_cards.rs`. A view would hold no
state and add events for a press the root answers anyway. The Composer's
draw code follows in the same shape, below.

A draft's band menu's rows are the fifteenth. The Project and Workspace
chips open a popup in the slot (`Popup::Band`), which keeps the open menu,
its rows and its cursor (ADR 0043). What the rows list is the registry's
Projects, the draft's binding, its line (a `path …` to register) and what
`Checkouts` holds of the repo. The root reads them (`project_menu_rows`,
`workspace_menu_rows`, the second asking git about a checkout not named
yet), and the rows are built from what was read as plain values
(`menu_rows::project_rows`, `workspace_rows`). A pick stays the root's
(`pick_band`), as does building them again as the line changes or a
listing lands (`sync_band_rows`, `sync_workspace_rows`).

The Session controls card is the sixteenth. It is a popup that hangs over
a Thread's Composer (`Popup::Session`), and the slot holds all it keeps:
the Session it is for, a Thread and its generation. What it shows is the
Session's: its permission modes, its MCP servers with the sign-in pages
the provider handed over, its background tasks, which controls it takes,
and why it refused the last one (`SessionControlError`). The root reads
them and hangs the card (`session_controls_element`); the card is drawn
from what was read as plain values (`SessionCard::of`, `render`,
`cockpit/session_card.rs`), none once another Session runs. Each row's
press sends its control through the root (`request_mode`,
`run_session_control`), after checking that the Session still runs and
still reports the server or task.

The Composer's footer is the seventeenth. Its controls (the queue strip's
acts, send and stop, the `+` and its menu, a Thread's permission mode chip
or a draft's mode word) and the permission menu the chip opens keep
nothing of their own on the root: the line is the Pane's Composer, and the
`+` menu's focus and the permission menu with the mode under the pointer
are the popup slot's (`Popup::Attach`, `Popup::Mode`). The root reads the
Pane's Thread once as it builds the toolbar (`composer_toolbar`): whether
its turn runs and its line is empty, its queue, its MCP servers and
plugins, its permission modes and mode. The controls are drawn from what
was read (`ferrite_pane::cell::footer`: `queue_acts`, `send_button`,
`stop_button`, `AttachMenu::of`, `ModeChip::of`, `draft_mode_word`; and
`ModeSegments` in `mode_menu.rs`). Every press acts through the root, as
before: the submit, the interrupt, a queued message's act, opening the `+`
menu and the file picker, a connector's control, a plugin's command on the
line, the permission menu's toggle and a mode's `request_mode`. What stays
the root's is the root's: putting the toolbar together, the model chip and
a draft's band chips, which open the model picker and the band menus, the
plan usage ring, and hanging the cards and menus over the Composer.

The nav's arrivals were the eighteenth, and are the nav's now (below).
`Arrivals` (`nav.rs`) keeps when each open Thread came into the section
it is listed in, which the list and the Working shelf order by, the
latest first: a Thread whose work, its background work included, ends
comes back at the top of the list. `NavView` holds it, and only its
`look` writes it, so its render only reads it.

## Amendment (2026-11-21): a feature subscribes to the model

The nav was handed its inputs by the root (`nav_inputs`) and read the
root as it drew, so every root notify redrew it, and the pump worked out
its list on every beat (125 a second) to note arrivals. It now reads the
model itself.

- **The model is an entity, and nobody notifies it.** The root holds
  core's `Cockpit`, the window's mirror of the Server, as
  `Entity<Cockpit>` and hands the handle to a feature as it makes it. The
  root and its pump write it with `update`. In the vendored gpui
  `update` only leases the entity and records it as read; only
  `as_mut`'s guard and `cx.notify` notify, and no code here uses either
  on the model. A notify would rebuild every cached part that read the
  model as it drew (`tracking_reads`): every shown Pane does. So the
  model says what moved as an event instead. The pump has it emit
  `Arrived` once, on a beat that brought something (a Session's events,
  a restart, a sweep that moved a fact or a clock, the Server's state, an
  ask, parked Threads, the Panes, a start, the models or the commands) or
  on which the Server said anything at all (`take_arrived`: another
  window's Snooze moves no Pane). An idle window never emits it.
- **A feature subscribes to the model's events and notifies itself; it
  reads what it draws as it hears them, never as it draws.** `NavView`
  subscribes to `Arrived` and observes the root. On either it looks
  (`look`): it notes `Arrivals`, builds its
  `NavInputs` from the model and what the window decides, and redraws
  only when they differ from what it drew. Its render reads nothing but
  what it kept, so a draw never writes, a root notify that moves nothing
  it draws leaves it, and an idle nav neither redraws nor works out its
  list. What the root learns without notifying itself it still tells the
  nav: a PR or a title that landed, branch labels git moved and a width
  changed by a drag (`notify_part(Part::Nav)`, which has the nav look
  once the root's update ends), a width measured as the root draws
  (`set_width`, since the nav draws in that same frame), and a working
  row's clock on the sweep (a plain redraw, since the clock is the
  time's, not the model's). A hover's fade is the nav's own motion: it
  asks for its frames, and a root notify no longer redraws it.
- **What the window decides stays the root's, and the nav reads it in
  its look.** The Project filter, the notice line, the open context menu
  and the menu buttons (the popup slot's), the rename field, the column
  width, the machines, the free disk, the update door, the open shelves
  and pins (the settings), the Facts (names, Projects, branches, PRs,
  last use), the titler's names and which Threads have Panes here. Two
  of them have more readers than the nav: the Facts (the palette, the
  Panes, the empty board) and the pins. They are the next to give a home
  of their own, not a `WindowState` grab bag: Facts beside the model,
  as what the window has read of the Server and git.
- **When a feature subscribes to the model, and when it takes inputs.**
  A feature subscribes to the model when what it draws is mostly the Server's
  (rows, states, counts) and moves on beats the root does not redraw
  for. It takes inputs when what it draws is mostly what the window
  decides, or when the root draws it inside its own render (a Pane's
  body, the Panel's Surfaces): there the root's notify is its redraw
  already.
- **The toast float is the second subscriber.** `ToastsView` holds the
  model's handle, subscribes to `Arrived` and observes the root and the
  window's machines. On any of them it looks: it reads the newest
  standing toast's row (`alert_row`: the model's Notices and requests, the
  Facts' names, a machine's status) and redraws only when the row moved.
  It reads one thing more of the window: where it hangs. The root hands
  it that as it lays out its columns (`place_toasts`): whether the nav is
  folded, the nav's width on screen and the board's right edge (the Panel
  and the Task panel). The root sets them while it draws, so the float
  draws them in that same frame, as the nav takes its width. A ring that
  changes the toasts standing, a toast taken down and a scene's toasts
  cleared have it look once the root's update ends (`look_again`). The
  root mounts it as a cached part, as it mounts the nav (`toast_float`),
  so a root redraw draws it again only when it notified itself. The
  hearing and the ring were on the pump's beat (`ring_alerts`) until the
  amendment below took them off it.
- **The Factory door already read the model.** Its dot (one of the
  door's Projects has a Task that needs the operator) has been part of
  `NavInputs` since the nav moved. The PRs door draws no count. What
  still fed it from the root was the list of Projects the door offers
  (`factory_nav_projects`), worked out once for the nav's look and again
  for the door's menu. The nav's look now works it out (`factory_projects`)
  and keeps it beside its inputs, uncompared, since the door draws only
  the dot. The menu (`factory_rows`) reads it from the nav. The menu's
  rows and counts are the popup slot's, read once as it opens (ADR 0043),
  as before.
- **The next three** were the Panes' heads, the toasts' hearing and the
  Facts. The amendment below (the toasts hear for themselves) did the
  second, found the first gone, and names the next three.
- **A feature does not ask the Server itself yet** (the Browser's asks no
  longer block, and an answer is an arrival: see the amendment below,
  which says what that leaves). Core's asks block:
  a remote one waits on the Server's socket for its reply, up to
  `REPLY_WAIT`, queueing every other message it reads (`Remote::execute`).
  What follows an ask is the root's too: the nav's Snooze is answered by
  the root (`snooze_answered`) after it clears the notice; a revive
  widens the Project filter and remakes the Panes. A feature that asked
  would block its own listener and still need the root after. Once asks
  answer through the model, a feature can ask and hear the answer as a
  change it hears.
- **The model is lent, not read, where a borrow of it must outlive a
  context.** Six root functions hand what they borrow of the model to
  another entity's update inside `self.cockpit.update(cx, |model, cx| …)`
  (`sync_transcript`, `sync_compact_transcript`, the Composers' commands
  in `render_cockpit`, `overview_sections`, `advance_pr_actions`,
  `pump_browsers`). While it is lent, reading the model through its
  handle panics (gpui's double lease, in every build), so code inside
  takes the lent `model`, never the handle. Each runs in the Cockpit's
  tests: four on every beat or frame, the side chat's and the
  Overview's in their own.
- **The pump keeps its beat** (until the amendment below, which has it
  wake on arrival instead). It could not wake only on arrival yet. The
  in-process Engine (`--demo`, `--load`, tests) is pumped by it: it reads
  each Session's events by polling (`try_recv`, at most 256 a beat, so a
  full beat leaves the rest for the next), retries a failed log write and
  steps its history and its startups. The doorbell that a Server message
  and a start ring (`doorbell::channel`) does nothing outside tests, and
  most of what the pump polls has no bell: 23 `try_recv`s in the Cockpit
  crate and 40 in core. And some of its work is the clock's: the sweep
  every two seconds, free disk once a minute, message times as the
  minute turns. What it
  stops is the work: an idle beat emits nothing, so nothing that
  subscribes to the model runs.

## Amendment (2026-11-21): the pump wakes on arrival

An idle window still ran its pump 125 times a second, and each beat
polled every queue the window reads. Now the pump runs when something
arrives or when a clock it owns is due.

- **One wake for the window: the doorbell.** The pump sleeps until the
  bell rings past what it had seen as its last run began
  (`doorbell::rung_past`), or until its next due time, whichever comes
  first. A ring during a run is not lost: the count was read before it.
  Wakes still come at most one a beat (`PUMP_MS`), so a stream of a
  thousand messages a second costs the window what it cost before, and
  an arrival at rest is read at once instead of on the next beat. gpui's
  test scheduler, which the tests and `--headless` run on, refuses a
  wake from another thread (it fails the test as nondeterministic), so
  there the pump hears its own thread's rings (`Hears::ThisThread`) and
  looks at the bell's count each beat for the rest: a beat that finds
  nothing rung and nothing due runs no pump. On a platform's own
  dispatcher any thread's ring wakes it, and it sleeps between.
- **Who rings.** Every producer the pump reads. A `doorbell::channel` or
  `doorbell::sync_channel` sender rings as it sends and as it hangs up:
  the Server's socket reader, the attach, a start, a git read
  (`read_here`), a Remote's answers (routed by the pump itself, so the
  consumer the pump has already passed reads it on the next beat), the
  in-process Engine's queues (a provider's events, the history loader,
  the parked rows, a handover's read, the memory sampler, follow-up
  suggestions, the model, session and command discoveries), the lexer, a
  machine's status and its setup. A provider's reader also rings once
  for each line it reads (`providers::read_line`), for the acks it hands
  to the Engine on channels the Session's trait names (a rewind, a fork,
  a tuning, a file search). The window rings for itself on its own
  notify (`observe_self`), and core's `Cockpit` on each act, each call
  into its in-process Engine, and each change to what the window tells
  the Engine (a Composer holding unsent text or not, the window's
  focus). A fixture world's Server message rings as a Server's does
  (`receive_fixture`). A Pane's preview rings as it queues what the pump
  opens: a link's page in the Browser, an annotation to edit, a remote
  attachment's preview. What a window action leaves for the pump is read
  on the next beat, as before.
- **When the pump is next due.** A beat that brought something (what
  `Arrived` is emitted for) is followed by another one beat later, so a
  full beat's rest (256 events), a failed log write's retry (it moves
  the Thread every beat) and a step that answers the next one finish as
  they did. A window with a live Browser tab keeps the beat: its pages'
  events, dialogs and controls, and the browser engine's start, come
  from the browser's own callbacks and worker, which do not ring.
  Otherwise the pump is next due at its sweep (every two seconds; free
  disk and message times ride it), and at the latest after
  `PUMP_FALLBACK` (250 ms). An alert's delay and a toast's end are the
  toasts' own clock, not the pump's (the amendment below).
- **What rides the fallback, and why no one waits on it.** Clock work
  with no due time of its own: a landing's deadline, the Remote's
  reattach backoff and restart timeout (seconds), the Composer's
  five-second wait before it lists files itself, a finished Panel save,
  the seconds an SSH verification prompt counts down (each up to 250 ms
  late). The in-process Engine's own clock work (`--demo`, `--load`,
  tests; a Server runs its Engine on its own loop): a snooze's end, and
  the three-second grace after a turn's last child settles before its
  finish is told. And the seen marks the pump reads without a notify.
  Each is a timeout, a retry, or a fact another program reads, none an
  answer to an operator's action; the countdown is a clock the operator
  reads, and a quarter second on it goes unseen. A test that moves the
  wall clock moves the window's clock by `PUMP_FALLBACK` too, for the
  pump that reads it.
- **An ask finds the Panes the roster has.** An act can receive what
  the Server said before the pump runs, a Focus with it. What that moved
  waits for the pump to report, as if the pump had received it: a PR
  watch's digest is the only word the window's Facts get of it. The render
  that presents the Focus has the Panes follow the roster first
  (`follow_roster`, as the pump does), so a Thread the CLI just opened
  is focused, not reopened. Seen once in a real run on the new pump:
  "Couldn't reopen … the Thread is open".
- **Tests drive the same pump.** A fake Session's stream is a
  `doorbell::channel`, so a test that sends and moves the clock one beat
  sees the pump run, as a real stream does. `tests/wakes.rs` counts the
  beats of an idle window and the wait from an arrival to its draw. On
  Linux, when this was written: an idle window's pump ran 18 times in
  four seconds (500 before), an arrival at rest waited 0.4 ms on average
  for its draw (4.5 ms before) and one in a stream 5.1 ms (as before),
  never more than a beat.

## Amendment (2026-11-21): the toasts hear for themselves

The pump read every Notice and every machine's news on each of its runs
(`ring_alerts`), and woke for the toasts' clock, so an idle window read
the Notices 18 times in four seconds. Now the toasts do their own
hearing, and the pump knows nothing of them.

- **`ToastsView` tends itself.** On the model's `Arrived`, a notify of
  the root or of the window's machines, it tends (`tend`): it hears what
  each held machine's Notices say (`news`, `NeedsYou::hear`; ADR 0045),
  rings what waited out its delay and takes down what went
  (`NeedsYou::ring`), keeps the system notifications posted, and looks.
  A pump run that brought nothing emits no `Arrived` and notifies
  nothing, so it reads no Notice. The view holds one timer for the next
  of its own times (`set_clock`, at `NeedsYou::next_due`): an alert's
  one-second delay, a toast's five- or eight-second end, the wait for a
  machine not yet heard from. It is set again only when that time moves.
- **The window plays and posts.** What a tend rang it says as
  `ToastEvent::Rang`: the tags of system notifications to take back, the
  chime (needs-you over done) and the postings. The root withdraws,
  chimes and posts, since the sound file and the notification centre are
  the window's. What it reads of the window to ring it reads through the
  root's handle in its own listener: whether the Server answers
  (`attached`), the Pane on the board (`visible_indices`), the focused
  Thread, the settings (`sounds_for`, `system_notifications`), and the
  postings' words and provider (`system_posting`, `alert_provider`,
  which read the Facts). The window active is the model's.
- **What the toast float still reads from the root.** The newest toast's
  row (`alert_row`: the Facts' names, the model's Notices and requests, a
  machine's status), where it hangs (`place_toasts`: the nav folded, its
  width, the board's right edge), and for the ring the window state
  above. It observes the root for the Facts and the Pane on the board.
- **The Panes' heads are gone.** The Pane head that drew its Thread's
  state word (`pane::group_head`) went with the Group view when the
  window came to show one Thread at a time (#318): a Pane has no head,
  and the state word is the nav row's tail, which the nav already reads
  from the model. The one head left is the view head over the board
  (`view_head`), drawn in the root's frame: the shown Thread's Project,
  a side chat's parent, its title or the rename field, its machine, its
  branch while the Panel is hidden, and its menu's chevron. No Thread
  state, and nearly all of it is the Facts and what the window decides
  (the rename, the popup slot's doors, the Panel, the nav's width), so by
  the rule above it takes inputs, and it moves with the Facts. Measured
  on Linux (`tests/wakes.rs`): a bare root notify builds the root, the
  view head and the shown Pane once each (three notifies: 3, 3, 3; the
  Pane off the board 0), and the stream test builds the head 71 times,
  once per root render. The shown Pane's rebuild comes from its cached
  part reading the root (`PartView`, `render_part`), not from a head, so
  a head of its own would not lower it.
- **Measured, before and after, on Linux.** Idle four seconds: 18 pump
  runs both, Notice readings 18 then 0. A done toast off the board, its
  second and its five seconds with the window otherwise idle: it comes at
  1000 ms and goes at 6000 ms both; Notice readings 29 then 3, pump runs
  29 both. The stream: 34 readings both, one on each run that brought
  something.
- **The next three, in order.** The Facts beside the model (names,
  Projects, branches, PRs): the nav, the toast's row, the palette, the
  view head and the Panes read them from the root, so each subscriber
  still observes the root, and a head can follow once they have a home.
  Then the pump's own root notifies: a run that is not precise (the
  Server's state, an ask, parked Threads, the models) notifies the whole
  root, which rebuilds the shown Pane and the head, and naming what each
  moved would let it notify only that part. Then the toasts' observe of
  the root: once the Facts and the Pane on the board say when they move,
  the float tends on those instead of on every root notify.

## Amendment (2026-11-21): an ask never waits on the window's thread

A remote ask sent its command and waited on the Server's socket for the
reply, up to `REPLY_WAIT` (10 s), on the window's thread
(`Remote::execute`). Nothing pumped or drew meanwhile. Measured at main
against the fixture Server (`tests/browser_asks.rs`): a press on the
Browser's Reload with a Server that never answers held the window for
10.03 s. Over a healthy link each Browser ask waited one round trip:
on the fixture Server, a page report 7.8 ms at the median and 18 ms at
worst, an action 17.5 ms, an input 1.1 ms. Every input on a page waited
like this before the page had it. The Browser is the first family that
never waits. The rest still do.

- **An ask returns at once with an `Ask<T>`** (core,
  `cockpit/ask.rs`), the asker's to hold. `answer()` is `None` while it
  is on its way and the answer once it came, taken once. The asker looks
  when it hears the model's `Arrived`, or on its pump step. The Browser
  looks in `answer_browser_asks`.
- **Its answer comes the way every Server message does.** The command
  goes out with an id. The Remote keeps the ask's answer channel under
  that id (`Remote::answers`, `ask_in_time`), as the asks that already
  answered later do (`changes`, `list_dir`, the reviews,
  `browser_dispatch`, `browser_capture`). The socket's reader rings the
  bell. The pump's `poll` reads the reply and routes it by its id to the
  ask's channel. That poll returns `Arrival::Answered`, so the model
  emits `Arrived` on that beat. It is one mechanism, with no second
  queue and no thread of its own.
- **A timeout and a dropped link answer it too.** A remote ask has until
  `REPLY_WAIT` from when it went, by core's clock (`clock::instant`, so a
  test moves it). The asks that already answered later keep their own
  deadlines: a capture's 45 s and its image's 30 s read. A dispatch
  grant waits as it did, until its page changes or its link goes. Each
  poll answers the late ones: "the Ferrite server did not answer in
  time". The pump is due at least every `PUMP_FALLBACK`
  (250 ms), and a window with a live Browser keeps its beat, so a timeout
  lands at most that late. A detach drops every channel. Each `Ask` then
  reads `NotConnected`: "the Ferrite server went away; this may not have
  arrived".
- **The in-process Link has the same shape.** An Engine in this process,
  a fixture world or a send that could not go answers through an `Ask`
  that is answered already (`Ask::answered`), and rings the bell. The
  caller has one path and never asks which Link it has. The Engine in
  this process has no Browser, and its `Ask` says so.
- **An ask whose reply nobody reads is a tell** (`Remote::tell`). That
  covers `browser_settled` and `browser_release`: every caller dropped
  their result, and the Server's broadcasts say what they changed.
- **What the Browser shows meanwhile is what it last heard.** It takes
  its tabs only from the Server's `Browser` messages. The Server sends one
  to the owning window before every reply that carries a state: attach,
  action, page and reclaimed publish, then reply. So a reply says only
  whether the ask went, and what it grants: a lease, an input's
  revision, a reservation. Applying a reply's state would undo a later
  message read in the same beat. The continuation sits next to the state
  it guards (the Browser's `Waiting`, a live annotation's `Beginning`). It
  runs on the answer.
  - An action pressed before the window owns the Browser waits for the
    attach (`Then`).
  - A recovered page stays paused for the agent until its Reload or
    Close is answered.
  - The operator's input reaches the page once the Server authorizes it,
    one at a time per page. The page is paused for the agent from the
    first input event, as before.
  - The address field keeps what was entered until the action lands.
    Choosing a page's tab leaves a new tab being entered at once. A
    page's own new window leaves the field as it is.
  - A page report or a reclaim is not asked again while one for its tab
    is on its way.
  - A failed ask, a timeout included, shows on the Browser's notice.
- **No blocking path for the Browser.** Its asks no longer reach
  `Remote::execute`. Tests take an answer in one step with
  `Cockpit::wait_for` (test-support), which pumps the model until it
  comes. The window never does. The browser smoke polls an attach's
  `Ask` frame by frame (`browser_smoke::served`).
- **The probe.** Under test-support, `cockpit::waits` records each
  command the window sends: its op, whether it waited for the reply, and
  how long the window's thread spent on it. `tests/browser_asks.rs`
  prints the distribution and requires that no Browser ask waited. After
  this change, on the fixture Server, a Browser ask costs the window's
  thread only its send: 50 to 200 µs at the median by kind over two
  runs, 0.5 ms at worst, and no wait. With a Server that never answers,
  the press takes 24 to 27 ms. The window pumps and draws on each of
  the next 20 beats, and the Browser's notice says the Server did not
  answer once the ask's time is up.
- **What is left, in the order to take it.** These are the remaining
  blocking asks at main by family: how many asks, then their call sites
  outside tests.
  1. Notices and seen: done. Its acts read no reply, so each is a tell
     (`Cockpit::tell`): opening, dismissing, clearing, marking read or
     unread, the Subject a Pane shows, and what is seen. What they change
     arrives as the Server's Notices do, a beat or more later.
     `acknowledge_seen` runs every frame, so it remembers what it told
     the Server was seen (`seen_told`) and tells the same unread things
     once. A new attach forgets that. An Engine in this process still
     carries each act out at once.
  2. Turn and Session: the acts that read no reply are tells now
     (interrupt, stopping a Subagent, Ferrite's own words, sending or
     taking back a queued prompt, settings changed). The Composer's ↵
     is an ask now: `Cockpit::send` and `Cockpit::queue` return an
     `Ask`, and the root runs what follows the answer
     (`CockpitView::when_answered`, settled by the pump; an Engine in
     this process answers in the press). A sent draft is on its way in
     the root's `Outbox` until the Server answers. A Thread's drafts go
     one at a time, in the order sent, so each is sent or queued by what
     the Thread is doing once the one before it is settled, as it was
     when the window waited. A refusal that comes after the press puts
     the draft back in the Thread's Composer, ahead of newer typing, or
     keeps it for when a Pane next shows the Thread. A side chat's
     question goes the same way. A replayed prompt (`send_again`:
     Retry, resend, the PR card, Create PR) goes through the same
     `Outbox`, behind the drafts sent before it. It has no draft to
     settle: what follows it hears where it went, and the PR card says
     it is handing the prompt over until then. The others whose reply
     the window reads
     are asks too, each with what follows its answer in
     `when_answered`: answering a Decision (a refusal shows under the
     request), rewind and fork (a refusal is said in the Thread), model,
     effort and provider (the same), and start fresh, whose resend
     needs the window and so goes out a beat after the answer. A side
     chat's tab opens at once and says a refused fork when the refusal
     comes. A test or a scene on an Engine in this process reads an
     answer with `Ask::now`. Nothing of this family waits.
  3. Thread lifecycle: naming a Thread, snoozing or waking it and a
     Session control are asks now, each with what follows its answer in
     `when_answered`: the nav's name and its notice, the undo a snooze
     offers, the snooze field's refusal, and why a control was refused.
     Park, close and delete are asks too. What follows one of them has
     two owners. The model's part (`Follow`: a parked Thread leaves the
     roster, a deleted Thread's parked row goes) is done by the model on
     the beat that brings the answer, before the window reads it. The
     window's part (the Panes, the nav's notice, the undo it offers)
     runs from `when_answered`, or from `when_all_answered` for a press
     on several Threads. So over a Server a closed Pane goes a beat
     after the press. Revive and reopen are asks the same way: a parked
     row's press, ⌘O, ⌘Z on a park, back and forward, a Notice opened
     and the Factory's earlier-Worker reader. The model's part of a
     reopen is a `Follow` too (the Thread takes the screen and the park
     order forgets it). What follows with the window in hand (a search
     that ends, a Subject shown, the keyboard let go) runs from
     `when_answered_in`, a turn of the loop after the answer. Open and
     import still wait, and a launch waits for the Threads it imported
     (`revive_and_wait`): it has no window yet.
  4. Queue: done. Sending a queued prompt now and taking one back were
     tells already. The three whose reply the window reads are asks now,
     and each puts its prompt in the Composer when the answer comes
     (`returns`): a held prompt taken back to edit, a prompt the agent
     handed back (the pump asks a Thread once at a time), and a step
     through prompt history. A Thread's history steps go one at a time,
     in the order pressed, since each starts from what the one before
     it put in the Composer.
  5. Projects: done. Registering a Project, naming it, removing it and
     adding or removing one of its directories are asks. The Project
     card says a refusal when the answer comes, and closes only if it is
     still the card that asked. Committing a new Project is two asks in
     a row (register, then name), so the card holds `committing` until
     the last answers and a second press commits nothing. A path
     registered from a new Pane's band lands on that Pane if it still
     stands. Telling a Server its CLI was replaced is an ask whose
     answer is only logged.
  6. Factory: 3 asks, 5 sites, all on the Server only (`factory_ask`).
  7. Server lifecycle: 4 asks, 6 sites. `stop_server_and_wait` waits on
     purpose: an installer replaces the executable next. The other three
     go last.
  8. History: 2 asks, no sites outside tests.
- **A feature view could now ask the Server itself, for the Browser's
  asks.** An `Ask` is a value its asker holds, and its answer is an
  arrival: the model emits `Arrived` on the beat that brings it, even a
  beat on which the Server said nothing (a timeout). The test is
  `an_asks_answer_is_an_arrival`. A feature that holds the model's handle
  and subscribes to `Arrived` can ask and then look at its `Ask` when it
  hears. It needs neither the root's pump nor a blocked listener.
  - What still needs the root is what follows an ask, where that is the
    root's: a Pane remade, focus, the Project filter.
  - The other families' asks still block, so a feature that used one
    would block its own listener.
  - The Browser's state stays the root's here (What does not get a
    view). Moving it is not part of this change.

## Amendment (2026-11-25): a feature view reads the window through a `Source`

A feature read its inputs through a weak handle to the root
(`WeakEntity<CockpitView>`, 21 files), so none could build, draw or be
tested without the whole window. An edit to the Archive rebuilt the
Cockpit, and each of its tests opened a Cockpit window.

- **A new feature view takes a `Source`, not a handle to the root.**
  `ferrite_ui::source::Source<T, Q>` is one read:
  `read(&Q, &App) -> Option<T>`, `None` once its owner is gone. The root
  makes it over itself as it makes the view
  (`Source::of(&root, |root, query, cx| root.archive_inputs(query, cx))`).
  `Q` is what the view asks to be answered: the Archive's search, the
  `Background work` dialog's Thread.
- **The read cannot write.** It takes `&App`, so a draw through it cannot
  update the root. It reads the root as an entity, so a root notify still
  redraws the view, as the read through the handle did (`tracking_reads`);
  a cached sibling that read nothing keeps its cache (`source.rs`'s
  test). Root code that runs while the root is borrowed still passes the
  values in: a `Source` read there panics, as the handle's did. There is
  no writing `Source`: a view the root builds rows for or that writes from
  render (PRs, Settings, the PR reader, the side chat) is redesigned
  before it moves.
- **What the view asks still goes out as its event**, and the root still
  answers by a method that takes its values.
- **A feature view is tested without the window.** `ferrite_ui::testing`
  (behind `test-support`) mounts one view under the toolkit's root in a
  window of its own. The test hands it a `Source` over an entity of its
  own and notifies that entity as the root would; `record` hears what the
  view emits. Whole-window tests stay for what the root wires: one
  journey through the Archive, the dialog's opening, closing and `Stop`.
- **The first two.** The Archive's list is `ferrite_factory::archive`, a
  crate that builds and tests without the Cockpit, the Engine and the
  Server: `cargo tree -p ferrite-factory` reaches none of them, and
  `scripts/ci_scope.py` forbids it the Cockpit. The search's matching is
  core's (`Task::matches`). The `Background work` dialog reads through a
  `Source` where it is, in the Cockpit, until the background work's own
  crate exists. The other root handles (19 files) move one view at a
  time, each keeping its handle until it does.
- **The Factory's leaves follow.** The New task sheet, the Rework and
  Direct this task sheets, a Task's say in its Overview, the start's
  inspector and the Workflows editor are `ferrite_factory`'s, each over a
  `Source` the root makes over itself. A sheet asks for its Project and
  what it is on (`NewTaskQuery`, `DirectQuery`), the editor for the
  Workflow and Step it chose (`WorkflowsQuery`). A picker's rows travel as
  the Factory's own `Pick`, now `ferrite_factory::picker`'s; a sheet's
  form and the menus' fuzzy filter are `ferrite_ui`'s, since the
  Settings' sheets and the Files' tree draw and filter with them too. The
  Factory itself (`FactoryView`) stayed at first: the root drew its board,
  handing it the Coordinator's and a Worker's Pane to lay out, which a
  read could not carry, and it held the Coordinator's view, then still the
  Cockpit's. It follows (below).
  Measured on the start's inspector (Windows, warm build, two other
  workers building): a one-line edit, to its tests green, took 27-39 s
  through the Cockpit and takes 11-18 s in `ferrite-factory`; one of its
  tests takes 0.02-0.1 s in its own window, against 0.3-1.5 s in a
  Cockpit window.
- **The Factory's board follows, with the Coordinator's surface.**
  `FactoryView` is `ferrite_factory::board` and `CoordinatorView`
  `ferrite_factory::coordinator`; the root holds the Factory in the
  board's place and mounts it as a child view, as it does the PRs. A Pane
  only the window lays out comes to them as its cell
  (`ferrite_pane::cell::CellSlot`: the Pane's cell view, cached as a part
  while the window caches its parts), which the root's own board draws
  with too (`pane_slot`). A read cannot make a cell's view, so the root
  makes the cells the Factory lays out as it draws (`make_factory_cells`)
  and its reads hand back the ones made (`made_cell_slot`): the
  Coordinator's in `CoordinatorInputs`, the Worker's on the open Task's
  Worker tab (`WorkerTab::Pane`). The board reads
  `Source<FactoryInputs, FactoryQuery>`, asking with the window's size,
  for the board's width, and with the open Task as it keeps it (whether
  it opened from the Archive, its tab, the earlier Worker being read):
  the read runs while the Factory draws, so it never reads the Factory.
  An earlier Worker's words and the Workers a Task had come as values
  (`WorkerTab::Read`, `WorkerTab::Had`), and a press on one says
  `FactoryEvent::ReadWorker`, where the root drew them with its own
  listeners. The skills found (`Skills`) are `ferrite_factory::workflows`'.
  The root keeps the wiring: making the Factory and its leaves, answering
  their reads, the Server asks, the focus pump, a Task's toasts and
  notifications, and the Factory's rows in the nav and the palette.
  Measured (Windows, warm build, another worker building): a one-line
  edit to the board, to its tests green and a typecheck, takes 6-7 s in
  `ferrite-factory`; a board test takes 0.04-0.15 s in its own window.
- **Measured** on Windows (i9-14900KF, warm build, the machine idle): a
  one-line edit to the Archive, to its tests green and a typecheck, took
  16-17 s through the Cockpit and takes 4 s in `ferrite-factory` (6-8 s
  with the Cockpit's typecheck after it). One Archive or dialog test
  takes about 0.02 s in its own window, against 0.2-0.4 s in a Cockpit
  window.
- **The tools.** `ferrite-tools` holds Files' `RemoteFiles` (over the
  window's `Submit`), the Terminal (`Source<TerminalInputs>`) and Changes
  (`Source<Origin, Diff>`: where the window reads a diff now, so an answer
  read elsewhere is dropped). A Menu button a feature crate draws is a
  `ferrite_ui::components::MenuDoor`, its press an event the root maps to
  its own `Door`. What both sides read went to the lowest crate both use:
  `ferrite_pane::hunks`, `ferrite_ui::answer`, `PrTarget` in core. A
  one-line edit to the Terminal, to its tests green and a typecheck, took
  17-27 s through the Cockpit and takes 8-12 s in `ferrite-tools`; a
  Terminal test takes about 0.2 s, against 1.2-1.9 s.
- **Settings.** The Project card, its folder browser, Connections and
  the Settings sheet are `ferrite_settings`' (`project_editor`,
  `folder_browser`, `connections`, `sheet`), which builds and tests
  without the Cockpit. The sheet was one the root built rows for: its
  rows are the sheet's now, built from values (`SettingsInputs`: the
  settings, the machine's defaults or what stands in for them, the
  Server's standing, the update's line), and each edit is said as one
  (`SettingsEvent::Change` over `Settings`, `Defaults` over the
  machine's defaults), which the root saves where it lives. A press
  that asks in a window (Restart, Stop, Update) carries the window it
  was pressed in. Connections reads each environment as the window
  reaches it (`Reached`), a value of its own, not the client's
  `ServerState`. The folder browser asks a Machine through a `List`. A
  one-line edit to the Project card, to its tests green, took 14 s
  through the Cockpit and takes 5 s in `ferrite-settings`; a card test
  took 1.26 s on average in a Cockpit window and takes 0.09 s in its
  own.
- **Files follows, its stops declared.** The Panel's keyboard row is the
  window's: its order is the head's stops, then the body's. Files used
  to register its tab stops in the root's `PanelFocus` as it drew, a
  write to the root from draw. Now the view declares them
  (`FilesView::tab_stops`: each tab's file and what it says), the root
  enters them in the row as it draws the body, and the view reads each
  tab's handle back through its `Source` and draws the tab with it. A
  stop's look and semantics (`Semantics`, `focusable`, `icon_stop`) are
  `ferrite_ui::stop`'s, and the tabs row's scroll is
  `ferrite_ui::tab_scroll`'s, since the Panel's own tabs scroll with it.
  A rendered file's find is `ferrite_pane::preview`'s, since a Pane's
  preview finds with it too. The ⋯ and a row's menus stay the root's: the
  view gives their rows as its own `Row`, which the root maps to its
  verbs. A feature with stops of its own (the PR reader) declares them
  the same way when it moves. Measured on Files (Windows, warm build,
  other workers building): a one-line edit, to its tests green, took
  18-48 s through the Cockpit and takes 10-32 s in `ferrite-tools`
  (14-40 s with the Cockpit's typecheck after it); a Files test takes
  0.6-0.8 s in its own window on average, against 2.0-11.9 s in a
  Cockpit window.

- **The nav's crate.** `ferrite-nav` holds the empty board
  (`Source<EmptyBoardInputs>`; a recent Thread comes as the fields of its
  nav row it draws, its mark, name and Project). A one-line edit to it,
  to its tests green and a typecheck, took 15-18 s through the Cockpit
  and takes 4-5 s in `ferrite-nav` (6-8 s with the Cockpit's typecheck
  after it); one of its tests takes 0.02-0.14 s in its own window, against
  0.2-0.75 s in a Cockpit window.
- **The toasts.** `ToastsView` is `ferrite_nav::toasts`, over two reads
  (`ToastSources`). A tend asks one question (`Source<Tending, Asked>`):
  it says each machine's watermark, the alerts standing and those whose
  delay is up, and the root answers what each machine said since, whether
  each alert still wants the operator and is in view, and whether each
  due one chimes and what it posts. The float reads its newest toast's
  row (`Source<Row, Alert>`). The view no longer holds the model: the
  root hands it its wakes (`watch`: the root's notify and the model's
  `Arrived`, which stays the Cockpit's), and it keeps its own clock.
  `Posting`, a system notification's words, is `ferrite_nav`'s, since
  the view hands it to the root to post.
- **The nav.** `NavView` is `ferrite_nav::view`, with its list, keys,
  search, picks, Snooze field and foot (`view/`), over three reads
  (`NavSources`). `Source<Vec<Listed>>` says each open Thread's section;
  the view notes when each came into it and asks `Source<NavLook,
  NavAsked>` with those arrivals, and the root answers what it draws
  (`NavInputs`) and the factories. A hover card's model is a read of the
  model (`Source<SharedString, (ThreadKey, Provider)>`). The open menu
  and the button holding it come as `NavMenu`, which the root maps to its
  `MenuTarget`; a press says `NavEvent::Menu` or `Door`, and the root
  hangs the menu and picks its edge. Where a menu button stands is
  draw-time geometry: as it builds each button the view asks `door`, a
  factory the root makes once over the popup slot's triggers, for that
  menu's `MenuDoor`, bound to its menu alone, as the Terminal's is. A
  factory, not a door in the inputs, since a row's Snooze button comes
  and goes with the row. The machine notices (`MachineNotice`) and their
  reconnect grace are the view's; the rename field's look is
  `ferrite_ui::components::rename_field`, which the board's title
  rename shares. The root keeps what the rest of the window decides: the list
  and its order (`listed_threads`, `thread_order`), the notice, the
  shelves' folds, the Filter and Snooze rows, a Park's undo
  (`nav_undo.rs`) and the keys that move focus to the list
  (`nav_keys.rs`).
- **The pull requests' crate.** `ferrite-prs` holds the PRs destination,
  the PR reader with its reads and actions, and the side chat, which
  build and test without the Cockpit. None of them leases the root in its
  render now. The reader reads `Source<PrInputs>` (what is shown, its
  read, the `⋯`'s `MenuDoor` and the handles of its stops) and asks
  `Source<bool, PrTarget>` whether an answer is still wanted, so one
  for a closed tab is let go. The PRs read `Source<PrsInputs>` (the
  Projects) and hold the window's reader as a child; Code's body for
  both is a `ferrite_tools::changes::ChangesPane` the root makes, handed
  in as an `AnyView`, since a feature crate does not reach another. The
  reader's stops are handles the root enters in the Panel's row as it
  draws (`register_pr_stops`), as Files' are. The side chat asks
  `Source<SideChatInputs, Forks>` with the forks its tabs hold; the root
  tells the shown fork's transcript what it holds from its pump
  (`sync_visible_transcripts`), not from the side chat's draw, and a
  fork's words still land in the frame that brings them. `PrTab` is
  `ferrite_prs`'s, kept on the Surface. What the root keeps: the forge,
  the Thread's agent and a hand-off's prompt, the reads' pacing
  (`PrReads`), the PRs' place on the board, the forks, and the context
  menu, whose PR rows it reads from the reader (`menu_rows`). Measured
  on the PRs (Windows, warm build): a one-line edit, to its tests
  green, took 14 s through the Cockpit and takes 3-4 s in `ferrite-prs`
  (2 s more for the Cockpit's typecheck after it); a PRs test takes
  0.11-0.22 s in its own window on average, against 1.4-1.9 s in a
  Cockpit window.

## Precedent

Zed builds its workspace this way: each panel is an entity that owns its
state, emits events, and is subscribed to by the workspace. We take the
shape only. We did not adopt Zed's `Panel` and `Dock` traits, its action
registry or its settings store. Ferrite has one nav and one Panel (ADR
0039), and nothing yet needs a general host for more. No Zed code is
copied.

## Words

The word is **nav**, and the nav is the Inbox (ADR 0040). _Avoid_ sidebar.

An **ask** is a command whose answer the asker reads (`Ask<T>`). A
**tell** is one whose reply nobody reads. _Avoid_ request and call for
either: a request is a Browser control the Server asks of a window.

---
status: accepted
date: 2026-11-22
---

# A Thread is named by its machine

The operator wants one sidebar holding every connected machine's Threads,
each usable in place, with a small icon on a Thread that lives on another
machine: "Its simple fully merged, and a little icon." Until P6 a window
was on one machine at a time (ADR 0026, "One machine on screen"). Going to
another rebuilt the whole view, because the window keeps per-Thread state
in dozens of places keyed by `ThreadId`, and a `ThreadId` is unique only
within one machine's Store: Thread 3 here and Thread 3 on box are two
Threads. A merged list needs a key that says which machine a Thread is on,
and a window that holds every machine at once.

## Decision

- **A Thread is named by a `ThreadKey`.** `ThreadKey { machine:
  MachineId, thread: ThreadId }` (`ferrite-core/src/thread.rs`) is Copy,
  Ord and Hash. Its fields are private and only core makes one, stamping
  the machine whose Server reported the Thread. A key that pairs a Thread
  with the wrong machine cannot be written, so no call site has to check
  it. Tests in other crates make one with `ThreadKey::fixture` or a
  Cockpit's `key(id)` (core's `test-support` feature). A Project is named
  the same way, by `ProjectKey { machine, project }`, because a
  `ProjectId` is per machine too.
- **An id read from a machine becomes a key there.** What one machine's
  Server said that every window shares (its registry, its Notices, its
  Factory board) is read as an `OnMachine` reading (`Cockpit::registry`,
  `notifications`, `factory`): it derefs to what it reads, and its `key(id)`
  and `project_key(id)` name what an id read in it is. An id a Thread's own
  transcript names, such as a link to a Thread it spawned, is a Thread of
  its machine (`ThreadKey::sibling`). No caller pairs an id with a machine
  it chose.
- **A `MachineId` lasts one run and is never reissued.** It is minted from
  one counter for the whole process as a Cockpit holds a machine
  (`MachineId::mint`). A machine that is let go takes its id with it, so a
  key held from it names nothing rather than a Thread of the next machine.
  The counter is per process, not per Cockpit, so a key carried to another
  Cockpit in the same process names nothing there either.
- **A Thread is saved as a `SavedThread`.** A `MachineId` means nothing in
  the next run, so what is saved is `SavedThread { machine: MachineKey,
  thread: ThreadId }`, which is `{"machine":"me@box","thread":3}` on disk.
  `MachineKey(Option<String>)` is the machine's target as Settings saves
  it, `None` for this computer. It moved into core from `ferrite-cockpit`,
  where it named the machines alerts come from (ADR 0017). A Thread saved
  before this change is a bare id, and it reads as this computer's: most
  were, and a wrong one is only a draft or a pin on another Thread, which
  the operator can clear. Pins (`Settings::pinned_threads`) are saved this
  way, each naming its Thread's machine. So are the drafts, whose one
  `drafts.json` holds every machine's. A Delegate on a linked machine will save its child the same way (ADR 0047, to come).
  Its design called the type `ThreadRef`; there is one name,
  `SavedThread`.
- **A saved name is fixed when the machine attaches.** What saved files
  call a machine is given to the Cockpit as it attaches there
  (`Attach::saved_as`, from the machine's entry in Settings), and the
  holder keeps it. `Cockpit::saved(key)` writes a key as a `SavedThread`
  and `Cockpit::resolve(saved)` reads one back, as a Thread of the machine
  saved under that name or as nothing. A pin, a draft, a cited source and
  a Panel are saved through them, so each is saved under its own machine's
  name, and read back for every machine the window holds.
- **The shape: one `Cockpit` holds each machine as a `HeldMachine`.** The
  window keeps its one `Cockpit`. Inside it, the private `HeldMachine`
  (`ferrite-core/src/cockpit/held.rs`) keeps what one link means: its
  `MachineId`, its saved name, the link (an Engine in this process or a
  Server's), the mirrors, the shared state, the parked rows, the logs,
  what the Server was told of this window and of what it has seen
  (`seen_told`), and what this window draws of its Threads (`Drawn`). The Cockpit keeps them in
  `BTreeMap<MachineId, HeldMachine>`. What exists once per window stays
  on the Cockpit, keyed by `ThreadKey`: the Roster, focus, `composing`,
  `visible_subjects` and `window_active`. `Drawn` is per holder, not per
  window as the design had it: each Server is told what this window draws
  of its own Threads, by their own ids. The Cockpit's public API names
  Threads and Projects by key only: `pump`'s `PaneUpdate`s, `threads`,
  `known_threads`, `sweep`'s `Restart`s and the browser's messages carry
  keys, and private `at`/`act_on` split a key into its machine's holder
  and the `ThreadId` there. The wire does not change: each Server keeps
  its own `ThreadId`s, and the CLI, the API and `window.*` actions address
  this computer's Server. The design named the holder `Server`. The
  Server process is the `ferrite-server` crate, so the holder is
  `HeldMachine`.
- **What stays a bare id in the window.** Only a board's layout leaves
  (`layout::Tree`, the view's `Leaf`), which name readers as well as
  Threads. A board holds one Pane and its reader, so the view maps a leaf
  back to its slot through the board (`leaf_slot`), never through every
  Pane, where two machines' Threads share ids. The Inbox and alerts read
  each machine's Notices (`notifications(machine)`) and key a Notice's
  Thread as they read it, an `Alert` names its `ThreadKey`, and the view's
  Project state (the nav filter, the Factory and draft bindings) is a
  `ProjectKey`.

## Rejected

- **One `Cockpit` per machine under a registry**, which is T3 Code's
  literal shape. The Cockpit holds state that must exist once per window:
  the Roster, focus, park order, `composing` and `window_active`. Splitting it N ways means either merging N Rosters
  wherever the board reads, or pulling the Roster out, which is the chosen
  shape plus a layer. And N public `Cockpit`s would each take any bare
  `ThreadId`, so the right machine would be a check at every call site. T3
  can do it because its state is derived atoms, not window state written
  in place.
- **Ids branded by machine in the type system.** Machines exist only at
  run time.
- **Rows for other machines that move the window there on a press**,
  which is today's nav. The operator rejected it: "there should be no
  remote view stuff just a single sidebar".

## The sequence

Each lands green and proves something.

1. **P1** `feat(core): a ThreadKey names a Thread and its machine`. The
   types above, `Cockpit::machine_id` (now `on_screen`), and this record.
2. **P2a to P2d** `refactor(cockpit): <feature> keys its per-Thread state
   by ThreadKey`, in parallel: (a) the browser; (b) the Files and terminal
   tools; (c) Surfaces, beside, the Panel's controls and overview; (d) side
   chats, sends, titling, the PR watch, background work and the Facts.
   Where a group met state still keyed by a bare id, it converted at
   that call with core's `Cockpit::thread_key(id)` and the key's
   `thread()`, a seam P4 and P5 removed; there was no seam on the root, so
   five changes in parallel did not each add one. A file a group saves moves to `SavedThread`s: the
   Panel's `panel.json` keeps every machine's Panels, the window holds
   those of the machine it is on, and writes the others back as it read
   them.
3. **P3** `refactor(cockpit): the nav, board and menus speak ThreadKey`:
   `NavKey`, `NavEvent`, the nav's and the board's `MenuTarget`s,
   `ThreadRow`, `Arrivals` and the nav's picks. Saved pins become
   `SavedThread`s. In parallel with P2. Three things the design listed
   stay: drag and drop holds a board's layout leaves, which name readers
   as well as Threads, so it moves with the layout (P4/P5);
   `Notifications.phases` lives in each Server's Store, where an id is
   whole; and `Alert.away` names a machine off screen, which has no
   `MachineId` until the window holds every machine, so it folds into the
   key in P6.
4. **P4 and P5, as one change** `refactor(core): the Cockpit holds each
   machine as a HeldMachine and names Threads only by key`. The split
   inside core, the key-only API with `thread_key(id)` gone, every caller
   moved, and the window-wide `on_machine()` replaced by a question about
   a key's machine (`Cockpit::local(machine)`) or, where an act names no
   Thread, about the machine on screen. A core test proves two machines
   holding a Thread of the same id stay apart; a view test proves pins
   stay each machine's (`each_machines_pin_is_its_own`). Most
   of the callers moved by a mechanical rewrite, kept out of the tree.
6. **P6** `feat(cockpit): the window holds every machine at once`. The
   window swap goes (`CockpitWindow::go_to`, `GoToMachine`, landing,
   `ServerStatus`, `Bell::moved`). A press opens in place, and a lost
   machine dims only its own Panes and rows. ADR 0026's "One machine on
   screen" and "Alerts from machines off screen" are superseded. What
   P4 and P5 left on the machine on screen moves to the key of what it
   acts on: Settings' environment and defaults, the Server page,
   attachments and their uploads, the project editor and draft panes,
   the default-branch and checkout lookups, free disk, remote files, a
   system notification's machine, and the Notices the Inbox reads (one
   `notifications()` per machine). So do the bare ids above and how
   remote Threads stream (which Threads each Server is told this window
   draws).

   Done. Every enabled machine is held at start and whenever one is turned
   on; one that is unreachable or wants a sign-in dims only its own rows
   and Panes. A machine other than this computer streams a Thread whole
   only while the window draws it, and its status, Decisions and Notices
   arrive for every Thread; this computer keeps its Threads at work
   mirrored as before (`HeldMachine::watched`). Whether a machine has been
   heard is per machine, and a prune of saved per-Thread state (a Panel,
   the Files' working sets) drops only a heard machine's Threads it no
   longer has. Where an act names no Thread or Project, the machine is an
   explicit argument or page state: a new Thread with no Project starts
   on this computer (with it off, on the first machine held), and so does
   an import not opened for a Pane; the Factory is about its Project's
   machine; Settings names the machine whose defaults it edits; and
   Connections and the Server page name theirs.

   Every machine's open Threads have Panes, but only a press, or that
   machine's `thread.focus`, goes to another machine's. Focus is `None`
   while the operator is in no Pane, so
   a Thread that opens on another machine, or stands next to a Pane that
   closed, is never where a new Thread takes its Project from. Only the
   start machine's Threads take an empty screen; a Pane that closes hands
   focus to a draft, or a Thread of its own machine or the start machine
   (`Roster::takes_focus_unasked`). The Project a launch starts in is the
   start machine's too.

   A machine turned off and on is the same machine to the window. Its
   Panels' tabs wait under its saved name and come back keyed by its new
   `MachineId` (`Surfaces::hold` says each re-hold). A terminal tab reads
   that its shell ended, and a side chat resumes on its fork, which the
   roster takes back but no Pane shows. Only a restart drops a tab that
   ends (`Restart::Ends`). Its drafts are put away and reopen with it,
   and while it is off nothing the window holds (the nav filter, the
   standing draft, the launch Project) names its Projects, so a new
   Thread starts on this computer. Settings names the machine it edits
   atop its pages about it, as T3's scope sentence does (`Applying
   settings on box`).
7. **P7** `feat(nav): one list across machines, a small icon marks a
   Thread on another machine`. The machine headings go, rows merge into
   the sections, and a 14px server glyph sits at the meta line's right
   end, as in T3. ADR 0026's "The nav", the `CONTEXT.md` Machine entry and
   the UI spec follow.

   Done. `nav::RowTag` names a row's elements, its machine in them only
   for another machine's row, so this computer's ids stay bare. What a
   heading carried went to a home or went: its connection glyph and word
   to the notice row ("Sign in to box", "box is offline", "box is
   reconnecting" after `RECONNECT_GRACE`); its name to the row's hover
   card, or on a row with no hover card (a parked or snoozed row, a search
   result) to the same glyph's tooltip; its "New project…" to Add project,
   which on more than one machine opens the palette over the machines the
   window reaches (T3's `choose-environment`, "Environments") and with
   none offers T3's "Connect an environment"; its fold nowhere. A draft
   aimed at another machine names it first in the strip over its Composer
   (T3's locked environment control). Ages and
   order read every machine's times on this computer's clock: a held
   machine keeps its Server's clock (`ServerClock`, from `welcome`) after
   its link goes, a Thread's digest carries its last activity
   (`Digest.active_ms`), and `Cockpit::last_used` reads a parked or
   digest-only Thread's through that clock. Where Projects are listed by
   name (the Filter menu), another machine's wears T3's icon, its tooltip
   "On box". The machine pick goes back a step on its arrow and on
   backspace in an empty line, and esc closes the palette, as T3's view
   does.

   Another machine's row is the same row as this computer's. Its branch
   is read by git where the checkout is (`Op::Branches`), so its card has
   the same three lines and height. On a slim row (parked, snoozed) the
   glyph stands just before the age in the right column. T3's slim row
   draws no machine icon; Ferrite keeps it so every row on another machine
   says so. The hover card ends on the Provider's mark and the model, named
   only once the window knows it: an unopened Thread on another machine
   says its Provider until it is opened.

## Answered questions

- **The icon.** One generic server glyph, T3's default kind. A kind per
  machine can come later, in Settings.
- **Signing in.** "Sign in to box" is a notice row in the nav naming the
  machine, plus the band on that machine's Panes. A session the machine
  refused or revoked says so too, also after a window restart, and pressing
  it opens that link's pairing in Connections, where a new code renews it.
- **The same repo on two machines** is two Projects. T3 Code groups them
  by repository unless set to keep them separate, and marks a Project on
  another environment with a machine icon whose tooltip says "On box".
  Ferrite keeps them separate and marks the other machine's the same way.
- **A new Thread with no Project** starts on this computer.

## Not done here

- System notification tags already name the machine (ADR 0017), a Factory
  task's too (`ferrite-task-{task}@{target}`, this computer's bare). Anything
  else that saves a `ThreadId` across runs must save a `SavedThread`.

## Precedent

T3 Code keys every thread by `ScopedThreadRef { environmentId, threadId }`
(`packages/contracts/src/environment.ts`), and one registry keeps a
connection per environment that every action names
(`packages/client-runtime/src/connection/registry.ts`). Its sidebar is one
list flattened across environments, with a machine icon on a thread that
is not on the primary one.

## Words

A **Thread key** is a Thread and the machine it lives on, for this run. A
**saved Thread** is the same, as it is written to disk. _Avoid_ thread ref
and scoped id. A **held machine** is one machine as a
window holds it: its link, what its Server said, and the name it is saved
under. _Avoid_ server for it: the Server is the process.

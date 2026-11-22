---
status: accepted
date: 2026-11-22
---

# A Thread is named by its machine

The operator wants one sidebar holding every connected machine's Threads,
each usable in place, with a small icon on a Thread that lives on another
machine: "Its simple fully merged, and a little icon." Today a window is
on one machine at a time (ADR 0026, "One machine on screen"). Going to
another rebuilds the whole view, because the window keeps per-Thread state
in dozens of places keyed by `ThreadId`, and a `ThreadId` is unique only
within one machine's Store: Thread 3 here and Thread 3 on box are two
Threads. A merged list needs a key that says which machine a Thread is on,
and a window that holds every machine at once.

## Decision

- **A Thread is named by a `ThreadKey`.** `ThreadKey { machine:
  MachineId, thread: ThreadId }` (`ferrite-core/src/thread.rs`) is Copy,
  Ord and Hash. Its fields are private and only core makes one, stamping
  the machine whose Server reported the Thread (`Cockpit::thread_key`). A
  key that pairs a Thread with the wrong machine cannot be written, so no
  call site has to check it. Tests in other crates make one with
  `ThreadKey::fixture` (core's `test-support` feature). A Project is named
  the same way, by `ProjectKey { machine, project }`, because a
  `ProjectId` is per machine too.
- **A `MachineId` lasts one run and is never reissued.** It is minted from
  one counter for the whole process as a Cockpit opens its link
  (`MachineId::mint`), and `Cockpit::machine_id` says which one a Cockpit
  holds. A machine that is removed takes its id with it, so a key held
  from it names nothing rather than a Thread of the next machine. The
  counter is per process, not per Cockpit, because until the window holds
  every machine a move between machines builds a new Cockpit, and a
  counter per Cockpit would give both machines the same id.
- **A Thread is saved as a `SavedThread`.** A `MachineId` means nothing in
  the next run, so what is saved is `SavedThread { machine: MachineKey,
  thread: ThreadId }`, which is `{"machine":"me@box","thread":3}` on disk.
  `MachineKey(Option<String>)` is the machine's target as Settings saves
  it, `None` for this computer. It moved into core from `ferrite-cockpit`,
  where it named the machines alerts come from (ADR 0017). A Thread saved
  before this change is a bare id, and it reads as this computer's: most
  were, and a wrong one is harmless, a pin passed over until it is
  unpinned. Pins (`Settings::pinned_threads`) are saved this way, and a
  pin made while the window is on a machine names that machine. A
  Delegate on a
  linked machine will save its child the same way (ADR 0047, to come).
  Its design called the type `ThreadRef`; there is one name,
  `SavedThread`.
- **The shape: one `Cockpit` holds one Server per machine.** The window
  keeps its one `Cockpit`. Inside it, a private per-machine holder keeps
  what one link means today: the link, the bridge, the mirrors, the shared
  state, the parked rows, the logs, the registry, the drafts and what the
  Server was told. The Cockpit keeps it in `BTreeMap<MachineId, _>`. What
  exists once per window stays on the Cockpit, keyed by `ThreadKey`: the
  Roster, `composing`, `visible_subjects`, `Drawn` and `window_active`.
  The Cockpit's public API takes `ThreadKey` only, and one private
  function splits a key into its machine's holder and the `ThreadId`
  there. `ServerStatus` goes, since a full link that draws nothing streams
  only status already (ADR 0032). The wire does not change: each Server
  keeps its own `ThreadId`s, and the CLI, the API and `window.*` actions
  address this computer's Server. The design named the holder `Server`.
  The Server process is now the `ferrite-server` crate, so the change that
  adds the holder (P4) gives it a name that does not clash.

## Rejected

- **One `Cockpit` per machine under a registry**, which is T3 Code's
  literal shape. The Cockpit holds state that must exist once per window:
  the Roster, focus, park order, `composing`, `Drawn` and
  `window_active`. Splitting it N ways means either merging N Rosters
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
   types above, `Cockpit::machine_id`, and this record.
2. **P2a to P2d** `refactor(cockpit): <feature> keys its per-Thread state
   by ThreadKey`, in parallel: (a) the browser; (b) the Files and terminal
   tools; (c) Surfaces, beside, the Panel's controls and overview; (d) side
   chats, sends, titling, the PR watch, background work and the Facts.
   Where a group meets state still keyed by a bare id, it converts at
   that call with core's `Cockpit::thread_key(id)` and the key's
   `thread()`; there is no seam on the root. A file a group saves moves
   to `SavedThread`s: the Panel's `panel.json` keeps every machine's
   Panels, the window holds those of the machine it is on, and writes
   the others back as it read them.
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
4. **P4** `refactor(core): one Cockpit holds a Server per machine`: the
   split inside core only, with the `ThreadId` API and the single attach
   kept. In parallel with P2 and P3.
5. **P5** `refactor(core): the Cockpit's API takes only ThreadKey`, after
   P2 to P4. The seam and its assert go, and the window-wide
   `on_machine()` becomes a question about a key's machine at every site.
6. **P6** `feat(cockpit): the window holds every machine at once`. The
   window swap goes (`CockpitWindow::go_to`, `GoToMachine`, landing,
   `ServerStatus`, `Bell::moved`). A press opens in place, and a lost
   machine dims only its own Panes and rows. ADR 0026's "One machine on
   screen" and "Alerts from machines off screen" are superseded.
7. **P7** `feat(nav): one list across machines, a small icon marks a
   Thread on another machine`. The machine headings go, rows merge into
   the sections, and a 14px server glyph sits at the meta line's right
   end, as in T3. ADR 0026's "The nav", the `CONTEXT.md` Machine entry and
   the UI spec follow.

## Answered questions

- **The icon.** One generic server glyph, T3's default kind. A kind per
  machine can come later, in Settings.
- **Signing in.** "Sign in to box" is a notice row in the nav naming the
  machine, plus the band on that machine's Panes.
- **The same repo on two machines** is two Projects.
- **A new Thread with no Project** starts on this computer.

## Not done here

- Ages and "newest send first" mix machines' clocks once the list merges.
  T3 corrects each environment's clock. That needs an offset per machine,
  taken when its Server says hello.
- System notification tags already name the machine (ADR 0017). Anything
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
and scoped id.

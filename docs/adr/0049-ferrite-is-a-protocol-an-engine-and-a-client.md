---
status: accepted
date: 2026-11-24
---

# Ferrite is a protocol, an Engine and a client

`ferrite-core` is 160,000 lines in one crate. Rust compiles a crate as a
unit, so a one-line edit anywhere in it rebuilds all of it and every crate
above it. Its modules also name each other freely, so no part of it can be
built, tested or reasoned about alone.

Ferrite already has the boundary a split should follow. A window and a
Server are two programs that talk over a socket (ADR 0025). What crosses
the socket is the only thing they must share.

## The shape

Three parts, and the crates they become:

| Part | Holds | Who needs it |
| --- | --- | --- |
| **shared** | the protocol's messages and the types they carry: Thread, Activity, transcript, the log's schema, Settings, the roster | both |
| **engine** | the Store, the providers and their Sessions, the Engine, the Factory, workspaces, the API, the Server's end of each connection | a Server |
| **client** | the Cockpit: what a window holds of its Machines' Servers and how it asks them | a window |

The engine and the client each build on the shared part and never on each
other. When that holds, an edit to a provider rebuilds no window code, an
edit to the window rebuilds no Server code, and each side's tests run
without the other.

It does not hold yet. `scripts/core-sides.json` puts every top-level module
of `ferrite-core` on the shared side or the engine's.
`scripts/check_core_sides.py` counts each **crossing**: a path to an engine
module from a shared module, or from one of the window's crates
(`ferrite-client`, `ferrite-cockpit`, `ferrite-pane`, `ferrite-frame`,
`ferrite-ui`, `ferrite-terminal`). There are 8:

| From | Crossings | Mostly |
| --- | --- | --- |
| shared modules | 0 | |
| `ferrite-client` | 3 | the Store a window on this computer reads logs from, and the Spawner a window discovers sessions and commands with |
| `ferrite-cockpit` | 5 | the titler, and the Spawner it hands each `Attach` |
| `ferrite-pane`, `ferrite-ui` | 0 | |

CI fails when a pair's count rises, a new pair appears, or a module is on
no side. The baseline only goes down. What a released app cannot compile
is not counted: a module whose `mod` line, or an ancestor's, has a `cfg`
that fails with `test` off and only the `shipped` features of
`core-sides.json` on (`cef`), and an inline `#[cfg(test)] mod` block. A
file's name skips nothing. A `pub use` counts once for each engine item it
hands on, since every crate above can then name it without naming the
engine. The `ferrite` crate is not counted: it is the program that starts
both.

A crossing is a type living on the wrong side, or work. `store::Provider`,
`workspace::WorkspaceChoice` and the Factory's board were facts a window
shows, kept in the module that happened to produce them. The fix for those
is a move: the type goes to a shared module and the engine module uses it
from there. The rest are work: a window crate calling the engine's code.
Those become a question to the Server over the protocol.

## Built: `ferrite-client`

The Cockpit is now the crate `ferrite-client`: 7,300 lines and as many
again of tests, with no UI in it. Nothing in core named it, so nothing
below changed shape to let it go.

- `ferrite-core` cannot name the window model. Cargo refuses the cycle.
- `ferrite-server` builds without it. Its tests drive one, so it is a
  dev-dependency there, and `scripts/ci_scope.py` fails a runtime one.
- `ferrite-client` draws nothing: the rule that keeps `gpui` out of core
  keeps it out here.

It still depends on all of `ferrite-core`, so an edit to the Engine still
rebuilds it and the window. The cut that ends that is the last one below.

The tests that drive a real Engine through a Cockpit moved with the
Cockpit. They read a few of the Engine's and the Store's insides. Those
seams are `engine/probe.rs` and the Store's fault switches, public only
under the `test-support` feature.

The move has a cost. About thirty items the Cockpit used as `pub(crate)`
are now `pub`: the roster's mutators, the `ThreadKey` and `ProjectKey`
constructors, a few Engine and Store reads.

## Built: the types a window shows of `workspace`, `store` and `pr_watch`

What a window shows of those three modules moved to five new shared
modules: `provider`, `project` (`ProjectId`, `Project`), `checkout` (the
Workspace binding, the choice, `effective_cwd`, a branch's drift and a
repo's listing), `pull_request` (a PR, its checks and `Watching`) and
`folder` (a directory's `Listing` and `expand_home`). `replace_file` moved
to `disk`. No serialized form changed. The crossings went from 284 to 177.
Most of the 32 left into those three modules are work: `git` and `gh`
reads, the Store, the registry, the Server's GitHub reads and its record
of a PR watch. The rest name two types that stay in `store`: `ThreadMeta`
holds a `RuntimeMode`, and `LogRecord` is the whole log schema. The
registry still mints a `ProjectId` and numbers a `Project`'s new branches,
and the Store still renames its files through `disk`. So four moved items
are `pub(crate)`, and the split in step 3 has to make them public.

## Built: the types a window shows of the Factory, the providers and the Engine

What a window shows of `factory`, `providers`, `engine`, `spawner` and
`import` moved to the shared side. `notifications` names nothing of the
engine's, so it is a shared module now. No serialized form changed. The
crossings went from 177 to 60.

- `factory_board`: the Factory's `Board` and all it holds, in modules that
  mirror `factory`'s: Tasks and the reads a window makes of one,
  Workflows, how they start, the operator's say, a Task's PR and
  evidence, the Archive, the Coordinator and its proposals. `factory`
  keeps the Factory: its store, its `Command`s and what it does on each.
- `models` (the model catalog) and `shell` moved whole from `providers`.
  The catalog's disk cache stays there, as `providers::model_cache`.
- `cli_update` (a CLI check's `Status`, its `Upgrade`), `allowance`
  (`LimitWindow`, `AllowanceWarning`), `prompt_queue` (`QueueItem`,
  `Delivery`), `pane_update` (`PaneUpdate`, `SubjectUpdate`,
  `event_changes_content`), `session_file` (an import's `Candidate`) and
  `file_change` (Codex's `fileChange` item as Hunks, which the transcript
  reads too).
- Into the module that owns the fact: `ProviderChoice` to `provider`,
  `title_from_prompt` and `HELD_AT_REST` to `digest`, `Bootstrapped` and
  `BootstrapResult` to `roster`, `SessionDefaults` to `settings`,
  `FileSuggestion` to `prompt_files`. `prompt_history` is public, so a
  window names `HistoryDirection` there and `ToolTiming` in `activity`,
  not through `engine`.

The Factory still adds its own methods to `Task`, in `factory` and
`factory::direct`: the transitions that raise its `Effect`s, and five
reads only it makes. A crate cannot add methods to another crate's type,
so step 3 makes them the Factory's functions. Nine functions the engine
side calls on moved items went to `pub(crate)`: `StepChange::apply`,
`ApprovalChange::apply`, `Workflow::new_step_id`, `Definition::step_mut`,
`Automation::turn_off`, `Task::unfinished`, `Task::evidence`,
`SessionDefaults::effort_for` and `parse_file_change`. Step 3 makes them
public.

The rest of the crossings are work, and step 2 takes most of it: the
Engine, its Spawner and their types, which a Cockpit needs only to run
an Engine in itself; the in-memory Factory and the `Command`s a fixture
drives it with; the provider `Session`s the demo fakes; the titler, the
import scan and the memory reading, which the window runs itself. Four
types stay on the engine's side: `store::ThreadMeta` holds
a PR watch's record (`pr_watch::WatchFact`) and the Store's
`ThreadSummary` (its `RuntimeMode` needs nothing of the engine's),
`store::LogRecord` is the whole log schema, and `api::Heard` holds the
API's `Request`.

## Built: what a window reads of the engine side

What a window reads of the engine's modules, and has no reason to ask a
Server for, moved to the shared side. `runtime_mode` names nothing of the
engine's, so it is a shared module now. No serialized form changed: one
`serde` attribute names `WatchFact` at its new path. The crossings went
from 60 to 32.

- `pull_request` holds a Thread's PR watches (`WatchFact` and all it
  holds), out of `pr_watch`. `store::ThreadMeta` then held nothing of the
  engine's, and it moved with `ThreadSummary` and `LoadError` to a new
  module, `header`.
- `git_read`, new: the git reads a window makes of a checkout and its repo
  (`head_of`, `checkout_stamp`, `tracked_ignored`, `branch_status`,
  `checkout_branch`, `default_branch`, `worktree_paths`, `repo_listing`),
  what they call, `canonical`, and the `git` runner under them.
  `workspace` keeps making and removing worktrees, with that runner.
- `memory`, new: `rss_bytes` and the watchdog's `Restart`. `prompt_queue`
  took `Admission`, `session_file` the import scan (`default_roots`,
  `candidates`), `prompt_files` `mention_files`, `spawn`
  `spawnable_program` and `cmd_shim`, and the crate root `Discovery`,
  beside `SessionCommand`. `ferrite-client` no longer re-exports
  `Admission`.
- The registry's read model is `project::Projects`: the Projects and their
  worktrees, the form its file and the wire share, and the reads.
  `workspace::registry::Registry` keeps the Store directory, the file and
  every verb that changes them, and derefs to its `Projects`. A window's
  copy is a `Projects`, so it no longer carries a Store directory.
- `DraftBinding::resolve` takes the Project it reads, not the registry.

The engine side still reaches into seven moved items, so they are
`pub(crate)`: `git_read::git`, `spawn::spawnable_program`, and the three
fields of `Projects` with its `parse` and `to_bytes`, which the registry's
verbs and file use. Step 3 has to make them public.

Two types stayed on the engine's side. `store::LogRecord` is the whole
log schema, and `protocol` carries it. `api::Heard` holds the API's
`Request`, and `link` carries it.

## Built: the log's schema and the Server's end of the link

`protocol` and `link` are the two modules both programs must share. They
no longer name the engine's side. The crossings went from 32 to 30.

- `thread_log`, new: the schema every Thread's log is written in, which
  is also what a Server sends a window as history (`WireRecord::Known`).
  It holds `SCHEMA_VERSION`, the `Header`, the `Facts`, a mark's `Summary`
  and `Base`, every persisted type, `Record` with its conversions from a
  live event and back to an input, `LogRecord`, `handover_notice`, and
  schema 9's Activity (`thread_log::activity`). No serialized byte
  changed. Each item moved whole, with its serde attributes.
- The Store keeps every read and write of a log's file: opening,
  appending, syncing, the worker, replacing a log and the fault switches.
  It also keeps the replay bases it tracks (`store::base`), the prompt ids
  a rewind cuts at, and a Subagent's history read back off its log
  (`store::activity`).
- `connection`, new, on the engine's side: the Server's end of each
  connection. It holds the `Outbox`, the `Ring`, `ClientId`, `Heard`,
  `Connection` and `accept_all`, with their tests. `link` keeps what both
  programs use: the address, the framing (`Lines`, `send`, `relay`), the
  socket (`listen`, `connect`, `Listener`, `Stream`, `TryClone`), the two
  limits a window plans for (`MAX_LINE`, `LAGGED_BYTES`) and a window's end
  (`Client`, `Attached`).

The Store still reads and writes the schema's types itself, so 82 moved
items are `pub(crate)`: 24 types, 34 fields, 22 functions,
`SCHEMA_VERSION` and the `activity` module. `engine` also calls
`handover_notice`, which was `pub(crate)` before the move. Step 3 has to
make them public.

What is left is work. The `server` module's GitHub reads and CLI checks
run `gh` and the providers' CLIs. A window still runs the titler, reads a
checkout's PR through `workspace::pull_request` (`gh`), and builds the
Engine, Store, Spawner and Sessions of its in-process, demo and fixture
worlds.

## Built: a released window runs no Engine

A Cockpit reached each machine's Engine through `held::Link`, an enum of an
Engine in the window's process and a Server, matched at about fifty sites.
The Engine in the window's process is a test fixture now:
`cockpit::in_process`, compiled only under `cfg(any(test, feature =
"test-support"))` of `ferrite-client`. A build without that feature names
nothing on the engine's side. The tests keep what they had: `Ask::now`,
`engine()`, `engine/probe.rs`, mirror parity and every fixture. They do not
move to a Server in the same process, which step 2 had planned: a socket
between a test and its Engine buys a test nothing the encoded crossing of
each command, reply and message does not.

Two shapes were sketched:

- **A. One interface, two links.** `Link` is a trait: everything a window
  does with a machine's Engine. `remote::Remote` implements it over the
  socket and `in_process::InProcess` in the fixture. The window holds a
  `Box<dyn Link>` and calls it, so its functions have no in-process branch
  to gate. What only a Server does stays on `Remote`, reached through
  `Link::remote`.
- **B. One fixture module behind a gated variant.** `Link` stays an enum,
  its `InProcess` variant gated, and each match's in-process arm calls the
  fixture module. Every match then needs a `#[cfg]` on that arm, about
  fifty of them, or a macro that hides them.

A was built. The `cfg` stands at the module, its re-exports, the trait's
two `in_process` accessors, `Mirrors::watched` and the parity check in
`receive_from`. B would have left the seam in every function that acts.
A's cost is a trait of forty-odd methods with two implementations, and one
indirection: an act an Engine in this process carries out at once is taken
in through `Link::answers_now`, not a branch of its own. The fixture link
also holds what a fixture world's Server has besides its Engine, which
were fields of every Cockpit: the forge, the GitHub login, the Factory,
the free disk, the browser's answers.

- The demo is behind a `demo` feature, of `ferrite-cockpit` (on with
  `headless` and `visual-reference`) and of `ferrite` (on with `headless`):
  `cargo run -p ferrite --features demo -- --demo`. Release packaging turns
  on `cef` alone, so the shipped app has no `--demo`, `--load`, `--panes`
  or `--demo parity`. Asked for one, it says the build has no demo and
  exits 2, as for a wrong command line. `--panes` seeds the demo's Panes;
  on a Server it panicked at `Ask::now`.
- `server::clis` is `providers::clis` and `server::github` is `github`, on
  the engine's side: they are a Server's work, and no shared module names
  them now.
- The checker reads `cfg`s and `pub use`s, as above. Under its old rules,
  which skipped a file by its name and counted a `pub use` once, the base
  counted 30. Under the new rules it counted 33, and counts 8 now.

What is left is work, all of it reachable from a released window:
`Store::reader` in `Cockpit::hold`, which reads this computer's logs for a
Thread's header (`HeldMachine::logs`); the Spawner in `Attach::discovery`,
which discovers sessions and commands; and the titler, with the provider
program it runs (`cockpit::titling`).

## What comes next, in order

Each step is one PR and drives its pairs in the baseline to zero.

1. **Types move to the shared side.** Every crossing that is only a type
   or a pure function goes, from shared modules and the window crates
   alike. No crate changes. Done (above): what is left is work.
2. **A released window runs no Engine.** Done (above): the Engine in a
   window's process is a fixture, and the demo is a development build's.
3. **`ferrite-core` splits in two.** First the window's last work goes
   to its Server: the titler, session and command discovery, and the logs
   a window reads itself become questions over the protocol. With no
   crossings left, the shared modules become one crate and the engine
   modules another. The window's crates drop the engine crate, and
   `ferrite-server` is the one crate that depends on it.

## Considered

- **Layers by who names whom.** The first draft of this decision sorted
  core's modules into seven layers from their existing references and put
  the protocol at the top. That described the tangle instead of the
  system: `protocol` is the most-edited module in core and the one both
  sides must share, and 37 of that draft's 59 "upward" references were
  modules naming it.
- **Make the Engine's fields public for the moved tests.** Smaller diff,
  and every crate could then reach past the Engine's API. The probe keeps
  that to tests.
- **Leave the Engine-and-Cockpit tests in core.** Core cannot depend on the
  crate that holds the Cockpit, so they would need a second, test-only
  Cockpit.
- **Name the schema's module `log`.** `crate::log!` is already the
  diagnostics log, and `log::Record` reads as the `log` crate's type.
  `thread_log` names the Thread's log.

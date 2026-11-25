---
status: accepted, complete 2026-11-24 (the engine is the crate ferrite-engine)
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

It holds between crates: `ferrite-core` is the shared part,
`ferrite-engine` the engine and `ferrite-client` the client (the last
Built section below). Cargo refuses a path to a crate that is not a
dependency, and `scripts/ci_scope.py` refuses a window crate that links
the engine.

Until that cut the shared part and the engine were one crate,
`ferrite-core`, and a checker held the line between its modules.
`scripts/core-sides.json` put every top-level module of `ferrite-core` on
the shared side or the engine's. `scripts/check_core_sides.py` counted
each **crossing**: a path to an engine module from a shared module, or
from one of the window's crates (`ferrite-client`, `ferrite-cockpit`,
`ferrite-pane`, `ferrite-frame`, `ferrite-ui`, `ferrite-terminal`). CI
failed when a pair's count rose, a new pair appeared, or a module was on
no side, so the baseline only went down. What a released app could not
compile was not counted: a module whose `mod` line, or an ancestor's, had
a `cfg` that failed with `test` off and only the `shipped` features of
`core-sides.json` on (`cef`), and an inline `#[cfg(test)] mod` block. A
file's name skipped nothing. A `pub use` counted once for each engine item
it handed on, since every crate above could then name it without naming
the engine. The `ferrite` crate was not counted: it is the program that
starts both.

A crossing was a type living on the wrong side, or work. `store::Provider`,
`workspace::WorkspaceChoice` and the Factory's board were facts a window
shows, kept in the module that happened to produce them. The fix for those
was a move: the type went to a shared module and the engine module used it
from there. The rest were work: a window crate calling the engine's code.
Those became a question to the Server over the protocol.

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

## Built: a Machine's CLI work is its Server's

The Spawner in `Attach::discovery` and the titler were a window running
this computer's provider CLI for work that belongs to a Thread's machine.
On a Machine that was three bugs. A Draft's `/` menu and a Workflow Step's
skills ran this computer's CLI in a folder that is the Machine's. The
import picker listed this computer's session files and sent their paths
to the Machine's Server in `import`. A Thread on a Machine was titled by
this computer's CLI, from the window that sent its first prompt.

Discovery, two shapes:

- **A. Ask the Server.** Two commands: `list_commands {provider, cwd}`,
  replied `commands`, and `list_sessions {cap}`, replied `sessions
  {sessions, roots}`. The Server answers off its pump with its own
  Spawner: its CLI, its folders, its session roots. A window asks through
  `Link` and holds an `Ask<T>`, as it holds every ask (ADR 0041).
- **B. Run the window's Spawner on the Machine.** The window would send
  each CLI probe down the tunnel to run there. That is a second way to run
  a process on a Machine beside its Server, with its own errors and its
  own idea of the Machine's PATH and home.

A was built. `Remote` asks with the untimed `Remote::ask`, as `list_dir`
does: the Server's own deadlines (a command probe's 10 s, Codex's session
list's 10 s) would race `REPLY_WAIT`. The window's walk of session files
and its `session_file_roots` are gone. The Server lists the roots it
finds on its own computer; an Engine in the window's process, a fixture,
walks the roots a test gives it (`fixture_session_roots`) when its
Spawner lists none. A Spawner that lists no commands answers an error, not
an empty list: an empty list is what a Step's skill check reads as nothing
installed.

Titles, two shapes:

- **A. The Engine titles on the first prompt.** `deliver` marks a Thread
  owed a title when the operator's first prompt goes out. The pump asks
  `Spawner::title` once (`engine::titles`); production runs the Thread's
  Provider (`titler::write`). The answer is a rename (`rename_thread`)
  unless the Thread has a title by then or titles were turned off, so it
  reaches every window as a rename does. While it is out, `ThreadState`
  says `titling`, which the nav draws as it drew the window's own.
- **B. The window asks the Server to title.** A `title {thread, prompt}`
  command after a first send. Every window that sent one would ask, the
  Server would have to drop all but one, and a window closed before the
  answer would leave the Thread untitled.

A was built: no window asks. Whether to title is Settings' *Name threads
automatically*, now a Server default (`ServerDefaults::auto_title`) beside
follow-up suggestions on the Machine defaults group, so the Machine that
runs the titler is the one whose switch it reads. The window still reads
this computer's switch to show an untitled Thread's first-prompt name.

A Server from before these commands refuses them as `unsupported`. The
menu and the picker then say to update ferrited on that Machine; the
window never runs its own CLI in their place. Such a Server writes no
titles, so its Threads keep their first-prompt names, and Settings shows
*Name threads automatically* on for it whatever is sent: it keeps no such
default.

`Attach::discovery`, `Reach::defaults`, `cockpit::titling`,
`Preferences::titler` and the window's walk are gone. The crossings went
from 8 to 2: what is left is `Store::reader` in `Cockpit::hold` and
`HeldMachine::logs`.

## Built: a window reads this computer's Threads as a Machine's

A window on this computer opened its Store itself (`Store::reader`,
`HeldMachine::logs`) for what no message carried. A Thread on a Machine
already did without each read:

| Read | Who reads it | On a Machine |
| --- | --- | --- |
| an open Thread's header: Project, workspace | the Project label, Open in | its mirror |
| an idle Thread's newest prompt or turn end (`last_activity`, the log's tail) | the nav's age and order | `Digest.active_ms` |
| a working Thread's last write (`last_used`, the log's mtime) | Working's order on first sight, the empty board's recent list, the startup Project | `Digest.active_ms` |
| Main's whole history once the view dropped its earliest Blocks | Copy conversation | not offered |
| what its Engine's Activity retains | its mirrors' limits | the default |

Two shapes:

- **A. The Server's messages, and asks for the rest.** The header is the
  mirror's and an idle Thread's age is `active_ms`, the same times the
  log's tail carries. The last write is a new digest field, `written_ms`:
  the Server reads the log's mtime each time it folds a digest. The whole
  history is a new command, `read_transcript {thread}`, replied `text`,
  which the Server replays off its pump as it reads `turn_result`. The
  limits are passed in, a Store's defaults, as a Server's Store has them.
- **B. A read-only log reader on the shared side.** The log's schema is
  shared already, so `Store::reader` could move. It keeps two ways of
  reading each fact above, one here and one on a Machine, and a window
  reading files its Server writes.

A was built. B keeps nothing A loses: every read has a message, and the
Machine's way becomes the only way. Reading `active_ms` for a working
Thread too was considered and not built. A working Thread's newest
prompt can be an hour old while it streamed a second ago, so Working's
order, the empty board's recent list and the startup Project would move
on this computer.

What changes:

- On this computer, the nav's order and ages and every header read as
  before. `written_ms` is the mtime at the last fold, and a streaming
  Thread folds up to four times a second (`DIGEST_REFOLD`), so it trails
  the log only by what is written after
  its last fold: up to one flush interval when a stream stops between
  flushes, and a write no fold follows (a rename, a mark) until the next
  fold. `Store::last_activity` capped its time at the mtime; `active_ms`
  is the time itself.
- On a Machine, a working Thread's order and last use read `written_ms`,
  as this computer's do, and Copy conversation copies the whole history
  when the view dropped its start. A Server from before `read_transcript`
  refuses it as `unsupported`: the copy is what the view holds, headed
  to update ferrited on that Machine. One from before `written_ms` sends
  none, and `active_ms` stands in.
- A Store that cannot be read no longer stops the window at launch. The
  window reads none of it, so holding a machine cannot fail
  (`Cockpit::hold`), and the window shows this computer detached, as it
  does when its Server does not start.
- The mockups stamp each log's time to the scripted clock, as before, and
  have their Engine fold each digest again
  (`Engine::fixture_refold_digest`).

`Store::reader`, `HeldMachine::logs`, `Cockpit::history_log` and the
window's log reads are gone. The crossings went from 2 to 0.

## Built: the engine is a crate

The 14 engine modules are the crate `ferrite-engine`, moved whole: every
moved file keeps its lines but its imports and crate roots. `ferrite-core`
keeps its name, its directory and the shared modules.

| Crate | Holds | Depends on the engine |
| --- | --- | --- |
| `ferrite-core` | the shared part | never: Cargo refuses the cycle |
| `ferrite-engine` | the Store, the providers and their Sessions, the Engine, the Factory, workspaces, the API, `connection`, `github` | itself |
| `ferrite-server`, `ferrite` | the Server, and the app that runs it as `ferrite server`, the API's command line and the Store's panic rescue | yes |
| `ferrite-client` | the window model | only under `test-support` (`cockpit::in_process`), an optional dependency |
| `ferrite-cockpit` | the window | only under `demo` (on with `headless` and `visual-reference`), an optional dependency |
| `ferrite-pane`, `ferrite-frame` | the window's parts | in their tests only, through `ferrite-client`'s fixture |
| `ferrite-ui`, `ferrite-terminal`, `ferrite-factory`, `ferrite-tools`, `ferrite-nav`, `ferrite-settings`, `ferrite-prs` | the window's parts | never |

- No re-export shim: each path names the crate that owns its item. In the
  engine, `crate::<shared module>` is `ferrite_core::`. Elsewhere,
  `ferrite_core::<engine module>` is `ferrite_engine::`. `ferrite-engine`'s
  root re-exports nothing, and neither crate re-exports the other.
- What the steps above left `pub(crate)` for this cut is `pub`: 123 items
  and the `file_change` module. 83 of them are the log's schema, which the
  Store builds and matches. `ProjectId`'s field, `Project::minted`,
  `Projects`' fields with `parse` and `to_bytes`, and the five Factory
  board mutators say in their doc comment that they are public for the
  engine alone. `engine::probe` names `ThreadId` at core's root, so
  `thread` stays private.
- An inherent impl must live in its type's crate, and `Task` is
  `factory_board`'s. The Factory's two `impl Task` blocks are the traits
  `factory::Transitions` (`note`, `to_work`, `complete`, `enter`) and
  `factory::direct::Runs` (`asker`, `running`, `supersede`, `retire`,
  `label`, `decides_in_own_run`, `decides`). Their call sites read as
  before.
- The 38 integration tests of `ferrite-core/tests/it` that drive a
  provider's Session, the Store or the Engine moved with `support.rs` to
  `ferrite-engine/tests/it`. Six stayed. The provider captures in
  `tests/fixtures` moved with them, and the seven test files elsewhere that
  read one read the engine's copy. draft.rs's tests resolve a Draft against
  the registry, so they are `workspace::draft_tests`. The placement tests
  number their Projects as a new registry does.
- Each crate has its own `test-support`. `ferrite-engine`'s turns on
  `ferrite-core`'s, and a crate that turned on `ferrite-core/test-support`
  turns on `ferrite-engine/test-support` too.

Cargo replaces the checker. `scripts/ci_scope.py`'s `check_boundaries`
adds two rules. First, no window crate reaches `ferrite-engine` in what a
release build links. That is its normal and build edges, with an optional
one only when its default features or `cef` turn it on, following the
features each edge asks of the crate it names. Second, `ferrite-engine`
reaches no `gpui`. `check_core_sides.py`, `core-sides.json` and their
tests are gone.

An edit to the engine now rebuilds the engine, `ferrite-server` and the
app, and no window crate. Timed as `cargo check -j 3 -p ferrite --locked`
after a blank line is added to one file, warm, 6 runs on each side:

| Edit | Before | After | Rechecked after |
| --- | --- | --- | --- |
| `engine.rs` | 14.1 s (13.2 to 14.4) | 3.2 s (3.1 to 3.3) | `ferrite-engine`, `ferrite-server`, `ferrite` |
| `providers/claude.rs` | 12.9 s (12.4 to 13.3) | 3.3 s (3.1 to 3.5) | the same |

Before, Cargo rechecked `ferrite-core` (8.4 s), then the window's crates
one after another (`ferrite-ui`, `ferrite-pane` and `ferrite-cockpit`,
5.0 s), then the app. After, the engine's own check (2.0 s) is most of the
time, then `ferrite-server` (0.4 s) and the app (0.3 s). An edit to a
shared module still rebuilds every crate above `ferrite-core`, the engine
included.

## The steps

Each step was one PR and drove its pairs in the baseline to zero.

1. **Types move to the shared side.** Every crossing that is only a type
   or a pure function goes, from shared modules and the window crates
   alike. No crate changes. Done (above): what is left is work.
2. **A released window runs no Engine.** Done (above): the Engine in a
   window's process is a fixture, and the demo is a development build's.
3. **`ferrite-core` splits in two.** Done (above): the window's last work
   went to its Server, then the engine modules became `ferrite-engine`.
   The window's crates link it only in their tests and fixtures;
   `ferrite-server` and the app depend on it.

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
- **A re-export shim for the cut.** `ferrite-engine` re-exporting
  `ferrite_core::*`, or core re-exporting the engine, would have kept every
  path as it was and hidden which crate owns an item. Every path names its
  crate instead.
- **Free functions for the Factory's Task methods.** Each call site would
  have changed from `task.note(...)` to `note(task, ...)`, and each body
  from `self` to `task`. The two traits keep both as they were.
- **Name the schema's module `log`.** `crate::log!` is already the
  diagnostics log, and `log::Record` reads as the `log` crate's type.
  `thread_log` names the Thread's log.

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
| **shared** | the protocol's messages and the types they carry: Thread, Activity, transcript, Settings, the roster | both |
| **engine** | the Store, the providers and their Sessions, the Engine, the Factory, workspaces, the API | a Server |
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
`ferrite-ui`, `ferrite-terminal`). There are 284:

| From | Crossings | Mostly |
| --- | --- | --- |
| shared modules | 68 | `protocol` naming Factory, provider and workspace types |
| `ferrite-client` | 52 | workspace types, and the Engine a Cockpit can run in itself |
| `ferrite-cockpit` | 156 | workspace, provider, Factory and Store types |
| `ferrite-pane`, `ferrite-ui` | 8 | |

CI fails when a pair's count rises, a new pair appears, or a module is on
no side. The baseline only goes down. Tests and fixtures are not counted.
The `ferrite` crate is not counted: it is the program that starts both.

Most crossings are a type living on the wrong side. `store::Provider`,
`workspace::WorkspaceChoice` and the Factory's replies are facts a window
shows, kept in the module that happens to produce them. The fix for those
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

## What comes next, in order

Each step is one PR and drives its pairs in the baseline to zero.

1. **Types move to the shared side.** The 68 crossings from shared modules
   go first, then the window crates' crossings that are only a type. No
   crate changes. This is most of the 284.
2. **The Cockpit stops running an Engine in itself.** `Cockpit::new(store,
   spawner)` builds an Engine in the window's process. Production attaches
   to a Server (`Cockpit::attach`); the demo, fixtures and tests use the
   in-process one. They move to a Server in the same process, reached the
   way a real one is.
3. **`ferrite-core` splits in two.** With no crossings left, the shared
   modules become one crate and the engine modules another. The window's
   crates drop the engine crate, and `ferrite-server` is the one crate that
   depends on it.

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

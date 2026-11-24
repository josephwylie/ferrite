---
status: accepted
date: 2026-11-24
---

# Core is layers, and the window model is its own crate

`ferrite-core` is 160,000 lines in one crate. Rust compiles a crate as a
unit, so a one-line edit anywhere in it rebuilds all of it and every crate
above it. The modules inside also name each other freely: by `crate::`
paths they form one cycle, so nothing in it can be built, tested or
reasoned about alone.

The cycle is shallow. Put the modules in seven layers and all but 59 of
their references point down.

## The layers

Lowest first. A module may name its own layer and any layer below.

| Layer | Holds |
| --- | --- |
| base | clocks, channels, disk, process spawning, small leaf types |
| domain | Thread, Activity, transcript, roster, browser and delegate types |
| workspace | checkouts, worktrees, the PR watch |
| store | the Store, Settings |
| sessions | the providers and their Sessions |
| engine | the Engine, the Factory, suggestions, titles, notifications |
| wire | the protocol, the API, links, Machines, the Server's half |

Above them all sits the window model, the Cockpit a window holds of its
Machines' Servers. Nothing in the layers names it.

`scripts/core-layers.json` is the list: every top-level module of
`ferrite-core` and its layer. `scripts/check_core_layers.py` counts each
`crate::<module>` path that points up, per pair of modules, and CI fails
when a pair's count rises, a new pair appears, or a module has no layer.
The baseline only goes down. Tests are not counted, and a name re-exported
at the crate root is not seen.

## The first cut: `ferrite-client`

The window model is now the crate `ferrite-client`: 7,300 lines and as
many again of tests, with no UI in it. It was the top of the stack, so
nothing below had to change shape to let it go. It is also where the window's work lands: every change to
how the window asks a Server for something (ADR 0041) edited it, and each
edit rebuilt the Engine, the Store and the providers with it. Now it
rebuilds itself and the window.

- `ferrite-core` cannot name the window model. Cargo refuses the cycle.
- `ferrite-server` builds without it. Its tests drive one, so it is a
  dev-dependency there, and `scripts/ci_scope.py` fails a runtime one.
- `ferrite-client` draws nothing: the same rule that keeps `gpui` out of
  core keeps it out here.

The tests that drive a real Engine through a Cockpit moved with the
Cockpit. They read a few of the Engine's and the Store's insides. Those
seams are `engine/probe.rs` and the Store's fault switches, public only
under the `test-support` feature. A test in another crate reaches the
Engine through them and nothing else.

The cut has a cost. About thirty items the Cockpit used as `pub(crate)`
are now `pub`: the roster's mutators, the `ThreadKey` and `ProjectKey`
constructors, a few Engine and Store reads. A window crate can now call
them where before only core could. The Cockpit stays their one caller by
convention until the wire layer leaves and takes most of them with it.

## What comes next, in order

Each cut is one PR, and each first drives its pairs in the baseline to
zero.

1. **wire** leaves next, as a crate over the Engine. Thirty-seven of the
   59 references stand in the way. Most are wire types used as domain types
   (`protocol::WirePng`, the Factory's and the Engine's replies): the type
   moves down to the layer that owns the fact, and the protocol carries it.
2. **base** and **domain** leave from the bottom, as the crate everything
   else builds on. `thread -> workspace` (`ProjectId`) and
   `activity -> pr_watch` are type moves.
3. **store**, **sessions** and **engine** part last. `store -> runtime_mode`
   and `providers -> api` are the knots: the Store takes its mode as a
   value, and the providers take their tools as a trait they own.

## Considered

- **Split from the bottom first.** It is the larger win for build time,
  since the bottom changes least. It needs the upward references in the
  domain layer gone first. The top came free.
- **Make the Engine's fields public for the moved tests.** Smaller diff,
  and every crate could then reach past the Engine's API. The probe keeps
  that to tests.
- **Leave the Engine-and-Cockpit tests in core.** Core cannot depend on the
  crate that holds the Cockpit, so they would need a second, test-only
  Cockpit.

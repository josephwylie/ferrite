# 0018 — Revival prepares off the UI thread, staggered at launch

Status: accepted 2026-10-15 (Herdr benchmark, Hygiene).

## Context

A launch reopens every Thread whose Pane was open (`revive_launch_threads`)
before the window exists. `Cockpit::revive` made each worktree-bound
Thread's workspace real first (`ensure_workspace`): `git worktree list` for
the adoption check, or `git worktree prune` and `git worktree add` for a
tree deleted while Ferrite was off. Those calls ran on the UI thread, one
Thread after another, and the window waited for all of them. Every CLI then
started in the same instant, contending for CPU, disk and the providers'
own startup work.

The CLI itself already started off the UI thread: the production spawner
returns a starting `SessionLifecycle` whose worker spawns the process, and
the pump adopts it when ready or shows "could not start" in the Pane.

## Decision

- `Spawner::start_after(prepare, request)` takes a `Preparation` — work a
  Session's start waits on and its caller should not — and runs it before
  the start. The default runs it inline, so scripted and demo spawners keep
  their synchronous behaviour unchanged; the production spawner runs it on
  its spawning thread, ahead of the CLI.
- `Cockpit::revive_after(thread, delay)` builds that preparation: sleep
  `delay`, then the worktree's git half (`placement`: the adoption check
  for a tree standing, `place_worktree` for one of Ferrite's own that is
  missing). The branch is looked up from the registry on the caller's
  thread; nothing the closure touches needs the Cockpit. `revive` is
  `revive_after` with no delay.
- Every later start of an existing Thread's Session — the send after a
  Session ended, the memory watchdog's restart, a model, effort or
  provider change — carries the same `placement`. A worktree a revival
  could not make is tried again by the next start, never spawned into
  while missing.
- A launch revives the n-th Thread with `n × LAUNCH_STAGGER` (100 ms):
  every Pane opens at once, in its starting state, and the CLIs follow one
  by one.

## Consequences

- A worktree that cannot be made (a squatting directory, a refused `git
  worktree add`) no longer refuses the revive. The Thread opens, and its
  start fails in the Pane like any CLI that will not start, and in the
  log (ADR 0019). The next send tries again, so a squatter removed or a
  git lock released needs no close and reopen. With a scripted spawner the
  old synchronous refusal stands.
- The same holds for each member of `revive_group`: a member whose
  worktree cannot be made no longer rolls the Group back. The Group opens
  whole, that member's Pane saying "could not start". A member whose log
  will not load still rolls it back, as before.
- Every restart of a worktree-bound Thread asks git once (`git worktree
  list`, the adoption check) on the spawning thread, never the UI's.
- Several revivals may place worktrees in one repository at once.
  `workspace::ensure_worktree` takes a process-wide lock around its
  `prune` and `add`: git marks a half-made worktree "initializing" only
  after creating its admin directory, and a `prune` in that instant would
  remove it.
- `open_choice` (a new Thread) still makes its worktree inline: its rollback
  (registry entry, store) depends on knowing the outcome before the Thread
  exists. Revival has nothing to roll back.

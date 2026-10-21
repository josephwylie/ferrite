---
status: accepted
date: 2026-10-20
---

# Sessions follow attention

## Context

The settled decision "open Pane = live Session, no hidden eviction" was cheap
while quitting ended every agent. ADR 0025 made the Server revive every open
Thread at boot, so 25 Panes left open meant 25 provider CLIs at login
(130–240 MB each, ~3 GB), plus each Thread's whole history replayed into the
Server and mirrored, in full, into every window. A heap profile of that boot
(25 open Threads, 644 MB of logs) found 817 MB live in the Server and 459 MB
in the window. 213 MB and 336 MB of the Server's were retention bugs (fixed
separately, PR #130). The rest was every open Thread held in full, twice,
whether or not anyone was looking at it.

## Decision

A Thread costs memory and a process only while it is used. Three things are
decided independently:

- **Whether its Session runs.** Boot starts none, except to deliver a queued
  prompt. A Session starts on a send, a control that needs one, or the first
  keystroke in the Thread's Composer. It ends after 10 minutes settled: no
  turn, queued prompt, Decision, working Subagent or background task. A
  Session that has scheduled work Ferrite cannot observe stays up until it
  exits: Claude's `ScheduleWakeup`, `CronCreate`, `Monitor` or an ambient
  task; a Codex goal that is `active`.
- **How much the Server holds.** A Thread with no Session that nobody
  watches holds its digest only, and is read back from its log when it is
  watched, sent to or read. Every other Thread holds today's full
  projection. (Amended 2026-10-20: a Session nobody watches no longer keeps
  only a short tail. The digest's changed files and the API's read need the
  whole projection, and such Threads are few now that Sessions follow use.)
- **What a window mirrors.** Only Threads drawn at Transcript level. Every
  other surface (nav rows, Wall cards, the bell, toasts, needs-you) reads a
  per-Thread digest the Server computes and sends to every window; Decisions
  already go to every window. Open no longer means mirrored. A window
  unwatches what it stops drawing. (Amended 2026-10-21: five minutes after
  it stops drawing it or the Thread stops working, whichever is later — a
  turn, Subagents or background tasks — so the operator comes back to a
  Thread as it is, never to a reload.)

Ending and restarting a Session is invisible. Waking adds no transcript row
and changes no status, unlike a revive or a Session that closed. A prompt sent
while the Session starts is queued, not refused. Slash commands, permission
modes, the model list and MCP status are served from the last live Session's
answers. A model, effort or Provider change on a Thread with no Session is
recorded and takes effect at the next start. The same rules apply to another
machine's Server (ADR 0026).

Watching reads history from the log's bounded base, as revive does. (Amended
2026-10-20: no paging on scroll yet. A window's mirror trims to the limits the
base is chosen for, so a bounded watch shows what a whole-log watch showed.) There is no new snapshot file. Finished tool
results keep their bounded preview, not the provider's whole JSON value.

## Considered options

- **Keep every open Thread's Session, and cap the count.** That would evict a
  Thread the operator expects to be live. It also leaves the Server's and the
  window's copies in place.
- **A hot/warm/cold state per Thread.** One state can't describe a Thread
  that works while nobody watches: it needs a Session and Server state, but
  no window mirror.
- **A serialized Activity snapshot per Thread.** Nothing in Activity or
  Transcript serializes today, and the bounded base already makes revive
  ~26 ms, and a mirror keeps no more than the base holds.
- **A visible "asleep" state.** The operator has nothing to do about it; a
  Session is an implementation detail of a Thread.

## Consequences

- A wake costs a cold start: under a second for Claude, more with remote MCP
  servers, several seconds for Codex. The Composer keystroke hides most of it.
- Scheduled work Ferrite can't see is protected only by the tool names above.
  A new provider mechanism will need adding there.
- Ferrite can no longer assume an open Thread has a live Session. Code that
  reads one must use the cached answers instead.
- Supersedes ADR 0018's launch stagger, since boot starts no Sessions to
  stagger, and ADR 0027's `agent_starting` refusal, since a second prompt
  now waits behind the first.

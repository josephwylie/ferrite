---
status: proposed
date: 2026-09-05
---

# Subagent activity within a Thread

Use one headless `Activity` per Ferrite Thread to own Main and subagent
transcripts, identity, parentage, status, tool timings, and Decisions.
Provider adapters attribute observations before any Main bookkeeping;
GPUI selects and renders the resulting Subject. This keeps routing and replay
out of the Pane and avoids duplicating them across existing Main consumers.
Requirements, provider evidence, and review findings live on
[issue #1](https://github.com/josephwylie/ferrite/issues/1).

## Ownership and identity

- A Subagent is observed through its owning Session. It does not acquire a
  Ferrite Thread, Session, worktree, or Roster entry. Selecting it sends no
  provider prompt, start, or resume command.
- [`Activity::apply/view`](../../crates/ferrite-core/src/activity.rs) owns
  per-Subject projections and returns accepted facts, scoped block changes,
  completed tool durations for persistence, identity redirects, and Main-only
  lifecycle signals. Cockpit retains the
  Session, provider settings, workspace, Title, Composer queue, and Store.
- `AgentKey` is namespaced by provider and root conversation. Native aliases
  join only with explicit correlation evidence; names and equal text are not
  identity. Preserve direct parentage and show only verified descendants.
  Session generation rejects late observations without changing durable keys.
- Claude spawn-tool, resumed-tool, task, and agent IDs can refer to the same
  child. Keep all aliases. Codex child IDs and `parentThreadId` establish
  ancestry; a child's changing `sessionId` is not a durable grouping key.
- Reported status, latest outcome, freshness, and transcript coverage are
  separate. Completion permits later work. History or disconnection cannot
  create fresh Working status; silence never implies idle.

## Decisions and Main isolation

Decisions form a collection of opaque handles bound to the live Session,
its generation, and an attributed Subject where known. Unattributed requests
remain visible and Session-owned. Answer the identified handle, never whichever
request happens to belong to the selected tab; preserve Codex wire-ID types.
Replacement, park, Handover, watchdog, and disconnect invalidate old handles.

Main retains prompting, model/effort changes, and interrupt. Child completion,
usage, and requests cannot change Main's resume identity, interrupt target,
accounting, or queued prompt. Autonomous Claude background Main results also
cannot release a foreground Main queue. Child views support observation,
answering actual requests, and one execution control: stop.

## Escape stops what the Pane shows

Escape on Main interrupts Main's current turn and nothing else: Subagents,
background agents, background shell tasks and workflows run on. Escape on a
Subagent's tab stops that Subagent alone (`Session::stop_subagent`, the
`stop_subagent` op); on a finished Subagent it does nothing, and never falls
through to Main. Nothing Ferrite sends kills every agent at once.

- Claude's CLI kills every background agent and workflow on an `interrupt`
  unless the host declares `perTaskStopAffordance: true` at `initialize`
  ("ABSENCE also fails closed", 2.1.299). Ferrite declares it: it does stop
  tasks one by one (the background shelf's ×, a Subagent's escape). Confirmed
  by capture on 2.1.299: without it, an interrupt answered a `stopped`
  notification for a background agent and killed its shell; with it, the
  agent and a background `sleep` survived while Main's turn ended
  `error_during_execution`. A Subagent stops by `stop_task` with its task id,
  the one alias that names it to the CLI; one with no task id is refused,
  never guessed.
- Codex children run their own threads. `turn/interrupt` names a thread and
  its turn, so Main's interrupt cannot reach a child; a child stops by
  `turn/interrupt` on its own thread's running turn, and one running no turn
  is left be. An interrupt leaves the turn's command running (0.163.0: an
  interrupted child's `sleep` ran to completion, and Main's own did too), so
  stopping a child also sends `thread/backgroundTerminals/clean` for its
  thread; Main's survivors stay visible as background terminals instead.
  Main's interrupt leaves children running, including one Main was `wait`ing
  on. A stopped child sends Main no completion, so a `wait` on it does not
  return (over 240 s observed) until Main is interrupted.

## Persistence and recovery

[`Store`](../../crates/ferrite-core/src/store.rs) writes schema 9 using its own
record types. Persist accepted attributed facts with actor and content identity;
never persist actionable Decision handles. Old schemas 1–8 remain Main-only.
Child terminal facts flush even after Main is idle; attributed chunks never
coalesce across actors or items. Failed writes retain records and accepted byte
offsets for retry. Cockpit surfaces errors and holds draining and queue release.

Restore chronologically through Activity in history mode, without provider
commands, queue sends, or re-recording. Child prompts/content never enter Main
prompt recall or the Handover digest. Handover preserves earlier child history
under its original provider namespace without resuming those children.

Render caches and pending history tails are bounded; identities and Decisions
survive cache eviction. One Cockpit disk worker restores an evicted Subject:
flush a byte checkpoint, scan aliases then bounded child content from that
prefix, and replay the separately buffered live tail once. Generation, request
identity, and header changes invalidate obsolete loads. Disk reads stay off
the paint path. Each scan retains one encoded line and decoded record; an
individual line has no hard size cap. A revive reads only from a replay
base onward ([ADR 0009](0009-store-off-the-ui-thread.md)). Children known from
before the base come back with identity, status and outcome but no content,
and this worker restores them when shown. Omitted saved content remains on
disk and is marked Partial.

Claude forwards complete child frames with `Live` coverage. Preserve outer
frame/block identity, because multiple frames can share an inner message ID;
forwarding does not supply complete historical child input. Ferrite restores
its saved observations without adding a Claude sidecar-history reader.

Codex reads verified child histories without automatically resuming children.
Merge settled full items by native identity only where newer live content has
not intervened. Active or summary snapshots cannot erase live output; older
items without a proven insertion position remain omitted with Partial coverage.
Readable `notLoaded` history does not mean live observation recovered.

## GPUI presentation

Use native GPUI kit `Tab` and `TabBar` with underline styling, a 120 ms movement
spring, and no close buttons or aggregate waiting text. Key the tab subtree by
Thread and ordered visible Subjects, retaining focus and held-key state per
Subject outside it, so reorder cannot retarget an in-progress interaction.
Working agents sort first, retaining discovery order within each group. Main
stays reachable; overflow appears only when measured header space is exhausted.
Show waving dots only for visible, fresh Working agents; honor reduced motion.

Selecting a child replaces the transcript and displayed name without renaming
the Thread. Keep scroll, follow-tail, disclosure, parsed text entities, and
Main's draft per Subject. Native selection ranges follow GPUI kit's normal
clearing on navigation; the toolkit exposes no public range restore API.
Scope block, highlighting, and view caches by Subject and content revision so
late work cannot update another transcript. Hidden requests remain reachable
through Decision navigation. The kit migration is recorded in
[ADR 0003](0003-native-gpui-components.md).

## Amendment (2026-10-21): live-only tabs and the done menu

The UI refresh ([ADR 0033](0033-sans-chrome-and-card-panes.md), SPEC §5.3)
changes the strip. Tabs are `Main` plus one per *live* Subagent (waiting,
working or failing), ordered waiting, working, failing, each by start; a
selected tab stays until the operator leaves it, even once its Subagent is
done. Every other Subagent (finished, stopped, idle) lives in an `N done ▾`
menu, newest first, with its result word; picking one selects its tab. When
the live tabs overflow the strip, their tail folds into the same menu under
a `Live` heading and the button reads `N more ▾`; `Main` and the selected
tab always show. Tabs keep a fixed width while shown, and still carry no
close button.

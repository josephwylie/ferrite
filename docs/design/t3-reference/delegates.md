# T3 Code delegated tasks: what the operator sees

Source: pingdotgg/t3code at `0fd48228`. Paths are from the repository root.
The Server side (the record, the follower, the wake and the agent's tools)
is ADR 0047; this page is the window's part, and where Ferrite draws each.

## The parent's Lineage row

`apps/web/src/components/chat/ThreadRelationshipsControl.tsx`

- `:289-358` (`RemoteSubagentRow`): one `h-8` row per task in a linked
  environment. The provider's icon with its status dot, the title
  (`Subagent` for none), then `on {label} · {Status}` at the right, in
  destructive ink when it failed. Its tooltip is the title over `Open on
  {label}`, or `Read the thread on {label}` when it is read through the
  linking environment. The press opens the child there; with no opener it
  is disabled.
- `:616-648`: open tasks follow the thread's own active agents; finished
  ones go under the folded `Previous agents`.
- `:649-652`: the heading's `runningCount` counts what is `running`, a
  remote task's status included.
- `:1006-1033`: a local subagent's row has `Stop subagent`, a destructive
  ghost icon at its right, shown on hover. A remote row has none.

`apps/web/src/components/chat/ThreadRelationshipIcon.tsx`

- `:13-52`: the status words: `Starting`, `Running`, `Waiting`, `Done`
  (completed), `Failed`, `Stopped` (cancelled or interrupted).
- `:96-106`: the dot is `info` while it starts, runs or waits,
  `destructive` when it failed, `success` when it completed, else muted.

Ferrite: `delegate_row` in `cockpit/details/lineage.rs`, from
`ThreadFacts::delegating`, the press through `open_elsewhere`
(`cockpit/delegates.rs`). The stop at its right is the operator's
addition, drawn as T3's local stop; under the pointer the row also says
what holds it up. A completed row's dot is the idle dot (Done is a shape,
not a hue). Ferrite has no read through another machine, so its hint is
always `Open on pc`.

## The child's Lineage row

`apps/web/src/components/chat/ThreadRelationshipsControl.tsx:364-463`
(`DelegatedFromRow`): the corner-left-up icon, the parent's title, and `on
{label}`, the environment's own label when this client knows it, else the
one the task was sent with. Its tooltip is `Open parent agent on {label}`,
or `Delegated from {label}. Connect to it to open the parent thread.`, and
it is disabled while not connected.

Ferrite: `delegated_from` in `cockpit/details/lineage.rs`, from
`ThreadFacts::delegated_from`, the press through `open_elsewhere`.

## Working while one is open

- `apps/web/src/components/chat/ThreadActivityBanners.tsx:197-330`: the
  background line lists the provider's pending background tasks; a remote
  task is not one of them.
- A thread with an open delegated task does not settle (ADR 0047, T3's
  `untilDelegatedWorkSettled`).

Ferrite: `Delegating::holds`, in the Thread's `Digest`, makes
`ActivityView::background_work` true, so the Thread stays on the Working
shelf and its Notice waits. By operator decision the background line lists
each open Delegate as `Fix the tests on pc` (`running_work`,
`cockpit/background_work.rs`).

## The wake in the transcript

`apps/web/src/components/chat/TimelineWorkEntryRow.logic.ts:154-170`: a
notification's icon is `bot` for a `delegated_task`, `eye` for a `monitor`,
and `circle-alert` when its outcome is `failed`.

Ferrite: `wake_icon` (`ferrite-pane`), by the wake's `wake::Source`.

## The cancel's words

`apps/server/src/peer/RemoteDelegation.ts:1008`: a cancel whose stop did
not get there reads "It could not be stopped in {label} and may still run
there: …". Ferrite says `on pc` for T3's `in {label}` (ADR 0047).

## Not drawn yet

- `apps/server/src/orchestration-v2/Orchestrator.ts:10416-10443` and
  `apps/web/src/components/chat/V2LifecycleRow.tsx:434-560`
  (`SubagentTimelineLink`): the parent's timeline has a subagent row for
  the task, `on {environment}`, that opens it. Ferrite's parent shows the
  `delegate_task` call as a tool row.
- `apps/web/src/components/chat/MessagesTimeline.tsx:1209-1243` and
  `ChatView.tsx:923-990`: the child's timeline opens with `Subagent of
  {title} on {environment}` and `Open parent thread`.
- `apps/web/src/components/Sidebar.logic.ts:594`: the sidebar nests a
  delegated thread under its parent. Ferrite's nav keeps it an ordinary
  row on its own machine (operator decision).

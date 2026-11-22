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
  ones go under the folded `Previous agents`, after the finished agents.
  Remote tasks keep the projection's order: by when they started.
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
`ThreadFacts::delegating`, oldest first, the press through `open_elsewhere`
(`cockpit/delegates.rs`). Its untitled row says `Delegate` (ADR 0047's
noun). The stop at its right is the operator's addition, drawn as T3's
local stop; under the pointer the row also says what holds it up. A
completed row's dot is the idle dot (Done is a shape, not a hue). Ferrite
has no read through another machine, so its hint is always `Open on pc`.

## The child's Lineage row

`apps/web/src/components/chat/ThreadRelationshipsControl.tsx:364-463`
(`DelegatedFromRow`): the corner-left-up icon, the parent's title, and `on
{label}`, the environment's own label when this client knows it, else the
one the task was sent with. Its tooltip is `Open parent agent on {label}`,
or `Delegated from {label}. Connect to it to open the parent thread.`, and
it is disabled while not connected. Its press reads the parent's title
there first: a failed read says `Could not open the parent thread` (an
error toast), none `The parent thread was deleted` (an info toast).

Ferrite: `delegated_from` in `cockpit/details/lineage.rs`, from a
`Delegator` (`cockpit/delegates.rs`): the parent's title as its machine
says it now, else as it was sent, and its machine by its saved name, else
the label its Server sent, never the nav's `local`. The press is
`open_delegator`: the machine's status link says whether its Server still
has the Thread (`ServerStatus::holds`), and the nav's notice says T3's
words as a problem or a hint.

## The parent's timeline row

`apps/web/src/components/chat/V2LifecycleRow.tsx:434-560`
(`SubagentTimelineLink`), `apps/server/src/orchestration-v2/Orchestrator.ts`
(the title's `taskTitle`) and
`apps/web/src/components/chat/MessagesTimeline.logic.ts:1409`
(`withoutSubagentDelegationRows`): the task's
row replaces its `delegate_task` call (kept when the call failed). The bot,
its title (else its task, cut past 72 characters to 69 and `...`), `on
{environment}`, then its result once settled, else its progress, else its
status label, and its elapsed time from its start and end. The status
label shows at the right while it has a detail and is not completed. Its
press opens the child.

Ferrite: `ToolKind::Delegate` (`transcript/tools.rs`) draws the call as an
agent row (`pane/agents.rs`), titled by `delegate_title`; its face is the
Delegate's (`CockpitView::delegate_faces`, by `delegate::started_by`), `on`
its machine (`AgentFace::on`), opening through `open_elsewhere` while this
window reaches it.

## The child's divider

`apps/web/src/components/chat/MessagesTimeline.tsx:1209-1243` and
`ChatView.tsx:923-990` (`TimelineSystemDivider`): hairline, a rounded pill
with the bot, `Subagent of` and `· {title} on {environment}` at 70%, then
hairline, under the history controls. Its tooltip is `Open parent thread`,
and its press opens the parent.

Ferrite: `RowKind::Parent` and `parent_divider` (`ferrite-pane`'s
transcript), from `TranscriptPresentation::parent`, which the cockpit
fills from the `Delegator` (`delegator_link`). It says `Delegate of`
(ADR 0047's noun) and has no press while the parent's machine is not
reached.

## A child waiting on the operator

`apps/web/src/components/chat/ThreadActivityBanners.tsx:52-92`
(`childInputNowItem`) and `packages/client-runtime/src/state/threadShell.ts:204-227`
(`childThreadInputsTitle`): a line at the timeline's end, first among
its lines, with ShieldQuestion (approval) or MessageCircle in info ink,
`Subagent needs approval` (`input`, `you`; `{n} subagents need …`), the
children's titles as inline buttons, and `Open approval` (else `Open
question`). Hidden while the parent itself waits on the user.

Ferrite: `delegate_waiting_tail` (`cockpit/delegates.rs`) over
`waiting_delegates`, which reads each machine's Notices and, off screen,
its digests for nested Delegates, drawn with `pane::now_line`. Each press
opens the Delegate's Thread at its request on its machine (`go_to_alert`).

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
`packages/provider-core/src/server/notification.ts:191`
(`delegatedCompletionNotification`): `Delegated task "{label}"
finished|stopped|failed`, or `{n} delegated tasks {verb}: a, b`, the label
the title's first line (else the task's), cut at 80.

Ferrite: `wake_icon` (`ferrite-pane`), by the wake's `wake::Source`; the
words are `Delegates::wake`'s, in core's `delegate.rs`.

## The cancel's words

`apps/server/src/peer/RemoteDelegation.ts:1008`: a cancel whose stop did
not get there reads "It could not be stopped in {label} and may still run
there: …". Ferrite says `on pc` for T3's `in {label}` (ADR 0047).

## Not drawn

- `apps/web/src/components/Sidebar.logic.ts:594`: the sidebar nests a
  delegated thread under its parent. Ferrite's nav keeps it an ordinary
  row on its own machine (operator decision).

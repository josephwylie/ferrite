# T3 Code PR watching: what the operator sees

Source: pingdotgg/t3code at `0fd48228`. Paths are from the repository root.
The Server side (the watcher, its reads, the wake and the agent's tools) is
ADR 0046; this page is the window's part, and where Ferrite draws each.

## The details card's eye

`apps/web/src/components/chat/ThreadDetailsPrRow.tsx`

- `:456-457`: a PR shows as watched when the card was given a stop
  (`onStopWatching`) and the PR is open.
- `:458-493`: the eye is a ghost control, `aria-label` `Stop watching
  #{number}`, in the `group/watch` group. `EyeIcon` hides and `EyeOffIcon`
  shows while the pointer is on it (`size-3.5` beside the checks). Its
  tooltip, on top: "Watching: the agent wakes when checks finish, someone
  comments, or the branch conflicts. Click to stop."
- `:531-546`: the quiet line under the title (`h-5`, `ps-9`) shows when
  there are checks, a trailing action or a watch: `#N`, the checks count,
  then the eye (`watchControl("icon")`).
- `:571-576`: with no detail read yet, the row and the eye are a split
  group (`watchControl("secondary")`).

`apps/web/src/components/chat/ThreadDetailsPrRows.tsx:96-111`: the stop is
`watchPullRequest` with `watching: false` for that PR. A press does nothing
else.

Ferrite: `PrBlock::watched` and `watch_eye` (`cockpit/details/content.rs`),
the press through `watch_pr` (`cockpit/pr_watching.rs`). Ferrite's card
always has the PR's detail when it draws the block, so the split group has
no counterpart.

## The PR menu

`apps/web/src/components/pullRequest/ThreadPullRequestsPanel.tsx:375-380`:
an open PR's row menu has `Watch for changes` (eye) or `Stop watching`
(eye-off), when the thread can start a watch or one is on.

Ferrite: the PR link's context menu (`cockpit/context_menu.rs`), and the
palette's `Watch PR for changes` and `Stop watching PR`
(`cockpit/palette.rs`). A start needs the Server's watcher
(`Cockpit::starts_pr_watches`).

## Working while a watch holds

`packages/shared/src/orchestrationV2PendingBackgroundWork.ts`

- `:332-356`: each watch is a pending background task, kind `monitor`,
  described `Watching pull request #N`. It is `awaited` unless every check
  has finished, `until` is not `merged`, and the PR has not merged: "Once
  every check finished, a person moves the PR next".
- `:102-118`: an awaited task holds the thread's completion; a monitor
  that is not awaited does not.

`apps/server/src/orchestration-v2/ThreadSettlementService.ts:171-174`: a
thread with a held completion does not settle, and a watched thread never
auto-settles ("Settling ends a watch").

Ferrite: `Watching::holds` (`ferrite-core/src/pr_watch.rs`), carried in the
Thread's `Digest`, makes `ActivityView::background_work` true, so the Thread
stays on the Working shelf, its background line says `Watching pull request
#N`, and its Notice waits.

## The wake in the transcript

- `apps/web/src/session-logic.ts:611-628`: a `notification` item is a
  `work` entry, labelled with its `summary`, tone `info`.
- `apps/web/src/components/chat/TimelineWorkEntryRow.logic.ts:154-175`: its
  icon is `eye` for a monitor's, `circle-alert` when its outcome is
  `failed`.
- `apps/web/src/components/chat/MessagesTimeline.logic.ts:975-980`: it
  starts a response, as the operator's message does.
- `apps/web/src/components/chat/TimelineWorkEntryRow.tsx:231-251, 283+`:
  it draws through `WorkEntryLogRow`, a compact log row that expands to its
  detail on a press.

Ferrite: a Block whose `wake` is set draws as that row
(`ferrite-pane`), never as the operator's bubble.

# Subagents and background work audit

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants a complete, source-backed account of missing subagent views, unexpected background-work navigation, and Ferrite behavior that falls short of current T3 Code. This report traces agent identity through provider events, retained history, transcript entry points, Panel tabs, notifications, background work and stop controls.

## Overview

Result: ISSUES. Nine findings, three P1 and six P2. None is runtime-reproduced. Ferrite has substantial subagent infrastructure, including provider-scoped identity, direct ancestry, child transcripts, independent decisions, interruption and stored-history recovery. The main failure is the integration of those capabilities into the new Panel route. A click on an agent row opens a compact read-only transcript that bypasses several capabilities still present in the older full-Pane route.

The phrase "unexpectedly lands on background work" has two source-defined explanations: opening a generic thread notice chooses the first pending child's Subject, and restarting drops Agent tabs and chooses a surviving neighboring tab, which can be Background. These are concrete triggers, not proof that every reported spontaneous navigation has this cause. Searches found Background tab openers in explicit transcript actions, the Panel menu and Overview task rows. No unsolicited background-status event that directly opens this tab was found.

## Method and boundaries

Ferrite HEAD was verified as `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. T3 was inspected in `/tmp/ferrite-t3-audit-source-20261113` at the supplied baseline `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, with origin `https://github.com/pingdotgg/t3code.git`. All T3 links below are commit-pinned. Ferrite citations are repository-relative. The Codex adapter, investigation instructions in poteto-mode, and unslop writing instructions were read. ADRs 0038, 0039 and 0040 were read as intent. Historical T3 specifications were not used as evidence of current T3 implementation.

This was source inspection, using broad `rg` searches followed by line-numbered reads of implementations and tests. No app was opened, no workers were launched, no application data was changed, and no application tests or builds were run. Only this report and its issue JSON were written. Existing tests cited here were inspected, not executed. A code path's existence does not establish that the installed binary reaches it.

## Key concepts and source map

| Concept | Ferrite implementation | Consequence |
| --- | --- | --- |
| Agent identity | `crates/ferrite-core/src/activity.rs:15-39` | AgentKey combines provider, root session and native identity. A child is a Subject within a Ferrite Thread. |
| Ancestry and coverage | `crates/ferrite-core/src/activity.rs:43-76` | Direct parent, native name, description, role and transcript coverage are stored independently. |
| Aliases | `crates/ferrite-core/src/activity.rs:2186-2249` | Canonicalization merges native aliases; named_agents counts each actual child once. |
| Full-Pane navigation | `crates/ferrite-cockpit/src/cockpit/subagents.rs:376-438` | Selects Subject, hydrates saved history, records visibility and focuses transcript instead of Main composer. |
| Primary agent-row route | `crates/ferrite-cockpit/src/cockpit.rs:2121-2123`; `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:59-110` | Resolves invocation to Subject and creates a Panel Drill. Main stays visible. |
| Compact Panel rendering | `crates/ferrite-cockpit/src/cockpit/side_chat.rs:477-574` | Shared with side chats; intentionally omits decision strip, child mappings, rewinds and reply. |
| Stored child recovery | `crates/ferrite-core/src/engine/history.rs:110-172` | Rehydrates an evicted child from Ferrite logs without resuming its provider. |
| Background summary/dialog | `crates/ferrite-cockpit/src/cockpit/background_work.rs:28-97,112-239,314-384` | Builds its own list of progress tasks and agents. |
| Background tab | `crates/ferrite-cockpit/src/cockpit/panel/background.rs:29-129` | Uses Main background tool calls and shows Running/Finished groups. |
| Background navigation | `crates/ferrite-cockpit/src/cockpit.rs:2157-2160`; `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:165-171` | Explicit user events open the tab. |
| Notification navigation | `crates/ferrite-cockpit/src/cockpit.rs:12274-12327` | Generic thread-open may select a child; exact decision-open selects its owner. |

## Traced flows

### Codex discovery, inspection and interruption

Codex discovers children from explicit parent metadata or collab spawn items, uses a provider/root/native key, and refuses silent reparenting when metadata conflicts. It extracts nickname, preview and role. It reads child metadata using `thread/read`, then pages `thread/turns/list`, with a finite read budget and explicit partial coverage. It does not resume a child to view it. See `crates/ferrite-core/src/providers/codex/activity.rs:505-613,630-735,795-904` and the history-only test at `:1735-1760`. This is real positive capability, not an absent provider integration.

An invocation alias links the parent spawn row to the child. Main rendering computes agent faces and passes them into transcript inputs at `crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:49-96` and `crates/ferrite-cockpit/src/cockpit.rs:1912-1930`. Clicking the row resolves it through Activity.invocations, then opens the Panel. The Panel reads existing child blocks but does not run the full-Pane history or header path. This is where live success and older-history failure diverge.

A separate full-Pane route still offers child-targeted Escape interruption. Main Escape interrupts Main; a selected live child calls stop_subagent; a settled child returns to Main. See `crates/ferrite-cockpit/src/cockpit.rs:5684-5727`. Whether Escape from a Panel drill is intercepted before it reaches Main is owned by the input-focus audit and requires live verification. This report does not claim that looking at a child necessarily makes Escape target it.

### Claude discovery and attribution

Claude uses `parent_tool_use_id` to attribute messages. Unknown nonempty IDs create child identity rather than contaminating Main, and malformed attribution is rejected. `Agent` and legacy `Task` calls establish direct parent, name, task description and subagent_type; resumed agent IDs and task IDs are aliases. Initial coverage can be ToolActivity and later become Live. See `crates/ferrite-core/src/providers/claude/activity.rs:484-506,1209-1256`, with alias persistence tests at `:1853-1891` and direct-parent tests at `:1783-1792`.

Task events also read `parent_task_id`; a parent not yet known leaves the relationship unset until a later frame supplies the relationship again. That limitation is explicit and tested at `crates/ferrite-core/src/providers/claude/activity.rs:1405-1412,2037-2058`. The audit did not prove eventual parent repair when no subsequent child frame arrives. Keep that as a provider-ordering runtime check, not a reported defect.

Child-owned progress is routed into the child's transcript rather than Main at `crates/ferrite-core/src/providers/claude/activity.rs:415-433`. That correct attribution makes a Main-only Background tab an incomplete inventory. The Panel faults affect both providers even though Claude and Codex obtain history differently.

### Background work and stopping

Main's background line uses progress tasks while Main runs. Once Main is idle it also adds fresh Pending, Working, Waiting or Unknown agents, counted canonically. Details opens a modal; Open/Open thread resolves an agent and opens its Panel drill. The Background tab is a different list of Main tool calls, and the palette's Background tasks entry is a third destination, the session-controls card. See findings 6 through 8.

The tab does have useful existing behavior: running items expand by default, finished items collapse, output shows the last 12 lines, stop buttons verify session generation and that the call still runs before dispatch. See `crates/ferrite-cockpit/src/cockpit/panel/background.rs:23-24,132-186,189-229,261-277`. These guards avoid accidentally stopping a replacement session's work. The summary's global pending set does not carry the same identity discipline.

## Detailed findings
### subagents-background-01: Opening an evicted agent through its transcript row never requests saved history

Priority P1. Category `defect`. Evidence `source-proven`.

A real agent can open as an empty transcript despite its history existing in Ferrite storage.

Trigger: Open an older child after retained child content has been evicted.

Expected: The visible agent requests its saved transcript just as select_subject does.

Actual and cause: The row creates a Drill and selects its Subject locally, then only syncs existing blocks. No call to ensure_subject_history occurs on this path.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:80-110`
- `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:154-163`
- `crates/ferrite-cockpit/src/cockpit/side_chat.rs:492-502`
- `crates/ferrite-cockpit/src/cockpit/subagents.rs:397-400`
- `crates/ferrite-cockpit/src/cockpit/subagents.rs:1055-1069`
- `crates/ferrite-core/src/engine/history.rs:110-172`
- `crates/ferrite-core/src/activity.rs:330-336`

T3 comparison:

- [Pinned implementation 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L466-L495)

Recommendation: Make the Panel agent opener request and observe saved history, render loading/error/coverage states, and support retry through the same loader as full-Pane navigation.

Observable acceptance check: With a small child-retention limit, finish and evict agent A, then click A in Main. Its stored prompt and response load without resuming its provider. A failing read shows a visible retry that succeeds after the storage failure clears.

### subagents-background-02: Nested agents and immediate-parent navigation are missing from the normal drill path

Priority P1. Category `partial-capability`. Evidence `source-proven`.

The core knows grandchildren, but inspecting a child loses the routes needed to inspect its children or identify the immediate parent.

Trigger: A provider child spawns another child; the user opens the first child from its Main transcript row.

Expected: Every known child has a reachable inspection route and correct ancestry.

Actual and cause: Compact transcript inputs deliberately supply empty agents. The subscribed event handler accepts only ToggleDisclosure. The full-Pane breadcrumb also hardcodes Main instead of the recorded parent.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/side_chat.rs:477-482`
- `crates/ferrite-cockpit/src/cockpit/side_chat.rs:527`
- `crates/ferrite-cockpit/src/cockpit/side_chat.rs:564`
- `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:125-143`
- `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:203-225`
- `crates/ferrite-cockpit/src/cockpit/subagents.rs:463-490`
- `crates/ferrite-cockpit/src/cockpit/transcript_glue.rs:49-96`
- `crates/ferrite-core/src/activity.rs:69-76`

T3 comparison:

- [Pinned implementation 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L514-L583)
- [Pinned implementation 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L784-L859)
- [Pinned implementation 3](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L70-L75)

Recommendation: Pass child mappings into agent transcripts, route OpenAgent from a Drill to another Drill, and expose immediate-parent and root navigation using AgentInfo.parent. Add a relationship list independent of retained spawn rows.

Observable acceptance check: Create Main > A > B with both providers. Open A from Main, then B from A. B identifies A as parent, opens A, and offers Main separately. Completed children remain reachable after spawn rows are outside the rendered window.

### subagents-background-03: Reopen closed tab restores an empty Agent tab

Priority P1. Category `defect`. Evidence `source-proven`.

The advertised reopen action returns a blank subagent view.

Trigger: Close an Agent tab, then use Reopen closed tab.

Expected: Reopening restores both tab identity and its transcript resource.

Actual and cause: Closing deletes Drill. Agent IDs remain eligible for the closed stack. Reopen only upserts the ID; agent_drill_body returns an empty div when Drill is absent.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/panel.rs:290-293`
- `crates/ferrite-cockpit/src/cockpit/panel.rs:313-319`
- `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:113-121`
- `crates/ferrite-cockpit/src/cockpit/surfaces.rs:48-52`
- `crates/ferrite-cockpit/src/cockpit/surfaces.rs:232-235`
- `crates/ferrite-cockpit/src/cockpit/palette.rs:1294-1297`
- `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:203-206`

T3 comparison: this is an internal Ferrite lifecycle defect. It does not depend on a T3 equivalent agent-tab implementation.

Recommendation: Reconstruct the agent resource through open_agent_drill when reopening its SurfaceId, or exclude it from reopen until a resource-aware restore exists.

Observable acceptance check: Open an agent with visible text, close its tab, then invoke Reopen closed tab. The same agent and transcript return. Repeat through Close all and confirm each restored tab has content.

### subagents-background-04: Agent Panel views omit coverage, decision guidance and full status

Priority P2. Category `partial-capability`. Evidence `source-proven`.

A partial, waiting, failed, or unavailable child can look like an ordinary read-only transcript with no explanation of what the operator should do.

Trigger: Inspect an agent whose state is not an ordinary live Working or complete Idle state.

Expected: The normal inspection route communicates the same critical state as the alternate full-Pane route.

Actual and cause: The body only renders the transcript. Its tab pending mark recognizes Working only. The full-Pane coverage/error/header and decision machinery are omitted from the Panel implementation.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:145-175`
- `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:203-225`
- `crates/ferrite-cockpit/src/cockpit/side_chat.rs:477-482`
- `crates/ferrite-cockpit/src/cockpit/side_chat.rs:558-565`
- `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:249-260`
- `crates/ferrite-cockpit/src/cockpit/subagents.rs:350-367`
- `crates/ferrite-cockpit/src/cockpit/subagents.rs:459-604`

T3 comparison:

- [Pinned implementation 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14-L75)
- [Pinned implementation 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/SubagentTooltipContent.tsx#L33-L61)
- [Pinned implementation 3](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatView.tsx#L1746-L1748)

Recommendation: Give the primary agent drill an identity/status header, coverage explanation, and an explicit route to its pending request. Preserve read-only prompting semantics. State whether Stop targets this child.

Observable acceptance check: Open agents in Pending, Waiting, Failed, NotLoaded, Partial, and ToolActivity states. Each view states its true status/coverage. Waiting exposes a route to the exact request; opening and answering it preserves the parent draft.

### subagents-background-05: Generic thread opening can replace Main with the first pending child

Priority P2. Category `interaction`. Evidence `source-proven`.

A user selecting a thread can unexpectedly land in a background agent and lose the visible Main composer, while a direct agent-row click instead opens a side tab.

Trigger: Invoke notice Verb::OpenThread while the first pending decision belongs to a subagent.

Expected: The label and route make clear whether the action opens Main or follows a child request.

Actual and cause: OpenThread reads pending_decisions().first(), lands on the thread, then selects that decision owner into the main Pane. OpenDecision similarly selects an exact child. Agent-row clicks use the Panel instead.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:12274-12285`
- `crates/ferrite-cockpit/src/cockpit.rs:12298-12327`
- `crates/ferrite-cockpit/src/cockpit/subagents.rs:415-438`
- `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:1-10`
- `crates/ferrite-cockpit/src/cockpit/subagents.rs:463-490`

T3 comparison:

- [Pinned implementation 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L63-L75)
- [Pinned implementation 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L466-L495)

Recommendation: Separate Open thread from Open request, and visibly name child destinations on request actions. Use one explicit inspection/follow policy; do not silently retarget generic thread opening to whichever decision happens to be first.

Observable acceptance check: With Main and two children awaiting different approvals, activate the generic thread card, then each exact request. The generic action opens Main; request actions name and open the selected owner. Back returns to the prior place and Main draft remains unchanged.

### subagents-background-06: Background Stop never clears its pending marker or offers retry after failure

Priority P2. Category `defect`. Evidence `source-proven`.

A rejected or ineffective stop can leave Stopping... permanently disabled. Reused task IDs can inherit this state across threads or session generations.

Trigger: Stop background work and have the control fail, time out, or leave the task working.

Expected: The current work owns its transient stop state, and the user can recover from a failed stop.

Actual and cause: The code inserts task IDs into a global HashSet before dispatch. The only references initialize, insert or query the set; none remove entries. run_session_control stores errors for the separate session-controls card.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/background_work.rs:123-128`
- `crates/ferrite-cockpit/src/cockpit/background_work.rs:183-199`
- `crates/ferrite-cockpit/src/cockpit/background_work.rs:206-224`
- `crates/ferrite-cockpit/src/cockpit.rs:598`
- `crates/ferrite-cockpit/src/cockpit.rs:11561-11578`
- `crates/ferrite-cockpit/src/cockpit.rs:11739-11747`

T3 comparison:

- [Pinned implementation 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L416-L463)

Recommendation: Key stop requests by thread, session generation and task; clear on failure/completion, expose failures beside Stop, and allow retry if the provider does not settle.

Observable acceptance check: Reject a Stop request. Its error appears beside the background line and Stop becomes enabled again. Reuse the same native task ID in another thread and a later session and confirm no inherited pending marker.

### subagents-background-07: Background-work Stop excludes every subagent it counts

Priority P2. Category `partial-capability`. Evidence `source-proven`.

A mixed commands-and-agents line offers Stop but stops only commands; an agents-only line has no Stop even where child interruption is supported.

Trigger: Main finishes while one or more children continue; optionally a shell also continues.

Expected: A Stop action next to the combined work summary has a clear and complete scope.

Actual and cause: Subagent Work records always have id=None. The Stop predicate requires an id and the dispatcher filters out None, so child work can never reach this control.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/background_work.rs:82-93`
- `crates/ferrite-cockpit/src/cockpit/background_work.rs:123-128`
- `crates/ferrite-cockpit/src/cockpit/background_work.rs:206-222`
- `crates/ferrite-cockpit/src/cockpit.rs:5707-5727`

T3 comparison:

- [Pinned implementation 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadActivityBanners.tsx#L170-L179)
- [Pinned implementation 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L434-L443)

Recommendation: Make Stop scope explicit and use the existing child-stop operation for stoppable child work. If one action cannot stop everything, give per-item controls and name the remaining work.

Observable acceptance check: Run one command and one stoppable child after Main settles. Stop either stops both, or clearly says Stop commands and leaves a named child Stop control. An agents-only state offers child Stop where supported.

### subagents-background-08: Background task views disagree about which work exists

Priority P2. Category `partial-capability`. Evidence `source-proven`.

The transcript can say Waiting on a subagent while the Background tab is empty; users must learn three different background-work views.

Trigger: Main ends with a live agent, or a progress-only shell task, then the user opens Background using the Panel menu.

Expected: Each background entry point represents the same known work and says whether Main actually awaits it.

Actual and cause: The tail enumerates progress tasks plus named agents; the tab enumerates only Main transcript.background_calls. The palette Background tasks opens session controls instead. The dialog derives all Waiting/Running labels from Main busy status, not per-task awaited evidence.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/background_work.rs:28-97`
- `crates/ferrite-cockpit/src/cockpit/panel/background.rs:29-39`
- `crates/ferrite-cockpit/src/cockpit/panel/background.rs:84-94`
- `crates/ferrite-cockpit/src/cockpit/palette.rs:1273-1276`
- `crates/ferrite-cockpit/src/cockpit/background_work.rs:345-379`

T3 comparison:

- [Pinned implementation 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadActivityBanners.tsx#L180-L208)
- [Pinned implementation 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/BackgroundWorkDetails.tsx#L39-L63)

Recommendation: Use one attributed work inventory for the transcript summary, detail dialog and Background tab. Include agents and shell tasks, retain owner identity, and distinguish awaited work from work merely running. Make palette naming identify the destination.

Observable acceptance check: With an agents-only workload, the transcript count, Details and Background tab show the same agent. With a command known only from a progress snapshot, every view includes it. A dev server running after Main completes reads Running unless the provider says it is awaited.

### subagents-background-09: Agent inspection tabs disappear on restart despite the Panel persistence contract

Priority P2. Category `partial-capability`. Evidence `source-proven`.

A user returning to a long review loses the agent inspection tabs and can land on a surviving Background tab instead.

Trigger: Restart with an Agent tab active alongside persistent tabs.

Expected: Stable agent history remains reachable in the restored workspace, and replacement navigation is explained.

Actual and cause: survives_restart explicitly rejects Agent. migrate chooses the nearest retained tab index when the selected tab is removed.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/panel.rs:124-140`
- `crates/ferrite-cockpit/src/cockpit/surfaces.rs:370-388`
- `docs/adr/0039-the-panel-is-one-tabbed-surface-host.md:17-31`

T3 comparison:

- [Pinned implementation 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L466-L495)

Recommendation: Persist stable agent identities and reconstruct saved drills lazily, or make this exception an explicit product decision with a clear recoverable list of recent agents. Preserve deliberate active-tab choice when an ephemeral tab cannot restore.

Observable acceptance check: Open Overview, Background and Agent A with A active, restart, and verify A restores with saved history or an explicit recovery state. The application does not silently show Background as if the user chose it.

## Supported T3 capabilities and differences

T3 has first-class thread lineage rather than representing every child as a Ferrite Subject. Provider-native children are hidden from ordinary thread lists by the contract at [orchestrationV2.ts:168-177](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/orchestrationV2.ts#L168-L177). Its Lineage panel derives immediate relationships, keeps unloaded subagent links reachable, separates active and previous agents, and opens the selected related thread. See [ThreadRelationshipsControl.tsx:514-608](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L514-L608) and [the row action at 844-859](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L844-L859). Ferrite need not copy this storage model, but it needs equivalent reachability for its own known relationships.

T3's provider-native child page replaces the composer with a provider/model/effort/status bar, says "Runs on its own", and offers Open parent. Provider-native approvals/questions are routed to the top-level parent, so T3 also does not make every child page an editable conversation. See [ProviderSubagentBar.tsx:14-75](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14-L75) and [ChatView.tsx:1746-1748](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatView.tsx#L1746-L1748). Ferrite's read-only child composer policy is therefore not itself a parity defect. The gap is missing identity and actionable guidance in the primary Panel route.

T3's transcript groups related agents with a count, status summary, provider avatars, elapsed time and independently expandable member rows. See [TimelineV2EventRow.tsx:54-159](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineV2EventRow.tsx#L54-L159). Its Lineage tooltip carries model, provider, origin, progress, result and parent/child context at [ThreadRelationshipsControl.tsx:787-804](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L787-L804). Ferrite already has grouped agent rows and progress/time faces, with UI tests at `crates/ferrite-cockpit/src/cockpit/tests/subagents.rs:787-834`; the missing tree and drill behavior should not be mislabeled as absence of all subagent cards.

T3 conditionally offers individual native child Stop for Claude with a strong native task reference and active state. That UI predicate is provider-specific, so the audit does not assume Codex and Claude expose identical child-stop semantics. See [ThreadRelationshipsControl.tsx:747-757,940-964](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadRelationshipsControl.tsx#L747-L964). Ferrite supports child-targeted stop through its own provider-neutral route and tests, including `crates/ferrite-core/tests/codex_subagent_interrupt.rs` and `crates/ferrite-cockpit/src/cockpit/tests/subagents.rs:647-707`. The report does not claim T3 universally has stronger stop support.

T3's background Details renders each task's own awaited flag and child link, and streams command output when supported. See [BackgroundWorkDetails.tsx:39-87](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/BackgroundWorkDetails.tsx#L39-L87). Its Stop request is scoped to environment/thread, reset on failure and made retryable after a deadline at [useThreadLifecycle.tsx:416-463](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L416-L463). Its related-thread opener guards delayed lookups against a newer navigation and explicitly handles unloaded child shells at [466-495](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useThreadLifecycle.tsx#L466-L495).

T3 also has a read-only "Subagent work" dialog with loading/error/empty/truncation text and Refresh at [NativeChildWorkDialog.tsx:29-105](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/NativeChildWorkDialog.tsx#L29-L105). The server query inspected is specifically for Antigravity-native child records at [NativeChildWorkQuery.ts:18-64](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/NativeChildWorkQuery.ts#L18-L64). This is not evidence that Codex or Claude need that same dialog.

No dedicated Inspect-versus-Follow mode was established in current T3 source. This report uses inspection for viewing a child, and following a request for explicit owner navigation. It does not invent missing T3 controls. T3's agent group chevron has a transform transition, while its group panel explicitly sets animate=false for virtualized layout stability at [TimelineV2EventRow.tsx:136-145](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineV2EventRow.tsx#L136-L145). Ferrite's background card changes chevron state and body immediately at `crates/ferrite-cockpit/src/cockpit/panel/background.rs:144-186`. Motion quality and reduced-motion verification belong to the motion audit. Neither implementation's current choice resolves the user's preference for useful motion.

## Positive Ferrite coverage

- Agent identity is scoped by provider and root, and alias merging avoids counting one Claude agent multiple times. The deduplicated background-count UI test is `crates/ferrite-cockpit/src/cockpit/tests/background_work.rs:70-102`.
- Both provider decoders preserve direct parentage and prevent child output from becoming Main output. Captured nested and overlapping fixtures exist in `crates/ferrite-core/tests/fixtures/subagents/`; Codex discovery and native history reads are covered by `crates/ferrite-core/src/providers/codex/activity.rs:1461-1537,1735-1804`.
- Child history is persisted and recoverable after eviction. Main and child history are separate, with explicit Complete/Partial/Live/ToolActivity/Unavailable coverage states. The problem is reaching this capability from Panel inspection.
- The full-Pane route prevents typing into Main's hidden composer and presents pending requests by exact Subject. See `crates/ferrite-cockpit/src/cockpit/subagents.rs:262-267,397-411`.
- The primary row route keeps Main visible beside the child, deduplicates one Panel tab per agent, and updates open drills from live child transcripts. See `crates/ferrite-cockpit/src/cockpit/agent_pane.rs:78-110,154-163`.
- Background detail opening and Escape dismissal have UI coverage in `crates/ferrite-cockpit/src/cockpit/tests/background_work.rs:43-67`. The new Agent tab's sizing/maximize/close path is covered in `crates/ferrite-cockpit/src/cockpit/tests/subagents.rs:877-932`.

## Verification gaps and focused runtime checks

The existing tests demonstrate a coverage split. Retention and decision tests exercise full-Pane selection, while the Panel test directly calls open_agent_from and checks size/maximize/close. It does not click a rendered nested row, evict a child, reopen a closed Agent tab, surface an approval in the drill or test saved-history errors. See `crates/ferrite-cockpit/src/cockpit/tests/subagents.rs:264-305,877-932` and `crates/ferrite-cockpit/src/cockpit/tests/retention.rs:77-82`. This explains why healthy core tests are compatible with inaccessible UI behavior; it does not claim those tests fail.

The parent should run these visible checks without starting unrelated workers:

1. Inspect an existing live child and an older finished child from real transcript rows, for both providers where existing sessions permit it. Record which route opens, whether prompt/response/history coverage appears, and whether Main's draft stays intact.
2. Open a known nested child, then follow its own child. Confirm immediate parent and Main navigation independently. Verify whether known agents with no visible spawn row have any alternative entry point.
3. Close and reopen an Agent tab through the user-facing command. This should settle finding 3 without provider traffic.
4. Open the generic thread card and exact decision notification on a thread with a child approval. Record destination and label. Do not call all background navigation automatic until this reproduces the user's path.
5. Compare Background tab, Details dialog and palette Background tasks for the same existing workload. Check agents-only and progress-only cases and whether Waiting truly means awaited.
6. Exercise a controlled failing stop through a test fixture, then retry. Do not stop the user's unrelated work merely to reproduce a control defect. Check task ID reuse across threads/generations.
7. After owner approval of a disposable app-state fixture, restore a workspace whose active Agent tab is adjacent to Background. Production restart is not required for the source finding.
8. Inspect native-provider version compatibility, especially out-of-order Claude parent_task_id records and Codex history paging/conflict handling. These are source-supported designs with runtime uncertainty, not additional confirmed defects.

Recommended remediation order: restore history and nested-agent reachability in the shared Panel opener; repair closed-tab resource restoration; unify child status/decision guidance; make notification destinations explicit; then unify background inventory and stop lifecycle. Keep provider attribution and history storage intact. No broad rewrite is justified by these findings.

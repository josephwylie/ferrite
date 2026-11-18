# Projects and navigation investigation

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user's goal is a complete, source-backed tally of unreliable Ferrite workflows and missing or weaker T3 capabilities. This slice covers projects, thread lists, search, selection, sorting, unread state, parking and reopening, rename, workspaces, navigation history, and multiwindow reachability. PR links and input propagation belong to other reports.

## Overview

Ferrite's current navigation is substantially implemented. The old project tree is intentionally gone. ADR 0040 replaces it with an inbox-style list and Working, Snoozed and Parked shelves. It would be incorrect to report the missing tree, missing separate Inbox mode, ordinary pinning, basic search, unread marks, archive-style parking, or history arrows as wholly absent.

The most concrete defect is an offered Rename action that never renders its editor for Parked and Snoozed rows. Other defects concern search selection and scrolling, reopen feedback, and incomplete Undo of a group-affecting park. Several T3 capabilities are partial or absent, including metadata/content search, rich project scope, pinned reorder and bulk operations beyond Park.

This report records 14 findings. There is one P1 and thirteen P2 findings. Categories comprise five defects, five partial capabilities, two interactions, one missing capability and one intentional difference. These counts describe this slice, not the complete audit. Product differences are separated from defects.

## Method and limits

Ferrite baseline is `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. T3 source is `74573fc29b901bafbf8f1dbb2eb29606da1fa448` in `/tmp/ferrite-t3-audit-source-20261113`, with remote `https://github.com/pingdotgg/t3code.git`. Ferrite HEAD was checked against the baseline. Inspection used `rg`, numbered source reads, and tracing of handlers, renderers, state and store operations. T3 links below pin the comparison commit.

The Codex adapter, poteto-mode investigation instructions, how output shape and unslop guidance were read. No subagents were spawned because this assigned investigation forbids them. Throughput checkpoint is n/a, read-only investigation.

No application code, configuration, branch, dependency or user data was changed. No tests, builds, live UI actions or runtime fixtures were run. Source-proven means that the relevant branch or missing connection is visible in the inspected code. It does not mean the installed binary was exercised. Runtime manifestations such as keyboard lockup still need the checks listed below.

## Key concepts

- A Project registers one or more directories. Project management remains in the existing editor. The nav stores a single optional project scope. See `crates/ferrite-cockpit/src/cockpit.rs:429,4883-4909,5140-5245`.
- A Thread has live core state or parked history. `Cockpit::reopen` revives it only when absent and then focuses the appropriate view. See `crates/ferrite-core/src/cockpit.rs:3099-3106`.
- A Group is a layout and membership relationship. It remains reachable from the filter menu even though the nav no longer renders the project tree. See `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:293-307`.
- Selection has two meanings. The shown thread gets the active row. A separate `nav_picked` vector powers multi-select Park. See `crates/ferrite-cockpit/src/cockpit.rs:8805-8814` and `crates/ferrite-cockpit/src/cockpit/nav_pick.rs:36-88`.
- Park is both a lifecycle operation and the source of the Parked shelf. It is not just a hidden flag. See `crates/ferrite-core/src/engine.rs:2276-2333` and finding 014.
- History contains Thread plus Subject or Group plus optional subject drill-down. Drafts and the empty board are explicitly excluded. It is bounded to 50 entries. See `crates/ferrite-cockpit/src/cockpit/history.rs:15-36,59-82`.

## Traced flows

### Building and sorting the navigation list

`nav_state` gathers pane and group threads, filters out threads the operator does not own and threads outside project scope, and deduplicates IDs. It assigns each open thread in priority order to Snoozed, Pinned, Working or Active. Pending decisions prevent classification as Working. Pins keep saved insertion order, Snoozed sorts by wake time, Active puts pending decisions first and then uses needs-you queue order, Working sorts by newest send, and Parked uses the parked list. See `crates/ferrite-cockpit/src/cockpit.rs:8797-8884`.

`nav_list` renders pins, inbox notices and local active cards, then anchored shelves. Shelf visibility persists in settings. A folded shelf still shows its selected row. Parked rows paginate. Machine headings are integrated around the local list. See `crates/ferrite-cockpit/src/cockpit/nav_list.rs:64-103,120-188`.

This is the accepted inbox ordering, not a missing old project-tree implementation. User-controlled pinned reordering is a separate gap in finding 010. A full performance conclusion would require measuring large collections. The loop builds row elements without proving how much the rendering backend lays out offscreen. No latency claim is made here.

### Choosing and opening a thread

Full cards attach a click handler that first applies Cmd/Shift selection. A normal click opens the notice destination or the thread, then releases keyboard focus. A right press preserves or clears the multi-selection and opens the context menu. Dragging a card carries it toward the board. See `crates/ferrite-cockpit/src/cockpit/nav_list.rs:275-345`.

Slim rows have direct open and wake/revive actions plus context menus. They do not share the full-card selection or editable-title composition. See `crates/ferrite-cockpit/src/cockpit/nav_list.rs:384-451`. Findings 001 and 011 are failures at that shared-behavior boundary.

`focus_thread` clears the factory/empty view and focuses the core thread. `revive_thread` calls the core reopen operation, synchronizes panes on success, and logs failures. See `crates/ferrite-cockpit/src/cockpit.rs:9033-9072`. Project scope stays unchanged, which explains finding 009.

### Search

Text changes reset the result cursor. A nonempty trimmed query replaces the list. Matching traverses scoped open and parked rows and compares a case-insensitive title substring. Arrow keys change an index, Enter opens the indexed thread, and Escape clears the query. Activation clears the query before opening the thread. See `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:53-83,103-120,203-272`.

The flow has complete basic wiring, but lacks metadata/content search, highlighted-result reveal and identity-stable selection. Findings 005 through 007 cover these separate user outcomes.

### Parking, reopening, deletion and Undo

Context-menu Park uses the multi-selection only when the target belongs to a pick of more than one thread. Press-and-sweep collects the drawn span. Both reach `park_listed`, which invokes the core for each ID and offers Undo for successes. See `crates/ferrite-cockpit/src/cockpit/context_menu.rs:692-702`, `crates/ferrite-cockpit/src/cockpit/nav_pick.rs:100-167`, and `crates/ferrite-cockpit/src/cockpit.rs:12830-12842`.

Core Park leaves groups, flushes the writer before dropping live state, and keeps the live owner when flush fails. Reopen loads history with no immediately running session; the session starts when used. See `crates/ferrite-core/src/cockpit.rs:3308-3332` and `crates/ferrite-core/src/engine.rs:2308-2338`. This has good durability intent. Missing per-target batch outcomes and incomplete group Undo remain in findings 003 and 004.

Permanent delete is distinct. Registered project worktrees survive as adoptable inventory. Only a legacy unregistered worktree is removed, after a cleanliness check; its branch is retained. Group state is snapshotted and restored if deletion fails. See `crates/ferrite-core/src/engine.rs:2201-2270`. The Parked shelf has a separate Delete parked threads action. Its test explicitly requires confirmation. See `crates/ferrite-cockpit/src/cockpit/context_menu.rs:300-315` and `crates/ferrite-cockpit/src/cockpit/tests/nav_list.rs:697-728`.

### Projects, drafts and worktrees

Add project is a visible search-row action. Filter is a static menu of projects and groups. Existing project editing is registered as palette commands and opens a modal that can rename and manage directories. See `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:144-160,280-309`, `crates/ferrite-cockpit/src/cockpit/palette.rs:883-890,1174` and `crates/ferrite-cockpit/src/cockpit.rs:4954-5005,5140-5245`.

New thread opens a draft; first send creates durable thread state. Cmd-Shift-N selects New worktree. Draft creation carries provider/model context and current project; an explicit project overrides that fallback. A repeated New thread focuses and retargets the existing nonstarting draft. See `crates/ferrite-cockpit/src/cockpit.rs:7150-7184,7289-7344,9083-9097`. This one-standing-draft policy should be evaluated with the composer report before declaring it a missing navigation feature.

The workspace selector supports Current checkout, existing worktrees, existing branches, New branch in the checkout and New worktree. Existing worktree branches are not redundantly offered as ordinary branches. See `crates/ferrite-cockpit/src/cockpit.rs:7646-7725`. Background listings also follow agent-created worktrees. On revival, an externally created worktree that disappeared falls back to main; registered Ferrite worktrees are handled differently. See `crates/ferrite-cockpit/src/cockpit.rs:2596-2597,2663-2716` and `crates/ferrite-core/src/engine.rs:2395-2405`.

The context action creates a new thread in the project checkout. It does not carry the originating thread's branch/worktree, although manual selection can reproduce it. Finding 013 records that distinction.

### Unread and attention

Unread state exists. The menu offers Mark as unread only for a nonvisible thread with a notice that can become unread. The core rewinds that thread's newest notice. Visible threads are acknowledged only while the window is active. In a group, all visible threads count as seen; fullscreen narrows that set. See `crates/ferrite-cockpit/src/cockpit/context_menu.rs:426-434,824`, `crates/ferrite-core/src/notifications.rs:380-403` and `crates/ferrite-core/src/cockpit.rs:1182-1217,2733-2740`.

This is a notice-based mark, not an arbitrary bookmark that can mark every thread unread. That scope is a product limitation to keep in mind, but no extra defect is asserted because the menu intentionally gates unavailable cases. Background routing and subagent notification semantics are owned by their separate report.

### Navigation history and multiple windows

History observes the rendered place, appends new visits, truncates the forward branch and prunes gone threads/groups. Back and Forward reuse focus, reopen and subject-navigation operations. Group member focus is not a distinct history entry unless it changes a subagent drill-down. See `crates/ferrite-cockpit/src/cockpit/history.rs:59-101,139-179,195-229`. T3 delegates desktop history to browser history. See [navigationHistory.ts:3-21](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/navigationHistory.ts#L3).

Ferrite's history does not encode a project filter, factory page, settings page, panel surface or draft. Some of those are intentional overlays or scopes. Back/Forward should be runtime checked across them, but this report does not count each as a defect without an agreed history contract. A failed revival during a history step is covered by finding 002.

The app startup opens a Cockpit window at `crates/ferrite/src/lib.rs:349-385`. The inspected nav menus contain no New window or Open thread in new window operation. A search of Ferrite Rust UI code and T3 web thread actions did not establish a matching native thread-detach feature in T3 either. Therefore there is no unsupported claim of a T3 multiwindow gap. Multiple client processes, shared-server notifications, ownership and independent view state require a runtime check. Closing a server-connected window intentionally lets agents continue, while in-process quitting warns about interrupted work. See `crates/ferrite-cockpit/src/cockpit/leaving.rs:1-4,10-23,64-87`.

## Source map

| Concern | Ferrite | T3 |
| --- | --- | --- |
| Nav assembly and shelves | `cockpit/nav_list.rs:29-225`; `cockpit.rs:8797-8884` | `components/Sidebar.tsx:624-690` |
| Search and scope | `cockpit/nav_chrome.rs:53-309` | `components/Sidebar.logic.ts:1122-1178`; `components/Sidebar.tsx:1429-1545,2986-3117` |
| Selection and actions | `cockpit/nav_pick.rs:23-167`; `cockpit/context_menu.rs:401-495` | `components/Sidebar.tsx:2320-2552`; `components/threadActionMenu.logic.ts:116-254` |
| Rename and project editor | `cockpit.rs:3392-3441,4883-5338,5398-5428` | `components/sidebar/SidebarThreadRow.tsx:1033-1039`; `hooks/useThreadActionMenu.ts:200-213` |
| Park/reopen durability | `ferrite-core/src/cockpit.rs:3099-3121,3308-3332`; `ferrite-core/src/engine.rs:2276-2405` | `hooks/useThreadActions.ts:363-437,799-837`; `components/settings/ArchivedThreadsPanel.tsx:138-186` |
| History | `cockpit/history.rs:23-240` | `navigationHistory.ts:3-21` |
| Workspace choice | `cockpit.rs:7646-7725`; `ferrite-core/src/engine.rs:2201-2270` | `hooks/useThreadActionMenu.ts:215-221`; `hooks/useThreadActions.ts:515-561` |

Ferrite shorthand in this table is relative to `crates/ferrite-cockpit/src/` unless it explicitly starts with `ferrite-core`, which is under `crates/`. T3 paths are relative to `apps/web/src/` at the pinned comparison commit.

## Detailed findings

### projects-navigation-001. Rename on Parked and Snoozed rows creates an editor that is never drawn

Priority P1. Category `defect`. Evidence `source-proven`.

Symptom and impact. An advertised rename action cannot be completed visibly on either slim shelf. The root also attempts to focus the absent editor, which creates a keyboard reliability risk.

Trigger. Expand Parked or Snoozed, then choose Rename from a row menu, or invoke the nav rename action on that slim row.

Expected behavior. The target row displays a focused rename field.

Actual behavior. The menu sets RenameTarget::Thread, but nav_slim renders row.name directly. Only nav_card asks editable_thread_title for the field.

Cause. The slim-row renderer does not accept or compose the rename element although the menu and focus policy treat all Thread targets alike.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/context_menu.rs:431-434,671-672`; `crates/ferrite-cockpit/src/cockpit/nav_list.rs:285-295,435-451`; `crates/ferrite-cockpit/src/nav/list.rs:188-197,265`; `crates/ferrite-cockpit/src/cockpit.rs:3392-3409,5405-5412,10109-10112,10266-10273`.

T3 evidence. [apps/web/src/components/sidebar/SidebarThreadRow.tsx:1033-1039,1218-1243](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L1033).

Recommendation. Pass the same editable title into full and slim rows. Focus it only after the row has rendered. Keep Enter and Escape on the visible editor.

Observable acceptance check. Right-click a Parked row and a Snoozed row, choose Rename, and see the existing title in a focused field. Type, save, reopen the app, and confirm the saved title. Escape must cancel without swallowing subsequent navigation keys.

### projects-navigation-002. Reopen failures produce a log entry without a visible error

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact. A parked thread can appear unresponsive when its stored history cannot be loaded. The search also clears before the failure, removing the user's recovery context.

Trigger. Open a parked thread whose store revival returns LoadError.

Expected behavior. The clicked row remains understandable and reports why reopening failed.

Actual behavior. revive_thread and reopen_thread log Err branches. revive_thread has already cleared the empty/factory view state; search-result activation has already cleared the query.

Cause. The UI handlers drop the Result into logging instead of a user-visible state.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/nav_list.rs:418-426`; `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:261-264,420-430`; `crates/ferrite-cockpit/src/cockpit.rs:9063-9072,9135-9144`; `crates/ferrite-core/src/cockpit.rs:3099-3106`; `crates/ferrite-core/src/engine.rs:2360-2372`.

T3 evidence. [apps/web/src/components/settings/ArchivedThreadsPanel.tsx:138-168](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ArchivedThreadsPanel.tsx#L138); [apps/web/src/hooks/useThreadActions.ts:363-381](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useThreadActions.ts#L363).

Recommendation. Return a visible per-thread recovery message and retain the row and query on failure. Let Undo report failed restores instead of consuming its notice silently.

Observable acceptance check. With a controlled unreadable or missing thread log, click the parked row, invoke Reopen last, and undo Park. Each path identifies the failed thread, shows the failure, and leaves a retryable state.

### projects-navigation-003. Undo Park does not restore group membership or a dissolved group

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact. Undo restores a transcript but cannot restore the grouped workspace that Park changed. Users can lose their arrangement after what appears to be a reversible action.

Trigger. Park a group member through the nav and use the offered Undo.

Expected behavior. Undo reverses the user-visible changes made by Park, including membership.

Actual behavior. park_thread first leaves the group. UndoAct::Parked contains only ThreadIds and undo_nav calls revive_thread for each. It has no group snapshot or restoration operation.

Cause. Undo models the closed sessions but omits the group mutation that occurs in the same user action.

Ferrite evidence. `crates/ferrite-core/src/cockpit.rs:3308-3332`; `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:20-24,420-430`; `crates/ferrite-cockpit/src/cockpit.rs:12830-12841`.

T3 evidence. [apps/web/src/hooks/useThreadActions.ts:814-835](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useThreadActions.ts#L814).

Recommendation. Capture the group and placement before parking, then restore surviving membership and layout on Undo. Report conflicts when concurrent edits prevent exact restoration.

Observable acceptance check. Create a two-thread group, park one member, then Undo. Both threads must return to the original group and arrangement. Repeat with multi-selection and a larger group.

### projects-navigation-004. Bulk Park discards individual failures and clears the selection

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact. A mixed success/failure batch reports only Parked. The operator cannot tell which requested parks failed without inspecting the remaining list.

Trigger. Use multi-select Park or press-and-sweep when one thread refuses a store flush or group mutation.

Expected behavior. The batch reports partial failure and preserves failed targets for retry.

Actual behavior. park_listed retains only is_ok results. The context menu clears nav_picked first and the sweep removes its targets before the result is known.

Cause. Batch mutation is implemented as a success filter rather than an outcome collection. A lower-level store error may also appear in the thread, but the batch itself has no failure summary.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit.rs:12832-12842`; `crates/ferrite-cockpit/src/cockpit/context_menu.rs:692-696`; `crates/ferrite-cockpit/src/cockpit/nav_pick.rs:125-133`; `crates/ferrite-core/src/engine.rs:2308-2311`.

T3 evidence. [apps/web/src/components/Sidebar.tsx:2433-2462,2528-2552](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L2433).

Recommendation. Collect a result for every requested thread. Show the successful and failed counts, retain failures in the selection, and offer Undo only for the successful subset.

Observable acceptance check. Force one of three selected threads to refuse Park. The UI says two parked and names the failure. The failing thread stays selected. Undo restores only the two successful parks.

### projects-navigation-005. Sidebar search matches titles only

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact. A thread cannot be found from its branch, PR number, or remembered message text through the sidebar search, even though the result row itself displays a PR number.

Trigger. Search using repository metadata or remembered conversation text instead of a title substring.

Expected behavior. T3 parity includes ranked title, branch and PR matches plus server-ranked message matches.

Actual behavior. The predicate is row.name.to_lowercase().contains(words). It searches scoped open rows and parked rows, with no metadata or transcript branch.

Cause. The implementation operates solely on cached title strings. This is a navigation-search gap, separate from any transcript-local find capability.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:71-83`; `crates/ferrite-cockpit/src/nav/chrome.rs:193-201`.

T3 evidence. [apps/web/src/components/Sidebar.logic.ts:1122-1158](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.logic.ts#L1122); [apps/web/src/components/Sidebar.tsx:1511-1520](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L1511).

Recommendation. Extend a shared thread-search model to branch and PR metadata, then add cancellable content search with snippets and navigation to the matched message. Keep the current project scope explicit.

Observable acceptance check. Search a unique branch, a PR number, and text found only inside a parked transcript. Each query returns the correct thread. A content result opens at the matching message.

### projects-navigation-006. Keyboard search navigation does not reveal the highlighted result

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact. Arrow-key navigation can move the highlighted search result beyond the viewport while the keyboard remains in the search field. Enter then opens an unseen row.

Trigger. Use arrow keys in a search with more results than the nav can display.

Expected behavior. The list scrolls the highlighted result into view.

Actual behavior. step_search changes the integer and notifies. Search result rows have no cursor_reveal, shown_reveal, focus target, or scroll request. Ordinary nav movement does set nav_reveal.

Cause. Search reimplements keyboard selection but omits the normal list's visibility invariant.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:203-244`; `crates/ferrite-cockpit/src/nav/chrome.rs:160-213`; `crates/ferrite-cockpit/src/cockpit/nav_keys.rs:121-138`.

T3 evidence. [apps/web/src/components/sidebar/SidebarThreadRow.tsx:1626-1630,1722-1724](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/sidebar/SidebarThreadRow.tsx#L1626).

Recommendation. Give search results the same explicit reveal behavior as ordinary nav rows. Reveal on query reset and every highlight change without moving keyboard focus out of the field.

Observable acceptance check. Populate enough matching threads to exceed the viewport. Press Down to the final result and Up to the first. Each highlighted row must remain visible throughout.

### projects-navigation-007. Live search results can change the Enter target or make Enter a no-op

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact. A background status change can reorder the results under the user's highlight. A result disappearing can leave a visibly selected row that Enter will not open.

Trigger. Keep a query open while matching threads change status, rename, disappear, or arrive.

Expected behavior. The highlight follows the selected thread and Enter opens the visibly highlighted row.

Actual behavior. Results follow live section order. Selection is nav_search_cursor. Rendering clamps it to the final row, but open_search_cursor uses the raw index with get.

Cause. The selection is positional and the paint and activation paths use different bounds handling.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:57-59,79-83,208-210,237-255`; `crates/ferrite-cockpit/src/cockpit.rs:8841-8867`.

T3 evidence. [apps/web/src/components/Sidebar.tsx:1429-1443,1541-1544](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L1429).

Recommendation. Store the selected ThreadId and resolve its current index. Use one resolved selection for drawing and activation. Pick a documented fallback when that thread disappears.

Observable acceptance check. Highlight a search result, then cause another matching thread to move between Active and Working. Enter must still open the highlighted identity. Remove the selected final result and confirm the visible fallback opens.

### projects-navigation-008. Project scope has no search, multi-project selection, or persistence

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact. Large project lists require manual scanning. Users cannot watch a chosen subset of projects together, and the chosen project scope is lost on a new view.

Trigger. Open the Filter menu with many registered projects or try to focus on two related repositories.

Expected behavior. Current T3 supports searchable project scope, multiple selected projects, and persisted scope.

Actual behavior. Ferrite creates a static menu of All projects, one row per project, then groups. Scope is Option<ProjectId>, initialized to None.

Cause. The filter was modeled as a single ephemeral project ID. ADR 0040 intentionally removed the project tree, which makes this menu the primary project selector.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:280-309`; `crates/ferrite-cockpit/src/cockpit.rs:429,1503,7222-7237,8876-8884`.

T3 evidence. [apps/web/src/components/Sidebar.tsx:1148-1173,1196-1208,3064-3067,3089-3094](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L1148).

Recommendation. Use a searchable scope picker with explicit All and selected-project states. Persist the selection if the product intends it to survive restart. Multi-project scope should remain optional.

Observable acceptance check. With 30 projects, type a project name into the filter, select two projects, and see only their threads. Reopen the application and confirm the selected scope is restored.

### projects-navigation-009. Opening a thread outside the project filter leaves its sidebar selection hidden

Priority P2. Category `interaction`. Evidence `source-proven`.

Symptom and impact. The main view can show a thread with no selected row in the nav. This weakens the orientation promise that selection follows the shown thread.

Trigger. Use a navigation path outside the filtered sidebar to open a thread from another project.

Expected behavior. The user can identify the destination in navigation without manually diagnosing the filter.

Actual behavior. focus_thread and revive_thread do not modify nav_filter. nav_state filters candidate threads through admitted before selection is drawn, so shown_reveal never receives the excluded thread.

Cause. Reveal only operates on already admitted rows. T3 explicitly widens a route reveal's project scope and persists that decision.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit.rs:8809-8826,8876-8884,9033-9041,9063-9069`; `crates/ferrite-cockpit/src/cockpit/history.rs:207-214`; `crates/ferrite-cockpit/src/cockpit/nav_list.rs:295-296`.

T3 evidence. [apps/web/src/components/Sidebar.tsx:1148-1174](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L1148).

Recommendation. On explicit cross-project navigation, reveal the destination by widening the filter or drawing a clearly labeled current-thread exception. Preserve the user's scope on background updates.

Observable acceptance check. Filter to project A, then open a thread in B through history or the palette. The destination has a visible selected nav row or an explicit outside-filter indicator. Background updates alone must not change the scope.

### projects-navigation-010. Pinned rows cannot be reordered directly

Priority P2. Category `missing-capability`. Evidence `source-proven`.

Symptom and impact. Users cannot arrange persistent pinned priorities by dragging. Unpinning and repinning moves only to the end, which is a cumbersome ordering substitute.

Trigger. Try to place a pinned thread above an earlier pin.

Expected behavior. T3 supports direct persisted pinned-order changes.

Actual behavior. Pin storage is an append-only ordering interaction. set_pinned removes and pushes; nav_state sorts by that position. The card drag carries a Thread onto the board, with no pinned-order mutation in the inspected nav implementation.

Cause. Ferrite has persisted pin order but no user-facing operation that writes arbitrary order.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit.rs:7240-7253,8841-8849`; `crates/ferrite-cockpit/src/cockpit/nav_list.rs:275-277,308-324`.

T3 evidence. [apps/web/src/components/Sidebar.logic.ts:465-484](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.logic.ts#L465).

Recommendation. Add an explicit reorder destination for pinned rows and persist the resulting order. Preserve the existing drag-to-board behavior by distinguishing its destination.

Observable acceptance check. Pin three threads, drag the final one to the first slot, restart, and see the same order. Dragging into the board must still use the established grouping behavior.

### projects-navigation-011. Multi-selection applies only to Park and is absent from slim rows

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact. The UI suggests a multi-thread selection model but Snooze, Mark unread, Unpin and Delete still act on one context-menu thread. Parked and Snoozed rows cannot join that selection with the same gesture.

Trigger. Cmd-click or Shift-click several cards, then invoke an action other than Park, or attempt selection in a slim shelf.

Expected behavior. The selection and menu make action scope unambiguous. T3 provides bulk settle, snooze, unpin, title regeneration, unread and delete where applicable.

Actual behavior. Only Park uses menu_threads and displays a count. Slim-row click handlers call open_nav_row directly without pick_card.

Cause. The selection mechanism is coupled to bulk parking rather than a shared action-target model.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/nav_pick.rs:45-88`; `crates/ferrite-cockpit/src/cockpit/context_menu.rs:421-434,475-494,688-695,824`; `crates/ferrite-cockpit/src/cockpit/nav_list.rs:327-335,435-449`.

T3 evidence. [apps/web/src/components/Sidebar.tsx:2352-2402,2467-2504,2528-2552](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L2352).

Recommendation. Make the supported batch operations explicit and label them with their target count. Apply the selection model consistently to applicable shelves. Do not imply a batch for actions that remain singular.

Observable acceptance check. Select three rows and invoke each supported batch action. Its label names all three targets and its outcome accounts for each. Cmd-click on a Snoozed or Parked row follows the same documented selection behavior.

### projects-navigation-012. Project editing and removal lack a contextual navigation entry point

Priority P2. Category `interaction`. Evidence `source-proven`.

Symptom and impact. After the project tree retired, users must discover the command palette to rename a project, manage its directories, or remove it. These capabilities exist but the normal project filter does not lead to them.

Trigger. Try to edit the project from its filter row or a thread belonging to it.

Expected behavior. The primary project affordance offers a route to existing project management.

Actual behavior. The filter menu only changes scope or opens groups. The thread menu offers New thread in this project but no project editor. Edit project commands are registered in the palette.

Cause. The tree was removed intentionally, but its management affordance was not carried into the replacement navigation.

Ferrite evidence. `docs/adr/0040-the-nav-is-the-inbox.md:24-28`; `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:280-309`; `crates/ferrite-cockpit/src/cockpit/context_menu.rs:401-495`; `crates/ferrite-cockpit/src/cockpit/palette.rs:883-890,1174`; `crates/ferrite-cockpit/src/cockpit.rs:4883-4909,5140-5245`.

T3 evidence. [apps/web/src/components/threadActionMenu.logic.ts:241](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/threadActionMenu.logic.ts#L241); [apps/web/src/hooks/useThreadActionMenu.ts:200-213](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useThreadActionMenu.ts#L200).

Recommendation. Add Edit project to the selected project's filter entry or to the thread context menu. Reuse the existing editor and its validation.

Observable acceptance check. Starting with a visible thread, reach its project editor with the pointer, rename the project, and manage its directories without opening the command palette.

### projects-navigation-013. New thread on the same branch or worktree requires manual reselection

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact. Starting another conversation about the same checkout takes an extra workspace-selection step and can accidentally aim work at the main checkout if the user assumes the contextual action preserves the branch.

Trigger. Open a context menu on a thread in an existing feature worktree and start a related thread.

Expected behavior. T3 exposes a branch-specific action that passes both branch and worktreePath.

Actual behavior. Ferrite offers New thread in this project, creates DraftTarget::Main, and choose_checkout(project) carries only the project checkout. The draft workspace menu can select an existing worktree afterward.

Cause. Project inheritance is implemented; checkout inheritance is not exposed as a contextual action. The current label is accurate, so this is a capability gap rather than a mislabeled command.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/context_menu.rs:453-456,791-817`; `crates/ferrite-cockpit/src/cockpit.rs:7264-7275,7646-7725`.

T3 evidence. [apps/web/src/components/threadActionMenu.logic.ts:123-130](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/threadActionMenu.logic.ts#L123); [apps/web/src/hooks/useThreadActionMenu.ts:215-221](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useThreadActionMenu.ts#L215).

Recommendation. Offer an explicit New thread in this checkout action that carries the thread's workspace binding. Keep the existing project-level action accurately named.

Observable acceptance check. From a thread in a non-main worktree, choose New thread in this checkout. The draft names the identical path and branch before send, and the new session starts there.

### projects-navigation-014. Park combines session closure and sidebar settlement, unlike T3

Priority P2. Category `intentional-difference`. Evidence `source-proven`.

Symptom and impact. Ferrite has no separate lightweight settled state or inactivity-based settlement control. Tidying a running thread by parking ends its session and can remove group membership. T3 users may expect settlement to be a list-placement action distinct from archive.

Trigger. Use Park to remove a thread from the active inbox, especially while it is working or grouped.

Expected behavior. ADR 0040 calls Parked Ferrite's word for T3 Settled, but parity requires a deliberate decision about their different lifecycle effects.

Actual behavior. Ferrite Park closes the session, unloads its thread, disconnects notifications and leaves groups. T3 exposes Settle and Archive separately and has auto-settle controls.

Cause. Ferrite models parked history as the lower shelf. It does not model a separate settlement field in the inspected nav classification. This is a product difference, not a demand to copy T3.

Ferrite evidence. `docs/adr/0040-the-nav-is-the-inbox.md:20-23`; `crates/ferrite-cockpit/src/cockpit/context_menu.rs:475-489`; `crates/ferrite-core/src/engine.rs:2276-2278,2313-2327`; `crates/ferrite-core/src/cockpit.rs:3308-3332`; `crates/ferrite-cockpit/src/cockpit.rs:8834-8867`.

T3 evidence. [apps/web/src/components/threadActionMenu.logic.ts:139-145,206-224,241-247](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/threadActionMenu.logic.ts#L139); [apps/web/src/hooks/useThreadActions.ts:384-425,799-835](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useThreadActions.ts#L384); [apps/web/src/components/settings/SettingsPanels.tsx:1872-1944](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsPanels.tsx#L1872).

Recommendation. Decide explicitly whether Park remains a session-lifecycle operation. If parity is wanted, add a separate settled placement and auto-settle policy. Explain the session effect before treating Park as the equivalent of T3 Settle.

Observable acceptance check. Document and exercise Settle or Park during a running turn, while grouped, after inactivity and after merge. The observed session and membership behavior must match the labels and chosen product policy.

## Supported T3 capabilities and Ferrite coverage

| Capability | Ferrite status | Qualification |
| --- | --- | --- |
| Flat inbox cards and lower shelves | Implemented | ADR 0040 intentionally replaces the tree. |
| Pending requests prioritized | Implemented | Active ordering checks pending decisions and needs-you queue. |
| Working and snoozed shelves | Implemented | Fold settings persist; selected rows survive folding. |
| Pinned threads | Implemented with gap | Insertion order persists; direct reorder is absent. |
| Title search and parked search | Implemented with gaps | Title substring only; search keyboard and dynamic selection defects above. |
| Project-scoped list | Partial | One ephemeral scope, static picker. |
| Per-thread rename | Partial | Full rows work; slim-row renderer omits the editor. |
| Multi-selection | Partial | Full cards and Park only. |
| Snooze and wake | Implemented | Time-based shelf and undo paths exist. |
| Unread mark and foreground acknowledgment | Implemented with scope difference | Ferrite marks existing notices; visible threads are immediately seen. |
| Archive/restore analogue | Implemented with lifecycle difference | Park unloads and closes sessions; T3 distinguishes settle from archive. |
| Delete and guarded worktree cleanup | Implemented with policy difference | Registered worktrees survive thread deletion. |
| Project rename and directory management | Implemented, hard to discover | Palette-only management route in inspected navigation. |
| Existing and new worktree starts | Implemented | Context action does not preserve the originating checkout automatically. |
| Navigation Back/Forward | Implemented | Thread/group/subject history, not every app page. |
| Native multiwindow thread detach | Unverified comparison | Neither inspected UI establishes a corresponding direct operation. |

The table's positive Ferrite claims are traced above. T3 search, action and scope comparisons are cited in findings 005, 008, 010, 011, 012, 013 and 014. A T3 implementation does not automatically establish a mandatory Ferrite requirement.

## Positive behavior worth preserving

- The nav visibly maps to the current thread, and folded shelves retain the selected row. Normal list keyboard movement requests reveal. See `cockpit.rs:8805-8814`, `cockpit/nav_list.rs:91-103` and `cockpit/nav_keys.rs:121-138` under `crates/ferrite-cockpit/src/`.
- The shelf fold state and pin order persist through settings changes. See `crates/ferrite-cockpit/src/cockpit/nav_list.rs:64-86` and `crates/ferrite-cockpit/src/cockpit.rs:7243-7253`.
- Rename rejects blank titles and reports store failures through `group_error`. The store update precedes updating the live title. See `crates/ferrite-cockpit/src/cockpit.rs:3417-3441` and `crates/ferrite-core/src/engine.rs:3088-3110`.
- Store flush failures keep the live session owner and retry buffer reachable. Worktree deletion does not silently remove registered project inventory. See `crates/ferrite-core/src/engine.rs:2308-2312,2201-2204`.
- A normal card click waits for the click rather than activating on press, preserving drag-to-board behavior. See `crates/ferrite-cockpit/src/cockpit/nav_list.rs:325-338`.
- Navigation already has real motion. Keyed card/slim transitions, leaving ghosts, hover state and Undo notice fades exist. Reduced motion is passed to the nav animation engine. Shelf folds intentionally snap. See `crates/ferrite-cockpit/src/cockpit/nav_list.rs:69-76,190-224` and `crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:39-49,351-356`. The motion worker owns whether timing, interruption and visual execution satisfy the user's preference.

## Verification gaps and targeted runtime checks

1. Exercise the slim-row rename failure first. It has a complete source chain from visible menu through an undrawn editor. Inspect rendered bounds, actual focus, typing, Enter and Escape. Keep input-focus ownership with that report for any broader dispatch failure.
2. Exercise search with at least 30 matching rows in a short window. Check initial scroll position, all arrow steps, query reset and Enter. Then change thread status while a nonfirst result is highlighted. The existing basic search test has three threads and one match, so it cannot cover offscreen selection. See `crates/ferrite-cockpit/src/cockpit/tests/nav_shelves.rs:173-223`.
3. Exercise Undo after parking one member of a two-thread group. The existing Undo test parks an ordinary thread and checks only revival and notice removal. It does not assert group restoration. See `crates/ferrite-cockpit/src/cockpit/tests/nav_shelves.rs:229-254`.
4. Inject controlled reopen and partial-park failures in an isolated store. Confirm which errors the remote server may separately report. Do not infer that a lower-level log or transcript banner supplies adequate batch feedback.
5. Exercise cross-project navigation with a filter through the palette, history and a deliberate linked-thread action. Background-focus behavior is owned by another slice. Confirm that only an explicit move widens scope if that is the chosen fix.
6. Verify project changes and pin/shelf persistence after restart. Verify paths and branches for all five workspace choices, including missing external worktrees, dirty legacy worktrees and two threads sharing a worktree. No live git mutation was performed in this investigation.
7. Verify two clients connected to one server and compare independent selection, history, unread acknowledgment and closure behavior. No native multiwindow parity claim is warranted from a startup `open_window` call alone.
8. Verify screen-reader semantics, keyboard focus continuity and drag destinations in the input report. The source-defined affordance and unit test are not proof that the installed window dispatches the pointer event correctly.

Existing tests provide useful coverage of selection, card geometry, basic search, shelf opening, pin insertion, ordinary reopening, F2 rename on full cards and delete confirmation. Examples are `crates/ferrite-cockpit/src/cockpit/tests/nav_parity.rs:15-46`, `tests/nav_list.rs:16-69,501-593,697-728,782-826,952-983,1011-1066`, and `tests/nav_shelves.rs:173-254`. These files were inspected, not executed. No green-test claim is made.

## Recommended order

Repair the invisible slim-row rename first. Next repair search reveal and identity, reopen feedback and group-aware Undo. Then improve project scope and contextual project management, followed by metadata/content search and complete batch actions. Treat pinned reorder and checkout inheritance as separate parity tasks. Decide Park versus Settle semantics before adding automatic settlement, because a lifecycle action that closes sessions should not be automated merely to imitate a visual shelf.

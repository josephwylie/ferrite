# Panel, files, changes, terminal, and preview audit

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user's goal is a detailed inventory of broken everyday workflows and missing T3 capabilities, including an awkward right panel. This report traces those tools through their entry points, state, renderers, persistence, and backend operations.

## Overview

Status is ISSUES. The inspected implementation has a usable shared outer panel and substantial working tool support. The largest gaps are inside those tools. Files retains a second tab system and unsafe save semantics. Changes reuses a renderer designed for abbreviated Overview hunks. Several filesystem and Git failure paths discard their errors.

This report records 18 findings. Two have conditional data-loss paths and use P0 under the audit's stated priority rule. This does not mean ordinary editing always loses data. Seventeen findings are source-proven and one layout finding is inferred. None is runtime-reproduced.

Ferrite is pinned at `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. T3 Code is pinned at `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, inspected in `/tmp/ferrite-t3-audit-source-20261113`. Ferrite HEAD matched the requested commit. The only working-tree addition at inspection was this audit directory. All Ferrite citations refer to that baseline. T3 links below are commit-pinned.

Method was read-only source inspection with `rg`, numbered source reads, and call-site tracing. No application code, settings, data, dependencies, or branches changed. No live UI actions, tests, builds, or benchmarks ran. No performance measurements are claimed. Historical T3 specifications informed intent only. Current T3 code supplies comparative evidence.

Throughput checkpoint is n/a, read-only investigation.

## Key concepts

- A `SurfaceId` is an outer panel tab. `ThreadSurfaces` stores tab order, active surface, open state, width, and maximized state per thread. `panel.rs:254-265` funnels ordinary tool opening into `upsert_surface`. The persistence schema is in `surfaces.rs:304-314`.
- Files is one outer surface containing its own `Vec<Tab>`, active index, tree, and editor settings. Those inner tabs are not the outer `SurfaceId::File` preview tabs. See `tools/files.rs:70-100`.
- Changes and a pull request's Code view use separate `Diff` keys with distinct scopes, but share the same `Changes` and hunk renderer. PR operations and PR reachability belong to other audit slices. See `tools/changes.rs:65-85,215-218,840-845`.
- A terminal outer tab owns one native PTY view. Closing it ends that shell. Restart deliberately excludes terminal tabs from restoration. See `tools/terminal.rs:50-61,162-185` and `panel.rs:124-141,314-325`.
- Preview documents and editable file tabs have different opening pipelines. Checkout text previews redirect into Files. Images use a lightbox when opened through Preview, and non-text tree openings bypass that path. See `tools/files.rs:271-293` and `panel/preview.rs:1-19`.

The paths shortened above are under `crates/ferrite-cockpit/src/cockpit/`.

## Traced flows

### Panel entry, activation, close, and restoration

The plus menu enumerates Browser, Terminal, Files, Changes, Pull request, Side chat, Background, and Overview. Local tools are disabled with a checkout requirement when unavailable. The menu dispatches Files and Changes through `upsert_surface`, and Terminal through `new_terminal_surface`. The body renderer selects the active surface. See `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:24-88,134-172` and `crates/ferrite-cockpit/src/cockpit/panel.rs:624-685`.

Outer tabs support close, close others, close to the right, close all, copy path for file previews, drag reordering, middle-click close, and horizontal overflow navigation. See `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:194-245,397-433,436-560`. The narrow input behavior and exact event propagation are owned by the input-focus worker.

The save path writes `panel.json` with atomic replacement. Loading deduplicates surviving IDs and chooses a remaining active tab when needed. It intentionally drops live browser, terminal, side-chat, and agent resources. See `crates/ferrite-cockpit/src/cockpit/surfaces.rs:342-405` and `crates/ferrite-cockpit/src/cockpit/panel.rs:124-141`. This is real outer-panel persistence, but it does not restore the inner Files working set.

ADR 0039 calls for one tabbed host and removal of per-surface strips. Files still has a second strip. The browser worker also identified an internal browser strip at `crates/ferrite-cockpit/src/cockpit/browser.rs:1355-1405,1490-1519`; browser-specific behavior remains in that report. The shared architectural inconsistency matters because an outer tab's close, overflow, restoration, and keyboard commands do not inherently apply to an inner tab.

### Open, edit, and save a file

The tree and file-link redirection call `open_in_files`. That activates Files, reveals the path in the tree, reuses an existing tab or reads a new file, and creates a temporary preview tab unless the caller asks to keep it. Typing promotes the temporary tab to a kept tab. Code uses the Editor. Markdown and prose use a rendered-only body. See `crates/ferrite-cockpit/src/cockpit/tools/files.rs:159-241,271-293`.

The Editor supports editing and autosave after 800 ms. It marks dirty on edit and emits an event. Save directly writes the cached text. Closing the inner tab invokes save after removing the tab. This complete route exposes the error and concurrency defects below. See `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:40-41,135-140,200-226` and `crates/ferrite-cockpit/src/cockpit/tools/files.rs:256-268`.

### Browse files

The tree loads directories lazily and computes Git status once on construction. Its filter traverses the filesystem to fuzzy-match paths. A tree context menu can attach context and copy path forms. Its refresh method is never called after construction by the inspected cockpit. See `crates/ferrite-cockpit/src/cockpit/tools/tree.rs:52-76,105-178` and `crates/ferrite-cockpit/src/cockpit/tools/files.rs:323-359`.

### Review changes and stage a file

Opening Changes initializes a filter and reads its default Uncommitted scope. A background job calls `review_against`, returns per-file hunks, and rejects stale asynchronous results using a generation counter. Scope and Refresh commands trigger rereads. The file header toggles folding, staging, reviewed state, or opens current source in Files. See `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:221-232,260-394,726-845`.

The renderer then reduces core diff rows to plain signed text with one context line, caps output at 120 displayed lines, and clips unwrapped content. This means the backend can load a complete diff while the visible panel still cannot review it completely. See `crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:21-26,271-329,334-425`.

### Open and use a terminal

The plus menu creates a PTY in the thread directory. Cmd+J reuses the last terminal surface or starts the first. A successful spawn creates a view, subscribes to events, and requests focus after the view is drawn. Copy, paste, select all, clear, and attaching selected output are wired. See `crates/ferrite-cockpit/src/cockpit/tools.rs:42-69` and `crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:93-149,188-264`.

A spawn failure disappears without feedback. An exited process removes the terminal and outer surface at `tools/terminal.rs:142-146`. That exit behavior is source-defined. Retaining terminal history after process exit is a possible product improvement, but this report does not claim T3 preserves it indefinitely. T3 also has an exit callback after writing a status message at [apps/web/src/components/ThreadTerminalDrawer.tsx:637-650](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L637).

## Source map

| Responsibility | Ferrite evidence | Current T3 evidence |
| --- | --- | --- |
| Outer tabs and storage | `crates/ferrite-cockpit/src/cockpit/surfaces.rs:293-405`; `panel/strip.rs:194-245` | [apps/web/src/rightPanelStore.ts:812-844,1190-1204](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L812) |
| Geometry and narrow windows | `crates/ferrite-cockpit/src/cockpit/panel.rs:149-177,606-619` | [apps/web/src/components/chat/ChatRightPanel.tsx:14-49](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatRightPanel.tsx#L14) |
| File tree | `crates/ferrite-cockpit/src/cockpit/tools/tree.rs:43-178` | [apps/web/src/components/files/FileBrowserPanel.tsx:420-432](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FileBrowserPanel.tsx#L420) |
| File editor and persistence | `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:66-140` | [apps/web/src/components/files/fileSaveSessions.ts:46-161](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/fileSaveSessions.ts#L46) |
| Preview mode dispatch | `crates/ferrite-cockpit/src/cockpit/tools/files.rs:201-241` | [apps/web/src/components/files/FilePreviewPanel.tsx:1209-1242](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1209) |
| Changes load and mutations | `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:221-394` | [apps/web/src/components/DiffPanel.tsx:329-390](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L329) |
| Changes rendering | `crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:334-425` | [apps/web/src/components/DiffPanel.tsx:1197-1220,1515-1542](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L1197) |
| Terminal lifecycle and content | `crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:93-264` | [apps/web/src/components/ThreadTerminalDrawer.tsx:975-1024,1763-1786](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L975) |

Short Ferrite paths in this table share the full directory of the preceding Ferrite path in the same cell. Each finding below uses full paths.

## Detailed findings

### panel-tools-001. A failed file save is silent and closing the file discards the edited buffer

Priority is P0. Category is `defect`. Evidence level is `source-proven`.

An operator can lose unsaved work when the file becomes unwritable or its parent directory disappears. P0 follows this audit's data-loss rule; this is conditional, not a claim that ordinary saves fail.

Trigger. Edit a source file, make its destination unwritable, then press Save or close the inner file tab.

Expected behavior. Show the save failure and retain the edited buffer until a successful save or an explicit discard.

Actual behavior. save() returns no result and ignores write failures. close_tab removes the tab before calling save, then drops it regardless of the outcome.

Cause. The editor has only a dirty boolean and performs std::fs::write(...).is_ok(). The close path cannot distinguish success from failure.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:135-140`.
- `crates/ferrite-cockpit/src/cockpit/tools/files.rs:256-268`.

T3 comparison evidence.

- [apps/web/src/components/files/fileSaveCoordinator.ts:67-88](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/fileSaveCoordinator.ts#L67).
- [apps/web/src/components/files/fileSaveSessions.ts:46-59,96-120,153-161](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/fileSaveSessions.ts#L46).

Recommendation. Return structured save results, report failures, keep the tab and draft on failure, and make close/save decisions explicit. Use atomic replacement to avoid partial-write corruption.

Observable acceptance check. In a disposable checkout, deny writes after editing. Save displays the filesystem error. Closing preserves the exact draft and offers retry or explicit discard. Restore permission and verify that retry writes the draft.

### panel-tools-002. Cached editor buffers can overwrite newer agent or other-thread changes

Priority is P0. Category is `defect`. Evidence level is `source-proven`.

An agent and operator often edit the same checkout. A later save from an old file buffer can erase the agent's changes. Two Ferrite threads pointing at one checkout also create independent buffers.

Trigger. Open a file in Files, change it externally or in another thread, then edit or save the original tab.

Expected behavior. Reload clean buffers and detect a changed disk revision before saving a dirty buffer. Concurrent editors should share a file identity or reconcile changes.

Actual behavior. Existing tabs are only activated. Files are read once when creating a tab. Saving writes the entire cached text without checking disk contents, modification time, or revision.

Cause. Files and Editor state are per ThreadId and path. There is no file watcher, revision token, reload path, or shared save queue in these modules.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools.rs:20-26`.
- `crates/ferrite-cockpit/src/cockpit/tools/files.rs:159-191,201-241`.
- `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:66-83,135-140`.

T3 comparison evidence.

- [apps/web/src/components/files/FilePreviewPanel.tsx:1322-1335](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1322).
- [apps/web/src/components/files/fileSaveSessions.ts:46-59,135-161](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/fileSaveSessions.ts#L46).

Recommendation. Track disk revisions, refresh clean buffers on workspace changes, and block or reconcile conflicting saves. Coordinate writes by canonical workspace and file, not by thread alone.

Observable acceptance check. Open the same file in two threads sharing a checkout. Change a different line in each, and repeat with an external agent edit. No successful save silently removes the other edit; a conflict is visible and recoverable.

### panel-tools-003. The file tree never refreshes cached directory entries or Git status

Priority is P1. Category is `defect`. Evidence level is `source-proven`.

Files created, deleted, or renamed by an agent are missing or stale in an already opened tree. Git colors remain based on its initial snapshot.

Trigger. Open Files and expand a directory, then create or delete a file or change Git status outside the tree.

Expected behavior. Workspace changes refresh affected entries and status, with a manual refresh fallback.

Actual behavior. FileTree::new calls refresh once. children caches listings. No .tree.refresh caller exists in the cockpit, and Files has no Refresh action. Closing the Files surface retains its Files object.

Cause. The refresh method is not wired into ongoing workspace events or a visible command.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/tree.rs:43-76,135-140`.
- `crates/ferrite-cockpit/src/cockpit/tools/files.rs:118-154,44-58`.
- `crates/ferrite-cockpit/src/cockpit/panel.rs:314-325`.

T3 comparison evidence.

- [apps/web/src/components/files/FileBrowserPanel.tsx:64-80,420-432](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FileBrowserPanel.tsx#L64).

Recommendation. Invalidate directories and Git status on workspace mutation and expose Refresh. Keep expanded paths and selection while applying updates.

Observable acceptance check. With Files already open, create, rename, delete, and modify files through a terminal or agent. Tree rows and status update without restarting or changing threads; manual refresh also works.

### panel-tools-004. File discovery silently omits entries beyond fixed limits

Priority is P2. Category is `partial-capability`. Evidence level is `source-proven`.

Large directories and repositories can contain files that the tree or fuzzy search never exposes, without a truncation notice.

Trigger. Expand enough directories to exceed 1,000 visible rows, or search a checkout containing more than 20,000 eligible files.

Expected behavior. Virtualize or page all results, or show an explicit limit with a way to continue.

Actual behavior. The tree renderer takes only 1,000 rows. Search stops after 20,000 files, before sorting the matches it has found. There is no continuation or limit notice in the rendered tree.

Cause. Hard cutoffs substitute for virtualized traversal and rendering.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/files.rs:735-750`.
- `crates/ferrite-cockpit/src/cockpit/tools/tree.rs:13-16,143-178`.

This limit is a Ferrite capability gap. This finding makes no claim that T3 has unlimited directory traversal.

Recommendation. Use incremental directory traversal and virtualized rows. If a limit remains, expose it and allow explicit loading or exact-path opening.

Observable acceptance check. Use a fixture with 1,100 files in one folder and a target after the 20,000th search entry. The final tree entry and target are reachable, or the UI explicitly provides continuation.

### panel-tools-005. Nested file tabs overflow into an inaccessible clipped region

Priority is P1. Category is `interaction`. Evidence level is `source-proven`.

Opening many files can put the active file's tab and close control offscreen. The unified panel strip's overflow controls do not control this separate strip.

Trigger. Keep enough long-named files open to exceed the Files header width.

Expected behavior. All file tabs remain reachable by scrolling or an overflow menu, and the active tab is revealed.

Actual behavior. Each inner file tab refuses to shrink. Its parent uses overflow_hidden with no scroll handle or chevrons. File selection is a separate inner-tab mechanism below the outer Files surface.

Cause. Files retained a second tab system rather than representing each editable file as a panel surface.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/files.rs:467-543`.
- `crates/ferrite-cockpit/src/cockpit/tools/files.rs:244-253`.
- `docs/adr/0039-the-panel-is-one-tabbed-surface-host.md:15-29`.

T3 comparison evidence.

- [apps/web/src/rightPanelStore.ts:812-844](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L812).

Recommendation. Move editable files into the shared panel surface list. If retaining an inner strip, add overflow navigation and automatic active-tab reveal with the same keyboard and close semantics.

Observable acceptance check. Open 20 kept files at a narrow panel width. Reach, activate, and close every tab by mouse and keyboard. A newly opened or reactivated file always has a visible selected tab.

### panel-tools-006. Restart restores the Files shell but loses its open files and tool preferences

Priority is P2. Category is `partial-capability`. Evidence level is `source-proven`.

Operators lose their file working set, selected file, explorer visibility, editor preferences, Changes scope, and review state after restarting.

Trigger. Open several editable files, change tool settings, choose a Changes scope, then restart Ferrite.

Expected behavior. Persist the useful working set and chosen preferences, or explicitly state which live resources cannot be restored.

Actual behavior. panel.json stores outer surface IDs and geometry. Ordinary checkout text files become inner Files tabs, which are absent from this schema. ensure_files starts empty with default settings; Changes also starts with defaults.

Cause. The persistence boundary stops at ThreadSurfaces while editor and review state live in nonserialized Tools maps.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/surfaces.rs:304-314,342-354`.
- `crates/ferrite-cockpit/src/cockpit/tools/files.rs:138-149,275-293`.
- `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:145-159`.

T3 comparison evidence.

- [apps/web/src/rightPanelStore.ts:812-844,1190-1204](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L812).
- [apps/web/src/components/files/FilePreviewPanel.tsx:1348-1357](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1348).

Recommendation. Persist canonical file surfaces, active file, and stable tool preferences. Define separate restoration rules for process resources and unsaved drafts.

Observable acceptance check. After restart, restore saved file tabs and active file, chosen tree visibility and wrapping, and the intended review scope. Unsaved drafts must be recovered or safely flushed, never silently lost.

### panel-tools-007. Markdown and prose files cannot be edited or navigated to a requested source line in Files

Priority is P2. Category is `partial-capability`. Evidence level is `source-proven`.

README files, instructions, and text documents open as rendered prose with no source-edit mode. Source links to their line numbers lose their target after redirecting into Files.

Trigger. Open README.md or notes.txt from the tree, or follow a checkout Markdown link with a line number.

Expected behavior. Offer rendered and source views, allow editing, and reveal a requested source line.

Actual behavior. tab_body creates a Document with line=None and returns TabBody::Prose whenever rich_source exists. Only TabBody::Code receives go_to_line. The prose body is a Markdown renderer with no source toggle.

Cause. File classification makes prose permanently read-only and discards the requested line before constructing its body.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/files.rs:192-196,201-228,668-687`.
- `crates/ferrite-pane/src/preview.rs:592-606`.

T3 comparison evidence.

- [apps/web/src/components/files/FilePreviewPanel.tsx:1211-1235,1648-1658](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1211).

Recommendation. Separate presentation mode from editability. Preserve line intent and switch to source view for line links, with an explicit render/source toggle.

Observable acceptance check. Edit and save README.md and notes.txt. Follow README.md:80 from a conversation and verify source line 80 is visible and highlighted; switching back to rendered view preserves the file.

### panel-tools-008. Opening image or document files from the tree bypasses the available preview path

Priority is P2. Category is `partial-capability`. Evidence level is `source-proven`.

The Files tree tells users that an image is not text instead of showing it. PDFs, audio, video, and document previews also lack the richer in-panel handling available in T3.

Trigger. Click a PNG or PDF in Files, then compare opening the same PNG through a conversation preview.

Expected behavior. Dispatch files to the appropriate viewer, with a useful fallback for unsupported formats.

Actual behavior. The Files tab path reads all bytes as UTF-8 and returns 'This is not a text file' on failure. Ferrite's separate Preview supports an image lightbox, but the tree does not route through it.

Cause. The editor's text loader acts as the only file-type dispatcher for tree openings.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/files.rs:178-189,201-212,688-696`.
- `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:1-6`.
- `crates/ferrite-pane/src/preview.rs:49-81`.

T3 comparison evidence.

- [apps/web/src/components/files/FilePreviewPanel.tsx:193-225,244-287,335-382,1596-1610](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L193).

Recommendation. Unify file opening around file-type dispatch. Reuse Ferrite's image viewer from Files and prioritize PDF, media, and table previews based on actual workflows.

Observable acceptance check. Open a PNG from both tree and transcript and reach equivalent image controls. Open a PDF and receive a preview or a specific supported fallback, not a generic text-decoding failure.

### panel-tools-009. Changes cuts each file at 120 displayed lines with no expansion action

Priority is P1. Category is `defect`. Evidence level is `source-proven`.

A primary review view hides later changes in a large file. A user can mark the file reviewed without having a way to inspect its omitted hunks in that view.

Trigger. Open Changes for a file with more than 120 displayed diff and context lines.

Expected behavior. Display all hunks through virtualization or provide a working load-more/full-diff action.

Actual behavior. Changes calls the Overview hunk renderer. It stops at MAX_LINES=120 and adds inert text '… +N lines'. That text has no click handler. The file's open action goes to current source, which does not reveal removed content.

Cause. A deliberately abbreviated Overview renderer was reused as the full Changes renderer.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:21-26,364-425`.
- `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:819-845`.

T3 comparison evidence.

- [apps/web/src/components/DiffPanel.tsx:1505-1525](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L1505).

Recommendation. Give Changes a full diff renderer with lazy hunk loading or virtualization. Keep summary caps only in explicitly abbreviated Overview cards.

Observable acceptance check. Create a diff with more than 200 changed lines and a distinctive deletion near the end. The user can inspect that deletion in Changes and reach every omitted hunk before marking the file reviewed.

### panel-tools-010. Disabling diff wrapping clips long lines instead of allowing horizontal scrolling

Priority is P2. Category is `defect`. Evidence level is `source-proven`.

Long changed expressions become unreadable once wrap is disabled, even though that is the intended mode for comparing code alignment.

Trigger. Disable Wrap in Changes for a line wider than the panel.

Expected behavior. Provide horizontal scrolling when wrapping is off.

Actual behavior. The hunk area uses whitespace_nowrap, and both rows and text containers use overflow_hidden. No horizontal scroller is attached.

Cause. The shared summary renderer interprets no-wrap as truncation.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:334-357,393-410`.
- `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:365,840-845`.

T3 comparison evidence.

- [apps/web/src/components/DiffPanel.tsx:1515-1519](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L1515).

Recommendation. Use a horizontal scroll container for unwrapped diffs and preserve readable line-number and sign gutters.

Observable acceptance check. Put a changed token at column 250, disable wrapping, and scroll to inspect and select that token without resizing the whole window.

### panel-tools-011. Hide whitespace changes only removes blank changed lines

Priority is P2. Category is `defect`. Evidence level is `source-proven`.

Indentation-only and trailing-space changes still dominate reviews when the operator explicitly asks to hide whitespace changes.

Trigger. Reindent a nonblank line or change only its trailing spaces and enable Hide whitespace changes.

Expected behavior. Compare content while ignoring whitespace and suppress equivalent old/new lines.

Actual behavior. without_whitespace filters each changed line only when trim().is_empty(). It never compares the nonblank old and new text.

Cause. A blank-line filter implements a semantically broader whitespace option.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:190-202,365-366,840-845`.

T3 comparison evidence.

- [apps/web/src/components/DiffPanel.tsx:373-384](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L373).
- [apps/server/src/vcs/GitVcsDriverCore.ts:3748,3797](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/vcs/GitVcsDriverCore.ts#L3748).

Recommendation. Compute the diff using an ignore-whitespace option and keep displayed statistics consistent with it.

Observable acceptance check. Use separate indentation-only, trailing-space-only, and substantive edits. With the option on, only substantive changes remain; turn it off to restore all changes.

### panel-tools-012. Changes stays stale after workspace edits and reviewed marks survive changed contents

Priority is P2. Category is `defect`. Evidence level is `source-proven`.

The review can show yesterday's loaded content while an agent keeps editing. A reviewed checkmark can remain on a file whose contents changed or whose comparison scope changed.

Trigger. Open Changes, mark a file reviewed, change it externally, then revisit or manually refresh. Also switch comparison scope.

Expected behavior. Refresh on relevant workspace mutations and invalidate reviewed state when the reviewed diff revision changes.

Actual behavior. ensure_diff reads only on initial insertion. Later opens do not reread. Manual refresh and scope changes replace load data but preserve reviewed and folded sets keyed solely by path.

Cause. The load lifecycle lacks workspace invalidation, and review identity is only a pathname.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:221-232,284-329,345-357,385-394`.
- `crates/ferrite-cockpit/src/cockpit/panel.rs:254-265`.

T3 comparison evidence.

- [apps/web/src/components/DiffPanel.tsx:350-390,549-556](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L350).

Recommendation. Refresh on a workspace revision signal and associate reviewed state with a diff/content identity. Preserve scroll and folds independently of review validity.

Observable acceptance check. Mark a file reviewed, modify it, and return to Changes. Updated hunks appear and the reviewed mark clears. A changed comparison base does not inherit unrelated reviewed marks.

### panel-tools-013. Changes lacks full diff navigation and comparison controls present in T3

Priority is P2. Category is `missing-capability`. Evidence level is `source-proven`.

Large code reviews require more scanning and external tooling. The panel cannot show old and new code side by side, reveal a file through a changes tree, or select a particular completed turn or commit.

Trigger. Review a multi-file refactor, compare one agent turn, or inspect one historical commit from the panel.

Expected behavior. If full T3 review parity is the product goal, provide split/stacked modes, a changed-file navigator, line numbers, syntax detail, and turn/commit scopes.

Actual behavior. Changes has All, Uncommitted, and named-branch scopes only. Its renderer has a sign and plain text, one context line, no source line numbers, no split layout, and no changes tree.

Cause. The panel implements a compact hunk summary plus file actions rather than a full code-review reader. Ferrite's separate Review slot is not evidence that these panel controls exist.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:5-17,36-62`.
- `crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:21-26,44-49,303-327,334-410`.

T3 comparison evidence.

- [apps/web/src/components/DiffPanel.tsx:329-364,1197-1220,1515-1542](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L329).

Recommendation. Build on the existing review core while giving the panel a full diff presentation and explicit scope model. Treat turn history as a separate product decision if checkpoints are not available.

Observable acceptance check. Switch stacked/split views, jump to a changed file, read old/new line numbers and syntax, and compare a selected commit or completed turn without leaving the panel. If a capability is deferred, document the supported panel scope.

### panel-tools-014. Stage and unstage failures are discarded

Priority is P2. Category is `defect`. Evidence level is `source-proven`.

A Git lock, permission error, or repository failure makes the stage button appear to do nothing. The operator cannot tell whether the operation ran or what to fix.

Trigger. Press Stage while the index is locked or otherwise unwritable.

Expected behavior. Show pending state, inspect the exit status, and present the Git error with retry.

Actual behavior. stage_file discards Command::output completely and refreshes regardless of success. staged_paths also treats failed command output as empty status.

Cause. The asynchronous mutation returns no result to the UI.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:180-187,260-280,806-816`.

T3 comparison is not required for this defect. The expected behavior follows the action's own semantics.

Recommendation. Return a typed result from staging, display stderr on failure, prevent duplicate in-flight actions, and refresh only with a clear operation outcome.

Observable acceptance check. In a disposable repository, create a temporary index lock, click Stage, and see a precise error. Remove the lock and retry successfully. Do the equivalent for Unstage.

### panel-tools-015. Terminal startup failures leave no visible result

Priority is P1. Category is `defect`. Evidence level is `source-proven`.

Terminal is a primary debugging fallback. A failed shell launch makes its menu item and shortcut look broken.

Trigger. Open Terminal when its checkout cannot be entered or its shell cannot launch.

Expected behavior. Open an error state with a useful reason and retry or shell selection.

Actual behavior. new_terminal_surface matches Terminal::spawn with let Ok and returns immediately on failure, before creating any surface or notification.

Cause. The spawn error is discarded at the UI boundary.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:93-116`.

T3 comparison evidence.

- [apps/web/src/components/ThreadTerminalDrawer.tsx:194-207,1079-1088](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L194).

Recommendation. Represent terminal startup as loading, running, or failed. Retain the failed surface so the operator can read the error and retry.

Observable acceptance check. Force a shell or cwd startup failure in a disposable thread. Both the menu and shortcut display the same actionable failure and recover after fixing the cause.

### panel-tools-016. Terminal surfaces cannot split into simultaneous shells

Priority is P2. Category is `missing-capability`. Evidence level is `source-proven`.

Running a server and a command side by side requires switching tabs or using an external terminal.

Trigger. Open two shells and attempt to view them together in the panel.

Expected behavior. For T3 terminal parity, support horizontal and vertical split groups with independent focus and close behavior.

Actual behavior. Each Terminal SurfaceId identifies one TerminalView. There is no split-group structure or split action in the terminal tools.

Cause. The terminal model is a flat numbered list.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:50-61,174-185,242-292`.
- `crates/ferrite-cockpit/src/cockpit/surfaces.rs:29-42`.

T3 comparison evidence.

- [apps/web/src/rightPanelStore.ts:42-47,165-173](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L42).
- [apps/web/src/components/ThreadTerminalDrawer.tsx:1763-1786](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L1763).

Recommendation. Add split groups only if this everyday multitasking workflow is in scope; retain simple single-terminal tabs as the default.

Observable acceptance check. Run a server in one split and commands in the other, resize each pane, focus by keyboard, and close one pane without terminating its sibling.

### panel-tools-017. Terminal output has no URL or file-path opening flow

Priority is P2. Category is `missing-capability`. Evidence level is `source-proven`.

Users must copy a localhost URL or compiler file path manually instead of opening the browser or file directly from terminal output.

Trigger. Print a localhost URL and a file:line path, then attempt the normal link-opening gesture.

Expected behavior. Recognize links and route them to a browser or file preview, with a consistent external-open modifier.

Actual behavior. Ferrite's terminal mouse handlers support cell selection and copying. There is no link detector or activation handler in the terminal view or cockpit terminal tool.

Cause. Terminal rendering operates on cells without a link-routing layer.

Ferrite evidence.

- `crates/ferrite-terminal/src/view.rs:570-612`.
- `crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:119-149,242-264`.

T3 comparison evidence.

- [apps/web/src/components/ThreadTerminalDrawer.tsx:975-1024](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L975).

Recommendation. Parse URLs and file locations from terminal text or terminal hyperlink metadata and use the existing browser/file entry points.

Observable acceptance check. Print a localhost address, an HTTPS URL, and a relative path with a line number. Open each with the documented gesture and verify the correct destination and line.

### panel-tools-018. Panel sizing cannot adapt to narrow layouts or recover a smaller width after wide tools

Priority is P2. Category is `interaction`. Evidence level is `inferred`.

The panel can consume disproportionate space on a small window. Opening Files or Changes permanently raises the saved width until the operator resizes it, even after returning to Overview.

Trigger. Resize the window narrowly, open Files, then return to Overview and compare the chat space.

Expected behavior. Keep the active tool usable and preserve useful chat space through a narrow-layout presentation and predictable width restoration.

Actual behavior. Ferrite always renders an absolute fixed-width side panel. Width is clamped to a surface floor and a minimum most_w of 320. tend_panel writes the wider floor into the shared thread width. There is no sheet breakpoint in this layout.

Cause. One stored width plus persistent minimum expansion encodes every tool's sizing. The exact visible clipping on the installed app remains unverified.

Ferrite evidence.

- `crates/ferrite-cockpit/src/cockpit/panel.rs:149-177,368-383,606-619`.
- `crates/ferrite-ui/src/theme.rs:989,2027-2029`.

T3 comparison evidence.

- [apps/web/src/rightPanelLayout.ts:1-3](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelLayout.ts#L1).
- [apps/web/src/components/chat/ChatRightPanel.tsx:14-49](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatRightPanel.tsx#L14).
- [apps/web/src/components/RightPanelSheet.tsx:20-25](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelSheet.tsx#L20).

Recommendation. Define narrow-window behavior and preserve a user-chosen compact width separately from temporary tool minimums. Consider a sheet or explicit panel-only mode.

Observable acceptance check. At 800, 1,000, and 1,400 pixel window widths, open each tool and return to Overview. All controls remain reachable and chat remains usable; temporary tool expansion does not silently replace the operator's compact width.

## Supported T3 capabilities relevant to this slice

These are source-backed capabilities, not a requirement that Ferrite copy every design choice.

- T3 opens each ordinary file as a shared panel surface, updates line reveal requests on reuse, and persists surface metadata. See [apps/web/src/rightPanelStore.ts:812-844,1190-1204](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L812).
- T3 provides explicit file refresh and mutation-driven explorer refresh. Text file reads also revalidate after workspace mutations when there is no pending local edit. See [apps/web/src/components/files/FileBrowserPanel.tsx:420-432](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FileBrowserPanel.tsx#L420) and [apps/web/src/components/files/FilePreviewPanel.tsx:1322-1335](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L1322).
- T3 has one ordered save session per environment, cwd, and file. The final surface release flushes that session, and unconfirmed contents are recoverable from query state. This is evidence of stronger coordination, not proof that T3 detects every external filesystem conflict. See [apps/web/src/components/files/fileSaveSessions.ts:46-161](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/fileSaveSessions.ts#L46).
- T3 can switch rendered Markdown, Mermaid, table, and HTML modes, and dispatches PDFs and other documents separately. It also has image, audio, and video preview components. See [apps/web/src/components/files/FilePreviewPanel.tsx:193-382,1209-1235,1596-1610](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.tsx#L193).
- T3 has split/stacked diff layouts, a resizable changed-file tree, horizontal scrolling for unwrapped diffs, and selected-turn and selected-commit scope state. See [apps/web/src/components/DiffPanel.tsx:329-364,1197-1220,1515-1542](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/DiffPanel.tsx#L329).
- T3 supports horizontal and vertical terminal splits plus URL/file link dispatch. See [apps/web/src/components/ThreadTerminalDrawer.tsx:975-1024,1763-1786](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ThreadTerminalDrawer.tsx#L975).
- T3 has a separate sheet presentation for narrow layouts, with explicit width constraints. See [apps/web/src/components/chat/ChatRightPanel.tsx:14-49](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ChatRightPanel.tsx#L14) and [apps/web/src/components/RightPanelSheet.tsx:20-25](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelSheet.tsx#L20).

## Positive Ferrite behavior

- There is a working shared surface state model with deduplication, explicit activation, close-family operations, reopenable closed tabs, per-thread width and open state, and atomic persistence. This is substantial implemented support. See `crates/ferrite-cockpit/src/cockpit/surfaces.rs:175-264,342-405`.
- Plus-menu rows explain local checkout requirements instead of hiding unavailable tools. See `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:134-189`.
- Outer panel tabs have overflow chevrons and wheel scrolling. Their chevron animation obeys reduced motion. See `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:397-433`.
- Files supports fuzzy path filtering, preview-tab promotion on edits, code editing, autosave, a manual save action, configurable wrapping and tab size, attach-as-context, and path copying. See `crates/ferrite-cockpit/src/cockpit/tools/files.rs:159-241,323-368` and `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:40-62,200-226,476`.
- Changes supports All, Uncommitted, and named-branch comparisons, filter text, test-file hiding, folding, reviewed marks, and actual Git stage/unstage commands. Late results for an older scope cannot overwrite a newer generation. See `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:36-62,163-178,260-394,806-845`.
- Terminal support is a real native PTY with delayed focus handoff, copy/paste, selection, attached output context, cursor options, scrollback options, and configurable copy-on-select. See `crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:93-117,188-310`.
- Preview has separate text, page, annotation, unsupported-file, and image-lightbox paths. It retains opened preview documents for tab switching and supports find in preview. These capabilities should be reused in Files. See `crates/ferrite-pane/src/preview.rs:49-97` and `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:48-104,174-195`.

## Motion boundary

The panel module explicitly says width, maximize, and visibility changes snap rather than animate at `crates/ferrite-cockpit/src/cockpit/panel.rs:22-27`. Outer-tab chevron scrolling has a tween and reduced-motion handling at `panel/strip.rs:397-433`. These facts do not settle the user's request for useful motion. The motion worker owns recommendations and cross-product motion comparison, so no duplicate motion issue appears in this slice's JSON.

## Verification gaps and test coverage

No tests ran because this is a source investigation and no implementation changed. The user's narrow-test policy remains applicable to later fixes.

Existing Ferrite tests cover successful file autosave, preview replacement, prose rendering, menu contents, path copying, and link redirection at `crates/ferrite-cockpit/src/cockpit/tests/files.rs:51-188`. They do not establish failed-save recovery, external-update reconciliation, large-tree reachability, or file-tab overflow. Existing Changes tests cover menus, filtering, scopes, folding, and successful staging at `crates/ferrite-cockpit/src/cockpit/tests/changes.rs:52-209`. Existing outer-panel tests exercise restoration, outer-tab overflow, drag thresholds, maximize, and resize at `crates/ferrite-cockpit/src/cockpit/tests/tools.rs:83-348`. Passing these tests would not prove inner Files tabs share those behaviors. Terminal tests cover creation, multiple tabs, selection attachment, initial focus, and refocusing at `crates/ferrite-cockpit/src/cockpit/tests/terminal.rs:37-236`.

The parent can settle the most consequential runtime questions with disposable fixtures.

1. Reproduce a failed save and external-change overwrite in a disposable checkout. Do not use the user's real edited files.
2. Open 20 kept source files, then test active-tab reveal and every close target at a narrow panel width.
3. Open the tree before creating and deleting files, then verify the stale-directory behavior.
4. Load a diff exceeding 200 changed lines and inspect the last removed line. Toggle wrapping and whitespace filtering against known fixtures.
5. Mark a file reviewed, change it externally, refresh, and switch scopes. Check whether the checkmark still claims review of new content.
6. Force a terminal spawn failure with a disposable invalid shell or working directory and inspect feedback.
7. Check the installed build at 800, 1,000, and 1,400 pixels with Files, Changes, Terminal, and Overview.
8. Exercise the tree's PNG opening separately from a transcript image link.

There are additional performance risks, without measured regressions. Tree construction runs `git status` synchronously at `tools/tree.rs:52-76,212-223`. Fuzzy matching walks the filesystem while building rendered tree rows at `tools/files.rs:735-750` and `tools/tree.rs:143-178`. New file opening reads its full contents synchronously at `tools/files.rs:201-211`. Save also performs a synchronous write at `tools/editor.rs:135-140`. A representative large checkout and slow filesystem measurement is needed before assigning performance severity. No latency numbers are inferred from these call sites.

The separate full Review slot, PR controls, browser internals, global input ownership, and transcript file-link parsing are outside this slice. Their existence must not be used to declare the panel complete without tracing a usable route to them.

## Remediation order

First protect editor drafts and detect stale-buffer saves. Then make Files and Changes trustworthy while agents are working by wiring mutation refresh and explicit errors. Replace the summary diff renderer before adding review polish. Unify file surfaces so overflow, persistence, and keyboard commands have one owner. Add prose source editing and reuse the preview dispatcher. After those fixes, choose the remaining T3 capabilities, such as terminal splitting and richer preview formats, according to everyday usage. Verify narrow-window presentation and useful motion with the parent-owned live UI checks.


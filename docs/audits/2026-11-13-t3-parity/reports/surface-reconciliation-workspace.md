# Workspace tools: architecture and surface reconciliation

Ferrite already has many of the tool components. The gap is also how they are assembled: where they open, what they replace, which resource owns focus, what survives closure, and whether the conversation remains usable. A component or menu entry cannot stand in for that complete contract.

The most consequential overlooked distinctions are the standalone PR workspace versus thread detail; a bottom terminal drawer versus a terminal tab; shared file resource tabs versus Ferrite’s second inner tab strip; the Changes panel versus Ferrite’s existing full Review reader; and ordinary side-chat Close versus Delete.

## Baseline and evidence

- Installed T3: 1264c206aee0dce805d6279809118bc091f29d2f, version 0.1.10-nightly.20261111.4544. All T3 source links use that commit. [Provenance](t3-installed-source-match.md).
- Newer comparison checkout: 74573fc29b901bafbf8f1dbb2eb29606da1fa448. Changed files are marked in JSON. Newer source is not used to explain installed native behavior.
- Ferrite source and earlier controlled runtime baseline: 62947f91d5cd6ee494aaae9ed09d794a853e5bab.
- Runtime belongs to the parent’s [native ledger](../evidence/t3-live/observations.md) and the specifically linked Ferrite correction reports. This worker performed no UI, model, test, build or product-code edit.
- Fourteen surface records each state nine contract dimensions. WS IDs and eight WREQ records are reconciliation units, not new additions to the 177 findings.
- Native screenshots remain inline conversation evidence unless a prior report supplies an actual exported file. No screenshot path is invented here.

## Composition that the audit must preserve

On a normal T3 chat route, the conversation column and right workspace are siblings. The terminal drawer sits below the conversation inside that column. The floating thread-details card is another surface. Inside the right workspace, one resource body is selected beneath a shared strip; Files can add an explorer column inside that body. The standalone PR route replaces the conversation area with its own list/detail workspace and synthetic owner. It does not inherit every chat responsive rule.

Ferrite’s real counterpart is an absolute right panel with a shared strip and resource dispatcher. Files and Browser add inner tab hierarchies. Terminal is a right-panel tool. The full Review reader is a distinct board reader slot with its own geometry, focus and motion contract. These existing implementations are useful foundations, but they are not interchangeable placements.

Common composition anchors: [ChatView.tsx:3931][mount]; [ChatView.tsx:3915][drawer_mount]; [rightPanelStore.ts:37][surfaces]; [PullRequestsPage.tsx:1495][pr_layout]; [panel.rs:550][f_panel]; [panel.rs:627][f_dispatch]; [beside.rs:224][f_review].

| Record | Surface | Main distinction |
| --- | --- | --- |
| WS-01 | Workspace frame and geometry | Conversation sibling, width, visibility, focus and sheet host |
| WS-02 | Resource tabs and launcher | One resource strip, launcher and close semantics |
| WS-03 | Standalone Pull requests workspace | Global list/detail route with synthetic owner |
| WS-04 | Linked PR list and thread-native detail | Thread association list and explicit native PR target |
| WS-05 | Files explorer and file resource tabs | Explorer and file resource topology |
| WS-06 | Source, rendered documents and media | Editable source versus rendered/media body |
| WS-07 | Project content search and filename palette | Project-content modal versus path filter and thread Find |
| WS-08 | Diff workspace versus separate Review reader | Workspace diff versus existing separate Review reader |
| WS-09 | Bottom terminal drawer | Below-chat command region, independently coexisting |
| WS-10 | Workspace terminal tabs and splits | Right-panel PTY resource and split group |
| WS-11 | Side-chat workspace placement and lifetime | Embedded conversation, host routing and Close/Delete |
| WS-12 | Device tool and setup modal | Actual device session, not browser emulation |
| WS-13 | Browser resource host boundary | Resource host, separate from browser chrome audit |
| WS-14 | Transient menus, confirmations and sheets | Temporary layer ownership, cancellation and geometry |

No row is marked as a whole-feature pass. Each record below names the bounded observations and the variants still unexecuted.

## WS-01: Workspace frame and geometry

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Keep a usable conversation beside a predictable thread-owned tool destination. |
| Placement | ChatRightPanel is a sibling of the chat column. PreviewPanelShell owns divider, visibility and workspace region. The floating thread-details card is a separate surface. |
| Entry | Panel toggle, resource open, tab selection and maximize/restore. Explicit user actions request destination focus; automatic reveals have a separate revision boundary. |
| Coexistence | Inline tools share the row with conversation and navigation. Maximize reduces chat to zero width and makes it inert; bottom terminal is independently inside the chat column. |
| Focus | Named regions support F 6 and remembered focus. Collapse moves focus before making hidden content inert. |
| Motion | Shared presence retains same-thread exit content and cancels on reopen. Width transitions apply to visibility, not unrelated width changes. Installed duration defaults to zero; reduced motion and route suppression gate it. |
| Lifetime | Hide retains tab identity. Close releases resources by kind. A visually hidden surface is not necessarily a stopped process. |
| Persistence | Thread width defaults to 540 CSS px, minimum 360, with viewport/sibling caps. Drag commits width; temporary clamping is separate. |
| Responsive | Chat uses a right sheet at max-width 980 unless maximized. Sheet has its own 760 px rule. This is not the standalone PR responsive path. |

Installed source: [ChatRightPanel.tsx:19][host]; [ChatView.tsx:3931][mount]; [ChatView.tsx:3286][maximize]; [PreviewPanelShell.tsx:65][shell]; [PreviewPanelShell.tsx:132][shell_focus]; [PreviewPanelShell.tsx:78][shell_motion]; [usePreviewPanelInlineSize.ts:13][size]; [useResizableWidth.ts:63][resize]; [rightPanelLayout.ts:1][breakpoint]; [RightPanelSheet.tsx:20][sheet]; [sheet.tsx:74][sheet_motion]; [panelAnimations.ts:31][motion]; [panelAnimations.ts:41][presence]; [workspaceFocus.ts:12][focus]; [workspaceFocus.ts:375][focus_open].

**Actual Ferrite counterpart.** Ferrite has a common absolute right panel and per-thread tab/width/maximize state. tend_panel writes a larger surface floor into that saved width.

**Architectural or placement gap.** Shared component presence does not establish coherent geometry. Floors, nav auto-fold and persistence produced the oversized reopen. Overview also places thread metadata in the workspace location.

Ferrite source: [panel.rs:550][f_panel]; [panel.rs:149][f_floor]; [panel.rs:368][f_tend]; [surfaces.rs:88][f_state]; [panel.rs:627][f_dispatch].

**Observed, with the original evidence boundary.**

- T3 maximize/restore and browser hide/reopen retaining tab and draft were observed.
- Ferrite Overview 320 px became 864 px after Files/Changes without divider input and remained wide through recorded return/closure/restart.

**Still unverified.**

- T3 right-panel interpolation, F 6, deliberate resize persistence and narrow geometry are unexecuted.
- Only T3 left-sidebar closing interpolation at 400 ms was observed; settled Ferrite headless images cannot certify motion.

**Earlier coverage.**

- panel-tools-018; panel-runtime-correction.md upgrades the width ratchet to runtime.
- input-focus-003/004/005 and motion findings retain their own evidence levels.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Separate chosen width from temporary minima and verify return after each wide tool.
2. Capture conversation, nav and tool geometry together at narrow widths.
3. Verify focus and hit targets through hide, maximize, thread switch and rapid reversal.

## WS-02: Resource tabs and launcher

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Give each resource one owner, title, activation and close/recovery contract. |
| Placement | RightPanelTabs supplies shared outer strip, add menu, empty launcher and content body. |
| Entry | Add panel surface, empty launcher, resource links and reopen converge on the surface store; disabled actions explain capability reasons. |
| Coexistence | One tool body is active. Other resource sessions may continue while their tabs are inactive. |
| Focus | Tabs have keyboard handlers and overflow reveal. Enter in the launcher belongs to its selection, never an unrelated composer. |
| Motion | Uses common frame motion; tab overflow navigation is separate from open/close interpolation. |
| Lifetime | Hide, selected close, close others/all and reopen differ. Browser and terminal resource release is specialized. |
| Persistence | Thread metadata persists in a versioned store; synthetic global PR tabs are excluded. Metadata persistence does not prove process survival. |
| Responsive | Inline and sheet hosts share the strip. Overflow must keep active tabs and close controls reachable. |

Installed source: [rightPanelStore.ts:37][surfaces]; [rightPanelStore.ts:93][state]; [rightPanelStore.ts:309][upsert]; [rightPanelStore.ts:1187][persist]; [rightPanelStore.ts:972][close]; [rightPanelStore.ts:1218][closed]; [RightPanelTabs.tsx:1097][launcher]; [RightPanelTabs.tsx:1529][empty]; [RightPanelTabs.tsx:964][tab_keys]; [RightPanelTabs.tsx:1069][tab_overflow]; [useRightPanelSurfaceClose.ts:54][release]; [useRightPanelSurfaceClose.ts:137][close_confirm].

**Actual Ferrite counterpart.** Ferrite has deduplicated outer tabs, close-family methods, reopen metadata and explanatory plus-menu rows.

**Architectural or placement gap.** Files and Browser add inner tab hierarchies under singleton outer tabs. Outer-strip tests cannot certify their inner close/overflow behavior. A working letter shortcut does not rescue misrouted Enter.

Ferrite source: [surfaces.rs:28][f_surface]; [surfaces.rs:88][f_state]; [strip.rs:134][f_launcher]; [panel.rs:313][f_release]; [panel.rs:130][f_restore]; [files.rs:469][f_file_tabs].

**Observed, with the original evidence boundary.**

- T3 launcher exposed Browser, Terminal, Files, Diff, Pull request, Linked pull requests, Device and Side chat; selected opens are recorded separately.
- Ferrite launcher Enter submitted PANEL_ENTER_SENTINEL to the provider; the selected Files opener did not open.
- Ferrite 18 kept inner Files tabs clipped strip controls beyond the window.

**Still unverified.**

- T3 every launcher letter, unavailable state, drag, bulk close, overflow and restart variant were not run.

**Earlier coverage.**

- input-focus-002 runtime-interactions.md; panel-tools-005/006; files-runtime-correction.md.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Define resource and close target at each tab level; justify any nesting.
2. Test pointer/Enter/Escape/modifiers with a protected unsent draft.
3. Verify overflow, active reveal and recovery across both outer and retained inner tab systems.

## WS-03: Standalone Pull requests workspace

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Browse and filter PRs independently of the selected conversation. |
| Placement | Dedicated /pull-requests route keeps app sidebar and owns list plus native detail tabs under a synthetic panel owner. |
| Entry | Global utility entry loads saved filters. Row selection carries explicit environment/project/host/repository/number; Escape returns to prior thread. |
| Coexistence | List and detail share the main row independently of thread tool tabs. |
| Focus | Route entry requests its own workspace region; page-specific close/toggle shortcuts honor palette boundaries. |
| Motion | Uses shared presence/shell. No standalone PR interpolation was captured. |
| Lifetime | Global detail tabs belong to this workspace, not the selected thread. Leaving the route and closing a detail are different actions. |
| Persistence | List preferences and width have separate keys. Synthetic PR tabs are explicitly excluded from thread-panel persistence. |
| Responsive | Page directly renders mode=inline with half-window default and its own width key. Chat's 980 px sheet behavior does not apply; narrow PR composition needs its own check. |

Installed source: [PullRequestsPage.tsx:180][pr_owner]; [PullRequestsPage.tsx:1123][pr_select]; [PullRequestsPage.tsx:1495][pr_layout]; [rightPanelStore.ts:1187][persist]; [workspaceFocus.ts:375][focus_open].

**Actual Ferrite counterpart.** Ferrite has branch PR facts, native detail and browser routes. No equivalent global list destination was identified in the traced audit.

**Architectural or placement gap.** Native detail or a GitHub page does not substitute for an independently navigable PR workspace. Earlier treatment of the inbox as optional must be reconciled with the user's explicit PRs-view complaint.

Ferrite source: [surfaces.rs:28][f_surface]; [cockpit.rs:1827][f_pr_entry]; [pull_request.rs:453][f_pr_detail].

**Observed, with the original evidence boundary.**

- T3 global entry loaded real authored rows; Merged changed route/results; Escape returned to the thread and reopen retained Merged.
- Plain ferrite search and PR #246 detail were observed. Cleanup restored empty query, Open and All.

**Still unverified.**

- Other filters, pagination, global-tab restart and narrow layout were not executed.
- Installed parser deliberately ignores repo: qualification; mixed results were not certified as a new runtime defect.

**Earlier coverage.**

- pr-navigation-03; t3-pr-live-source-match.md cases 01–04.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Make the global entry reach its intended usable destination.
2. Keep list scope independent from explicit detail target and restore prior draft on return.
3. Exercise narrow list/detail, empty/loading/error and selected-target removal.

## WS-04: Linked PR list and thread-native detail

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Read a thread's associated PRs and an explicit PR target beside its conversation. |
| Placement | Linked pull requests is its own thread list surface; a selected target opens native detail with Summary/Guide/Timeline/Code subviews. |
| Entry | Linked row, sidebar badge, Add Pull request and Add Linked pull requests. Cross-thread badge must activate the same owner receiving panel state. |
| Coexistence | Detail stays beside chat; subtabs belong inside one PR resource, while multiple PR targets are outer tabs. |
| Focus | Detail shortcuts require the visible active resource. Location-specific right-click behavior must be tested separately. |
| Motion | Inherits frame motion. Mounted invisible detail content is not proof of visible content or a screen-reader defect. |
| Lifetime | Last-tab close hides the panel; badge upserts the explicit target. Link/unlink association is independent of checkout or remote mutation. |
| Persistence | PR metadata includes project/repository/host/number and subtab. PR Code file-tree preference is global, so closing cannot restore it. |
| Responsive | Chat-hosted detail uses inline/sheet; global-page detail uses that page's different inline path. |

Installed source: [RightPanelSurfaceContent.tsx:120][pr_detail]; [ThreadPullRequestsPanel.tsx:364][pr_list]; [ThreadPullRequestsPanel.tsx:158][pr_link]; [PullRequestDetailPanel.tsx:1872][pr_tabs]; [rightPanelStore.ts:37][surfaces]; [rightPanelStore.ts:972][close]; [rightPanelStore.ts:309][upsert].

**Actual Ferrite counterpart.** Ferrite has native detail, but common open_pr routes to Browser and its surface identity is only a PR number within thread context.

**Architectural or placement gap.** Underlining or changing a hidden B panel while A stays selected is not navigation. A branch-bound read cannot stand in for the clicked explicit PR.

Ferrite source: [cockpit.rs:1827][f_pr_entry]; [pull_request.rs:453][f_pr_detail]; [surfaces.rs:28][f_surface].

**Observed, with the original evidence boundary.**

- T3 linked PR #246 to Calculator; with another Sonnet thread selected, the badge activated Calculator and opened correct native merged detail.
- Close/reopen and local unlink were observed. Sidebar badge right-click opened thread actions, without Open PR.
- Ferrite cross-thread badge failed; same-thread badge opened Browser; Overview reached working native Summary/Timeline.

**Still unverified.**

- Multiple-linked list, errors, modifiers and every lifecycle remain unexecuted.
- T3 same-identity freshness was not tested; Ferrite stale refresh is independently reproduced.

**Earlier coverage.**

- pr-navigation-01/02/04; pr-entry-runtime-correction.md; pr-operations-001.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Assert selected thread/project/repository/number/tab and loaded visible body after the rendered click.
2. Keep native open, external open, association and remote action distinct.
3. Refresh changed comments/body/checks without changing PR identity; reopen is not the same test.

## WS-05: Files explorer and file resource tabs

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Navigate the actual workspace without losing conversation or file location. |
| Placement | Files/file share FilePreviewPanel. Explorer fills the body alone or forms a resizable column beside a selected file; files use outer resource tabs. |
| Entry | Add Files, tree selection, file link and search result. openFile removes the standalone Files placeholder, deduplicates path and increments reveal identity. |
| Coexistence | Explorer and file coexist inside the active resource view; other files remain shared tabs. |
| Focus | Tree selection/Enter can focus the editor. Explorer filter owns its text; path/line reveal must reach the intended file. |
| Motion | Frame motion is shared; explorer divider follows input. No file-opening animation was measured. |
| Lifetime | Body switching and pending saves have separate lifetimes. Workspace mutations and revisits must refresh clean state. |
| Persistence | Resource metadata persists with the thread. Explorer has separate saved width, default 256 px; tab persistence does not imply all tree state persists. |
| Responsive | Explorer resizes independently when a file is selected. Chat's sheet does not alone prove the inner columns remain usable. |

Installed source: [RightPanelSurfaceContent.tsx:203][files_dispatch]; [rightPanelStore.ts:812][file_open]; [FilePreviewPanel.tsx:1619][file_tree]; [FileExplorerPane.tsx:16][explorer_size]; [FilePreviewPanel.tsx:1165][file_focus].

**Actual Ferrite counterpart.** Ferrite singleton Files has its own Vec<Tab>, active index, preview promotion and tree, separate from SurfaceId::File previews.

**Architectural or placement gap.** Nested tabs bypass common outer-strip behavior. The same path can get different editing, preview and line handling depending on entry.

Ferrite source: [files.rs:89][f_files_state]; [files.rs:159][f_files_open]; [files.rs:469][f_file_tabs]; [files.rs:297][f_file_find]; [panel.rs:130][f_restore].

**Observed, with the original evidence boundary.**

- T3 opened README source/rendered and followed long-guide.md into another tab.
- Ferrite same-tab revisit retained stale Markdown; expanded tree missed added/deleted children;18 kept tabs clipped inner strip controls.

**Still unverified.**

- T3 large tree, rename/delete, mutation refresh, file overflow and restart were not run.
- Ferrite dirty-buffer recovery was not established through controls because typing focus failed.

**Earlier coverage.**

- panel-tools-003/005/006; files-runtime-correction.md; input-focus-010.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Unify resource ownership across tree/link/search/attachment entry.
2. Check external mutation and clean-buffer revisit during agent work.
3. Keep tab/close controls reachable through many files, switching and restart.

## WS-06: Source, rendered documents and media

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Show the same file as editable source or accurate preview, with trustworthy save state. |
| Placement | FilePreviewPanel dispatches editor/source, Markdown, table, Mermaid, browser-backed HTML/PDF and document/media bodies; attachments retain distinct identity. |
| Entry | Rendered/source toggle and line reveal. Requested source line overrides incompatible rendering preference until handled. |
| Coexistence | Modes share file identity; explorer remains where supported. A document preview is not automatically an editable text buffer. |
| Focus | Editor establishes a caret and owns typing/Find. Rendered-content Find is a separate unresolved behavior; read-only/truncated state must be explicit. |
| Motion | Uses frame lifecycle. Loading/read-error/pending-save geometry needs checking independently of panel motion. |
| Lifetime | Per-file save sessions serialize revisions, debounce 500 ms and flush final release. This is source support, not proof of every conflict recovery. |
| Persistence | Rendered preferences may be global; save coordination is environment/cwd/file scoped, while resource tabs are thread scoped. |
| Responsive | Wrap, horizontal scrolling, tables and media have inner bounds beyond the outer sheet contract. |

Installed source: [FilePreviewPanel.tsx:1127][file_modes]; [FilePreviewPanel.tsx:1566][file_render]; [FilePreviewPanel.tsx:1165][file_focus]; [fileSaveSessions.ts:139][file_save]; [fileSaveCoordinator.ts:26][file_order]; [RightPanelSurfaceContent.tsx:203][files_dispatch].

**Actual Ferrite counterpart.** Ferrite Code/Prose/Unshown branches send rich prose directly to a reader and reject non-UTF 8; other preview paths support richer content.

**Architectural or placement gap.** An image lightbox elsewhere does not certify opening the image from Files. Read-only prose without source/line navigation is not an editable document counterpart.

Ferrite source: [files.rs:201][f_file_modes]; [editor.rs:135][f_editor]; [files.rs:159][f_files_open]; [panel.rs:627][f_dispatch].

**Observed, with the original evidence boundary.**

- T3 source/rendered Markdown, source Find and reversible on-disk edit were verified. Autosave prevents attributing writes specifically to Command-S.
- Ferrite editor/filter typing reached the main composer.

**Still unverified.**

- Rendered Find, media formats, failed save, immediate-close save and external conflicts remain unexecuted.
- Ferrite dirty-save/error variants were not tested through a real dirty buffer.

**Earlier coverage.**

- panel-tools-001/002/007/008 source findings; runtime-interactions.md focus result; t3-files-terminal-live-source-match.md FILES 02/03/05.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Use consistent identity and dispatch across entry points; show unsupported formats clearly.
2. Retain dirty/pending/error state until disk confirmation; preserve text across recoverable failure.
3. Test editor, explorer, rendered content, browser and composer shortcuts with a protected draft.

## WS-07: Project content search and filename palette

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Find a file or matching source line across the intended workspace. |
| Placement | CommandPalette has files and content modal modes. ProjectContentSearchDialog is distinct from tree filtering and thread Find. |
| Entry | Content shortcut/palette action opens workspace search; selecting a current readable match closes it and opens exact file+line. |
| Coexistence | Modal overlays workspace; destination becomes an ordinary resource tab, not another persistent reader. |
| Focus | Search input/result selection owns keys. Cancel restores prior owner; navigation hands focus to its destination. Modal boundaries contain traversal. |
| Motion | Dialog lifecycle is separate from panel width motion; interpolation was not measured. |
| Lifetime | Query/options/selection are dialog state. Superseded async results must not open a previous workspace. |
| Persistence | Search state is not file-tab metadata or rendered-mode preference; restart retention is not claimed. |
| Responsive | Modal bounds/result window need separate long-path, small-width and large-result checks. |

Installed source: [CommandPalette.tsx:361][search_overlay]; [ProjectContentSearchDialog.tsx:152][search_match]; [ProjectContentSearchDialog.tsx:111][search_state]; [workspaceFocus.ts:232][focus_boundary].

**Actual Ferrite counterpart.** Ferrite has filename filtering in Files and repository content search in its separate Review reader.

**Architectural or placement gap.** Command-Shift-F opening a filename filter is not content search. Review search existing elsewhere does not prove the expected global entry or file handoff.

Ferrite source: [files.rs:297][f_file_find]; [tools.rs:43][f_tools]; [review.rs:1175][f_review_search]; [beside.rs:224][f_review].

**Observed, with the original evidence boundary.**

- T3 content search returned 82 matches in 3 files; README line 8 opened source. Exact highlight geometry was not measured.

**Still unverified.**

- Invalid regex, stale-query cancellation, inaccessible workspace and every filename-picker path were unexecuted.
- Ferrite separate Review search was not runtime checked.

**Earlier coverage.**

- input-focus-009/010; t3-files-terminal-live-source-match.md FILES 04. Cross-surface search ownership was not an explicit earlier repair unit.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Distinguish current-file, thread, filename and project-content search in labels and dispatch.
2. Open a known offscreen match and assert file, line, content and focus.
3. Replace/cancel query or workspace mid-read and reject stale activation.

## WS-08: Diff workspace versus separate Review reader

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Read the intended complete change set with scope and review meaning intact. |
| Placement | T3 Diff embeds into common workspace with scope selector, code and file tree. PR Code is a separate PR-specific body. |
| Entry | Add Diff selects branch scope; other paths select turn, commit, unstaged or tool-call scope. |
| Coexistence | Diff stays beside chat; inner tree/code share one resource while selected scope is separate state. |
| Focus | Scope, file/code navigation and selection own their input; reveal should yield to manual navigation. |
| Motion | Workspace motion applies. Ferrite's separate Review reader already has its own animated slot contract; not all Ferrite motion is absent. |
| Lifetime | Data and reviewed revision require mutation refresh. A renderer's existence does not prove complete rows or visible stage errors. |
| Persistence | Selected scope has its own store; Split/Stacked is client preference, tree visibility its own key, wrapping local state. |
| Responsive | Separate resizable tree and unwrapped horizontal code scrolling still need narrow geometry checks. |

Installed source: [RightPanelSurfaceContent.tsx:108][diff_dispatch]; [DiffPanel.tsx:283][diff_scope]; [DiffPanel.tsx:1177][diff_layout]; [DiffPanel.tsx:1489][diff_render].

**Actual Ferrite counterpart.** Ferrite has Changes panel and richer ReviewView in a distinct board reader slot: Solo splits Pane|review; group/fullscreen splits the pane cell, with independent focus/closure.

**Architectural or placement gap.** A richer reader elsewhere does not repair expected Changes-panel truncation. Conversely, that reader exists and should not be reimplemented because the panel is shallow.

Ferrite source: [changes.rs:36][f_changes]; [hunks.rs:26][f_hunks]; [beside.rs:224][f_review]; [beside.rs:12][f_review_motion]; [review.rs:1175][f_review_search].

**Observed, with the original evidence boundary.**

- T3 PR Code showed actual diff, Split/tree and explicit partial-data warning; this was not every ordinary Diff scope.
- Ferrite long fixture showed+160/-160 and first hunk; source caps panel at 120 rows. Scrolling to omission notice was not established.

**Still unverified.**

- Ordinary T3 turn/commit/unstaged/tool-call and remote review/stage mutations were unexecuted.
- Ferrite full Review reader stayed unchecked in the Files correction.

**Earlier coverage.**

- panel-tools-009–014; files-runtime-correction.md. Initial audit excluded the full Review slot and its relationship to panel entry.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Choose a primary review presentation and make entries reach it with explicit scope.
2. Read the final long-patch line and disclose partial data accurately.
3. Tie reviewed marks/drafts to revision and surface stage/refresh errors.

## WS-09: Bottom terminal drawer

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Run commands below conversation while another workspace tool remains beside it. |
| Placement | ChatTerminalDrawers mounts independently inside chat; drawer has own height. Embedded side chats omit it. |
| Entry | Terminal visibility and project-script/code-block execution use drawer state. Add Terminal in workspace is another destination. |
| Coexistence | Chat, drawer and right PR/Files/browser can coexist. Panel terminal IDs are excluded from drawer sessions. |
| Focus | Drawer owns an independent terminal workspace region and active group; typing/interrupt must reach the intended session. |
| Motion | Grid-row transition uses common gate and thread-scoped retention; no native drawer motion was recorded. |
| Lifetime | Bounded recently used views remain mounted. View retention and server session lifetime differ; hide does not imply process termination. |
| Persistence | Separate versioned store saves thread drawer visibility, height, IDs and groups. Process survival across restart is unverified. |
| Responsive | Height is clamped using conversation/composer bounds; narrow composition must include drawer and right sheet together. |

Installed source: [ChatView.tsx:3915][drawer_mount]; [ChatTerminalDrawers.tsx:41][drawer_retention]; [PersistentThreadTerminals.tsx:124][drawer_partition]; [PersistentThreadTerminals.tsx:241][drawer_motion]; [useChatTerminalActions.ts:137][drawer_entry]; [terminalUiStateStore.ts:769][drawer_store]; [ThreadTerminalDrawer.tsx:1512][terminal_geometry]; [ThreadTerminalDrawer.tsx:1833][terminal_mode].

**Actual Ferrite counterpart.** Ferrite Cmd-J activates/creates a right-panel Terminal tab. No independent bottom drawer was identified in the traced cockpit layout.

**Architectural or placement gap.** That tab replaces visible Files/PR/browser. It does not supply drawer coexistence. This architectural difference is separate from missing split shells.

Ferrite source: [tools.rs:43][f_tools]; [terminal.rs:93][f_terminal]; [panel.rs:550][f_panel]; [surfaces.rs:28][f_surface].

**Observed, with the original evidence boundary.**

- The earlier native audit exercised workspace terminal tabs. Subsequent parent-owned checks opened the bottom drawer beside loaded README and an intact composer draft, then closed the empty shell through its confirmation. No drawer command or split was exercised. See ../evidence/surface-reconciliation/observations.md.

**Still unverified.**

- Drawer resize, motion interpolation, command execution, splits, thread-switch retention and process survival remain unverified.

**Earlier coverage.**

- panel-tools-015–017 covered spawn/splits/links but omitted drawer placement. Source-supported parity decision, not a new runtime-proven defect.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Explicitly decide drawer parity; do not silently credit the panel terminal.
2. If supported, verify chat+drawer+right tool with independent focus and close.
3. Separate hidden-view retention, session ownership and process termination.

## WS-10: Workspace terminal tabs and splits

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Provide an interactive shell or split-shell group as a tool resource. |
| Placement | PersistentThreadTerminalPanel hosts the terminal renderer in panel mode; outer surface owns IDs and split direction. |
| Entry | Add Terminal places destination before launch, then focuses its terminal; split creates another pane in its group. |
| Coexistence | Several shells can share one tab. Bottom drawer is separate; other workspace tools stay inactive tabs. |
| Focus | Terminal input/copy/interrupt and hidden visibility have explicit session ownership. Close must not target the parent conversation. |
| Motion | Frame handles visibility; resizing follows direct geometry. Split animation/smoothness was not measured. |
| Lifetime | Observed individual closes confirmed process stop/history clear. Resource close releases sessions; bulk-close is a separate branch. |
| Persistence | Tab/group metadata differs from process state. Closed terminals are excluded from generic panel-tab reopen memory. |
| Responsive | Panel fills host, unlike drawer height. Split orientations/limits and minimum dimensions need checks. |

Installed source: [useChatTerminalActions.ts:330][terminal_add]; [useThreadTerminalActions.ts:73][terminal_launch]; [PersistentThreadTerminals.tsx:380][terminal_panel]; [ThreadTerminalDrawer.tsx:1512][terminal_geometry]; [ThreadTerminalDrawer.tsx:1691][terminal_close]; [ThreadTerminalDrawer.tsx:1833][terminal_mode]; [PersistentThreadTerminals.tsx:124][drawer_partition]; [useRightPanelSurfaceClose.ts:137][close_confirm]; [useRightPanelSurfaceClose.ts:54][release].

**Actual Ferrite counterpart.** Ferrite has real PTYs, delayed focus, selection/copy/paste and one outer tab per shell. No split-group shape; spawn failure returns silently.

**Architectural or placement gap.** Multiple tabs do not provide simultaneous splits. A PTY does not prove safe Cmd-W routing or launch feedback.

Ferrite source: [terminal.rs:93][f_terminal]; [terminal.rs:200][f_terminal_focus]; [tools.rs:43][f_tools]; [surfaces.rs:28][f_surface].

**Observed, with the original evidence boundary.**

- T3 actual command printed cwd/sentinel; Ctrl-C interrupted 120-second command; horizontal split created second pane; confirmed closes removed both and final process inspection found no command.

**Still unverified.**

- Vertical split, limits, failure/retry, links, clipboard/IME and all close shortcuts were not run. No new Ferrite terminal runtime here.

**Earlier coverage.**

- panel-tools-015/016/017; input-focus-004 source-only terminal-close branch; t3-files-terminal-live-source-match.md.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Verify cwd/live output/interrupt/cancel/confirmed cleanup through rendered controls.
2. If splits are in scope, test sibling survival and usable minimum geometry.
3. Retain an actionable launch/error/retry surface.

## WS-11: Side-chat workspace placement and lifetime

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Keep a real separate conversation beside its parent with deliberate lifetime boundaries. |
| Placement | SideChatPane embeds ChatView under separate composer/background contexts; tool opens register the parent's workspace host. |
| Entry | Add Side chat; ordinary Close; explicit Delete; reopen used conversation. |
| Coexistence | Parent and side chat coexist. Embedded chat does not recursively mount another panel/drawer; another active tool may replace its visible body. |
| Focus | Main/side composers own independent text. Child tool focus routes through the designated host. |
| Motion | Uses common frame; no side-specific interpolation measured. |
| Lifetime | T3 close deletes only an unused side chat; explicit Delete clears history after confirmation. Used history remains reopenable. |
| Persistence | Surface remembers thread identity while conversation/draft have their own stores; keeping a component is not persistence. |
| Responsive | Embedded transcript/composer inherit workspace width/sheet and must remain usable without recursive panel widths. |

Installed source: [SideChatPane.tsx:45][side_host]; [SideChatPane.tsx:170][side_embed]; [SideChatPane.tsx:93][side_delete]; [useRightPanelSurfaceClose.ts:54][release]; [rightPanelStore.ts:1218][closed]; [ChatView.tsx:3931][mount]; [ChatView.tsx:3915][drawer_mount].

**Actual Ferrite counterpart.** Ferrite backing fork and composer exist. Ordinary release_surface calls close_side_tab, removes local state and retires backing thread; Delete reaches the same close path.

**Architectural or placement gap.** Close and Delete are conflated in inspected source. Generic reopenable SurfaceId does not prove a retired conversation can recover.

Ferrite source: [side_chat.rs:169][f_side]; [side_chat.rs:308][f_side_close]; [panel.rs:313][f_release]; [panel.rs:130][f_restore].

**Observed, with the original evidence boundary.**

- T3 real Sol side reply and next-message Bring back delivery were verified; closing and Cmd-Shift-T restored original prompt/response/context.

**Still unverified.**

- Delete, unsent draft disposal, creation failure, tool switch during generation, restart and narrow focus were not executed.
- Ferrite deletion-conflation is source-supported; this worker did not reproduce close/reopen.

**Earlier coverage.**

- Earlier presence inventory omitted this lifetime contract. Conversation reconciliation owns provider/transfer semantics; workspace owns placement/closure.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Separate Close from Delete for used chats and verify real history/draft recovery.
2. Route embedded tools through parent host without recursive panels or wrong-root focus.
3. Protect parent/side drafts during streaming, approval and tool switching.

## WS-12: Device tool and setup modal

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Inspect/control an actual device session scoped to thread. |
| Placement | Distinct device resource carries host/device/platform; unavailable onboarding renders setup dialog. |
| Entry | Add Device opens setup or device surface with environment/thread-scoped operations. |
| Coexistence | Device resource/floating presentation differs from browser mobile emulation; setup has a modal boundary. |
| Focus | Input targets selected device; pending operations retain originating scope. |
| Motion | Uses workspace/dialog hosts; stream responsiveness or motion unobserved. |
| Lifetime | View visibility and device session lifetime differ; late operations cannot appear under another thread. |
| Persistence | Panel target metadata is represented; transport/setup restart persistence is not certified. |
| Responsive | Device viewport/setup bounds require independent checks. |

Installed source: [DevicePanel.tsx:50][device]; [DevicePanel.tsx:203][device_setup]; [rightPanelStore.ts:37][surfaces]; [RightPanelTabs.tsx:1097][launcher].

**Actual Ferrite counterpart.** No Device surface in Ferrite's inspected dispatch/opener set. Browser is a separate capability.

**Architectural or placement gap.** Browser emulation cannot count as connected-device support. This is a distinct product-scope decision, not a defect inferred from disabled integration.

Ferrite source: [surfaces.rs:28][f_surface]; [panel.rs:627][f_dispatch]; [strip.rs:134][f_launcher].

**Observed, with the original evidence boundary.**

- Device control was inspected; device hub was not installed. Browser iPhone emulation was separately exercised.

**Still unverified.**

- No real device setup, connection or lifecycle was executed.

**Earlier coverage.**

- Initial panel-tools inventory omitted this surface; native ledger correctly left device integration unverified.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Record actual-device scope separately from browser emulation.
2. If in scope, make setup/error/pending destination deliberate and retain originating identity.

## WS-13: Browser resource host boundary

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Host web sessions without input collisions or losing conversation context. |
| Placement | PreviewPanel embeds under shared resource strip keyed by session; floating preview is another presentation. |
| Entry | Chat link action, Add Browser, target-blank and address navigation; ordinary external and native PR preferences differ. |
| Coexistence | Browser coexists with chat/drawer; a floating preview can coexist with another tab. |
| Focus | Web content, address, Find and composer require separate input owners. |
| Motion | Frame motion differs from browser loading/page retention; neither certifies complete chrome quality. |
| Lifetime | Close releases session; hide may invoke float/miniplayer policy before hiding panel. |
| Persistence | Resource metadata and runtime browser identity differ; reopen is browser-specific. |
| Responsive | Workspace bounds, device emulation and floating windows are distinct geometry layers. |

Installed source: [RightPanelSurfaceContent.tsx:86][browser_host]; [rightPanelStore.ts:37][surfaces]; [useRightPanelActions.ts:165][visibility_actions]; [useRightPanelSurfaceClose.ts:54][release]; [rightPanelStore.ts:1218][closed].

**Actual Ferrite counterpart.** Ferrite singleton Browser has inner page tabs and shares padded preview body with files.

**Architectural or placement gap.** A webview existing does not certify surrounding hierarchy or natural input. Outer/inner bars and padding must be reviewed as the actual whole.

Ferrite source: [surfaces.rs:28][f_surface]; [panel.rs:627][f_dispatch]; [panel.rs:313][f_release].

**Observed, with the original evidence boundary.**

- Parent verified input/navigation/Find/close/reopen/hide/draft retention/error recovery and floating coexistence. Floating activation remained confounded.

**Still unverified.**

- No whole-browser quality pass; browser report retains prompt() error and unexecuted variants.

**Earlier coverage.**

- browser.md; input-focus.md; t3-browser-live-source-match.md. Browser reconciliation owns chrome defects.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Verify whole composition, usable viewport and control hierarchy.
2. Distinguish hidden/floated/closed/reopened states while preserving drafts.

## WS-14: Transient menus, confirmations and sheets

| Contract | Installed T3 surface |
| --- | --- |
| Purpose | Contain temporary choices without submitting unrelated work. |
| Placement | Add/context menus, search modal, PR picker, terminal close and device setup are owned transient layers; a narrow sheet is a host, not another tool. |
| Entry | Each trigger has explicit owner and scope; launcher shortcut respects other layers. |
| Coexistence | Underlying resource stays identifiable; closing layer must not close parent conversation. |
| Focus | Escape dismisses active layer; Enter activates its selection; cancel restores prior owner. Modal/menu boundaries constrain traversal. |
| Motion | Sheet duration is explicit, including zero. Menu motion cannot substitute for sidebar/workspace interpolation. |
| Lifetime | Popup state is ephemeral; confirmed resource close follows its resource lifecycle while cancellation preserves it. |
| Persistence | Popup retention differs from saved preferences and does not imply restart restoration. |
| Responsive | Anchoring/clipping at small sizes and nested submenu focus require direct checks. |

Installed source: [RightPanelTabs.tsx:1097][launcher]; [RightPanelTabs.tsx:1529][empty]; [workspaceFocus.ts:232][focus_boundary]; [RightPanelSheet.tsx:20][sheet]; [sheet.tsx:74][sheet_motion]; [CommandPalette.tsx:361][search_overlay]; [useRightPanelSurfaceClose.ts:137][close_confirm]; [DevicePanel.tsx:203][device_setup].

**Actual Ferrite counterpart.** Ferrite has real menus/guards but focus/actions are distributed across cockpit/tool modules.

**Architectural or placement gap.** Menu visibility does not establish layer ownership. Working letter keys cannot excuse Enter reaching chat; one context menu cannot certify every link.

Ferrite source: [strip.rs:134][f_launcher]; [panel.rs:313][f_release]; [panel.rs:550][f_panel].

**Observed, with the original evidence boundary.**

- T3 terminal confirmations and specific PR filter/link/unlink flows were observed.
- Ferrite launcher Enter sent draft and ordinary link-menu Escape remained open in earlier runtime.

**Still unverified.**

- Every submenu, cancellation, bulk-close, traversal and modal geometry branch was not executed.

**Earlier coverage.**

- input-focus-002/003/004/005; runtime-interactions.md. Layer geometry/focus restoration was often omitted from control inventories.

**Production requirements omitted or insufficiently connected in the earlier audit.**

1. Record trigger, active layer, focus, outcome and restored owner.
2. Test cancel/return with unrelated drafts and running tools protected.
3. Capture clipping, occlusion and invisible exit hit targets.

## Repair-unit consequences

These requirements concern placement and quality of the completed journey. They must not be reduced to adding controls or declaring a component present.

- **WREQ-01 (WS-01, WS-02).** Shared geometry/focus/lifetime contract across all tools, including deliberate resize versus automatic minima. Evidence: Ferrite width ratchet and launcher Enter are runtime-reproduced; other variants remain source/planned.
- **WREQ-02 (WS-03, WS-04).** Distinct global PR workspace, thread association list and explicit native PR target; never count Browser or hidden panel state as equivalent destination. Evidence: T3 bounded native global/cross-thread journey; Ferrite existing cross-thread red.
- **WREQ-03 (WS-05, WS-06, WS-07).** Coherent file resource topology across explorer, source/rendered/media, search and link entry rather than disconnected inner tabs/readers. Evidence: Ferrite stale/overflow/focus runtime; remaining save/persistence variants unexecuted.
- **WREQ-04 (WS-08).** Decide how Changes and full Review reader relate; existing richer reader cannot silently stand in for shallow/truncated panel. Evidence: Source architectural distinction; full Review runtime not checked.
- **WREQ-05 (WS-09, WS-10).** Treat bottom-drawer coexistence separately from shell capability and terminal splits. Evidence: Installed source supports drawer; parent only exercised workspace terminal.
- **WREQ-06 (WS-11).** Separate ordinary side-chat close from deletion and preserve reopenable used history. Evidence: T3 close/reopen native; Ferrite closure conflation source-supported.
- **WREQ-07 (WS-12, WS-13).** Separate actual-device tool, browser emulation, browser host and floating presentation. Evidence: Device unavailable/inspected; browser specific journeys observed.
- **WREQ-08 (WS-14).** Transient-layer focus, cancellation, anchoring and natural keys are production requirements, not menu-presence checks. Evidence: Selected native journeys and Ferrite launcher failure; exhaustive variants unexecuted.

## Boundaries with adjacent investigations

- The conversation reconciliation owns root/provider-child distinctions, composer behavior and Bring back semantics. This map owns side-chat placement, shared host and close lifetime.
- The browser reconciliation owns browser chrome and internals. WS-13 records only its place and resource ownership.
- Shell/settings reconciliation owns the global settings route/modal and thread-details card. Neither is an interchangeable right-panel resource.

## Evidence sources and cleanup

The [PR correspondence](t3-pr-live-source-match.md) and [Files/Terminal correspondence](t3-files-terminal-live-source-match.md) pin earlier native journeys to the installed source. [Browser correspondence](t3-browser-live-source-match.md) retains its own observed and unexecuted branches.

Ferrite runtime evidence: [panel sizing](panel-runtime-correction.md), [Files and Changes](files-runtime-correction.md), [natural input and launcher](runtime-interactions.md), and [PR entry/refresh](pr-entry-runtime-correction.md). The [original tools inventory](panel-tools.md), [input analysis](input-focus.md) and [older surface comparison](t3-surface-contracts.md) remain useful, but their source-only and newer-version claims are not promoted by this map.

Final parent cleanup in the ledger restored the PR query/state/involvement, hidden PR file tree, Stacked diff and original 0 ms panel duration. PR #246 was unlinked, tool tabs closed, disposable roots archived, owned commands and fixture server stopped, and T3 quit. This worker did not operate or independently recheck those UI states.

Validation checked all 14 records, nine contract fields per record, 39 exact-installed source-content hashes, 70 T3 line anchors and 28 current Ferrite line anchors. The failed terminalState.ts path guess is collection metadata, not evidence that terminal state is absent; terminalUiStateStore.ts was fetched and traced. No tests or builds were appropriate for this documentation-only reconciliation.

The disk-space interruption was handled by avoiding temporary shell heredocs and removing only this report’s redundant downloaded source-text cache. Final JSON retains hashes and immutable links. No unrelated files were deleted by this worker.

[Machine-readable map](surface-reconciliation-workspace.json).

[host]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatRightPanel.tsx#L19
[mount]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3931
[maximize]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3286
[shell]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L65
[shell_focus]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L132
[shell_motion]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L78
[size]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/usePreviewPanelInlineSize.ts#L13
[resize]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/useResizableWidth.ts#L63
[breakpoint]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelLayout.ts#L1
[sheet]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/RightPanelSheet.tsx#L20
[sheet_motion]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/sheet.tsx#L74
[motion]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/panelAnimations.ts#L31
[presence]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/panelAnimations.ts#L41
[focus]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/lib/workspaceFocus.ts#L12
[focus_boundary]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/lib/workspaceFocus.ts#L232
[focus_open]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/lib/workspaceFocus.ts#L375
[surfaces]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L37
[state]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L93
[upsert]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L309
[persist]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L1187
[close]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L972
[closed]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L1218
[launcher]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/RightPanelTabs.tsx#L1097
[empty]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/RightPanelTabs.tsx#L1529
[tab_keys]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/RightPanelTabs.tsx#L964
[tab_overflow]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/RightPanelTabs.tsx#L1069
[release]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/useRightPanelSurfaceClose.ts#L54
[close_confirm]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/useRightPanelSurfaceClose.ts#L137
[pr_owner]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L180
[pr_select]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1123
[pr_layout]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestsPage.tsx#L1495
[pr_detail]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L120
[pr_list]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/ThreadPullRequestsPanel.tsx#L364
[pr_link]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/ThreadPullRequestsPanel.tsx#L158
[pr_tabs]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/PullRequestDetailPanel.tsx#L1872
[files_dispatch]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L203
[file_open]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L812
[file_tree]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L1619
[file_modes]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L1127
[file_focus]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L1165
[file_render]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L1566
[file_save]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/fileSaveSessions.ts#L139
[file_order]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/fileSaveCoordinator.ts#L26
[explorer_size]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FileExplorerPane.tsx#L16
[search_overlay]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/CommandPalette.tsx#L361
[search_match]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/search/ProjectContentSearchDialog.tsx#L152
[search_state]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/search/ProjectContentSearchDialog.tsx#L111
[diff_dispatch]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L108
[diff_scope]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/DiffPanel.tsx#L283
[diff_layout]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/DiffPanel.tsx#L1177
[diff_render]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/DiffPanel.tsx#L1489
[drawer_mount]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3915
[drawer_retention]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatTerminalDrawers.tsx#L41
[drawer_partition]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/PersistentThreadTerminals.tsx#L124
[drawer_motion]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/PersistentThreadTerminals.tsx#L241
[drawer_entry]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useChatTerminalActions.ts#L137
[drawer_store]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/terminalUiStateStore.ts#L769
[terminal_add]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useChatTerminalActions.ts#L330
[terminal_launch]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/terminal/useThreadTerminalActions.ts#L73
[terminal_panel]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/PersistentThreadTerminals.tsx#L380
[terminal_geometry]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadTerminalDrawer.tsx#L1512
[terminal_close]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadTerminalDrawer.tsx#L1691
[terminal_mode]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadTerminalDrawer.tsx#L1833
[side_host]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/SideChatPane.tsx#L45
[side_embed]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/SideChatPane.tsx#L170
[side_delete]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/SideChatPane.tsx#L93
[device]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/device/DevicePanel.tsx#L50
[device_setup]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/device/DevicePanel.tsx#L203
[browser_host]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L86
[visibility_actions]: https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useRightPanelActions.ts#L165
[f_surface]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/surfaces.rs:28
[f_state]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/surfaces.rs:88
[f_panel]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:550
[f_floor]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:149
[f_tend]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:368
[f_restore]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:130
[f_release]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:313
[f_dispatch]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:627
[f_launcher]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel/strip.rs:134
[f_tools]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools.rs:43
[f_files_state]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools/files.rs:89
[f_files_open]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools/files.rs:159
[f_file_modes]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools/files.rs:201
[f_file_tabs]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools/files.rs:469
[f_file_find]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools/files.rs:297
[f_editor]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools/editor.rs:135
[f_changes]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools/changes.rs:36
[f_hunks]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel/hunks.rs:26
[f_review]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/beside.rs:224
[f_review_motion]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/beside.rs:12
[f_review_search]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/review.rs:1175
[f_terminal]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:93
[f_terminal_focus]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:200
[f_side]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/side_chat.rs:169
[f_side_close]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/side_chat.rs:308
[f_pr_entry]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:1827
[f_pr_detail]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:453
[f_focus]: /Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:10220

# T3 live shell checklist

Coverage design for the running T3 app. No runtime passes are claimed. Source pin: `74573fc29b901bafbf8f1dbb2eb29606da1fa448` at `/tmp/ferrite-t3-audit-source-20261113`; repository `https://github.com/pingdotgg/t3code`. Record installed version separately. Parent owns all UI input and the existing GPT-6.1 Sol live interaction. This author launched nothing, changed no settings, sent no model request, and only wrote this checklist and its JSON.

## Record coverage

Use `not_exercised`, `inspected`, `pass`, `fail`, `blocked`, `not_applicable`, or `version_mismatch`. The JSON contains a separate observations/evidence array for every item. `source_supported` means a source path exists, not that it works live; `discovery_probe` leaves behavior open; `quality_probe` is an acceptance test without a source guarantee. Record actual menu labels/capability gates. Use audit threads/pages for reversible state changes; inspect destructive/share/update/account-storage actions without executing them against existing user data. Restore changed audit layout/page settings and leave no recording or overlay running.

Two distinctions: T3 panel animation defaults to **0ms**, while optional machinery exists; and its generic desktop hyperlink menu only proves **Copy Link**, whereas the browser toolbar separately supports **Open in system browser**. Verify actual installed behavior instead of assuming either difference away.

## Match each action to fresh evidence

The JSON now gives every case an exact source path, definition line, symbol, and explicit before/event/after state transition. The source symbol identifies the implementation to compare with the observed action. Probe references identify the subject under inspection and do not claim that their desired result is already implemented.

Record the pre-action route, thread, tab, focus, and timestamp. Perform one action. Obtain a fresh post-action observation. Native accessibility output can lag the screen. When accessibility and a screenshot disagree, record observation lag and obtain another fresh capture or wait for an observable state change. Do not repeat an uncertain write automatically. Require two consistent fresh observations when the first result is ambiguous. A stale observation alone is not a product failure.

Experience First shaped the interaction cases. The checklist tests geometry, focus, loading, recovery, and motion instead of counting visible controls. Prove It Works shaped the evidence gate. Every runtime result remains unexercised until a fresh observation confirms it.

Throughput checkpoint. n/a, read-only investigation.

## Short runnable route

1. Record installed version, active thread/model label, window size, theme, current animation ms, and current surfaces (BASE-01-04).
2. Cycle left sidebar, right panel, thread details and terminal using their mouse controls and Cmd+B / Cmd+Option+B / Cmd+Shift+B / Cmd+J; capture before/intermediate/after and restore (SIDE-01, PANEL-01, WINDOW-08, MOTION-01-02).
3. Open right-panel launcher; inspect every available entry and disabled reason. Open Browser; focus it then Cmd+L; load a harmless public page with text and links (PANEL-02-04, BROWSER-01-02).
4. Exercise page history/reload/Stop; capture loaded versus loading geometry. Compare URL-hover Open in system browser with webpage/transcript link right-click menus (BROWSER-04-09).
5. Open More: Find with Enter/Shift+Enter/Escape, zoom/reset, appearance, device toolbar/presets/custom dimensions/rotate/lock; restore original page settings (BROWSER-15-25).
6. Float preview over chat and return; inspect separate-window/move-runtime availability. Capture screenshot and annotation comment into audit draft without sending from this checklist (BROWSER-26-33).
7. Add second Browser plus Files/Diff/PR/Linked PRs/Terminal/Device/Side chat as enabled; verify actual body and routing for each. Reorder/overflow/context-menu tabs, close and Cmd+Shift+T restore (PANEL-05-15).
8. Resize compact/wide, switch Browser→Files→Browser, close/reopen, A→B→A; maximize/restore and interrupt a drag. Record widths, focus and page bounds (PANEL-16-20, MOTION-03-07).
9. Work through sidebar search, project filter, thread selection/rename/context menu, shelves and footer PR/Usage/Settings routes. Use only audit rows for reversible state changes (SIDE-03-22).
10. Test window resize around980/767/760px when allowed, app/page focus and history boundaries, native menus, overlay click capture, narrow popups; restore initial geometry and leave no audit recording/overlay running (WINDOW-01-13).

## Default macOS shortcuts

Custom installed bindings and tooltips take precedence. Preview and terminal focus change several chords.

| Keys | Action | Context |
| --- | --- | --- |
| Cmd+B | Left sidebar | Global; selected-text/editor precedence may apply |
| Cmd+Option+B | Right panel | Global |
| Cmd+Shift+B | Thread details | Outside terminal |
| Cmd+J | Terminal drawer | Global |
| Cmd+T | New panel surface | Right panel open, outside terminal |
| Cmd+W | Close surface | Outside terminal; terminal focus closes terminal |
| Cmd+Shift+T | Reopen closed view | Reopen stack required |
| Cmd+Shift+J | Browser toggle | Default binding |
| Cmd+L / Cmd+R / Cmd+F | URL / refresh / find | Preview focus |
| Cmd+= / Cmd+- / Cmd+0 | Page zoom / reset | Preview focus; native menu app zoom is separate |
| F6 / Shift+F6; Cmd+F6 variants | Next/previous workspace area | Only available areas |
| Cmd+[ / Cmd+] | App route history | Outside terminal; compare browser mouse history |
| Cmd+Shift+[ / Cmd+Shift+] | Previous/next thread | Default binding |
| Cmd+1…9 | Jump to thread | Desktop; model picker changes meaning |
| Cmd+K / Cmd+P / Cmd+Shift+F | Command palette / file picker / project search | Outside terminal |
| Cmd+N / Cmd+Shift+N | New thread / new current-project thread | Outside terminal |
| Cmd+Shift+P / Cmd+Shift+S | Pin / settle thread | Audit thread only |
| Cmd+Z | Thread action undo | Outside editable/terminal focus |
| Cmd+, | Settings | Native desktop menu |

## Baseline and evidence discipline

Source paths under the pinned clone: `apps/desktop/src/window/DesktopApplicationMenu.ts:169-191`; `apps/web/src/components/settings/AppearanceSettingsPanel.tsx:350-411`; `packages/shared/src/keybindings.ts:58-140`.

- [ ] **BASE-01. Identify the actual installed build** (`source_supported`; exercise). Open About T3 Code; record version/build, platform, window logical dimensions, display scale, theme, and source-pin match if known.  
  Observe: Record the installed version independently of source commit74573fc29b901bafbf8f1dbb2eb29606da1fa448. A source-only feature absent in an older installed build is version-gated, not a runtime failure.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/desktop/src/window/DesktopApplicationMenu.ts:116` `make`.  
  State. Version unrecorded. Event. Open About and inspect session metadata. Result to verify. Installed build and window baseline recorded.

- [ ] **BASE-02. Keep the existing subscription visible** (`discovery_probe`; inspect). In the parent-owned audit thread, inspect the provider/model selection and existing GPT-6.1 Sol subscription before its live interaction; record displayed labels.  
  Observe: This checklist creates no provider configuration or model request. A successful live model run is evidence the parent must capture separately.  
  Gate: Existing authenticated app.  
  Source. `apps/web/src/components/chat/ModelPickerSidebar.tsx:32` `ModelPickerSidebar`.  
  State. Audit draft selected. Event. Inspect provider and model. Result to verify. Displayed subscription and model recorded without sending.

- [ ] **BASE-03. Record animation and shortcut settings** (`source_supported`; inspect). Open Settings→Appearance advanced Panel animations; record current ms without changing it. Inspect relevant keyboard shortcuts if tooltips differ from defaults.  
  Observe: Pinned source defaults to0ms; configured and effective duration can differ because reduced motion and navigation suppression apply. Use displayed/custom bindings in the live run.  
  Gate: Settings accessible.  
  Source. `apps/web/src/components/settings/AppearanceSettingsPanel.tsx:87` `AppearanceSettingsPanel`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`; `apps/web/src/panelAnimations.ts:31` `usePanelAnimationSettings`.  
  State. Settings unknown. Event. Inspect current duration and bindings. Result to verify. Configured duration and effective gates recorded.

- [ ] **BASE-04. Record current surface inventory** (`quality_probe`; inspect). Capture left sidebar, all open right-panel tabs, details panel, bottom terminal, floating browser, and active thread before the loop.  
  Observe: Subsequent opens, closes, restores, and cleanup can be attributed to this audit rather than existing user state.  
  Gate: Existing session.  
  Source. `apps/web/src/components/AppSidebarLayout.tsx:410` `AppSidebarLayout`; `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`; `apps/web/src/components/chat/PanelLayoutControls.tsx:38` `PanelLayoutControls`.  
  State. Existing layout untouched. Event. Capture current thread and open controls. Result to verify. Initial layout available as restoration baseline.

- [ ] **BASE-05. Capture outcomes rather than click acknowledgements** (`quality_probe`; inspect). For every exercised item, record before, action, visible after, and any error; attach screenshot/video/time sequence as appropriate.  
  Observe: Only then replace not_exercised with pass/fail/blocked/not_applicable/version_mismatch. A menu item being visible is only inspect evidence.  
  Gate: All items.  
  Source. `apps/web/src/components/AppSidebarLayout.tsx:410` `AppSidebarLayout`.  
  State. Action acknowledged but result unconfirmed. Event. Obtain fresh post-action UI evidence. Result to verify. Fresh visible state recorded or observation remains pending.

- [ ] **BASE-06. Use disposable state for persistent/destructive actions** (`quality_probe`; inspect). Use the parent-created audit thread/page and audit files for rename/pin/settle/snooze/close. Inspect Delete, Archive, Clear cookies/cache, Share with guest, update/install, and quit paths without executing against existing user data.  
  Observe: Coverage can record a reachable menu/confirmation without claiming the resulting destructive operation succeeded. Do not send or publish via these menu explorations.  
  Gate: All state-changing items.  
  Source. `apps/web/src/components/threadActionMenu.logic.ts:116` `buildThreadActionMenuItems`; `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`; `apps/desktop/src/window/DesktopApplicationMenu.ts:116` `make`.  
  State. Existing user data present. Event. Inspect irreversible paths and choose disposable audit state. Result to verify. No existing user data modified by coverage setup.


## Left sidebar, projects, thread rows and footer

Source paths under the pinned clone: `apps/web/src/components/sidebar/SidebarChrome.tsx:65-74,225-300`; `apps/web/src/components/sidebar/SidebarThreadHeader.tsx:60-115`; `apps/web/src/components/Sidebar.tsx:1284-1293,1510-1607,2111-2175,2335-2404,2950-3120,3200-3334,3485-3678`; `apps/web/src/components/sidebar/SidebarThreadRow.tsx:608-637,858-940`; `apps/web/src/components/threadActionMenu.logic.ts:39-275`; `apps/web/src/components/AppSidebarLayout.tsx:428-490`; `apps/web/src/components/ui/sidebar.tsx:109,324-403,473-542`.

- [ ] **SIDE-01. Toggle left sidebar by mouse and key** (`source_supported`; exercise). Use Toggle main sidebar, reopen, then Cmd+B; repeat from composer, browser page, and an input with selected text.  
  Observe: Sidebar visibility changes without misrouting to another panel; selected text and global-shortcut precedence are observed rather than assumed.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/sidebar/SidebarChrome.tsx:43` `SidebarChromeHeader`; `apps/web/src/components/ui/sidebar.tsx:281` `Sidebar`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`.  
  State. Sidebar open or closed. Event. Toggle by pointer and key. Result to verify. Opposite visibility then original visibility restored.

- [ ] **SIDE-02. Resize left sidebar and reset** (`source_supported`; exercise). Drag its rail smaller/larger; record width and remaining main width. Double-click rail to reset. Close/reopen and revisit thread.  
  Observe: Default256px; source minimum208px or measured brand minimum; bounds preserve640px main area where space permits; reset clears chosen width. No stuck drag.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/AppSidebarLayout.tsx:410` `AppSidebarLayout`; `apps/web/src/components/ui/sidebar.tsx:281` `Sidebar`.  
  State. Sidebar at remembered width. Event. Drag rail then double-click reset. Result to verify. Chosen width persists until reset returns default width.

- [ ] **SIDE-03. Home and app navigation** (`source_supported`; exercise). Click T3 Code home, then return to audit thread through sidebar/history.  
  Observe: Visible route and active-row treatment agree; existing draft/thread state remains reachable.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/sidebar/SidebarChrome.tsx:43` `SidebarChromeHeader`; `apps/web/src/components/AppSidebarLayout.tsx:151` `registerNavigationHistoryCommands`.  
  State. Audit thread active. Event. Navigate home then back. Result to verify. Audit thread becomes active again.

- [ ] **SIDE-04. New thread entry points** (`source_supported`; exercise). Open header New thread, project-folder New thread, and Cmd+N/Cmd+Shift+N entry points; cancel unneeded drafts.  
  Observe: Context chooses current project or project chooser as applicable; do not assume every New thread entry means the same project.  
  Gate: Use audit project; terminal focus changes Cmd+N meaning.  
  Source. `apps/web/src/components/sidebar/SidebarThreadHeader.tsx:34` `SidebarThreadHeader`; `apps/web/src/components/Sidebar.tsx:902` `Sidebar`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`.  
  State. Current project or unscoped context. Event. Invoke each New thread entry. Result to verify. Draft or project chooser corresponds to entry context.

- [ ] **SIDE-05. Add project chooser** (`source_supported`; inspect). Open Add project from header or empty state; inspect chooser, folder drop affordance and cancel.  
  Observe: Chooser appears, can be dismissed, and focus returns without adding an unintended project.  
  Gate: Header/empty-state availability varies.  
  Source. `apps/web/src/components/sidebar/SidebarThreadHeader.tsx:34` `SidebarThreadHeader`; `apps/web/src/components/Sidebar.tsx:902` `Sidebar`.  
  State. Project chooser closed. Event. Open Add project then cancel. Result to verify. Chooser closes and prior focus returns.

- [ ] **SIDE-06. Project filtering and keyboard menu** (`source_supported`; exercise). Open Filter threads by project; search; select one; Shift-click or Shift+Enter a second; choose All projects. Open highlighted project context menu using Shift+F10 if supported.  
  Observe: Multi-project selection does not prematurely close/freeze search; current filter is represented in trigger; no-match state is legible.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/Sidebar.tsx:902` `Sidebar`.  
  State. All projects selected. Event. Search and select one or several projects then clear. Result to verify. Filtered list then all-project list appears.

- [ ] **SIDE-07. Project context actions** (`source_supported`; exercise). Right-click project in filter/folder: inspect New thread, Copy path, Pin/Unpin project when supported, Project settings; open/cancel settings.  
  Observe: Actions target the intended project; Copy path can be verified via disposable field; no sibling click fires.  
  Gate: Project available; pin capability conditional.  
  Source. `apps/web/src/components/Sidebar.tsx:902` `Sidebar`; `apps/web/src/components/sidebar/SidebarProjectFolderHeader.tsx:38` `SidebarProjectFolderHeader`.  
  State. Project row available. Event. Open its context menu and settings. Result to verify. Menu or settings targets same project and dismisses.

- [ ] **SIDE-08. Folder collapse and hover affordances** (`source_supported`; exercise). If folder-grouped sidebar is active, collapse/expand project; hover/focus it.  
  Observe: Collapsed count/status dot remains informative; chevron changes; New thread action replaces count without overlapping; clicking it does not merely toggle folder.  
  Gate: Folder grouping enabled; do not change global grouping merely to manufacture a pass.  
  Source. `apps/web/src/components/sidebar/SidebarProjectFolderHeader.tsx:38` `SidebarProjectFolderHeader`.  
  State. Project folder expanded. Event. Collapse then expand and use hover action. Result to verify. Hidden count/status shown while collapsed and rows restored.

- [ ] **SIDE-09. Thread search success and message matches** (`source_supported`; exercise). Search known title, then known message text; use ArrowDown/Up and Enter.  
  Observe: Result highlight remains visible; selected message match opens the thread find context; clear search returns normal list.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/Sidebar.tsx:902` `Sidebar`.  
  State. Normal thread list. Event. Search known content and activate result. Result to verify. Matching thread active with find context when match is a message.

- [ ] **SIDE-10. Thread search empty, pending and cancel** (`source_supported`; exercise). Search a deliberately unmatched term; observe Searching thread messages… versus No threads found; Escape and Clear.  
  Observe: Distinct pending/empty states, no stale highlight; Escape clears search without closing unrelated surfaces.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/Sidebar.tsx:902` `Sidebar`.  
  State. Search query present. Event. Search unmatched term then Escape. Result to verify. Pending or empty result appears then normal list returns.

- [ ] **SIDE-11. Thread activation and nested click targets** (`source_supported`; exercise). Click row body, focus row and Enter/Space, then click a nested link/status action.  
  Observe: Body activates intended thread; nested links/actions do not also activate the row. Capture any unwanted jump to background work.  
  Gate: Use available row links/statuses.  
  Source. `apps/web/src/components/sidebar/SidebarThreadRow.tsx:642` `SidebarThreadRow`; `apps/web/src/components/Sidebar.tsx:902` `Sidebar`.  
  State. Inactive thread row or nested action visible. Event. Activate row then nested action separately. Result to verify. Only intended activation path fires.

- [ ] **SIDE-12. Rename with mouse and keyboard** (`source_supported`; exercise). On audit thread: double-click title or F2; edit; Escape; repeat and Enter; test blank title; restore original.  
  Observe: Cancel preserves original; Enter/blur commits once; blank title shows warning; trailing double-click does not navigate twice.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/sidebar/SidebarThreadRow.tsx:642` `SidebarThreadRow`; `apps/web/src/components/Sidebar.tsx:902` `Sidebar`.  
  State. Thread title not editing. Event. F2 or double-click then cancel or commit. Result to verify. Original or changed title reflects selected outcome once.

- [ ] **SIDE-13. Modifier selection and bulk context menu** (`source_supported`; inspect). Cmd-click two audit rows; Shift-click range; right-click selection. Inspect bulk Settle/Snooze/Unpin/Regenerate title/Mark unread/Delete.  
  Observe: Selection differs from normal navigation; bulk count and enabled states match selected rows.  
  Gate: At least two audit rows.  
  Source. `apps/web/src/components/Sidebar.tsx:902` `Sidebar`.  
  State. No multi-selection. Event. Cmd-click or Shift-click rows then context menu. Result to verify. Selection and bulk count reflect intended range.

- [ ] **SIDE-14. Thread context menu inventory** (`source_supported`; inspect). Right-click audit thread and record complete menu plus Copy and Auto-settle submenus.  
  Observe: Source supports conditional Find, New thread on branch, Pin/Unpin, Settle/Un-settle, Snooze/Wake, Rename, Regenerate title, Mark unread, Share with guest, project filter/promotion, Auto-settle, Copy Path/Branch/Thread/ID, Project settings, Archive, Delete. Record capability-gated omissions.  
  Gate: Backend permissions/capabilities and branch determine items.  
  Source. `apps/web/src/components/threadActionMenu.logic.ts:116` `buildThreadActionMenuItems`; `apps/web/src/components/Sidebar.tsx:902` `Sidebar`.  
  State. Thread context menu closed. Event. Right-click and inspect submenus. Result to verify. Capability-gated actions are visible for same thread.

- [ ] **SIDE-15. Pin, unpin, settle and undo** (`source_supported`; exercise). On audit thread, pin/unpin; settle then invoke visible Undo or Cmd+Z outside editable focus; restore original state.  
  Observe: Correct row moves/restores, selected-thread navigation is understandable, undo notice is reachable, typing Undo does not trigger thread undo.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/threadActionMenu.logic.ts:116` `buildThreadActionMenuItems`; `apps/web/src/components/Sidebar.tsx:902` `Sidebar`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`.  
  State. Audit row active or unpinned. Event. Pin or settle then undo. Result to verify. Original row state and navigation restored.

- [ ] **SIDE-16. Snooze custom and wake path** (`source_supported`; exercise). Inspect preset labels/dates and Custom… dialog; cancel. If using disposable audit state, snooze then Wake immediately.  
  Observe: Unavailable-running/queued states explain themselves; wake returns intended thread and status.  
  Gate: Snooze capability and eligible thread; never alter production thread schedule.  
  Source. `apps/web/src/components/threadActionMenu.logic.ts:116` `buildThreadActionMenuItems`; `apps/web/src/components/sidebar/SidebarThreadRow.tsx:642` `SidebarThreadRow`.  
  State. Eligible audit row awake. Event. Inspect snooze then optionally snooze and wake. Result to verify. Custom dialog cancels or row returns awake.

- [ ] **SIDE-17. Draft row actions** (`source_supported`; exercise). Inspect an unsent audit draft row and its context menu; switch away/back with draft text; inspect Discard.  
  Observe: Draft marker and text survive navigation; Copy Path/Branch and Project settings are appropriately conditional; do not discard unrelated drafts.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/sidebar/SidebarDraftBlock.tsx:145` `SidebarDraftBlock`.  
  State. Unsent audit draft with text. Event. Navigate away and back and inspect menu. Result to verify. Same unsent draft and text retained.

- [ ] **SIDE-18. Shelves and older settled rows** (`source_supported`; exercise). Expand/collapse Working, Snoozed and Settled shelves that exist; use Show more for old settled rows.  
  Observe: Counts and chevrons match; Loading, retry/error and more labels are distinguishable; scrolling remains stable.  
  Gate: Shelves/data may be absent; record unavailable reason.  
  Source. `apps/web/src/components/Sidebar.tsx:902` `Sidebar`.  
  State. Working or snoozed or settled shelf present. Event. Expand then collapse and load older entries. Result to verify. Rows and counts change with visible pending or retry state.

- [ ] **SIDE-19. Thread drag and cancellation** (`source_supported`; exercise). On audit rows in an eligible flat list, drag within pinned/active sections or toward section boundaries; cancel once with Escape and once by dropping outside.  
  Observe: Supported drop targets show feedback; canceled drag changes nothing. Source has conditional drag disabling/grouped-list restrictions, so do not promise every list permits reorder.  
  Gate: Flat/list mode and supported row; use audit rows.  
  Source. `apps/web/src/components/Sidebar.tsx:902` `Sidebar`; `apps/web/src/components/sidebar/SidebarThreadRow.tsx:642` `SidebarThreadRow`.  
  State. Eligible audit row at initial order. Event. Drag then cancel or drop. Result to verify. Canceled move leaves order unchanged or supported target commits.

- [ ] **SIDE-20. Pointer stability during list changes** (`quality_probe`; exercise). Hover a row/action while a live audit thread changes running→done status; move slightly, click the intended row, then scroll.  
  Observe: Rows should not shift into an unrelated action beneath the held pointer; capture any wrong-row activation.  
  Gate: Parent-owned live model turn provides status changes.  
  Source. `apps/web/src/components/Sidebar.shiftGuard.ts:37` `createSidebarShiftGuard`; `apps/web/src/components/Sidebar.motion.ts:15` `createSidebarListMotion`.  
  State. Pointer over row while status may update. Event. Observe status transition then click. Result to verify. Click still targets intended row or defect is captured.

- [ ] **SIDE-21. Sidebar footer routes and return** (`source_supported`; exercise). Click Settings, Pull requests when present, Usage; use Back from each utility page.  
  Observe: Each destination is accessible, has an actual body, and returns to the expected app/thread context; PR icon absence records capability/version, not assumed failure.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/sidebar/SidebarChrome.tsx:221` `SidebarUtilityMenu`.  
  State. Main app route. Event. Open Settings or PRs or Usage then Back. Result to verify. Utility body then prior main app context appears.

- [ ] **SIDE-22. Update/provider notice affordances** (`source_supported`; inspect). If update/provider pills exist, inspect tooltip, release notes and dismiss affordance; do not install updates/restart.  
  Observe: Long notice text fits; release notes open and close; update state is not confused with a thread control.  
  Gate: A notice must actually exist.  
  Source. `apps/web/src/components/sidebar/SidebarUpdatePill.tsx:195` `SidebarUpdatePill`; `apps/web/src/components/sidebar/SidebarProviderUpdatePill.tsx:45` `SidebarProviderUpdatePill`.  
  State. Update notice present. Event. Open tooltip or notes and dismiss inspection. Result to verify. Notice/details state changes without installing update.


## Right panel, surface launcher and shared tabs

Source paths under the pinned clone: `apps/web/src/components/RightPanelTabs.tsx:330-480,1019-1025,1100-1271,1325-1539`; `apps/web/src/components/chat/PanelLayoutControls.tsx:55-146,151-182,205-259`; `apps/web/src/components/chat/ChatRightPanel.tsx:30-49`; `apps/web/src/components/ChatView.tsx:1064,3976`; `apps/web/src/hooks/usePreviewPanelInlineSize.ts:13-73`; `apps/web/src/hooks/useResizableWidth.ts:38-108`; `apps/web/src/components/preview/PreviewPanelShell.tsx:55-219`.

- [ ] **PANEL-01. Open right panel and distinguish details panel** (`source_supported`; exercise). Toggle right panel with mouse and Cmd+Option+B; separately toggle thread details with Cmd+Shift+B.  
  Observe: Controls target different surfaces; active/pressed states match visible panel; closing one does not silently open the other.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/chat/PanelLayoutControls.tsx:38` `PanelLayoutControls`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`.  
  State. Right panel and details visibility known. Event. Toggle each independently. Result to verify. Only intended visibility state changes.

- [ ] **PANEL-02. Empty launcher and keyboard navigation** (`source_supported`; exercise). In empty audit right panel, focus Open a surface; use arrows/Enter; test B/T/F/D/P/L/M/S letters when launcher is focused and when a text input owns focus.  
  Observe: Available actions correspond to Browser/Terminal/Files/Diff/PR/Linked PRs/Device/Side chat; letters do not hijack typing or another blocking menu.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`.  
  State. Panel has no active surface. Event. Use launcher arrow or letter shortcut. Result to verify. Selected available kind opens and typing remains protected.

- [ ] **PANEL-03. Plus menu and all surface entry points** (`source_supported`; exercise). Open Add panel surface; record each visible item, disabled reason, submenus, keyboard hover/focus state.  
  Observe: No dead launcher item; unavailable surfaces have a meaningful reason. Record actual installed menu rather than projecting source inventory.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`.  
  State. Add menu closed. Event. Open and inspect each branch. Result to verify. Menu entries reflect capabilities and disabled reasons.

- [ ] **PANEL-04. Browser and profile submenu** (`source_supported`; exercise). Open Browser in default profile; open Browser submenu and inspect configured profiles; optionally open second audit browser in an existing profile.  
  Observe: Correct tab activates with appropriate identity; no new profile/account configuration is created.  
  Gate: Profiles conditional.  
  Source. `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`; `apps/web/src/components/chat/useRightPanelActions.ts:68` `useRightPanelActions`.  
  State. Current active surface. Event. Open browser or existing profile browser. Result to verify. New or selected browser resource becomes active.

- [ ] **PANEL-05. Terminal surface routing** (`source_supported`; exercise). Open Terminal from plus menu and compare to bottom terminal drawer.  
  Observe: Terminal surface has a usable body, focus and controls; it is distinguished from the separately toggled drawer.  
  Gate: Terminal capability; no shell command required for routing check.  
  Source. `apps/web/src/components/chat/useRightPanelActions.ts:68` `useRightPanelActions`; `apps/web/src/components/chat/RightPanelSurfaceContent.tsx:62` `RightPanelSurfaceContent`; `apps/web/src/components/chat/ChatTerminalDrawers.tsx:41` `ChatTerminalDrawers`.  
  State. Non-terminal surface active. Event. Open Terminal from launcher. Result to verify. Terminal surface active independently of drawer.

- [ ] **PANEL-06. Files surface routing** (`source_supported`; exercise). Open Files; select one known safe file; use Copy path on file tab if present; return to Files.  
  Observe: Tree/file body is reachable, selected file tab is correctly identified, navigation does not unexpectedly jump to another thread.  
  Gate: Workspace files available.  
  Source. `apps/web/src/components/chat/useRightPanelActions.ts:68` `useRightPanelActions`; `apps/web/src/components/chat/RightPanelSurfaceContent.tsx:62` `RightPanelSurfaceContent`.  
  State. Files tree available. Event. Open Files then safe file. Result to verify. File resource tab becomes active and returns to tree.

- [ ] **PANEL-07. Diff surface routing** (`source_supported`; exercise). Open Diff; inspect clean/changed state and switch back to Browser.  
  Observe: Diff has an actual appropriate body/empty state; opening it does not silently steal a different resource tab.  
  Gate: Diff capability; no repository edits required.  
  Source. `apps/web/src/components/chat/useRightPanelActions.ts:68` `useRightPanelActions`; `apps/web/src/components/chat/RightPanelSurfaceContent.tsx:62` `RightPanelSurfaceContent`.  
  State. Browser active. Event. Open Diff then Browser. Result to verify. Correct bodies swap and original browser remains.

- [ ] **PANEL-08. Current PR and linked PR entry points** (`source_supported`; exercise). Open Pull request and Linked pull requests from launcher; also compare sidebar PR route.  
  Observe: Each route displays correct PR/list/empty/unavailable state and links are actually clickable. Capture unreachable/dead paths, not just visible tab labels.  
  Gate: PR capabilities and repository association conditional.  
  Source. `apps/web/src/components/chat/useRightPanelActions.ts:68` `useRightPanelActions`; `apps/web/src/components/chat/RightPanelSurfaceContent.tsx:62` `RightPanelSurfaceContent`; `apps/web/src/components/sidebar/SidebarChrome.tsx:221` `SidebarUtilityMenu`.  
  State. No PR body selected. Event. Use current PR then linked PR then sidebar PR entry. Result to verify. Distinct target body or supported empty state appears.

- [ ] **PANEL-09. Device surface routing** (`source_supported`; inspect). Open Device if enabled; inspect empty/select-device state, available simulator/emulator choices and tab naming.  
  Observe: No fabricated connected device; unsupported/absent hardware is recorded, and Escape/close returns cleanly.  
  Gate: Device capability/hardware conditional; do not launch emulator for this checklist alone.  
  Source. `apps/web/src/components/chat/useRightPanelActions.ts:68` `useRightPanelActions`; `apps/web/src/components/chat/RightPanelSurfaceContent.tsx:62` `RightPanelSurfaceContent`.  
  State. No Device selected. Event. Open Device and inspect chooser. Result to verify. Device body or no-device state appears.

- [ ] **PANEL-10. Side chat routing** (`source_supported`; inspect). Open Side chat if enabled; inspect provider/thread binding and empty state without sending.  
  Observe: The side chat is distinct and retains parent-thread context; disabled reason is visible when unavailable.  
  Gate: Capability and provider state conditional.  
  Source. `apps/web/src/components/chat/useRightPanelActions.ts:68` `useRightPanelActions`; `apps/web/src/components/chat/RightPanelSurfaceContent.tsx:62` `RightPanelSurfaceContent`.  
  State. No side chat selected. Event. Open Side chat without sending. Result to verify. Side-chat body reflects parent thread.

- [ ] **PANEL-11. Activate, reorder and overflow tabs** (`source_supported`; exercise). Create several disposable surfaces; click each; drag reorder; overflow horizontally; use chevrons and vertical wheel over strip.  
  Observe: Active tab/body agree; drag target and final order are clear; overflow controls stay reachable; wheel does not unexpectedly move conversation.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`.  
  State. Several tabs at known order. Event. Activate and drag then overflow scroll. Result to verify. Correct active body and new order visible.

- [ ] **PANEL-12. Tab close by x, middle click and keyboard** (`source_supported`; exercise). Close disposable tab with x; another with middle click if input hardware supports it; another with Cmd+W outside terminal focus.  
  Observe: Only target surface closes; next active surface is sensible; Cmd+W does not close app window. Mark unavailable hardware rather than inventing a middle-click pass.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`.  
  State. Disposable tab active. Event. Close by x or middle click or shortcut. Result to verify. Target removed and app window remains open.

- [ ] **PANEL-13. Reopen closed view** (`source_supported`; exercise). After each supported close, use Cmd+Shift+T and verify reopened resource and active tab.  
  Observe: Actual reopen behavior is captured by surface type; do not claim native/live resources all restore identically.  
  Gate: Reopen stack has audit entries.  
  Source. `apps/web/src/components/ReopenClosedViewShortcut.tsx:62` `ReopenClosedViewShortcut`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`.  
  State. Supported audit view recently closed. Event. Invoke reopen closed view. Result to verify. Same closed resource reappears according to supported type.

- [ ] **PANEL-14. Tab context menu and conditional actions** (`source_supported`; exercise). Right-click browser, file, device tabs; inspect Close/Close others/Close to right/Close all, Mute/Unmute, Copy path, Rename.  
  Observe: Disabled edge cases match tab position/count; conditional actions target correct surface. Use disposable tabs for close groups.  
  Gate: Type-specific actions conditional.  
  Source. `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`.  
  State. Type-specific tab visible. Event. Open context action and optionally close disposable tabs. Result to verify. Only targeted resource group changes.

- [ ] **PANEL-15. Rename a device tab** (`source_supported`; exercise). If audit Device exists, Rename; Enter commit and Escape cancel; restore name.  
  Observe: Title editing does not activate/drag/close wrong tab; only this surface name changes.  
  Gate: Device tab and rename capability.  
  Source. `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`.  
  State. Device title not editing. Event. Rename then cancel or commit. Result to verify. Device title follows selected outcome.

- [ ] **PANEL-16. Resize width and reopen** (`source_supported`; exercise). Drag right edge handle to chosen compact/wide values; close/open; switch Browser→Files→Diff→Browser; record width each time.  
  Observe: Source uses per-thread shared width; a tool switch alone does not impose Ferrite-style tool floors. Record first paint and settled bounds, not just final width.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/hooks/usePreviewPanelInlineSize.ts:20` `usePreviewPanelInlineSize`; `apps/web/src/hooks/useResizableWidth.ts:31` `useResizableWidth`; `apps/web/src/components/chat/ChatRightPanel.tsx:19` `ChatRightPanel`.  
  State. Panel at initial chosen width. Event. Resize then reopen and change tool. Result to verify. Width and resource restoration can be measured.

- [ ] **PANEL-17. Cross-thread width and tab restoration** (`source_supported`; exercise). Set different audit widths/surface selections on two threads; navigate A→B→A; close/reopen panel.  
  Observe: Correct thread owns its stored width and surfaces; no preview/tab from B remains in A.  
  Gate: Two audit threads available.  
  Source. `apps/web/src/components/chat/ChatRightPanel.tsx:19` `ChatRightPanel`; `apps/web/src/hooks/useResizableWidth.ts:31` `useResizableWidth`.  
  State. Thread A has different panel state from B. Event. Navigate A to B to A. Result to verify. Each thread restores its own width and selected resource.

- [ ] **PANEL-18. Resize interruption** (`source_supported`; exercise). Start resize; release outside; start again then collapse or switch thread; reopen and drag once more.  
  Observe: No stuck resize, wrong-thread persistence, or animated lag during drag. Test real pointer sequence only.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/hooks/useResizableWidth.ts:31` `useResizableWidth`; `apps/web/src/components/preview/PreviewPanelShell.tsx:38` `PreviewPanelShell`.  
  State. Resize not active. Event. Start drag then release outside or collapse. Result to verify. Drag ends without persistent capture or wrong-thread commit.

- [ ] **PANEL-19. Maximize and restore** (`source_supported`; exercise). Use Maximize panel, inspect browser/Files/body and title controls, then Restore panel size; repeat with sidebar collapsed.  
  Observe: Maximize actually changes content allocation; restore returns prior width; controls do not overlap or become covered.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/chat/PanelLayoutControls.tsx:140` `RightPanelMaximizeControl`; `apps/web/src/components/preview/PreviewPanelShell.tsx:38` `PreviewPanelShell`.  
  State. Panel restored to chosen width. Event. Maximize then restore. Result to verify. Full panel allocation then prior width returns.

- [ ] **PANEL-20. Focus order and collapse return** (`source_supported`; exercise). F6/Shift+F6 between workspace areas; focus input in panel, collapse with shortcut, then type in expected returned target.  
  Observe: Hidden panel cannot retain keyboard input; focus returns without scrolling conversation unexpectedly. Separator keyboard resize is a discovery probe: source shell does not wire it by default.  
  Gate: Use visible focus indication; Cmd+F6 variants if plain F6 is captured.  
  Source. `apps/web/src/lib/workspaceFocus.ts:100` `createWorkspaceFocusController`; `apps/web/src/components/preview/PreviewPanelShell.tsx:38` `PreviewPanelShell`.  
  State. Keyboard focus inside panel. Event. Cycle focus then collapse. Result to verify. Focus moves to a visible valid area.


## Browser navigation, chrome, menus, page states and capture

Source paths under the pinned clone: `apps/web/src/components/preview/PreviewChromeRow.tsx:80-299`; `apps/web/src/components/preview/PreviewView.tsx:430-490,549-776,787-890,964-1165`; `apps/web/src/components/preview/PreviewMoreMenu.tsx:80-185`; `apps/web/src/components/preview/PreviewFindBar.tsx:14-138`; `apps/web/src/browser/BrowserDeviceToolbar.tsx:173-337`; `apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx:541-619`; `apps/web/src/components/preview/PreviewEmptyState.tsx:35-110`; `apps/web/src/components/preview/PreviewUnreachable.tsx:61-140`; `apps/web/src/components/preview/thumbButtons.ts:1-21`; `apps/desktop/src/window/DesktopWindow.ts:654-735`.

- [ ] **BROWSER-01. Blank browser composition** (`source_supported`; exercise). Open a fresh Browser; capture whole panel before navigation and inspect recent/local destinations if present.  
  Observe: Empty title/description or structured recent/local cards are rendered; URL entry remains available; no page/header overlaps.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/preview/PreviewEmptyState.tsx:21` `PreviewEmptyState`; `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Fresh browser has no URL. Event. Open browser. Result to verify. Empty or recent/local destination content visible.

- [ ] **BROWSER-02. URL editing, cancellation and Enter** (`source_supported`; exercise). Focus page, Cmd+L; type safe public URL, Escape; repeat Enter. Click address and select/copy text.  
  Observe: Focus selects editable URL; Escape restores actual URL and blurs; Enter navigates once; clicking address does not route elsewhere.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/preview/PreviewChromeRow.tsx:53` `PreviewChromeRow`.  
  State. URL input not focused. Event. Focus URL then Escape or Enter. Result to verify. Draft cancels to URL or one navigation starts.

- [ ] **BROWSER-03. Search and malformed input** (`discovery_probe`; exercise). Try a harmless search phrase, hostname without scheme, whitespace, then an invalid address; record normalization/error.  
  Observe: Placeholder promises Search or enter URL, but exact installed behavior must be observed; invalid input should recover through address edit.  
  Gate: Use public/disposable navigation.  
  Source. `apps/web/src/components/preview/PreviewChromeRow.tsx:53` `PreviewChromeRow`; `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Valid current page. Event. Submit search or malformed URL. Result to verify. Actual normalization or recoverable error recorded.

- [ ] **BROWSER-04. Navigation and history buttons** (`source_supported`; exercise). Visit two public pages in same tab; Back, Forward, refresh; inspect disabled states at history ends.  
  Observe: Correct page history, title/URL and button states move together; app/thread history remains unchanged.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`; `apps/web/src/components/preview/PreviewChromeRow.tsx:53` `PreviewChromeRow`.  
  State. Browser tab has history. Event. Navigate back then forward and reload. Result to verify. Page history and URL controls agree.

- [ ] **BROWSER-05. Loading and Stop geometry** (`source_supported`; exercise). Reload a page; capture loading and settled frames, then press Stop during a visible load if possible.  
  Observe: Refresh becomes Stop in same slot;2px progress overlays toolbar without inserting a new row; page top and address/control positions stay stable.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/preview/PreviewChromeRow.tsx:53` `PreviewChromeRow`; `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Page loaded. Event. Refresh then Stop during load. Result to verify. Same control slot changes and progress stops.

- [ ] **BROWSER-06. Keyboard refresh and focus scope** (`source_supported`; exercise). Cmd+R while browser page focused; repeat after composer/app focus and compare to native View→Reload.  
  Observe: Record what reloads in each context. Browser command is previewFocus-gated; native menu targets app. No broad claim from one chord in one focus context.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`; `apps/desktop/src/window/DesktopApplicationMenu.ts:116` `make`.  
  State. Focus is in browser or app. Event. Invoke refresh in each focus context. Result to verify. Correct target reloads and focus scope is recorded.

- [ ] **BROWSER-07. Open in system browser** (`source_supported`; exercise). Hover URL group while unfocused; activate Open in system browser; return to T3.  
  Observe: External browser opens the current URL; preview stays intact; action remains accessible through keyboard focus/tooltip.  
  Gate: OS external browser handler present.  
  Source. `apps/web/src/components/preview/PreviewChromeRow.tsx:53` `PreviewChromeRow`; `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Current page loaded and URL unfocused. Event. Reveal external-open control and click. Result to verify. System browser opens current URL while preview remains.

- [ ] **BROWSER-08. Webpage link context menu versus toolbar external-open** (`discovery_probe`; inspect). Right-click a known hyperlink inside page and a transcript hyperlink separately; record exact menu and whether an external-open option exists.  
  Observe: Pinned generic desktop menu proves Copy Link, not Open Link in Browser. This is a discovery probe, independent of the supported toolbar external-open action.  
  Gate: Actual link needed.  
  Source. `apps/desktop/src/window/DesktopWindow.ts:648` `installContextMenu`; `apps/web/src/components/preview/PreviewChromeRow.tsx:53` `PreviewChromeRow`.  
  State. Link under pointer. Event. Open context menu. Result to verify. Actual link menu observed independently of toolbar action.

- [ ] **BROWSER-09. Context menu text, image and email** (`source_supported`; exercise). Right-click selected page text, editable input, image and mailto link; inspect Copy Image/Copy Email Address/edit roles where present; verify copy into disposable field if useful.  
  Observe: Editing roles target guest page rather than host composer; disabled Cut/Paste/Select All accurately match context.  
  Gate: Use harmless public/test content.  
  Source. `apps/desktop/src/window/DesktopWindow.ts:648` `installContextMenu`.  
  State. Selection or image or mailto link under pointer. Event. Open context menu and copy safe content. Result to verify. Context-specific roles and clipboard output agree.

- [ ] **BROWSER-10. Page links, new windows and modifier clicks** (`discovery_probe`; exercise). Click same-tab and target-blank links; try Cmd-click/middle-click if supported; inspect where new content appears.  
  Observe: Discover actual installed policy and tab/window identity; do not assume conventional browser-tab creation from source toolbar alone.  
  Gate: Public page with links.  
  Source. `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`; `apps/desktop/src/window/DesktopWindow.ts:648` `installContextMenu`.  
  State. Page has navigation links. Event. Click same-tab and new-window links. Result to verify. Actual destination policy recorded without assumption.

- [ ] **BROWSER-11. Mouse thumb-button routing** (`source_supported`; exercise). With real thumb-button input, navigate back/forward over page, toolbar, and panel tab strip.  
  Observe: Browser surface history changes without moving the application route; URL-field focus is not stolen.  
  Gate: Hardware/tool support needed; otherwise blocked, not pass.  
  Source. `apps/web/src/components/preview/thumbButtons.ts:12` `takeThumbButtonNavigation`.  
  State. Browser has back or forward history. Event. Use mouse thumb button over browser area. Result to verify. Page history changes without app route change.

- [ ] **BROWSER-12. Page title, favicon, long URL and localhost ports** (`source_supported`; exercise). Inspect real page title/favicon; compare long titles/URLs and two distinct local ports if already available.  
  Observe: Tabs identify page with title/host including port; labels ellipsize while close/mute controls remain reachable.  
  Gate: No local server launch required; port comparison conditional.  
  Source. `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`; `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Loaded page with title or URL. Event. Inspect tab label and compare resources. Result to verify. Identity and truncation remain readable.

- [ ] **BROWSER-13. Multiple tabs and browser state** (`source_supported`; exercise). Open two pages through Browser launcher; type unsent text in one page field, switch tabs/thread and return.  
  Observe: Active page, history, URL and form state follow correct tab; background page does not steal focus.  
  Gate: Disposable public/test page form only.  
  Source. `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`; `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Two browser resources with distinct state. Event. Switch tab and thread then return. Result to verify. Correct history and form state restored.

- [ ] **BROWSER-14. Unreachable page and retry** (`source_supported`; exercise). Navigate to a known invalid .invalid hostname or an unused local endpoint; inspect explanatory body, Details and Reload; then navigate valid URL.  
  Observe: Error is composed in page body with useful recovery; toolbar remains usable; no raw error line shifts chrome.  
  Gate: Use safe deliberately unreachable target.  
  Source. `apps/web/src/components/preview/PreviewUnreachable.tsx:54` `PreviewUnreachable`; `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Valid loaded page. Event. Navigate unreachable URL then valid URL. Result to verify. Error body and recovery return to usable page.

- [ ] **BROWSER-15. Browser More menu inventory** (`source_supported`; inspect). Open More and capture all enabled/disabled items and nested Appearance.  
  Observe: Find, Hard reload, conditional DevTools/separate window/move-runtime, Device toolbar, Appearance, Zoom and Clear cookies/cache are correctly scoped to selected page.  
  Gate: Bridge/runtime capability affects items.  
  Source. `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`.  
  State. More menu closed. Event. Open and inspect branches. Result to verify. Selected page actions and availability recorded.

- [ ] **BROWSER-16. Find success and zero matches** (`source_supported`; exercise). More→Find or Cmd+F in page; search frequent text then unique unmatched term.  
  Observe: Search bar opens selected/focused, shows active/total count, zero-results styling and disabled navigation.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/preview/PreviewFindBar.tsx:14` `PreviewFindBar`; `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`.  
  State. Find closed. Event. Search matching then unmatched text. Result to verify. Visible result count then zero-match state.

- [ ] **BROWSER-17. Find keyboard and tab isolation** (`source_supported`; exercise). Enter next, Shift+Enter previous, repeat Cmd+F to select query, Escape close; reopen then switch browser tab.  
  Observe: One command advances one match; Escape closes only find; query/highlights do not leak into unrelated page; focus returns usefully.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/preview/PreviewFindBar.tsx:14` `PreviewFindBar`.  
  State. Find query active. Event. Step forward/backward and Escape or switch tab. Result to verify. Match or closed state changes without cross-tab leakage.

- [ ] **BROWSER-18. Hard reload** (`source_supported`; exercise). Use More→Hard reload on audit page.  
  Observe: Actual refresh occurs with stable chrome and recoverable state; distinguish from app Force Reload.  
  Gate: Public/disposable page.  
  Source. `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`; `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Page loaded. Event. Invoke Hard reload. Result to verify. Page begins loading and recovers.

- [ ] **BROWSER-19. DevTools routing** (`source_supported`; exercise). Open DevTools from browser More if present; inspect correct inspected URL then close.  
  Observe: Tools inspect browser page, not T3 shell; closing tools does not close current page/app.  
  Gate: Desktop bridge exposes action.  
  Source. `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`; `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Browser DevTools closed. Event. Open then close DevTools. Result to verify. Correct page inspected and preview remains.

- [ ] **BROWSER-20. Page appearance menu** (`source_supported`; exercise). Inspect System/Light/Dark; on disposable preview cycle Light/Dark/System, restore original.  
  Observe: Selected value and webpage appearance update; app theme is not accidentally changed.  
  Gate: Page may not react to prefers-color-scheme; record actual behavior.  
  Source. `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`; `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Page color scheme recorded. Event. Change scheme then restore. Result to verify. Page preference returns to original value.

- [ ] **BROWSER-21. Page zoom and keyboard scope** (`source_supported`; exercise). Use More minus/plus/reset, then Cmd+/Cmd-/Cmd+0 while preview focused; repeat after returning app focus.  
  Observe: Page zoom count and rendered page agree; reset works; app and page zoom scopes are distinguished.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`; `apps/desktop/src/window/DesktopApplicationMenu.ts:116` `make`.  
  State. Page and app zoom recorded. Event. Change page zoom then reset and compare app menu. Result to verify. Correct zoom target and value restored.

- [ ] **BROWSER-22. Device toolbar presets** (`source_supported`; exercise). More→Show device toolbar; open Responsive/Standard preset list; inspect all installed preset labels; choose phone and wide preset.  
  Observe: Viewport dimensions and page framing change together, presets menu stays inside window; no app-level resize mistaken for page emulation.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/browser/BrowserDeviceToolbar.tsx:43` `BrowserDeviceToolbar`; `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`.  
  State. Page viewport fill mode. Event. Show device toolbar and choose preset. Result to verify. Preset geometry and selector agree.

- [ ] **BROWSER-23. Custom viewport dimensions and invalid values** (`source_supported`; exercise). Edit width/height, Enter and blur; try blank/out-of-range value then valid value.  
  Observe: Validation and pending state are visible; accepted dimensions apply once; invalid dimensions do not strand page or overflow toolbar.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/browser/BrowserDeviceToolbar.tsx:43` `BrowserDeviceToolbar`.  
  State. Device size valid. Event. Enter invalid then valid dimensions. Result to verify. Validation state then accepted viewport dimensions.

- [ ] **BROWSER-24. Aspect lock, rotate and viewport drag** (`source_supported`; exercise). Lock aspect ratio; edit one dimension; rotate; unlock; drag viewport handles if visible.  
  Observe: Ratio/rotation/dimensions remain coherent; pointer resizing tracks; narrow toolbar still exposes essential controls.  
  Gate: Rendered viewport handles/presets supported.  
  Source. `apps/web/src/browser/BrowserDeviceToolbar.tsx:43` `BrowserDeviceToolbar`.  
  State. Viewport unlocked or locked. Event. Toggle aspect lock then edit and rotate. Result to verify. Dimension relation and orientation update coherently.

- [ ] **BROWSER-25. Hide device toolbar and restore fill** (`source_supported`; exercise). Use Close device toolbar or More→Hide.  
  Observe: Page returns to fill mode and remaining chrome does not retain blank reserved space.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/browser/BrowserDeviceToolbar.tsx:43` `BrowserDeviceToolbar`; `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`.  
  State. Device toolbar visible. Event. Close toolbar. Result to verify. Viewport returns to fill mode.

- [ ] **BROWSER-26. Floating preview lifecycle** (`source_supported`; exercise). Click Float preview over chat; drag/resize floating preview if available; interact with page; Open in right panel; float again then Close.  
  Observe: Floating and right-panel states hand off same page; controls remain reachable; source floating action closes right panel, which must not be misreported as accidental panel loss.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`; `apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx:195` `ThreadPreviewMiniPlayer`.  
  State. Browser in right panel. Event. Float then reopen in right panel then close float. Result to verify. Same page moves between presentations without orphan.

- [ ] **BROWSER-27. Separate native preview window** (`source_supported`; exercise). If More offers Open separate preview window, open it; inspect title/focus/close; return.  
  Observe: This is distinct from in-app floating preview; correct page is shown and closing separate window does not quit app.  
  Gate: Desktop action conditional.  
  Source. `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`; `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Separate preview window absent. Event. Open and close separate preview. Result to verify. Main app and page remain usable.

- [ ] **BROWSER-28. Move browser runtime** (`source_supported`; exercise). If More offers Open on this computer or Open in environment browser, inspect target; exercise only on disposable page in existing configured environment.  
  Observe: New runtime opens correct URL/profile/viewport and old session closes only after success; failed move shows recoverable message.  
  Gate: Requires alternate runtime; otherwise not_applicable.  
  Source. `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`; `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`.  
  State. Preview in current runtime. Event. Move to existing alternate runtime. Result to verify. Success changes runtime or failure preserves recoverable state.

- [ ] **BROWSER-29. Screenshot capture** (`source_supported`; exercise). Capture screenshot of disposable audit page; inspect returned attachment/toast/path without sending.  
  Observe: Image corresponds to current page and is legible; attachment affordance is usable; failure communicates itself.  
  Gate: Desktop capture capability; may add draft attachment.  
  Source. `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`; `apps/web/src/components/preview/PreviewChromeRow.tsx:53` `PreviewChromeRow`.  
  State. Capture idle. Event. Capture screenshot. Result to verify. Current-page image or explicit failure appears.

- [ ] **BROWSER-30. Recording start/stop** (`source_supported`; exercise). Shift-click camera to record briefly; click Stop recording; inspect resulting artifact/toast; leave no recording running.  
  Observe: Recording indicator/Stop label agree; stop succeeds and correct artifact is reachable.  
  Gate: Desktop recording capability; audit page only.  
  Source. `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`; `apps/web/src/components/preview/PreviewChromeRow.tsx:53` `PreviewChromeRow`.  
  State. Recording idle. Event. Shift-click capture then stop. Result to verify. Recording indicator starts then ends with artifact or error.

- [ ] **BROWSER-31. Annotation select and cancel** (`source_supported`; exercise). Annotate preview; select an element; inspect overlay/comment controls; cancel by offered control or shortcut; reopen and cancel via toolbar.  
  Observe: Pick state and focus reset cleanly; ordinary page clicks work again; current page/tab remains correct.  
  Gate: Desktop element-pick capability.  
  Source. `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`; `apps/web/src/components/preview/PreviewChromeRow.tsx:53` `PreviewChromeRow`.  
  State. Element pick idle. Event. Start pick then cancel. Result to verify. Pick mode ends and prior focus returns.

- [ ] **BROWSER-32. Annotation with comment and attachment handoff** (`source_supported`; exercise). Pick a known element and enter audit comment; use add-to-composer path if offered; inspect text and image attachment; do not submit model request from this checklist.  
  Observe: Comment/element context and optional screenshot survive handoff; screenshot failure retains text/context with explicit notice. Parent may verify model send in its own live task.  
  Gate: Capability and UI submission choices conditional; audit draft.  
  Source. `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Picked element and comment prepared. Event. Add annotation to composer without send. Result to verify. Annotation context and optional image added to audit draft.

- [ ] **BROWSER-33. Annotation interruption and focus** (`source_supported`; exercise). Begin pick, switch tab or close panel, then return; repeat with accepted annotation and another click.  
  Observe: Canceled pick does not leak overlay; accepted annotation survives later clicks/unmount; focus restores without jump-scroll.  
  Gate: Audit page and annotations only.  
  Source. `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`.  
  State. Pick active or accepted. Event. Change tab or close panel. Result to verify. Unaccepted pick cancels and accepted annotation remains.

- [ ] **BROWSER-34. Mute and audio state** (`source_supported`; exercise). If page emits audio, inspect tab audio indicator and Mute/Unmute in context menu; toggle and restore.  
  Observe: Icon, menu label and actual sound state agree; inactive tabs remain identifiable.  
  Gate: Existing harmless audio source; do not autoplay arbitrary external media.  
  Source. `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`.  
  State. Page audio playing or muted. Event. Mute then unmute. Result to verify. Actual sound and tab state agree.

- [ ] **BROWSER-35. Cookie/cache actions visibility** (`source_supported`; inspect). Inspect Clear cookies and Clear cache with profile label. Do not execute on existing authenticated profiles.  
  Observe: Action scope is visible; inspection is not recorded as a successful data-clearing test.  
  Gate: Existing profile; destructive storage change.  
  Source. `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`.  
  State. Profile data unchanged. Event. Inspect clear-storage commands only. Result to verify. Profile data remains unchanged.

- [ ] **BROWSER-36. Browser input and selection fundamentals** (`quality_probe`; exercise). In a disposable page input, click insertion point, select by drag/double-click, use arrows/Home/End where appropriate, copy/paste, Tab/Shift+Tab, scroll and horizontal scroll.  
  Observe: Input goes to intended guest control; host shortcuts do not hijack typing/selection; scroll stays in hovered/active surface.  
  Gate: Test page/input needed.  
  Source. `apps/web/src/components/preview/PreviewView.tsx:162` `PreviewView`; `apps/desktop/src/window/DesktopWindow.ts:648` `installContextMenu`.  
  State. Page input focused. Event. Edit select paste tab and scroll. Result to verify. Guest page receives intended input and host state stays unchanged.


## Window, navigation, overlays and responsive layout

Source paths under the pinned clone: `apps/desktop/src/window/DesktopApplicationMenu.ts:169-339`; `packages/shared/src/keybindings.ts:58-140`; `apps/web/src/components/chat/PanelLayoutControls.tsx:55-146,205-259`; `apps/web/src/rightPanelLayout.ts:1`; `apps/web/src/components/RightPanelSheet.tsx:20-25`; `apps/web/src/components/ui/sidebar.tsx:109,324-350`; `apps/web/src/lib/workspaceFocus.ts`; `apps/web/src/components/ReopenClosedViewShortcut.tsx`.

- [ ] **WINDOW-01. Native menu inventory** (`source_supported`; inspect). Open T3 Code, File, Edit, View, Window, Help menus; record visible menu items and disabled states.  
  Observe: About/Settings/update; close; editing/Paste as Text; reload/devtools/app zoom/fullscreen; minimize/zoom/front; help update are reachable as installed. Hidden accelerators need not appear.  
  Gate: Native desktop app.  
  Source. `apps/desktop/src/window/DesktopApplicationMenu.ts:116` `make`.  
  State. Native menu closed. Event. Open each native menu. Result to verify. Installed labels and enabled states recorded.

- [ ] **WINDOW-02. Settings shortcut and menu return** (`source_supported`; exercise). Cmd+, and native Settings menu; close/Back and return to audit thread.  
  Observe: Both reach same settings context; focus/state recover; no unintentional preference change.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/desktop/src/window/DesktopApplicationMenu.ts:116` `make`; `apps/web/src/components/sidebar/SidebarChrome.tsx:221` `SidebarUtilityMenu`.  
  State. Main app route active. Event. Open Settings then return. Result to verify. Settings then original route visible.

- [ ] **WINDOW-03. Native editing focus** (`source_supported`; exercise). Copy harmless text and use Edit→Paste as Text in disposable composer/page input; compare shortcut once.  
  Observe: Exactly one paste occurs; native editing roles affect intended focused surface, not another thread/page.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/desktop/src/window/DesktopApplicationMenu.ts:116` `make`; `apps/desktop/src/window/DesktopWindow.ts:648` `installContextMenu`.  
  State. Disposable input focused. Event. Paste as Text once. Result to verify. Exactly one plain-text insertion in intended target.

- [ ] **WINDOW-04. App zoom versus browser zoom** (`source_supported`; exercise). Native View→Zoom In/Out/Actual Size once each; inspect entire shell then restore initial app zoom.  
  Observe: Whole shell changes, while browser More zoom remains separate. Capture control clipping at larger shell scale.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/desktop/src/window/DesktopApplicationMenu.ts:116` `make`.  
  State. Initial app zoom recorded. Event. Native zoom then reset or restore. Result to verify. Whole app geometry changes then original scale returns.

- [ ] **WINDOW-05. Window move, resize and native controls** (`source_supported`; exercise). Drag empty title region; click nearby tabs/buttons; resize wide/narrow; use native maximize/restore/fullscreen only if straightforward to restore.  
  Observe: Drag region moves window; interactive tab/control region remains clickable; no accidental window dragging when clicking panel tabs.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/AppSidebarLayout.tsx:410` `AppSidebarLayout`; `apps/web/src/components/chat/PanelLayoutControls.tsx:38` `PanelLayoutControls`.  
  State. Window geometry recorded. Event. Drag titlebar then resize and restore. Result to verify. Window moves or resizes without swallowing control clicks.

- [ ] **WINDOW-06. App history versus browser history** (`source_supported`; exercise). Navigate thread→Settings→thread and use Cmd+[ / Cmd+] outside terminal; compare to browser toolbar history.  
  Observe: App history changes route; browser history changes page. Browser-focused mouse buttons do not jump app route.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/AppSidebarLayout.tsx:151` `registerNavigationHistoryCommands`; `apps/web/src/components/preview/thumbButtons.ts:12` `takeThumbButtonNavigation`.  
  State. Application route and page histories differ. Event. Use app history then page history. Result to verify. Only selected history changes.

- [ ] **WINDOW-07. Workspace focus cycle** (`source_supported`; exercise). Cycle F6/Shift+F6 and Cmd+F6 variants through sidebar, conversation, terminal and right panel when open; repeat with one area hidden.  
  Observe: Visible focus target advances among available areas; hidden areas skipped; opening/closing overlays preserves a sensible return target.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/lib/workspaceFocus.ts:100` `createWorkspaceFocusController`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`.  
  State. Visible workspace areas known. Event. Cycle focus forward and back. Result to verify. Next available area receives focus.

- [ ] **WINDOW-08. Terminal drawer combination** (`source_supported`; exercise). Toggle bottom drawer with Cmd+J; combine with right panel and details; close in alternate orders.  
  Observe: Drawer and right-panel terminal remain distinct; content sizes stay usable; terminal-focus-specific Cmd+N/W/D bindings do not target app tabs.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/chat/ChatTerminalDrawers.tsx:41` `ChatTerminalDrawers`; `apps/web/src/components/chat/PanelLayoutControls.tsx:38` `PanelLayoutControls`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`.  
  State. Terminal drawer closed or open. Event. Toggle drawer alongside right panel. Result to verify. Independent drawer and panel state retained.

- [ ] **WINDOW-09. Command/file/search overlay coexistence** (`source_supported`; exercise). Open Cmd+K palette, Cmd+P file picker, Cmd+Shift+F project search with browser/sidebar visible; dismiss by Escape/click outside.  
  Observe: Overlay is topmost, keyboard belongs to it, underlying webview does not cover it or consume clicks; focus returns.  
  Gate: Workspace/capability conditional.  
  Source. `apps/web/src/components/ReopenClosedViewShortcut.tsx:62` `ReopenClosedViewShortcut`; `apps/web/src/components/AppSidebarLayout.tsx:410` `AppSidebarLayout`; `packages/shared/src/keybindings.ts:57` `DEFAULT_KEYBINDINGS`.  
  State. Browser visible and overlay closed. Event. Open palette or picker or search then dismiss. Result to verify. Overlay receives input and returns focus on dismissal.

- [ ] **WINDOW-10. Right panel responsive switch** (`source_supported`; exercise). Resize across980px while right panel is non-maximized; inspect inline→sheet and back; maximize/restore and repeat.  
  Observe: Source switches non-maximized panel to sheet at<=980px; preserve resource/state and provide reachable close. Installed behavior may differ with version.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/chat/ChatRightPanel.tsx:19` `ChatRightPanel`; `apps/web/src/components/RightPanelSheet.tsx:5` `RightPanelSheet`.  
  State. Viewport above980 and panel not maximized. Event. Resize below980 then above. Result to verify. Panel switches inline to sheet then inline retaining resource.

- [ ] **WINDOW-11. Small sheet and sidebar coexistence** (`source_supported`; exercise). If native window permits, resize below760px/767px; open left mobile sheet and right sheet; close via available outside/Escape controls.  
  Observe: Right sheet uses88vw cap384px below760; left mobile sidebar activates<=767; no trapped invisible layer or titlebar controls above the wrong sheet.  
  Gate: Window minimum dimensions may block thresholds; record blocked dimensions.  
  Source. `apps/web/src/components/ui/sidebar.tsx:281` `Sidebar`; `apps/web/src/components/RightPanelSheet.tsx:5` `RightPanelSheet`.  
  State. Viewport permits small layout. Event. Resize below thresholds and open/close sheets. Result to verify. Mobile/sidebar presentation dismisses without invisible layer.

- [ ] **WINDOW-12. Popup collision and narrow layouts** (`quality_probe`; exercise). At compact width open tab context menu, More submenus, project filter, tooltip and selector near each edge.  
  Observe: Menus flip/shift within viewport, text remains readable, submenu can be entered without disappearing, rightmost buttons remain clickable.  
  Gate: Use actual visible targets.  
  Source. `apps/web/src/components/RightPanelTabs.tsx:1032` `RightPanelTabs`; `apps/web/src/components/preview/PreviewMoreMenu.tsx:72` `PreviewMoreMenu`; `apps/web/src/components/Sidebar.tsx:902` `Sidebar`.  
  State. Compact viewport and menu closed. Event. Open menus near edges. Result to verify. Actual collision handling and reachable content recorded.

- [ ] **WINDOW-13. Reload/close/update paths inspection** (`source_supported`; inspect). Inspect native Reload/Force Reload, Close Window/Quit, Check for Updates and installation prompts without disrupting live session.  
  Observe: Cmd+W renderer tab-close is distinct from native Close Window. Do not count reload/restart/update success from menu visibility.  
  Gate: Session-preserving inspection only.  
  Source. `apps/desktop/src/window/DesktopApplicationMenu.ts:116` `make`.  
  State. Live session remains active. Event. Inspect reload close quit update commands. Result to verify. No disruptive operation executed during inspection.


## Motion, first paint, interruption and visual continuity

Source paths under the pinned clone: `packages/contracts/src/settings.ts:155-164,414`; `apps/web/src/panelAnimations.ts:11-68`; `apps/web/src/components/ui/sidebar.tsx:387-403,473-542`; `apps/web/src/components/preview/PreviewPanelShell.tsx:55-219`; `apps/web/src/components/chat/PanelLayoutControls.tsx:205-259`; `apps/web/src/components/settings/AppearanceSettingsPanel.tsx:350-411`; `apps/web/src/components/Sidebar.motion.ts`; `apps/web/src/components/Sidebar.shiftGuard.ts`; `apps/web/src/index.css:2424-2455`.

- [ ] **MOTION-01. Measure current left-sidebar motion** (`source_supported`; exercise). Record timed frames/video of mouse and shortcut open/close under current recorded settings; repeat twice.  
  Observe: Report observed duration and intermediate geometry;0ms is consistent with pinned default but does not satisfy user-requested Ferrite motion. A still cannot establish smoothness.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/ui/sidebar.tsx:281` `Sidebar`; `apps/web/src/panelAnimations.ts:31` `usePanelAnimationSettings`.  
  State. Sidebar visible at current effective duration. Event. Toggle twice and record frames. Result to verify. Measured visibility transition recorded.

- [ ] **MOTION-02. Measure current right-panel motion** (`source_supported`; exercise). Capture open/close from compact and wide remembered widths, with browser inside.  
  Observe: When configured>0 and effective, content stays pinned right while clip/width uncovers chat; titlebar controls do not slide under native controls.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/preview/PreviewPanelShell.tsx:38` `PreviewPanelShell`; `apps/web/src/panelAnimations.ts:41` `usePanelPresence`.  
  State. Right panel open at recorded width. Event. Close and reopen with frames. Result to verify. Clip and width progression recorded when enabled.

- [ ] **MOTION-03. Reverse transitions before completion** (`source_supported`; exercise). If duration>0 already enabled, rapidly close→open→close each sidebar/panel; click target after settling.  
  Observe: No stuck width, orphan overlay, retained pointer interception or stale content. If0ms, record not_applicable for animated reversal rather than pass.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/panelAnimations.ts:41` `usePanelPresence`; `apps/web/src/components/ui/sidebar.tsx:281` `Sidebar`.  
  State. Animation enabled and transition running. Event. Reverse transition before completion. Result to verify. Final requested state reached without stale input layer.

- [ ] **MOTION-04. Route switch during closing** (`source_supported`; exercise). With nonzero motion, close panel and immediately navigate another audit thread; reopen there.  
  Observe: Old thread content does not remain visible; route navigation suppresses transition until new route paints.  
  Gate: Nonzero effective duration; otherwise record gate.  
  Source. `apps/web/src/panelAnimations.ts:41` `usePanelPresence`; `apps/web/src/panelAnimations.ts:11` `usePanelNavigationSuppression`.  
  State. Old thread panel closing. Event. Switch to next thread. Result to verify. Retained old content excluded by scope.

- [ ] **MOTION-05. Width/maximize changes versus open/close** (`source_supported`; exercise). Record direct drag, a saved-width thread switch, maximize and restore separately from toggle.  
  Observe: T3 intentionally suppresses inline width transitions for ordinary resize/maximize changes; do not label all snaps as failed open/close animation.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/components/preview/PreviewPanelShell.tsx:38` `PreviewPanelShell`; `apps/web/src/hooks/useResizableWidth.ts:31` `useResizableWidth`.  
  State. Panel open at stored width. Event. Resize or maximize or switch width. Result to verify. Ordinary width updates remain direct; open/close is separate.

- [ ] **MOTION-06. Focus during animated collapse** (`source_supported`; exercise). Focus page/address/find; close panel during typing readiness; Tab/type after closure.  
  Observe: Before-paint inert state prevents hidden target focus; focus returns outside panel without transcript scroll jump.  
  Gate: Use safe disposable text.  
  Source. `apps/web/src/components/preview/PreviewPanelShell.tsx:38` `PreviewPanelShell`; `apps/web/src/lib/workspaceFocus.ts:100` `createWorkspaceFocusController`.  
  State. Input inside closing panel. Event. Collapse then type or Tab. Result to verify. Inert hidden panel cannot receive new focus.

- [ ] **MOTION-07. First paint oversized flash** (`source_supported`; exercise). Open panel at narrow available width with wide saved value; record first frames and settled width.  
  Observe: Host measures actual shared row before paint and clamps; no momentary giant panel or repeated layout flash.  
  Gate: Always inspect; execute when enabled.  
  Source. `apps/web/src/hooks/usePreviewPanelInlineSize.ts:20` `usePreviewPanelInlineSize`.  
  State. Saved panel width exceeds available row. Event. Open panel at smaller available row. Result to verify. First paint already clamped to row constraint.

- [ ] **MOTION-08. Motion settings preview and restoration design** (`source_supported`; inspect). Inspect Panel animations setting preview and current value. If parent elects a controlled settings test, record original, test supported25ms increments0-400, then restore exactly; this child makes no changes.  
  Observe: Preview communicates speed; actual panels follow effective duration; test restores initial settings.  
  Gate: Parent-controlled optional setting change only.  
  Source. `apps/web/src/components/settings/AppearanceSettingsPanel.tsx:87` `AppearanceSettingsPanel`.  
  State. Original panel duration known. Event. Inspect preview; optional parent-controlled adjustment. Result to verify. Actual duration tested then original restored if changed.

- [ ] **MOTION-09. Reduced motion observation** (`source_supported`; inspect). Record whether reduced motion is already active/visible in environment; inspect motion with current state.  
  Observe: Active reduced motion suppresses panel transitions regardless of configured ms. Do not change OS preference just to manufacture coverage.  
  Gate: Observable OS/app state; otherwise unknown.  
  Source. `apps/web/src/panelAnimations.ts:31` `usePanelAnimationSettings`.  
  State. Current reduced-motion state known or unknown. Event. Observe effective transition gating. Result to verify. Reduced motion suppresses transition or gate remains unverified.

- [ ] **MOTION-10. Cross-surface state motion** (`quality_probe`; exercise). Observe tooltip/menu appearances, sidebar shelf/folder chevrons, tab reorder, loading progress, recording indicator and browser floating handoff.  
  Observe: Record each actual transition and disruption; source does not promise every surface animates. Look for snap/overlap/focus problems rather than treating animation presence as quality by itself.  
  Gate: Only currently reachable states.  
  Source. `apps/web/src/components/Sidebar.motion.ts:15` `createSidebarListMotion`; `apps/web/src/components/Sidebar.shiftGuard.ts:37` `createSidebarShiftGuard`; `apps/web/src/components/preview/PreviewChromeRow.tsx:53` `PreviewChromeRow`.  
  State. Hovered or loading or floating state active. Event. Trigger supported state changes. Result to verify. Actual frame sequence and any disruption recorded.

## Keep the coverage boundary explicit

This checklist contains 107 independently recordable shell/browser items. It covers all source-discovered menu branches and principal shell interaction states in this assignment; it is not proof that every installed feature is available. Detailed content of PR review, editor/file rendering, terminal commands, settings, provider administration, and model/transcript behavior belongs to the corresponding live audit tracks after their entry paths here are exercised. A blocked hardware/network/account state must remain explicit.

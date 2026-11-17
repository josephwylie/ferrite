# Shell surface reconciliation

Exact installed T3: `1264c206aee0dce805d6279809118bc091f29d2f`. Ferrite: `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. This is a read-only source map of **placement, coexistence, state ownership and lifetime**, with 25 grouped surfaces and a conditional-child ledger. It adds no runtime passes or new defect count.

The concrete correction is that **provider children use the main conversation, Thread details is an independent anchored card, and the terminal drawer is separate from the right-workspace Terminal tab**. Ferrite has much of the underlying content; its current ordinary routes/containers do not establish those composition contracts. Sidebar snapping and excessive opening widths remain explicit acceptance problems.

The later T3 checkout `74573fc29b901bafbf8f1dbb2eb29606da1fa448` is a separate reference. All T3 source anchors below are the exact installed commit. Parent runtime observations are attributed to [Thread details correction](thread-details-card-correction.md) and [subagent placement correction](subagent-placement-correction.md); this worker did not operate the app.

## Practical corrections

- **SH05 — confirmed composition correction:** Ordinary provider-child entry currently opens Ferrite's Agent panel; the observed/reference contract is a full main conversation. Repair guidance must use the full Subject/Pane path rather than certify compact child drilldown.
- **SH07 — confirmed composition correction:** Overview is not an equivalent Thread details card. The details surface needs independent visibility, compact placement, narrow fallback and coexistence with an active tool.
- **SH09 — known placement difference:** Command-J into the shared right panel cannot demonstrate the bottom drawer alongside Browser/Files. Distinguish the product decision and coexistence contract before declaring terminal parity.
- **SH02 — existing user report and source path:** Ferrite sidebar snapping remains a real issue in its boolean/constant-width path. T3’s configurable motion gate must not turn this into a false negative.
- **SH08 — existing user report needs measurement:** Opening a massive workspace needs first-open/saved-width/clamping evidence for each tool. Existing functionality and per-tool floors do not prove a satisfactory opening size.
- **SH10-SH25 — coverage boundary:** Transient/global/auxiliary surfaces are enumerated to prevent composition omissions. Unexercised source-supported hosts are not added as confirmed Ferrite defects or automatic feature priorities.

## Surface map

Each entry distinguishes the visible surface from similarly named content elsewhere. Anchors identify the inspected render/ownership boundary; they do not certify all deeper workflows.

### SH01. Application ancestors, route gates and recovery

Own application-wide lifetime and choose between authenticated app, welcome/connect flows and recovery. **Placement:** Root service hosts; auth/onboarding and errors occupy the main route, outside the ordinary thread shell.

- **Trigger:** Startup; authentication and local/hosted mode; first-run decision; route loading/error/not-found.
- **Coexistence:** AppRoot keeps ElectronBrowserHost and QuitHoldOverlay beside RouterProvider. __root keeps ToastProvider/AnchoredToastProvider above all route branches. The ordinary AppSidebarLayout is only in the application/welcome shell; ThemeEditorHost is a sibling outside that gate.
- **Responsive fallback:** ManagedAuthShell chooses Electron or browser implementation. Root standalone pages and FirstRunGate recovery replace the main shell; no claim that every auth screen is a modal.
- **Visibility and persistence:** Root route state and first-run decision own visibility; global service lifetime is separate from a selected thread. FirstRunGate can render children, recovery, or an interim blank shell.
- **Motion, focus and lifetime:** AppRoot browser host survives router changes. Recovery has Retry/reload actions; root error provides router retry, reload and copy. No runtime recovery exercise in this work.

**Installed source:** [AppRoot (apps/web/src/AppRoot.tsx:8–16)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/AppRoot.tsx#L8-L16); [ManagedAuthShell selection (apps/web/src/main.tsx:27–51)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/main.tsx#L27-L51); [RootRouteBranch (apps/web/src/routes/__root.tsx:130–230)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/__root.tsx#L130-L230); [RootRouteErrorView (apps/web/src/routes/__root.tsx:234–262)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/__root.tsx#L234-L262); [FirstRunGate / Recovery (apps/web/src/components/onboarding/FirstRunGate.tsx:170–218)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/onboarding/FirstRunGate.tsx#L170-L218).

**Ferrite:** Native shell has a different route/lifetime model; no auth-flow equivalence established. Root dialog and notification layers are infrastructure, not proof of corresponding onboarding workflows. [CockpitView::render_cockpit root children (crates/ferrite-cockpit/src/cockpit.rs:10618–10673)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:10618).

**Earlier audit:** providers-settings.md covers connection capabilities; earlier shell inventory did not enumerate these root branches. **Remaining coverage:** Exact root branches are now enumerated; auth, pairing, welcome and recovery workflows still require their owner’s runtime/source audit.

**Acceptance:** Navigation must preserve unrelated browser/workspace lifetime; a recoverable connection failure must retain a usable retry surface rather than strand the app.

### SH02. Left navigation frame and collapse control

Allocate navigation space and allow collapse, width resize and reopening without stealing input. **Placement:** Desktop left off-canvas navigation plus fixed toggle/rail; mobile left Sheet.

- **Trigger:** Mouse toggle, Cmd+B subject to editor/keybinding guards, menu action; edge drag; rail double-click resets width.
- **Coexistence:** Sits beside the routed main surface; BackgroundThreadAnnouncer and history feedback remain siblings outside the inert collapsed content.
- **Responsive fallback:** Mobile <=767 px uses Sheet. Desktop default 256 px, minimum max(208 px, measured brand width); maximum preserves 640 px main width where possible. Drag growth is rejected when the wrapper would leave less than 640 px.
- **Visibility and persistence:** Desktop open state stored as chat_thread_sidebar_open; width stored separately as chat_thread_sidebar_width. Mobile open state is separate. Reset removes the width override.
- **Motion, focus and lifetime:** Width/left/right transitions only when panel animation settings are active; collapse makes content inert. Rail drag disables transition. Cmd+B capture avoids taking a rich-text Bold selection. Panel animations can be zero and are suppressed around route changes.

**Installed source:** [SidebarControl (apps/web/src/components/AppSidebarLayout.tsx:329–405)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/AppSidebarLayout.tsx#L329-L405); [AppSidebarLayout (apps/web/src/components/AppSidebarLayout.tsx:410–496)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/AppSidebarLayout.tsx#L410-L496); [width resolution (apps/web/src/components/threadSidebarWidth.ts:1–34)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/threadSidebarWidth.ts#L1-L34); [SidebarProvider state (apps/web/src/components/ui/sidebar.tsx:143–178)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/sidebar.tsx#L143-L178); [Sidebar mobile/desktop branches (apps/web/src/components/ui/sidebar.tsx:313–403)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/sidebar.tsx#L313-L403); [usePanelAnimationSettings (apps/web/src/panelAnimations.ts:11–38)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/panelAnimations.ts#L11-L38).

**Ferrite:** Existing navigation capability with a concrete motion mismatch. Ferrite switches width between zero and COLUMN_WIDTH and boolean collapse state. No interpolated collapse width is established in this path; the user’s snapping report is not refuted. [nav_width / nav_folded / auto_fold_nav (crates/ferrite-cockpit/src/cockpit.rs:2829–2855)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:2829); [toggle_nav_now / set_nav_collapsed (crates/ferrite-cockpit/src/cockpit.rs:6170–6198)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:6170); [nav / nav_edge_tab (crates/ferrite-cockpit/src/cockpit.rs:12845–12880)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:12845).

**Earlier audit:** motion.md, input-focus.md and t3-live-shell-checklist.md covered collapse; keep their known issue rather than recounting it as newly discovered. **Remaining coverage:** Intermediate-frame proof, interrupted reversal, natural drag/click targets during transition and reduced-motion behavior remain required.

**Acceptance:** Record before/intermediate/after opening and closing, including rapid reversal. The main column and controls must move coherently, retain focus, and settle without a sudden replacement or click-through.

### SH03. Navigation contents, shelves and footer doors

Expose thread destinations and workspace/account commands within the navigation frame. **Placement:** Scrollable sidebar content with header/project scope, thread/draft/group rows, shelf sections and footer.

- **Trigger:** Search, project scope, row selection, shelf expansion, drag/drop, Show more/Retry; footer commands.
- **Coexistence:** Navigation groups and placeholders share one scroll lane; the footer is a separate sibling. Search results replace the normal sortable list instead of becoming an extra workspace tab.
- **Responsive fallback:** Inherits sidebar desktop/mobile frame; list is scrollable. Empty states distinguish no projects, no threads and scoped project emptiness.
- **Visibility and persistence:** Shelf/filter/order state belongs to sidebar stores; this report verifies rendered branches, not every persistence backend. Older-settled loading/failure has its own button state.
- **Motion, focus and lifetime:** Interactive rows and menus are inside the navigation area. Empty/drop placeholders and retry buttons are separate rendered states, not thread surfaces.

**Installed source:** [Sidebar root/header (apps/web/src/components/Sidebar.tsx:2967–2985)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/Sidebar.tsx#L2967-L2985); [search/list branch (apps/web/src/components/Sidebar.tsx:3205–3245)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/Sidebar.tsx#L3205-L3245); [draft/project/drop rows (apps/web/src/components/Sidebar.tsx:3438–3499)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/Sidebar.tsx#L3438-L3499); [shelves / show more / empty / footer (apps/web/src/components/Sidebar.tsx:3503–3653)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/Sidebar.tsx#L3503-L3653).

**Ferrite:** Existing navigation and footer; labels do not establish identical destinations. Ferrite footer PR opens the shown thread’s PR only when one is present; Usage is the shown provider’s account-limit popover. Neither label proves a global PR or aggregate Usage destination. [nav_state (crates/ferrite-cockpit/src/cockpit.rs:8807–8840)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:8807); [nav_foot (crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:314–350)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:314).

**Earlier audit:** projects-navigation.md covered navigation; PR and Usage destination scope was already a substantive mismatch. **Remaining coverage:** Global destinations must be checked separately from thread-level content. Search/drag/empty/error branch walkthrough is not certified by visiting one populated sidebar.

**Acceptance:** Every apparent navigation link must reach its named destination. Show distinct disabled/empty/error feedback and retain selection/scroll after return.

### SH04. Main route, draft and background conversation

Render the selected full conversation and retain it safely behind route modals. **Placement:** Main SidebarInset; Settings/Usage overlay a retained inert thread.

- **Trigger:** Thread route navigation; draft selection/promotion; hidden-thread redirect; opening and closing an app modal.
- **Coexistence:** _chat renders the retained ThreadRouteView plus modal Outlet when Settings/Usage opens. A valid draft promotion preserves the ChatView slot/key. Ordinary thread navigation uses the same unkeyed ChatView shell with explicit ownership updates.
- **Responsive fallback:** Main inset owns viewport height and overflow. Draft forces expanded mobile composer. No selected thread has separate empty/new-project/draft-landing branches.
- **Visibility and persistence:** Router target and retained backgroundTarget govern visibility; hidden/off-shell routes may redirect or wait. A missing active thread returns NoActiveThreadState rather than rendering ordinary chat children.
- **Motion, focus and lifetime:** Background conversation is inert and does not perform foreground redirect. Local per-thread state must be changed by explicit scope ownership, not accidental component remount assumptions.

**Installed source:** [ChatLayout (apps/web/src/routes/_chat.tsx:18–44)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.tsx#L18-L44); [route lifetime contract (apps/web/src/components/ThreadRouteView.tsx:34–52)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadRouteView.tsx#L34-L52); [draft/server/render branches (apps/web/src/components/ThreadRouteView.tsx:159–234)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadRouteView.tsx#L159-L234); [IndexDraftLanding (apps/web/src/routes/_chat.index.tsx:25–90)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.index.tsx#L25-L90); [no active thread (apps/web/src/components/ChatView.tsx:3176–3183)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3176-L3183).

**Ferrite:** Full Pane and board composition exist; route model differs. Ferrite supports selected Subject in a full Pane; that does not establish that ordinary child navigation uses it. [board grid layer (crates/ferrite-cockpit/src/cockpit.rs:10618–10640)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:10618); [PaneView::select_subject (crates/ferrite-pane/src/pane.rs:403–428)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-pane/src/pane.rs:403); [render_pane (crates/ferrite-pane/src/pane.rs:881–939)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-pane/src/pane.rs:881).

**Earlier audit:** projects-navigation.md and live conclusions covered main routes; earlier recommendations failed to preserve their significance for provider children. **Remaining coverage:** Draft promotion, modal return, thread switch and selected-subject viewport restoration must be tested as a sequence.

**Acceptance:** Open another thread, Settings, and return: preserve the correct transcript/scroll, make only the foreground surface interactive, and avoid accidental background selection.

### SH05. Provider child as full main conversation

Inspect a provider-created subagent using the primary transcript layout with parent navigation and status. **Placement:** Main ChatView route; ProviderSubagentBar replaces the editable composer.

- **Trigger:** Child relationship/tool-output navigation follows the child route; parent breadcrumb returns to the owner. Side-chat targets explicitly take a different tab-opening branch.
- **Coexistence:** Owns the primary conversation lane. The workspace and Thread details remain separate surfaces. This is not the embedded SideChat right-panel surface.
- **Responsive fallback:** Same main canvas/responsive header as a normal conversation. Child status bar condenses labels at narrow widths.
- **Visibility and persistence:** Thread route and provider-child classification select the surface. Child main route is not an additional top-level thread row requirement.
- **Motion, focus and lifetime:** Lifecycle checks stale/unavailable targets; side-chat target branches before ordinary child navigate. Child status/footer appears without implying it accepts user input.

**Installed source:** [onNavigateToRelatedThread (apps/web/src/components/chat/useThreadLifecycle.tsx:480–497)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useThreadLifecycle.tsx#L480-L497); [showProviderSubagentBar / composerMounted (apps/web/src/components/ChatView.tsx:1713–1714)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L1713-L1714); [ProviderSubagentBar branch (apps/web/src/components/ChatView.tsx:3535–3567)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3535-L3567); [ProviderSubagentBar (apps/web/src/components/chat/ProviderSubagentBar.tsx:14–79)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14-L79); [parent breadcrumb (apps/web/src/components/chat/ChatHeader.tsx:287–319)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatHeader.tsx#L287-L319).

**Ferrite:** Confirmed composition mismatch; capability exists but ordinary entry uses the wrong-sized surface for this contract. Ferrite ordinary child row calls open_agent_drill and upserts SurfaceId::Agent. Existing full Pane subject support is a feasible basis; the compact drill is not equivalent to full main navigation. [open_agent_from / open_agent_drill (crates/ferrite-cockpit/src/cockpit/agent_pane.rs:69–110)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/agent_pane.rs:69); [Agent panel body (crates/ferrite-cockpit/src/cockpit/panel.rs:691–693)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:691); [select_subject (crates/ferrite-pane/src/pane.rs:403–428)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-pane/src/pane.rs:403).

**Earlier audit:** subagent-placement-correction.md records original source/live observation and the repair recommendation’s mistaken compact-panel assumption. **Remaining coverage:** Full child transcript, parent return, child status, nested child navigation, unavailable child and scroll preservation remain acceptance dimensions. Do not relabel the compact drill as parity.

**Acceptance:** Click a provider child from the transcript: it should occupy the main conversation lane, retain full transcript behaviors, visibly show parent/status, and return to the parent’s previous position. Keep side chat distinct.

### SH06. Conversation header and independent layout controls

Name/navigate the conversation and operate details, terminal and workspace independently. **Placement:** Top breadcrumb/title/menu, no-drag controls; workspace-owned titlebar controls when docked.

- **Trigger:** Project/parent click; title menu/context menu; title double-click rename; Enter commits/Escape cancels; separate details, terminal drawer, right-panel and maximize controls.
- **Coexistence:** Details control may relocate when workspace owns the header; workspace maximize can make chat width zero/inert. Controls are not four labels for one shared surface.
- **Responsive fallback:** Measured breadcrumb truncation; desktop traffic-light clearance; controls suppressed while mobile sidebar sheet is mounted. Separate root/inline/sheet control ownership prevents duplicate hit areas.
- **Visibility and persistence:** Title edit local state; panel/terminal/details visibility in separate stores. Maximize is distinct from close/open.
- **Motion, focus and lifetime:** Explicit no-drag regions and focus handling. Opening details requests details workspace focus; closing while details owns focus returns content focus. Header control entry motion is conditional.

**Installed source:** [title/menu/rename (apps/web/src/components/chat/ChatHeader.tsx:199–245)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatHeader.tsx#L199-L245); [breadcrumb assembly (apps/web/src/components/chat/ChatHeader.tsx:247–397)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatHeader.tsx#L247-L397); [independent toggles/maximize (apps/web/src/components/chat/PanelLayoutControls.tsx:57–170)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/PanelLayoutControls.tsx#L57-L170); [header control placement (apps/web/src/components/chat/PanelLayoutControls.tsx:174–230)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/PanelLayoutControls.tsx#L174-L230); [header ownership (apps/web/src/components/ChatView.tsx:3231–3332)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3231-L3332).

**Ferrite:** Ferrite intentionally consolidates tools behind one Panel toggle; independent T3 placement is not implemented by labels in that menu. Parent breadcrumb in this header path is described for side chats. Provider-child main-route behavior needs its own verified wiring. [documented single-toggle design (crates/ferrite-cockpit/src/cockpit/view_head.rs:1–22)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/view_head.rs:1); [title_tools_element / solo_head (crates/ferrite-cockpit/src/cockpit/view_head.rs:226–280)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/view_head.rs:226).

**Earlier audit:** input-focus.md covered hit testing; thread-details-card-correction.md corrects the missed independent surface. **Remaining coverage:** Click/drag boundary around titlebar, narrowed breadcrumb, control relocation and maximize focus return remain unproven as a combined journey.

**Acceptance:** Each visible control must operate its own named surface. Opening context details must not automatically take a large workspace lane or replace the current tool; titlebar gaps must still drag while controls click.

### SH07. Thread details anchored card and popover

Expose workspace/git/scripts/relationships and optional metrics beside the transcript without occupying the tool workspace. **Placement:** Independent upper-right canvas card; anchored header popover when inline placement cannot fit. It is not draggable and not a workspace tab.

- **Trigger:** Details toggle or Cmd+Shift+B; right-click card customization; section actions. Responsive placement switches presentation.
- **Coexistence:** May coexist with the right workspace, Find, composer and floating preview. Layout accounts for preview overlap and Find inset. Inline and popover visibility are separate per-thread flags.
- **Responsive fallback:** 280 px width, 12 px gap; inline placement requires chat-lane clearance and at least 160 px height. Full/compact/essential densities prune content; otherwise header popover with collision shift.
- **Visibility and persistence:** Default inline open, popover closed. Only closed-inline exceptions are persisted; popover open is reset. Item choices stored in client settings; Usage and Context are opt-in.
- **Motion, focus and lifetime:** Inline card has only a conditional 150 ms top-inset transition here; no general entry/exit animation is proven. Popover remains in tree for clean exit; opening requests details focus, Find interaction does not auto-dismiss; closing returns content focus where appropriate.

**Installed source:** [mode / visibility / density (apps/web/src/components/chat/ThreadDetailsCard.tsx:38–105)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsCard.tsx#L38-L105); [card / inline / popover (apps/web/src/components/chat/ThreadDetailsCard.tsx:115–196)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsCard.tsx#L115-L196); [layout thresholds (apps/web/src/components/chat/threadDetailsCardLayout.ts:4–45)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/threadDetailsCardLayout.ts#L4-L45); [conditional details sections (apps/web/src/components/chat/ThreadDetailsPanel.tsx:151–269)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsPanel.tsx#L151-L269); [item defaults/persistence (apps/web/src/components/chat/threadDetailsItems.ts:4–53)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/threadDetailsItems.ts#L4-L53); [visibility defaults (apps/web/src/rightPanelStore.ts:211–214)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L211-L214); [persisted visibility (apps/web/src/rightPanelStore.ts:1188–1210)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L1188-L1210); [focus transition (apps/web/src/components/chat/useRightPanelActions.ts:222–235)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useRightPanelActions.ts#L222-L235).

**Ferrite:** Confirmed composition mismatch; content overlap does not make Overview equivalent. Ferrite Overview consumes the shared docked panel and cannot be counted as the independent anchored card. Existing Plan/context popovers are different purposes and do not substitute for it. [open_panel opens Overview (crates/ferrite-cockpit/src/cockpit/panel.rs:461–463)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:461); [docked panel_element (crates/ferrite-cockpit/src/cockpit/panel.rs:550–626)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:550); [single active surface body (crates/ferrite-cockpit/src/cockpit/panel.rs:657–693)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:657); [panel_sections (crates/ferrite-cockpit/src/cockpit/panel/sections.rs:25–74)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel/sections.rs:25).

**Earlier audit:** thread-details-card-correction.md corrects the earlier toggle-only inventory and records parent native observations of independent toggle, workspace coexistence, Escape return and inline restoration. **Remaining coverage:** Customize/density, narrow windows, every tool combination, Find/preview clearance, live values and motion are not fully verified. Native correction captures were viewed in conversation, not exported as report artifacts.

**Acceptance:** Open/close details while Browser or Files remains selected. Details should use a compact measured card or anchored fallback; preserve tool, transcript width and keyboard focus. Test Escape, resize, Find, floating preview and thread switching independently.

### SH08. Tool workspace: docked, sheet and maximized

Host one selected tool surface among retained per-thread tabs. **Placement:** Resizable right dock; responsive right Sheet; maximized workspace; empty launcher.

- **Trigger:** Right-panel toggle/Cmd+Option+B, plus/tool command, tab selection/close/drag/rename, maximize/restore.
- **Coexistence:** Separate sibling of chat column and bottom terminal drawer; maximized workspace hides/inerts chat. Details can remain independently accessible depending on presentation.
- **Responsive fallback:** Sheet at <=980 px unless maximized. Inline defaults 540 px, min 360 px, max 70% viewport constrained to retain 360 px sibling. Sheet width min(42vw,28rem), min320/max448; <=760 uses min(88vw,24rem), min0.
- **Visibility and persistence:** Per-thread surfaces/active/open/maximized state persisted; panel width key is thread-scoped. Closing uses presence retention tied to scope, preventing a closing panel from carrying another thread’s content.
- **Motion, focus and lifetime:** Inline width transition conditional on motion; close returns focus before subtree becomes inert. Resize handle only inline/nonmaximized. Keep-mounted sheet supports exit; browser/tool lifetime must not be equated to selected tab visibility.

**Installed source:** [workspace state/presentation (apps/web/src/components/ChatView.tsx:1029–1086)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L1029-L1086); [workspace assembly (apps/web/src/components/ChatView.tsx:3931–3993)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3931-L3993); [inline vs sheet (apps/web/src/components/chat/ChatRightPanel.tsx:19–49)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatRightPanel.tsx#L19-L49); [size defaults (apps/web/src/hooks/usePreviewPanelInlineSize.ts:13–39)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/usePreviewPanelInlineSize.ts#L13-L39); [width cap (apps/web/src/hooks/usePreviewPanelInlineSize.ts:68–73)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/usePreviewPanelInlineSize.ts#L68-L73); [responsive sheet (apps/web/src/components/RightPanelSheet.tsx:12–28)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/RightPanelSheet.tsx#L12-L28); [focus/transition/resize (apps/web/src/components/preview/PreviewPanelShell.tsx:132–206)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L132-L206); [surface cases (apps/web/src/components/chat/RightPanelSurfaceContent.tsx:86–233)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L86-L233); [persisted surface state (apps/web/src/rightPanelStore.ts:1188–1204)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L1188-L1204).

**Ferrite:** Existing tool panel, with different size and composition rules; not missing wholesale. Ferrite Browser/File/PR floor uses 42% viewport clamped420–820; Files/Changes uses 60% remaining width with420 minimum. These are not the T3 540 default. Source sizing can explain large openings; a screenshot measurement is needed per tool. [floor_w / most_w / panel_w (crates/ferrite-cockpit/src/cockpit/panel.rs:144–180)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:144); [panel_element (crates/ferrite-cockpit/src/cockpit/panel.rs:550–626)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:550); [surface body switch (crates/ferrite-cockpit/src/cockpit/panel.rs:657–693)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:657); [browser/file/PR width (crates/ferrite-ui/src/theme.rs:1069–1071)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-ui/src/theme.rs:1069); [files width / transcript minimum (crates/ferrite-ui/src/theme.rs:2027–2029)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-ui/src/theme.rs:2027).

**Earlier audit:** panel-tools.md and t3-surface-contracts.md cover capabilities/sizing; user’s 'massive thing every time' is a concrete opening/restore journey, not a feature request. **Remaining coverage:** Measure first open versus saved reopen for each tool, narrow-window clamping, active tool after details toggles, focus ownership, click hit regions and motion reversal.

**Acceptance:** Record viewport/nav/transcript/panel rectangles on first open, resize, close and reopen. The chosen tool must stay active, preserve a useful reading lane and restore an intentional width without obscuring natural clicks.

### SH09. Bottom terminal drawer versus workspace Terminal

Run a shell below the conversation while another tool remains visible. **Placement:** Bottom drawer inside chat column; separately, Terminal may be a right-workspace tab using shared terminal engine.

- **Trigger:** Drawer control/Cmd+J; workspace plus Terminal is a different placement. Open/close/split/terminal-session actions are not interchangeable.
- **Coexistence:** Bottom drawer coexists with the right workspace. Maximized workspace hides the active drawer. Recently opened inactive-thread terminal views can remain mounted for continuity.
- **Responsive fallback:** Drawer uses height/constraints and grid track expansion; panel-mode terminal takes the workspace dimensions.
- **Visibility and persistence:** Terminal UI store persists open/height/session UI; drawer rendering retains recently used thread terminals and chooses active scope. Process/session lifetime is separate from visibility.
- **Motion, focus and lifetime:** Drawer grid animates 0fr/1fr only with active panel animation. Closing can retain presence for exit; inactive drawers are hidden. Panel terminal is separate mode of PersistentThreadTerminals.

**Installed source:** [retained per-thread drawers (apps/web/src/components/chat/ChatTerminalDrawers.tsx:36–106)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatTerminalDrawers.tsx#L36-L106); [drawer layout (apps/web/src/components/PersistentThreadTerminals.tsx:237–280)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/PersistentThreadTerminals.tsx#L237-L280); [workspace panel mode (apps/web/src/components/PersistentThreadTerminals.tsx:380–414)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/PersistentThreadTerminals.tsx#L380-L414); [persist setup (apps/web/src/terminalUiStateStore.ts:583–594)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/terminalUiStateStore.ts#L583-L594); [storage version (apps/web/src/terminalUiStateStore.ts:769–776)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/terminalUiStateStore.ts#L769-L776); [drawer sibling (apps/web/src/components/ChatView.tsx:3915–3929)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3915-L3929).

**Ferrite:** Existing working terminal capability, missing this distinct placement/coexistence contract. Ferrite Command-J brings a SurfaceId::Terminal into the shared right panel. It does not reveal a bottom drawer alongside the existing Files/Browser tab. [all tools share Panel (crates/ferrite-cockpit/src/cockpit/tools.rs:1–7)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools.rs:1); [toggle_terminal (crates/ferrite-cockpit/src/cockpit/tools.rs:42–68)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/tools.rs:42); [Terminal body (crates/ferrite-cockpit/src/cockpit/panel.rs:688–690)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:688).

**Earlier audit:** motion.md explicitly noted no separate Ferrite drawer; checklist PANEL-05/WINDOW-08 called for comparison. Earlier repair framing still risked treating the two terminal placements as one capability. **Remaining coverage:** Need coexistence and focus test in addition to already exercised terminal output/split/termination journeys.

**Acceptance:** With Browser active, open the shell without replacing Browser; close/reopen the shell while preserving command output and process state. Distinguish interrupt, close-tab, hide-drawer and terminate.

### SH10. Floating browser/device mini-player

Keep a preview visible within the conversation when it is not the active docked source. **Placement:** Movable/resizable frame in the chat canvas with hover/focus/touch controls.

- **Trigger:** Float preview/device from its workspace flow; drag header; edge/corner resize; close pill.
- **Coexistence:** Independent from details and drawer. Duplicate display of the same active workspace source is suppressed; the frame participates in canvas/details clearance.
- **Responsive fallback:** Resolved within measured canvas; drag/resize constraints account for available space. Eight resize handles, separate header grab area.
- **Visibility and persistence:** Plain per-thread source-keyed store (not persisted in the inspected store); position/size retained while the entry exists, close deletes entry. Source key guards stale writes.
- **Motion, focus and lifetime:** Pointer-capture drag/resize; controls reveal on hover/focus/touch and outside press dismisses pill. Not the same surface as the anchored details card.

**Installed source:** [mini-player eligibility (apps/web/src/components/ChatView.tsx:1075–1086)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L1075-L1086); [preview sync/mini-player render (apps/web/src/components/ChatView.tsx:3812–3824)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3812-L3824); [browser/device branch (apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx:194–211)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L194-L211); [shell state (apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx:416–455)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L416-L455); [drag/resize/control assembly (apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx:460–633)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L460-L633); [per-thread source/frame store (apps/web/src/previewMiniPlayerStore.ts:62–143)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/previewMiniPlayerStore.ts#L62-L143).

**Ferrite:** No independently verified floating browser/device counterpart in the inspected Ferrite shell; closest is docked Browser. This is a bounded source comparison, not a repository-wide claim that no preview capability exists. Browser visual/function details belong to surface-reconciliation-browser. [docked panel (crates/ferrite-cockpit/src/cockpit/panel.rs:550–626)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:550); [Browser surface body (crates/ferrite-cockpit/src/cockpit/panel.rs:685–685)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:685).

**Earlier audit:** Earlier browser investigation covered docked browser extensively; whole floating composition was not established by those findings. **Remaining coverage:** Native floating preview, collision, resize, focus and return-to-dock behavior are not certified by this report.

**Acceptance:** If a floating preview is part of the intended scope, verify independent visibility, source identity and canvas clearance. Do not replace the reported browser styling/interaction repairs with a new float feature.

### SH11. Conversation canvas, composer and approvals

Compose in the conversation while keeping transcript, pending approvals and status understandable. **Placement:** Timeline in main canvas; absolute centered draft-hero or bottom composer overlay; child approvals above composer; provider-child status substitutes for editable composer.

- **Trigger:** Draft versus active thread; composer contents; provider-child classification; pending approvals; queue/handoff/status.
- **Coexistence:** ThreadFindCanvas measures composer/Find/preview/details. The composer overlay leaves pointer events through outside its own content; it does not replace the bottom terminal drawer.
- **Responsive fallback:** Hero centers where height allows, with short-height bottom fallback. Ordinary composer remains bottom positioned with maximum-height constraint; transcript clearances come from canvas measurement.
- **Visibility and persistence:** Draft/server state, approval and queue atoms determine children. Revert/queued-saving states can make composer inert; this is not a new navigation route.
- **Motion, focus and lifetime:** Composer pointer/focus ownership is explicit. Timeline footer conditionally shows handoff, ThreadNow, status, guest pending messages and queued runs. Detailed transcript/editor internals belong to conversation worker.

**Installed source:** [timelineFooter (apps/web/src/components/ChatView.tsx:3091–3124)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3091-L3124); [ThreadFindCanvas/timeline (apps/web/src/components/ChatView.tsx:3336–3380)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3336-L3380); [composer/approval/subagent composition (apps/web/src/components/ChatView.tsx:3480–3567)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3480-L3567); [context strip/hero banners (apps/web/src/components/ChatView.tsx:3733–3810)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3733-L3810); [canvas measurement (apps/web/src/components/chat/ChatCanvas.tsx:32–116)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatCanvas.tsx#L32-L116).

**Ferrite:** Full Pane/composer exists; compact Agent/SideChat and main Pane are not interchangeable. Conversation worker owns citation, attachments, jump-scroll and full transcript controls; this row records their containing surface. [render_pane (crates/ferrite-pane/src/pane.rs:881–939)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-pane/src/pane.rs:881); [compact related conversation bodies (crates/ferrite-cockpit/src/cockpit/panel.rs:691–692)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:691).

**Earlier audit:** Transcript/composer reports existed; subagent-placement-correction.md documents how compact composition lost those affordances. **Remaining coverage:** Hero/ordinary/child/approval transitions with workspace, drawer and narrow height remain a combined layout check.

**Acceptance:** Visible approval and composer controls must remain reachable and natural to click with the sidebar/workspace open; selected child must retain full transcript affordances.

### SH12. Conversation Find bar

Search the conversation without changing destination or losing search context. **Placement:** Absolute upper-right canvas overlay, z40, width bounded to available space.

- **Trigger:** Find shortcut/action opens search; input focus/select; next/previous and close.
- **Coexistence:** Shares canvas with details and preview. Details excludes Find interactions from popover outside-dismiss; ScrollToEndButton is suppressed while Find is visible.
- **Responsive fallback:** Width min(280 px, available canvas width); canvas reserves Find inset for overlapping details.
- **Visibility and persistence:** ThreadFindProvider owns store; open and focus-request state govern bar; close invokes caller’s focus callback.
- **Motion, focus and lifetime:** 150 ms fade with reduced-motion override; focused input selected on open/request. This is distinct from file editor Find or project search.

**Installed source:** [ThreadFindProvider/close (apps/web/src/components/chat/ThreadFindProvider.tsx:10–46)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadFindProvider.tsx#L10-L46); [focus / conditional overlay (apps/web/src/components/chat/ThreadFindBar.tsx:29–65)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadFindBar.tsx#L29-L65); [Find mount (apps/web/src/components/ChatView.tsx:3337–3356)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3337-L3356); [Find dismissal exception (apps/web/src/components/chat/ThreadDetailsCard.tsx:135–146)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsCard.tsx#L135-L146).

**Ferrite:** Find exists; route/focus ownership must be verified per surface. Ferrite focuses the selected pane’s Find path here; do not infer editor/browser Find routing from conversation Find. [open_find (crates/ferrite-cockpit/src/cockpit.rs:6144–6168)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:6144).

**Earlier audit:** input-focus.md and t3-files-terminal-live-source-match.md cover separate Find contexts. **Remaining coverage:** Find/detail collision and return focus; command routing across conversation/editor/browser still need fresh observations.

**Acceptance:** Cmd+F should search the focused surface, leave other panes intact, and return focus on Escape. Search UI must not cover required title controls or reset transcript position unexpectedly.

### SH13. Provider/error banner lane

Report actionable provider status and thread error above the conversation. **Placement:** Absolute top canvas lane, z30, with independent dismiss/refresh paths.

- **Trigger:** Provider status/error atoms and thread error state; dismiss or provider refresh.
- **Coexistence:** Remains in the current conversation; not a replacement project/browser screen. Hero draft can render a detached banner stack.
- **Responsive fallback:** Banners use bounded lane geometry and pointer-events isolation outside interactive content; short-height hero fallback is separate.
- **Visibility and persistence:** Provider and thread error dismissal keys differ; refresh request and payload determine whether the underlying condition cleared.
- **Motion, focus and lifetime:** No universal entrance animation established in inspected lane. A disappeared banner after refresh is an observation, not proof of root cause.

**Installed source:** [ChatBannerLane (apps/web/src/components/chat/ChatBannerLane.tsx:43–61)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatBannerLane.tsx#L43-L61); [banner props (apps/web/src/components/ChatView.tsx:3357–3374)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3357-L3374); [detached hero banner stack (apps/web/src/components/ChatView.tsx:3794–3810)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3794-L3810).

**Ferrite:** Notices/failure surfaces exist, but exact provider routing/refresh equivalence is not established. A global toast is not automatically equivalent to a persistent in-conversation provider status with a refresh action. [toast/dialog/notification layers (crates/ferrite-cockpit/src/cockpit.rs:10669–10673)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:10669); [toast_float (crates/ferrite-cockpit/src/cockpit/palette.rs:1504–1562)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/palette.rs:1504).

**Earlier audit:** t3-files-terminal-live-source-match.md records provider timeout/banner disappearance after refresh, explicitly without a proven backend cause. **Remaining coverage:** Timeout payload, refresh request/outcome, repeated failure and narrow layout remain unproven.

**Acceptance:** A provider failure must show a relevant action and preserve conversation input/context; refresh success/failure must be visible without unrelated navigation.

### SH14. Drag feedback and scroll-to-end affordance

Give local drop feedback and a safe return to the live transcript end. **Placement:** Transient canvas overlay and floating transcript-edge button.

- **Trigger:** Workspace file drag enters/leaves; scroll state enables Jump/scroll-to-end.
- **Coexistence:** Drop overlay is pointer-events-none status feedback. Scroll button is hidden during Find and clears the composer; neither is a modal.
- **Responsive fallback:** Overlay is inset within canvas; scroll button position includes composer clearance.
- **Visibility and persistence:** Derived transient drag/scroll state; no independent persisted surface.
- **Motion, focus and lifetime:** Scroll button pointer-down prevents default to retain focus; content remains mounted. Jump correctness/virtualization is conversation-worker scope.

**Installed source:** [WorkspaceFileDropOverlay / ScrollToEndButton (apps/web/src/components/chat/TimelineOverlays.tsx:5–54)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/TimelineOverlays.tsx#L5-L54); [drop render (apps/web/src/components/ChatView.tsx:3355–3356)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3355-L3356); [scroll affordance condition (apps/web/src/components/ChatView.tsx:3480–3488)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3480-L3488).

**Ferrite:** Existing transcript/drop/jump behavior, with user-reported correctness concerns; not missing-feature evidence. Exact scroll mechanics and defects are delegated to surface-reconciliation-conversation; this row owns placement/focus only. [main pane container (crates/ferrite-pane/src/pane.rs:881–939)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-pane/src/pane.rs:881).

**Earlier audit:** Transcript investigation and user jump-scrolling report already cover behavior; no new defect count here. **Remaining coverage:** Dropped text/file target and focus retention while a panel is present; jump under streaming/large transcript must be replayed.

**Acceptance:** Natural drop/click should land in the visible target, preserve input focus, and scroll predictably without an obscured or stale button.

### SH15. Thread-owned dialogs and media overlay

Keep consequential actions and media tied to the selected conversation. **Placement:** Modal dialogs: device setup, PR preparation, branch restore, revert/edit-from-here, link PR; expanded image/media overlay.

- **Trigger:** Explicit thread action, image click, PR/device setup flow; close/cancel/confirm.
- **Coexistence:** Most hosts mount only for nonembedded ChatView; RevertConfirmHost and ExpandedImageDialogHost also mount for embedded chat. They overlay the existing route.
- **Responsive fallback:** Shared dialog viewport; expanded media uses 92vw/92vh with narrower device rules. Details do not become a dialog merely because some items open dialogs.
- **Visibility and persistence:** ChatOverlayHosts atom values include thread identity and register cleanup on thread change. Expanded media owns/revokes blob URLs. Link PR has its own thread-ref atom and pending close guard; do not assume all hosts have identical lifetime.
- **Motion, focus and lifetime:** Action focus uses shared Dialog behavior; thread-scoped dialogs close on scope change where registered. Media and confirmations use their own close/complete cleanup.

**Installed source:** [thread-scoped hosts (apps/web/src/components/chat/ChatOverlayHosts.tsx:37–237)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatOverlayHosts.tsx#L37-L237); [media dialog (apps/web/src/components/chat/ExpandedImageDialog.tsx:162–174)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ExpandedImageDialog.tsx#L162-L174); [thread ownership/blob lifecycle (apps/web/src/components/chat/ExpandedImageDialog.tsx:298–332)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ExpandedImageDialog.tsx#L298-L332); [link PR atom/host (apps/web/src/components/pullRequest/LinkPullRequestDialog.tsx:27–54)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/LinkPullRequestDialog.tsx#L27-L54); [pending/initial focus (apps/web/src/components/pullRequest/LinkPullRequestDialog.tsx:123–139)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/pullRequest/LinkPullRequestDialog.tsx#L123-L139); [final overlay host children (apps/web/src/components/ChatView.tsx:3995–4003)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3995-L4003).

**Ferrite:** Generic dialogs/media and action flows exist; specific workflow equivalence must be proved individually. A common modal layer is not proof that PR preparation/linking, restore and revert workflows work. [lightbox_element (crates/ferrite-cockpit/src/cockpit/lightbox.rs:60–95)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/lightbox.rs:60); [overlay/dialog layers (crates/ferrite-cockpit/src/cockpit.rs:10657–10673)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:10657).

**Earlier audit:** PR/media reports cover individual capabilities; prior broad inventory did not enumerate per-thread host ownership. **Remaining coverage:** Cancel/confirm, thread switch, stale data, focus restoration and loading close guards for each host.

**Acceptance:** A dialog must retain the originating thread identity, prevent stale confirmation after switching threads, return focus and expose failure without silently closing.

### SH16. Command palette and search modes

Offer global commands and file/content search without becoming a main route. **Placement:** Modal CommandDialog with command/files/content modes over inert app children.

- **Trigger:** Palette command/shortcut; nested mode selection; Escape backs out of a mode before closing where handled.
- **Coexistence:** Wraps AppSidebarLayout children; ThreadContextDragGhost is a sibling overlay. Settings/app-modal return is handled separately from composer focus.
- **Responsive fallback:** Content mode uses a fixed-height layout; command/files modes share modal framing. Detailed result rendering is outside shell scope.
- **Visibility and persistence:** Palette open/mode state owns visibility; local result lifecycle not equated to route navigation.
- **Motion, focus and lifetime:** App subtree becomes inert while open; close restores workspace/composer except during app modal. Escape mode handling differs from dismiss-all.

**Installed source:** [CommandPalette / CommandDialog (apps/web/src/components/CommandPalette.tsx:309–400)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/CommandPalette.tsx#L309-L400).

**Ferrite:** Palette exists, with its own focus and exit motion. Ferrite palette is board-bounded overlay with its own retained exit view. Mode/command coverage is separate. [toggle/open/close palette (crates/ferrite-cockpit/src/cockpit/palette.rs:190–250)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/palette.rs:190); [sheet_floats (crates/ferrite-cockpit/src/cockpit/palette.rs:1415–1499)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/palette.rs:1415).

**Earlier audit:** projects-navigation.md/input-focus.md cover palette commands; live source report covers project search. **Remaining coverage:** Nested Escape, cancellation from another modal, panel-focused commands and stale-result selection.

**Acceptance:** Palette should accept keys immediately, route actions to the intended thread/tool, and restore the same input or pane after cancel.

### SH17. Settings route modal and settings child routes

Configure the app while preserving the underlying working conversation. **Placement:** AppRouteModal with settings sidebar, breadcrumb/header and route Outlet.

- **Trigger:** Settings navigation/shortcut; section selection; Escape/backdrop close navigates to remembered nonmodal page after exit.
- **Coexistence:** Background ThreadRouteView stays mounted inert. Theme editor may be a separate floating sibling. A route does not automatically mean an independent screen design.
- **Responsive fallback:** AppRouteModal is full-screen at small sizes; larger viewport has padding, max80rem width/max60rem height. Setting section scope notices can replace its content.
- **Visibility and persistence:** Location and remembered return href own visibility; settings scope/project availability determine section branch. Beta/Integrations/Storage are redirect aliases; Access/Connections share one component with different view props.
- **Motion, focus and lifetime:** Initial focus stays on dialog popup rather than a field; close blurs field and waits for exit before return. Popup fades/scales over150ms; finalFocus is deliberately false because route restoration owns focus.

**Installed source:** [route modal lifetime/focus/layout (apps/web/src/components/AppRouteModal.tsx:11–82)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/AppRouteModal.tsx#L11-L82); [settings route/scope/body (apps/web/src/routes/_chat.settings.tsx:45–143)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.settings.tsx#L45-L143); [modal route/return href (apps/web/src/components/sidebar/mainAppLocation.ts:6–44)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/sidebar/mainAppLocation.ts#L6-L44); [Secrets feature gate (apps/web/src/routes/_chat.settings.secrets.tsx:6–10)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.settings.secrets.tsx#L6-L10); [Access view (apps/web/src/routes/_chat.settings.access.tsx:4–10)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.settings.access.tsx#L4-L10); [Environments view (apps/web/src/routes/_chat.settings.connections.tsx:4–10)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.settings.connections.tsx#L4-L10); [Labs route (apps/web/src/routes/_chat.settings.labs.tsx:1–4)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.settings.labs.tsx#L1-L4); [Snap Shot route (apps/web/src/routes/_chat.settings.snap-shot.tsx:5–10)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.settings.snap-shot.tsx#L5-L10); [licenses route (apps/web/src/routes/_chat.settings.open-source-licenses.tsx:3–7)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.settings.open-source-licenses.tsx#L3-L7).

**Ferrite:** Settings surface exists; route-modal lifetime, return destination and child inventory are not equivalent by name. This anchor describes the in-cockpit settings surface; it is not a claim that every Ferrite settings launch uses only this path. [settings_element (crates/ferrite-cockpit/src/cockpit.rs:4456–4495)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:4456); [settings layer (crates/ferrite-cockpit/src/cockpit.rs:10664–10664)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:10664).

**Earlier audit:** providers-settings.md and parent live run inspected many sections; '21 inspected' is a runtime count, not the installed route count or proof of all conditionals. **Remaining coverage:** Labs availability, feature-gated Secrets, licenses child page, Snap Shot and aliases must be tracked distinctly; inner panels remain the settings worker’s boundary.

**Acceptance:** Open Settings from a tool, change section and cancel: return to the same work with state/focus intact. Test deep link/disabled-section fallback and small window independently.

### SH18. Usage route modal and subordinate dialogs

Provide aggregate usage/limits analysis as its own destination. **Placement:** AppRouteModal separate from Settings; nested UsageModelDialogHost and price override dialog.

- **Trigger:** Usage navigation; model breakdown/price customization; limits versus overview selection.
- **Coexistence:** Background conversation is preserved like Settings. Usage model dialog is mounted only outside limits mode; price dialog has separate state.
- **Responsive fallback:** Same responsive AppRouteModal; body distinguishes no environment, loading skeleton, inaccessible data, limits and overview.
- **Visibility and persistence:** Route owns surface; data/mode and dialog state own subordinate visibility. No persistence assertion for uninspected inner controls.
- **Motion, focus and lifetime:** Same route close/focus model; loading/empty/error are distinct visible branches, not evidence of a missing Usage page.

**Installed source:** [Usage route (apps/web/src/routes/_chat.usage.tsx:1–8)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.usage.tsx#L1-L8); [UsagePage modal branches (apps/web/src/components/usage/UsagePage.tsx:504–581)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/usage/UsagePage.tsx#L504-L581).

**Ferrite:** Different scope and placement: Ferrite footer Usage is a provider limits popover. Shared account-limit content does not equal the aggregate Usage route. [plan_usage_popover (crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:341–350)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/nav_chrome.rs:341).

**Earlier audit:** Earlier providers/settings inventory includes usage capability gaps; this reconciliation makes destination/subdialog composition explicit. **Remaining coverage:** Data freshness, empty/access-denied states and nested close/return are not covered by opening one usage popover.

**Acceptance:** Keep account-limit quick access distinct from any aggregate Usage destination; navigation labels must describe the reachable surface and data scope.

### SH19. Theme editor floating session

Preview/edit theme while viewing the actual app underneath. **Placement:** Movable/resizable fixed floating panel, z110; minimizable dialog semantics, no modal backdrop in this component.

- **Trigger:** Theme-edit session opened from settings; drag header, resize handle, minimize, close.
- **Coexistence:** Mounted beside welcome/app/first-run branches, so navigation does not discard an active editor session. It can coexist with the live app and route modals.
- **Responsive fallback:** Default width min(26rem, viewport minus2rem), max height min(42rem, viewport minus6rem); local position/size move the panel.
- **Visibility and persistence:** Host uses session store; per-session local minimized/position/size. No geometry persistence is established in this file. Closing restores or applies session theme through host logic.
- **Motion, focus and lifetime:** Lazy/Suspense panel; session key remounts a different session, not every route. This is not the same surface as an Appearance settings form.

**Installed source:** [ThemeEditorHost sibling (apps/web/src/routes/__root.tsx:208–230)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/__root.tsx#L208-L230); [session host (apps/web/src/components/settings/ThemeEditorHost.tsx:25–113)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/settings/ThemeEditorHost.tsx#L25-L113); [local geometry (apps/web/src/components/settings/ThemeEditorPanel.tsx:352–360)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/settings/ThemeEditorPanel.tsx#L352-L360); [floating dialog/header (apps/web/src/components/settings/ThemeEditorPanel.tsx:1113–1140)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/settings/ThemeEditorPanel.tsx#L1113-L1140); [resize handle (apps/web/src/components/settings/ThemeEditorPanel.tsx:1235–1241)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/settings/ThemeEditorPanel.tsx#L1235-L1241).

**Ferrite:** No equivalent independently verified in inspected Ferrite shell; existing settings is only a partial content counterpart. This is source coverage, not a recommendation to build a theme editor before fixing ordinary interactions. [settings_element (crates/ferrite-cockpit/src/cockpit.rs:4456–4495)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:4456).

**Earlier audit:** providers-settings.md noted theme capabilities; standalone floating composition was not a complete audited journey. **Remaining coverage:** No runtime pass for floating/minimizing/resizing/restoring editor; keep it out of confirmed defect count.

**Acceptance:** If theme customization is claimed, verify preview/cancel/apply lifetime against the real app; do not count an Appearance page as the same composition.

### SH20. Global confirmation and custom snooze hosts

Present cross-route confirmations and date/duration scheduling without tying them to a transient menu. **Placement:** Global AlertDialog/confirm host and custom-snooze Dialog with subordinate date picker.

- **Trigger:** Confirm request; custom-snooze request from thread menu; explicit accept/cancel/date selection.
- **Coexistence:** Mounted once in appHosts above the app shell, callable from different surfaces. A mounted host is normally invisible.
- **Responsive fallback:** Shared Dialog/AlertDialog constraints; snooze max-md with calendar popover.
- **Visibility and persistence:** Confirm store status controls dialog; close completion cleans request. New snooze request resolves previous request null; unmount also resolves pending request null.
- **Motion, focus and lifetime:** Confirm focus targets action as configured; cancel/close cleanup is explicit. Snooze’s pending promise lifetime must not survive its owner invisibly.

**Installed source:** [global action hosts (apps/web/src/routes/__root.tsx:189–195)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/__root.tsx#L189-L195); [ConfirmDialogHost (apps/web/src/components/ConfirmDialogHost.tsx:54–99)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ConfirmDialogHost.tsx#L54-L99); [request/host/dialog (apps/web/src/components/CustomSnoozeDialog.tsx:39–88)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/CustomSnoozeDialog.tsx#L39-L88).

**Ferrite:** Generic modal infrastructure exists; exact snooze/confirmation workflows not established by it. Compare particular actions when exercised; no repository-wide absence claim. [Root dialog/notification layers (crates/ferrite-cockpit/src/cockpit.rs:10672–10673)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:10672).

**Earlier audit:** Earlier thread/navigation inventory covered snooze concepts; no complete global host lifetime trace located. **Remaining coverage:** Request replacement, route change, cancel, keyboard focus, nested picker Escape.

**Acceptance:** No invisible pending confirmation or stale thread action may remain after cancel or navigation.

### SH21. Guest sharing and pending guest work

Create/view a share session and expose its state at relevant conversation locations. **Placement:** Global GuestShareDialog; small ChatHeader GuestShareIndicator; guest pending messages in timeline footer; guest route bypasses ordinary shell.

- **Trigger:** Share action opens thread-ref dialog; create/copy/revoke states; guest route entry; pending guest request.
- **Coexistence:** One feature deliberately has multiple placements; indicator/dialog/footer are not substitute surfaces.
- **Responsive fallback:** Dialog uses shared responsive layout; indicator/footer follow header/timeline constraints. Guest route chooses its own Outlet branch.
- **Visibility and persistence:** Global atom stores thread ref; host returns null when absent and keys by environment/thread. Header indicator only for server thread. Pending messages only for relevant active server state.
- **Motion, focus and lifetime:** Dialog lifetime follows share request/thread identity; header and footer remain in their normal surfaces. No runtime sharing operation performed here.

**Installed source:** [guest/auth route branch (apps/web/src/routes/__root.tsx:155–164)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/__root.tsx#L155-L164); [GuestShareDialogHost (apps/web/src/components/chat/GuestShareControls.tsx:48–57)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/GuestShareControls.tsx#L48-L57); [dialog states (apps/web/src/components/chat/GuestShareControls.tsx:105–146)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/GuestShareControls.tsx#L105-L146); [indicator/pending messages (apps/web/src/components/chat/GuestShareControls.tsx:177–220)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/GuestShareControls.tsx#L177-L220); [header indicator (apps/web/src/components/chat/ChatHeader.tsx:396–397)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatHeader.tsx#L396-L397); [guest footer branch (apps/web/src/components/ChatView.tsx:3091–3124)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3091-L3124).

**Ferrite:** No verified like-for-like guest sharing workflow in inspected Ferrite shell. Root overlay capability alone does not establish sharing. [enumerated root composition (crates/ferrite-cockpit/src/cockpit.rs:10618–10673)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:10618).

**Earlier audit:** No full guest sharing composition walkthrough located in the earlier shell audit. **Remaining coverage:** Guest route, owner dialog, indicator, pending approval and revocation remain unexercised.

**Acceptance:** Keep this as bounded coverage debt unless sharing is in required scope; do not inflate existing interaction defects with an untested feature wish.

### SH22. Cross-app and project-machine global dialogs

Configure use in another app and copy/add a project to another machine from multiple entry points. **Placement:** Global UseInAnotherApp wizard and AddProjectToMachine Dialog.

- **Trigger:** Command/menu/settings public opener with environment/project request; completion/cancel.
- **Coexistence:** Root hosts are available regardless current thread. Wizard/request content is not a right-panel tool.
- **Responsive fallback:** Shared dialog framing; add-project dialog max-xl. Environment/cloud availability can replace wizard content.
- **Visibility and persistence:** UseInAnotherApp keeps request alive in atom; add-project request store has session key and closes if the project disappears.
- **Motion, focus and lifetime:** Session key resets dialog state; source owner deletion invalidates the project request. No external app config or project copy performed.

**Installed source:** [request/host/WizardDialog (apps/web/src/components/mcp/UseInAnotherAppDialog.tsx:59–81)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/mcp/UseInAnotherAppDialog.tsx#L59-L81); [request session/ownership (apps/web/src/components/AddProjectToMachineDialog.tsx:62–90)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/AddProjectToMachineDialog.tsx#L62-L90); [Dialog framing (apps/web/src/components/AddProjectToMachineDialog.tsx:212–225)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/AddProjectToMachineDialog.tsx#L212-L225); [root hosts (apps/web/src/routes/__root.tsx:194–195)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/__root.tsx#L194-L195).

**Ferrite:** No exact matching wizard composition verified; Ferrite project editor/dialog mechanism is only a partial infrastructure counterpart. Do not infer connection/project transfer parity from generic machine or project settings. [project editor and global dialog layer (crates/ferrite-cockpit/src/cockpit.rs:10665–10673)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:10665).

**Earlier audit:** providers-settings.md covers machine capability differences; this adds explicit global request ownership. **Remaining coverage:** Availability gates, stale project/environment, cancellation and completion are source-only here.

**Acceptance:** Any offered cross-machine action must name the destination, remain bound to the correct project, and show completion/failure; no stale modal after source deletion.

### SH23. Connection onboarding, relay installer and SSH prompt

Guide account/device connection and answer desktop authentication prompts. **Placement:** Separate root dialogs/wizard; first-run recovery is SH01, not this modal group.

- **Trigger:** Signed-in account transition; request to install relay; desktop bridge password request queue.
- **Coexistence:** Root appHosts siblings, independent of selected thread; only one queued SSH request is shown at a time.
- **Responsive fallback:** Shared dialog constraints; SSH max-md with password input initial focus; connection wizard may be unavailable without cloud config.
- **Visibility and persistence:** Onboarding has account/opt-out state and local storage. Account change closes current request. Relay retains closing view until transition completes. SSH is request-ID/queue scoped.
- **Motion, focus and lifetime:** Relay allows cancel/close in confirming state, not active installing state. SSH request key isolates input and hides normal close control; result returns through bridge. No credential entered by this worker.

**Installed source:** [availability/opt-out (apps/web/src/components/cloud/ConnectOnboardingDialog.tsx:42–55)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/cloud/ConnectOnboardingDialog.tsx#L42-L55); [account request gating (apps/web/src/components/cloud/ConnectOnboardingDialog.tsx:104–136)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/cloud/ConnectOnboardingDialog.tsx#L104-L136); [install modal lifecycle (apps/web/src/components/cloud/RelayClientInstallDialog.tsx:35–63)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/cloud/RelayClientInstallDialog.tsx#L35-L63); [desktop request queue (apps/web/src/components/desktop/SshPasswordPromptDialog.tsx:49–85)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/desktop/SshPasswordPromptDialog.tsx#L49-L85); [prompt focus/frame (apps/web/src/components/desktop/SshPasswordPromptDialog.tsx:156–164)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/desktop/SshPasswordPromptDialog.tsx#L156-L164).

**Ferrite:** Ferrite has a different SSH connection flow; specific T3 dialogs are not established counterparts. Existing providers-settings report describes remote sign-in in terminal rather than stored-password flow; not a new claim of missing connection support. [SSH machine design (crates/ferrite-core/src/machine.rs:1–7)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-core/src/machine.rs:1); [SSH invocation (crates/ferrite-core/src/machine.rs:354–380)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-core/src/machine.rs:354); [generic dialog layer (crates/ferrite-cockpit/src/cockpit.rs:10672–10673)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:10672).

**Earlier audit:** providers-settings.md covers machine/relay capability differences; modal ownership/queue was not previously enumerated. **Remaining coverage:** Install failure/cancel boundaries, account switch, repeated SSH prompts and stale request cleanup are unexercised.

**Acceptance:** Connection failures should stay actionable and preserve work; no blocked invisible prompt or repeated lost-focus credential dialog.

### SH24. Toast layers and background accessibility announcement

Deliver transient global feedback and announce other thread state without navigating away. **Placement:** Global top-right toast viewport and anchored toasts; separate sr-only live region outside collapsed sidebar.

- **Trigger:** Toast creation; background listed/nonarchived other-thread state change.
- **Coexistence:** Toast providers are above all root branches. BackgroundThreadAnnouncer remains outside inert navigation so collapse does not suppress announcement. It is not the Background Work dashboard.
- **Responsive fallback:** Toast viewport constrained near top-right; anchored toasts follow their anchor. Screen-reader announcement has no visible geometry.
- **Visibility and persistence:** Toast store owns lifetime; announcer compares eligible background thread transitions. Neither changes selected route by itself.
- **Motion, focus and lifetime:** Toast transitions specify transform/opacity .5s and height .15s in inspected CSS. Announcer is aria-live polite; no screen-reader runtime pass.

**Installed source:** [persistent toast providers (apps/web/src/routes/__root.tsx:130–139)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/__root.tsx#L130-L139); [toast provider/viewport/motion (apps/web/src/components/ui/toast.tsx:556–614)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/toast.tsx#L556-L614); [anchored provider (apps/web/src/components/ui/toast.tsx:714–744)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ui/toast.tsx#L714-L744); [eligibility / live region (apps/web/src/components/BackgroundThreadAnnouncer.tsx:32–90)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/BackgroundThreadAnnouncer.tsx#L32-L90); [announcer sibling (apps/web/src/components/AppSidebarLayout.tsx:491–494)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/AppSidebarLayout.tsx#L491-L494).

**Ferrite:** Ferrite toasts and Background Work surface exist; these do not establish a background announcement counterpart. A background task status should not silently route the user into Background Work; navigation and notification are different effects. [toast_float (crates/ferrite-cockpit/src/cockpit/palette.rs:1504–1562)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/palette.rs:1504); [background_dialog_element (crates/ferrite-cockpit/src/cockpit/background_work.rs:253–280)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/background_work.rs:253); [Background panel surface (crates/ferrite-cockpit/src/cockpit/panel.rs:672–683)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/panel.rs:672).

**Earlier audit:** subagents-background.md and motion.md cover status/notifications; whole-shell separation clarifies the user’s unwanted background-navigation concern. **Remaining coverage:** Fresh event-to-navigation trace needed for the reported unexpected switch; source-only host placement does not prove its trigger.

**Acceptance:** Completion/error feedback must not change the current conversation/tool unless the user activates an explicit destination; collapsed navigation must not hide essential feedback.

### SH25. History swipe feedback and hold-to-quit hint

Show navigation gesture progress and protected quit-key instruction without changing layout. **Placement:** Desktop edge swipe indicators at z100; route-independent top-center quit hint at z100, pointer-events-none.

- **Trigger:** Electron wheel/trackpad history gesture; desktop quit bridge key-down/up/double-click mode.
- **Coexistence:** Swipe indicator overlays current route; QuitHoldOverlay is above router as AppRoot sibling and may appear on any route.
- **Responsive fallback:** Swipe uses viewport edge; quit hint centers at top22%. No modal or layout reservation.
- **Visibility and persistence:** Swipe gesture local state. Quit module external-store visibility with 1200ms hold hint linger; double-click release hides immediately. Bridge subscription cleaned when last listener unmounts.
- **Motion, focus and lifetime:** Swipe release fades/translates200ms, icon feedback100ms; quit hint is status role and does not capture pointer/focus. Do not infer a hold animation from its name.

**Installed source:** [gesture / indicator (apps/web/src/components/HistorySwipeNavigation.tsx:112–190)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/HistorySwipeNavigation.tsx#L112-L190); [bridge/lifetime (apps/web/src/components/QuitHoldOverlay.tsx:5–46)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/QuitHoldOverlay.tsx#L5-L46); [status overlay (apps/web/src/components/QuitHoldOverlay.tsx:51–68)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/QuitHoldOverlay.tsx#L51-L68); [outside-router placement (apps/web/src/AppRoot.tsx:12–14)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/AppRoot.tsx#L12-L14).

**Ferrite:** History buttons exist; gesture feedback and quit hint equivalence are not verified. Keyboard history support is not proof of trackpad gesture affordance. No recommendation to prioritize quit hint over reported interaction defects. [history arrows placement (crates/ferrite-cockpit/src/cockpit/view_head.rs:105–128)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit/view_head.rs:105); [title/history/nav overlays (crates/ferrite-cockpit/src/cockpit.rs:10647–10650)](/Users/josephwylie/Desktop/Projects/ferrite/crates/ferrite-cockpit/src/cockpit.rs:10647).

**Earlier audit:** motion.md explicitly left history-swipe counterpart unestablished; quit overlay ancestor was omitted from earlier root inventory. **Remaining coverage:** Native gesture, interrupted gesture and protected quit require controlled runtime testing; no pass claimed.

**Acceptance:** History gestures must not trigger while scrolling the intended pane or steal focus; any quit confirmation should be visible without a hidden modal.

## Conditional render ledger

This lists direct conditional children at the selected shell roots and important wrapper boundaries. Nonvisual services are named so they cannot disappear from the ownership map, but they are not counted as extra screens.

### main.tsx > AppRoot

- ManagedAuthShell only when publishable key and cloud configuration; Electron/browser implementation chosen by runtime.
- AppAtomRegistryProvider contains RouterProvider, ElectronBrowserHost, QuitHoldOverlay as siblings. Browser host lifetime is delegated to browser reconciliation, not owned by route layout.

[startup/ManagedAuthShell (apps/web/src/main.tsx:27–51)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/main.tsx#L27-L51); [AppRoot (apps/web/src/AppRoot.tsx:8–16)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/AppRoot.tsx#L8-L16).

### __root RootRouteBranch

- ToastProvider > AnchoredToastProvider wrap route branch.
- Guest path: Outlet. Pair/connect/connect-agent/approve-agent paths: Outlet. Hosted prompt landing: PromptLinkLanding. Unauthenticated: Outlet. Welcome: appHosts + appShell. Normal app: FirstRunGate containing appHosts, EventRouter when primary authentication exists, and appShell.
- appHosts: DesktopAppActivationCoordinator when primary authenticated; DesktopWebLinkCoordinator when authenticated or local environment disabled; RunningThreadKeepAlive only Electron; RelayClientInstallDialog; ConnectOnboardingDialog; SshPasswordPromptDialog; SnapShotCoordinator; ReopenClosedViewShortcut; ConfirmDialogHost; CustomSnoozeDialogHost; GuestShareDialogHost; ProjectCloneToastCoordinator; AddProjectToMachineDialogHost; UseInAnotherAppDialogHost; DevReplayHost only DEV + authentication.
- appShell: CommandPalette > AppSidebarLayout(showSidebar=!welcome) > Outlet.
- ChatGptWelcomeCoordinator plus ThemeEditorHost sit at branch-level; ThemeEditorHost remains outside welcome/app FirstRunGate choice.
- Separate root route error and not-found components are main-page branches, not transient workspace panels. Coordinators/services above are not each counted as a visible UI surface.

[RootRouteBranch (apps/web/src/routes/__root.tsx:130–230)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/__root.tsx#L130-L230).

### AppSidebarLayout

- PanelAnimationSuppressionProvider > SidebarProvider.
- When showSidebar: SidebarBrandWidthProbe (measurement only), left Sidebar > ThreadSidebar + SidebarRail.
- Routed children always occupy sibling slot.
- BackgroundThreadAnnouncer; SidebarControl(showSidebar); HistorySwipeNavigation only Electron.
- Sidebar chooses noncollapsible, mobile Sheet, or desktop offcanvas branch. Collapsed desktop subtree becomes inert; mobile sheet state is separate.

[AppSidebarLayout (apps/web/src/components/AppSidebarLayout.tsx:457–496)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/AppSidebarLayout.tsx#L457-L496).

### ThreadSidebar inner boundary

- SidebarChromeHeader; SidebarContent > SidebarThreadHeader/project scope and SidebarGroup; SidebarChromeFooter.
- Search branch versus normal sortable groups. Normal branch includes thread rows, SidebarDraftBlock, project folder headers, drag boundaries, Active placeholder, Working/Snoozed/Settled section headers, Settled placeholder.
- Older settled Show more/Loading/Retry button when expanded with hidden rows; empty state when no search/folders/drafts/threads, with no-project Add project action.
- Leaf row context menus/project chooser internals are delegated to navigation/workflow inventories; listing this boundary does not certify every menu.

[header (apps/web/src/components/Sidebar.tsx:2967–2985)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/Sidebar.tsx#L2967-L2985); [list branches (apps/web/src/components/Sidebar.tsx:3205–3245)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/Sidebar.tsx#L3205-L3245); [rows/footer (apps/web/src/components/Sidebar.tsx:3438–3653)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/Sidebar.tsx#L3438-L3653).

### _chat > ThreadRouteView

- ThreadLinkPrompt only server target and no app modal.
- Shown target: retained background target during Settings/Usage, otherwise route target. ThreadRouteView if target; blank SidebarInset for app modal without target; otherwise Outlet. Modal Outlet is a sibling when modalOpen.
- ThreadRouteView alternatives: hidden draft -> home redirect; valid draft -> ChatView draft; promoted draft -> ChatView server with preserved handoff key plus route redirect; missing draft -> home redirect; hidden server -> home; off-shell blocked -> waiting/conditional redirect; ready/loading with known shell -> ChatView server; otherwise no view.
- SidebarInset inert while background; redirects withheld while background. Index route has hosted-empty, draft landing/error, no-project hero branches distinct from NoActiveThreadState.

[ChatLayout (apps/web/src/routes/_chat.tsx:18–44)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.tsx#L18-L44); [ThreadRouteView (apps/web/src/components/ThreadRouteView.tsx:159–234)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadRouteView.tsx#L159-L234).

### ChatView outer shell

- No active thread: null if embedded, otherwise NoActiveThreadState.
- DeviceSetupDialogHost if !embedded; root panelLayoutControls if rightPanelControlsAtRoot && !embedded.
- Chat column inert/zero-width when workspace maximized. Header only !embedded: desktop control-hit-test region when applicable, layout controls unless root/sheet owns them, separate details header control when workspace owns titlebar, ChatHeader.
- ThreadFindCanvas: ThreadFind; WorkspaceFileDropOverlay only during drag; ChatBannerLane; conversationArea/WorkspaceMutationIdContext > ThreadTimeline; ScrollToEndButton only showScrollToBottom && !Find.
- Composer overlay: DraftHeroHeadline when hero; ChildThreadApprovals; ComposerSurface shell/host; ProviderSubagentBar for provider child; NativeChildWorkDialog only Antigravity path; ChatComposer when not provider child; context BranchToolbar when showComposerContextStrip; detached banner stack for draft hero when banners exist.
- PreviewSessionSync only eligible active nonembedded server browser context (nonvisual). ThreadPreviewMiniPlayer only shown active nonembedded frame.
- BranchRestoreConfirmHost, ThreadDetailsPanel and PullRequestThreadDialogHost only !embedded.
- ChatTerminalDrawers only !embedded. ChatRightPanel only present retained panel state with thread && !embedded, owning RightPanelSurfaceContent.
- RevertConfirmHost even embedded; LinkPullRequestDialogHost only !embedded; ThreadVisitWatermark nonvisual; ExpandedImageDialogHost even embedded. FileMetadataThreadProvider wraps result, nonvisual.
- ThreadTimeline footer independently gates pending handoff, ThreadNow/status, guest pending messages and queued run controls. Transcript/tool/message leaf renderers are owned by conversation reconciliation.

[no active (apps/web/src/components/ChatView.tsx:3176–3183)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3176-L3183); [outer/header/canvas (apps/web/src/components/ChatView.tsx:3270–3380)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3270-L3380); [composer (apps/web/src/components/ChatView.tsx:3480–3567)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3480-L3567); [remaining shell siblings (apps/web/src/components/ChatView.tsx:3733–4006)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3733-L4006).

### ChatHeader and panel controls

- Project breadcrumb; optional parent breadcrumb; title input or title/menu trigger; origin badge when present; GuestShareIndicator only server thread.
- PanelLayoutControls conditionally renders distinct Thread details, terminal drawer and right-panel controls. Maximize control is separate. ThreadPanelHeaderControl/WorkspaceTitlebarControls avoid mobile Sheet overlap and follow current header ownership.

[header children (apps/web/src/components/chat/ChatHeader.tsx:247–397)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatHeader.tsx#L247-L397); [controls (apps/web/src/components/chat/PanelLayoutControls.tsx:57–230)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/PanelLayoutControls.tsx#L57-L230).

### ThreadDetailsPanel / ThreadDetailsCard

- Inline aside only placement && inlineOpen. Popover stays rendered but opens only mode=popover && popoverOpen; card body belongs to current mode.
- Workspace section: environment only full density and enabled; Open in only nonessential + enabled + capability; scripts only enabled and project scripts defined.
- Version control: branch only cwd + git repository + enabled; git section when cwd + project + git actions enabled or changes enabled with git repository.
- Servers and automations only full density, nondraft and enabled. Relationships only full density, nondraft and lineage or sideChats enabled.
- Usage only enabled; context only enabled with context-window data. Empty card exposes chooser. Context-menu item chooser has all visibility toggles and reset.
- Twelve configured items: environment, openIn, scripts, branch, gitActions, changes, servers, automations, lineage, sideChats default enabled; usage/context default disabled. Density and capability may suppress enabled rows.

[section gates/menu (apps/web/src/components/chat/ThreadDetailsPanel.tsx:151–307)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsPanel.tsx#L151-L307); [inline/popover (apps/web/src/components/chat/ThreadDetailsCard.tsx:130–196)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ThreadDetailsCard.tsx#L130-L196).

### RightPanelSurfaceContent

- No active surface -> workspace launcher. Active -> preview/browser, terminal, diff, pullRequest, linked-pull-request list, sidechat, device, files or file.
- Pull request capability unknown/unavailable is its own visible branch. There is no provider-Agent case in this switch; provider child main navigation is separate.
- Workspace tab chrome/menus/tool internals belong to browser and workspace reconciliation. Shared terminal engine does not merge drawer and dock placements.

[surface switch (apps/web/src/components/chat/RightPanelSurfaceContent.tsx:86–233)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L86-L233); [empty/body branch (apps/web/src/components/RightPanelTabs.tsx:1529–1554)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/RightPanelTabs.tsx#L1529-L1554).

### Settings/Usage and global dialogs boundary

- Settings AppRouteModal: scope unavailable/ineligible notice or scoped Outlet; General-specific restore control; section header/breadcrumb.
- Labs route exists; whether its navigation item is exposed is a separate availability condition. Secrets beforeLoad redirects when feature disabled. Open source licenses and Snap Shot are child pages. Access/Connections share ConnectionsSettings with different view props. Beta/Integrations/Storage are old-link aliases, not extra independent screen designs.
- Usage AppRouteModal: no environment, limits, skeleton, inaccessible, overview branches; UsageModelDialogHost when !showingLimits; price overrides dialog when open.
- Root-mounted dialog hosts are normally null/closed until request. SH20–SH23 enumerate visible request-driven groups; their deep inner forms are not all runtime audited.
- Canonical exhaustive route/global-host registry is surface-reconciliation-completeness. This shell report does not equate route count, host count, surface count, observed screen count or defect count.

[Settings (apps/web/src/routes/_chat.settings.tsx:45–143)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.settings.tsx#L45-L143); [Usage (apps/web/src/components/usage/UsagePage.tsx:504–581)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/usage/UsagePage.tsx#L504-L581); [alias (apps/web/src/routes/_chat.settings.beta.tsx:1–4)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.settings.beta.tsx#L1-L4); [alias (apps/web/src/routes/_chat.settings.integrations.tsx:1–4)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.settings.integrations.tsx#L1-L4); [alias (apps/web/src/routes/_chat.settings.storage.tsx:1–4)](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/routes/_chat.settings.storage.tsx#L1-L4).

## Measurable verification sequence

1. At a fixed viewport, record left nav, main transcript lane and right workspace rectangles. Toggle nav open/close/rapid reverse, recording intermediate frames and input focus; repeat reduced-motion behavior separately.
2. Open a provider child from the parent transcript. Confirm main-lane occupancy, full transcript affordances, status/footer, parent breadcrumb, parent-return scroll and child nested navigation. Do not substitute a SideChat surface.
3. With workspace closed, toggle Thread details. Open Browser/Files while details is present, then toggle details/Escape independently. Verify which visibility state is restored after workspace closes.
4. Resize to the inline-details boundary: verify 280px card, 12px gap, measured chat clearance, >=160px available height and popover fallback. Add Find/floating preview only if available; verify clearance and focus.
5. Measure Browser/File/PR and Files/Changes first open, manual resize, close/reopen, thread switch, maximize/restore and narrow viewport. Preserve tool identity and a usable transcript lane.
6. Open terminal drawer while another workspace tool remains active, then distinguish hide, close terminal, interrupt and terminate. Existing terminal output/split checks do not replace this coexistence test.
7. Open Settings/Usage over working conversation; close nested dialogs then route modal; check retained inert background, return destination and focus. Exercise command palette nested Escape separately.
8. Trigger provider error/refresh and background completion only in a controlled permitted run; capture fresh state after settling. Do not diagnose a timeout from banner text or count stale native accessibility output as a product defect.

## Boundaries

- Source is the exact installed SHA via official raw files; later 74573 checkout is named only as a separate comparison reference. No installed runtime fact is inferred from the later checkout.
- No app launched, UI input, model task, build or test was run by this worker. No product files changed. This is a render-tree/source reconciliation, with parent observations cited by their existing report.
- Major conditional direct children are enumerated at the named roots and their crucial wrapper boundaries. Deeper transcript, browser, tool and settings forms remain delegated; this is not a claim that every possible T3 menu state has been live exercised.
- Default panel animation is not asserted here. The installed source applies motion only for positive configured duration, no reduced motion, and no navigation suppression. User-required Ferrite animation is an independent acceptance bar.
- Ferrite counterparts are anchored to inspected paths. 'No verified counterpart' is bounded evidence, not a repository-wide absence proof; generic modal, panel or content reuse cannot establish placement equivalence.
- Prior native Thread details/child observations are already recorded in thread-details-card-correction.md and subagent-placement-correction.md. Details native captures were not exported as artifact screenshots; this report does not invent them.
- Counts deliberately differ: 25 grouped shell surfaces, conditional children, routes, runtime observations and historical177 findings are separate measures. No new failure tally is assigned here.

Companion maps: [conversation](surface-reconciliation-conversation.md), [browser](surface-reconciliation-browser.md), [workspace JSON](surface-reconciliation-workspace.json), [canonical completeness](surface-reconciliation-completeness.md), [gap reconciliation](surface-reconciliation-gaps.md). The structured version of this report is [surface-reconciliation-shell.json](surface-reconciliation-shell.json).


# T3 surface contracts: existing surfaces must work and look finished

Date: 2026-11-13. This is the corrective source comparison for the user's complaint about oversized panels, abrupt open/close, and an unfinished browser. It is not a feature inventory and does not establish runtime success.

Sources are pinned to Ferrite `62947f91d5cd6ee494aaae9ed09d794a853e5bab` and T3 Code `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, cloned at `/tmp/ferrite-t3-audit-source-20261113`. T3 references below are paths under that clone; Ferrite references are paths under this repository. Line numbers refer to those commits. The original T3 repository is `https://github.com/pingdotgg/t3code`.

Evidence boundary: all conclusions in this report come from actual component assembly and state/size code. No T3 screenshot or live T3 session was produced. Pixel values translated from Tailwind spacing assume the normal 4px spacing unit and 16px root size; the exact classes are included where that matters. Ferrite's native screenshot and interaction evidence belongs in the parent corrective audit.

## The three distinctions the audit must preserve

1. The left thread sidebar is a separate surface from the right workspace panel. Each has its own sizing and motion path.
2. The browser is not just a webview plus a list of capabilities. Its tab hierarchy, address editing, control placement, loading geometry, empty/error composition, and relationship to the panel must be reviewed together.
3. T3's current default panel animation duration is **0ms**. T3 contains opt-in transition and exit/focus machinery. It does not prove that T3 animates by default. The user's explicit request for open/close animation remains an acceptance requirement regardless of that default.

## 1. Right-panel width: why Ferrite can keep opening a massive panel

| Contract | T3 actual host | Ferrite actual host | Consequence to verify |
| --- | --- | --- | --- |
| Starting inline width | 540px; `usePreviewPanelInlineSize.ts:18,35`. | New stored thread width is 0; rendered width is raised to the active surface's floor. Overview is 320px; Browser/File/PR is 42% of the **whole window**, clamped 420–820px. `surfaces.rs:107`; `panel.rs:149–176`; `theme.rs:989,1069–1071`. | Different openers produce different initial footprints before the user ever resizes. |
| Minimum | 360px, shared by the traced inline host. `usePreviewPanelInlineSize.ts:14`. | Overview/Background320px; Browser/File/PR420–820px dynamic floor; Files/Changes max(420px,60% of window minus nav); Terminal/SideChat/Agent420px. `panel.rs:149–161`; `theme.rs:1987,1991,2027–2029`. | Browser cannot always be dragged to a compact useful width. A tool's chosen floor can dominate the operator's explicit resize. |
| Maximum | `max(360, min(floor(viewport*0.7), floor(actualSharedRowWidth)-360))`. `usePreviewPanelInlineSize.ts:68–73`. | `max(320, window-nav-480)`. `panel.rs:164–176`. | The implementations reserve different amounts of conversation space; a visual comparison must record both window and nav widths. Neither formula guarantees both minima fit in an impossibly narrow row. |
| First-paint clamp | Host ref callback measures the real parent/shared row before paint; row resize is observed; window resize is coalesced with animation frames. The source specifically prevents an oversized initial flash. `usePreviewPanelInlineSize.ts:40–62`. | Width is recomputed from `window_w`, `nav_width`, active floor, and stored width; `tend_panel` writes the active floor into state when necessary. `panel.rs:368–391,398–411`. | Opening must be captured over time, not judged from a settled screenshot only. |
| Width ownership | One width per active thread: `t3code:preview-panel-width:${activeThreadKey}`. `ChatView.tsx:3976`; `ChatRightPanel.tsx:30–35`. | One width per thread, persisted in `panel.json`. `surfaces.rs:88,311,352,398`. | T3 does **not** establish per-tool width restoration here. Both hosts share a thread width. |
| What changes the remembered width | User drag commit, explicit set, or storage-key change. Bounds clamp the rendered width; shared hook does not inspect Files/Browser surface kinds. `useResizableWidth.ts:38–67,76–108`. | Merely showing a surface raises stored width if it is below that surface's floor. `tend_panel` calls `surfaces_mut`, which marks persistence dirty, then `save_surfaces`. `panel.rs:244–248,378–391`. | Files/Changes can permanently enlarge subsequent Browser/Overview openings in the same thread without a resize action. |
| Drag behavior | CSS width updates during drag; transitions temporarily disabled; state/storage commit at drag end; abort restores original width; starting storage key is captured so a thread switch cannot save to the wrong thread. `useResizableWidth.ts:71–108`. | Pointer movement updates thread state after clamping to current active floor; maximizing is cleared; notify; persistence is serviced by `tend_panel`. `panel.rs:500–524`. | Test drag, abort/release outside, collapse during drag, and thread switch during drag as complete interactions. |
| Narrow layout | At viewport <=980px, a non-maximized panel becomes a sheet. `rightPanelLayout.ts:1`; `ChatView.tsx:716,1064`. | The traced panel host remains a right-side absolute column; `panel_pane` controls whether it exists and `panel_element` gives it `panel_column_w()`. `panel.rs:181–190,548–619`. | The same large floor must not be excused as a desktop-only choice when the available conversation width is small. |

### A numerical reproduction recipe, derived from source

With a 1680px window and Ferrite's full 280px nav column (256px content + two12px insets), usable width is1400px. A fresh Browser floor is705.6px. Opening Files raises stored width to840px. Returning to Browser still renders840px because its smaller floor does not lower stored width, and maximum920px does not clamp it. Closing/reopening that thread's panel preserves the wider value. These are calculated values, **not observed screenshot measurements**.

At a matching1400px T3 shared-row width and1680px viewport, the inline maximum is1040px and default remains540px; opening another surface does not itself impose Ferrite's60% floor in the traced host. A user may still have deliberately saved a wider T3 panel.

Ferrite references for this calculation: `theme.rs:879–883,912–913,3179`; `nav.rs:62`; `cockpit.rs:2829–2834`; `panel.rs:149–176,378–391`. T3: `usePreviewPanelInlineSize.ts:13–18,68–73`.

### T3's sheet is a different size contract

`RightPanelSheet.tsx:20–25` uses `w-[min(42vw,28rem)] min-w-80 max-w-[28rem]`; at widths below760px it uses `w-[min(88vw,24rem)] min-w-0`. With normal root sizing, that means an ordinary320px minimum/448px cap, then an88vw/384px cap on the smaller layout. It stays mounted for transition handling. This must not be confused with T3's540px inline default, nor copied as a desktop-browser sizing claim.

## 2. Open/close motion: support, default, and visual continuity

| Surface/lifecycle | T3 contract | Ferrite source fact |
| --- | --- | --- |
| Default and reduced motion | `DEFAULT_PANEL_ANIMATION_DURATION_MS=0`; accepted range0–400ms. `packages/contracts/src/settings.ts:155–164,414`. `panelAnimations.ts:31–38` activates only for duration>0, no reduced-motion preference, and no route suppression. | Motion catalog explicitly lists no sidebar or panel animation. `crates/ferrite-ui/src/motion.rs:33–35`. `panel.rs:23–27` documents snap open/shut/width behavior. |
| Left sidebar open/close | Desktop gap transitions width; fixed sidebar container transitions left/right/width when animation flag is enabled. Offcanvas closed gap is0 and container moves beyond the edge. `ui/sidebar.tsx:387–403`. | `cockpit.rs:2829–2838` returns width0 or fixed column width from a boolean folded state. `set_nav_collapsed` changes that state directly (`cockpit.rs:6186`). |
| Right inline open/close | Width transitions between0 and its remembered width when enabled; ordinary width updates/maximize changes suppress that transition. `preview/PreviewPanelShell.tsx:81–103,161–188`. | `panel_element` returns no element when `panel_pane` is absent; otherwise draws the absolute panel at the current width. `panel.rs:548–619`. |
| Closing content placement | Inner content stays at its full width pinned to the right; outer clipping uncovers chat instead of pushing the tab bar underneath native title controls. `PreviewPanelShell.tsx:195–219`. | Current direct removal has no corresponding closing geometry. |
| Exit lifecycle | `usePanelPresence` retains same-scope content until duration expires; reopening cancels the timer; changing scope cannot show another thread's retained panel. `panelAnimations.ts:41–68`. | A snap policy does not supply an exit lifecycle; adding only a tween constant would not fix this. |
| Focus during/after collapse | Before-paint layout effect makes collapsed content inert and restores focus to a valid external origin, or the panel toggle; focus restoration uses preventScroll. `PreviewPanelShell.tsx:55–66,120–148`. | This report does not establish Ferrite's complete keyboard focus behavior. Parent runtime verification must check focus and shortcuts while opening/closing. |
| Thread navigation | T3 suppresses panel animations until two browser frames have painted the new route. `panelAnimations.ts:11–28`. | Do not treat a thread switch as the same animation event as the operator toggling a panel. |
| Resize | Sidebar drag writes directly to live width targets and disables transition during the drag, then persists on completion. `ui/sidebar.tsx:473–542`. Right width hook similarly suppresses transitions. | Motion acceptance must preserve immediate pointer tracking; animated opening must not make resizing trail the cursor. |

The corrective requirement is an interruptible, coherent open and close with stable content/focus, including reduced-motion behavior. T3's opt-in machinery is a reference for handling those states. Its0ms default is not a reason to mark the user's reported snap as false or acceptable.

## 3. Left sidebar sizing and restoration

T3's app-level sidebar starts256px, minimum208px or the measured brand/header width if larger. Maximum leaves640px for main content. It clamps stored values to the current viewport; resize acceptance also checks actual wrapper width minus proposed sidebar width>=640px. The rail supports double-click reset of stored width. Visibility is separately persisted under `chat_thread_sidebar_open`. Sources: `components/threadSidebarWidth.ts:1–33`; `AppSidebarLayout.tsx:428–490`.

The generic sidebar switches to mobile sheet behavior at767px and below. Its mobile width is `calc(100vw - var(--spacing)*3)`. Sources: `ui/sidebar.tsx:30–33,109,324–350`. These values belong to the left thread navigation, not the right preview sheet.

Ferrite's traced nav width is fixed280px when fully shown and0 when folded, with auto-fold rules for available board width. Sources: `theme.rs:912–913,3179`; `nav.rs:62`; `cockpit.rs:2829–2860`. A screenshot should record whether the nav folded automatically or the user explicitly collapsed it; those are different transitions and can affect right-panel calculations.

## 4. Actual browser construction, not a component checklist

### T3 browser hierarchy

```text
PreviewPanelShell: flat panel, inline seam at left, width controlled by host
  RightPanelTabs: one shared row containing each browser page and other surfaces
  PreviewView: flexible body
    PreviewChromeRow: navigation / editable address / page tools
    flexible clipped page body: web contents or composed empty/error state
```

Sources: `RightPanelTabs.tsx:1325–1395,1539`; `preview/PreviewView.tsx:964–1046`; `PreviewPanelShell.tsx:161–219`.

T3 page tabs use the page title if known, then URL host, with a12px favicon/globe. `URL.host` preserves a port. Tabs are24px tall, max144px wide,12px label type,2px internal gap,6px left/8px right padding, and rounded-md. Active background and foreground differ from muted inactive tabs; overflow belongs to a horizontal scroll area with hidden scrollbars/fade. Sources: `RightPanelTabs.tsx:596–618,1019–1025,1351–1371`. Native/titlebar integration adjusts reserved controls space; it does not add another browser tab strip.

### Ferrite browser hierarchy

```text
absolute right panel
  surface_head: shared panel tabs,44px high
  horizontal16px inset and bottom8px inset
    rounded reader card and ring
      reading_head: Browser/preview title,39px high
      browser-tabs: a second strip for browser pages
      toolbar: Back / Forward / Reload / sometimes Annotate / Address / field / Go
      optional normal-flow Loading… row
      optional normal-flow error sentence
      page / annotation body
```

Sources: `panel.rs:568–573,636–645,685`; `panel/strip.rs:372`; `panel/preview.rs:383,498–515`; `browser.rs:1356–1550`; `ferrite-pane/src/preview.rs:641–660,689–708`; `theme.rs:879–883,923,990–992,1179–1181,2242`.

The two tab levels are explicit in the data model: Ferrite `SurfaceId::Browser` is a singleton whose page tabs live inside it (`surfaces.rs:35–37`). This nesting plus a generic reader header/card is a concrete source-level explanation for the browser looking like a browser pasted into a file preview. The issue is the assembled hierarchy, even if every individual control exists.

The inner page tabs use only hostname/full-URL fallback, max190px width,13px type,8px horizontal and4px vertical padding, and a28px close control. Their minimum height derives to36px from that close button plus padding. A loaded page therefore has at least44+39+36+36=155px of stacked headers/toolbar above it from the window top, before additional loading/error content, assuming the row children fit their nominal sizes. This is a source-derived lower bound to check against a capture, not a measured screenshot. The toolbar has no explicit height; its28px controls plus two4px padding imply at least36px.

T3 has one configured workspace topbar plus40px browser chrome budget in ordinary inline mode; it does not carry the39px reader header and second page-tab strip. Its base workspace topbar variable is52px, with window-controls-overlay overrides, so do not claim the topbar has identical height on every host. `index.css:123,156`.

## 5. Browser chrome: geometry and state differences that affect quality

| Area | T3 source contract | Ferrite source and visual implication |
| --- | --- | --- |
| Toolbar rhythm | `PreviewChromeRow.tsx:95`: fixed40px/min40px,8px horizontal inset,4px gap. Inline modifier makes it28px/min28px plus12px bottom margin; border becomes transparent. | `browser.rs:1406` creates an intrinsic-height row with4px all-around padding and4px gaps, inside the reader card. There is no shared explicit geometry with its tabs/header. |
| Navigation controls | Separate group with2px gap. `Button size=icon-xs` is24px at sm+ or28px below; normal desktop icon14px. Shared ghost hover/focus/disabled styling. `PreviewChromeRow.tsx:98–150`; `ui/button.tsx:8–9,29–30,49–50`. | Shared head-door28px controls with16px icons; disabled look is0.35 opacity and click-time guard. `browser.rs:1407–1460`; `preview.rs:689–708`; `theme.rs:1179–1181`. Do not equate visual dimming with complete keyboard/disabled semantics without runtime proof. |
| Address hierarchy | A flexible28px ghost InputGroup. No visible “Address” label consumes width; placeholder is “Search or enter URL.” Hover gives a quiet fill, focus gives the group's focus border/ring/background. `PreviewChromeRow.tsx:153–215`; `ui/input-group.tsx:8,26–27`. | A visible “Address” text child, separate Go icon, and a Composer configured as a one-row field. `browser.rs:180–184,1478–1488`. Inspect the actual field rendering before declaring a specific border missing; this comparison establishes the extra label/control allocation. |
| Address editing | URL display is separate from focused draft. Focus seeds/selects it; Escape resets/blurs; Enter honors composition; submit trims and blurs. `PreviewChromeRow.tsx:80–88,159–189`. | Keyboard success is outside this source-only report. The existing role/Go handler alone is not enough to call address editing production-ready. |
| Annotation slot | Pick handler/slot is based on bridge/tab availability; loading is not its visibility condition. Availability can disable the control rather than insert/remove it. `PreviewView.tsx:987–1002`; `PreviewChromeRow.tsx:218–242`. | `annotatable` explicitly requires `!tab.loading`; the text button is wholly absent while loading and appears before Address once ready. `browser.rs:1462–1477`. The address field must change width/position between those states. |
| Loading | Refresh changes to stop state in the same slot. A2px absolute progress line overlays the bottom of the toolbar; it never occupies a new normal-flow row. `PreviewChromeRow.tsx:136–150,295–299`; `index.css:2424–2455`. | A normal-flow “Loading…” line is inserted between toolbar and page. `browser.rs:1524–1535`. The page's available height/top edge changes during navigation. |
| Error | `PreviewUnreachable.tsx:61–140` composes a body state with icon, host, readable explanation, optional technical details, and reload/actions; toolbar stays in its established place. | `browser.rs:1536–1548` adds raw error plus “Edit the address or reload…” as a normal-flow sentence above body. Long error text can change chrome height. |
| Empty state | `PreviewEmptyState.tsx:35–46,51–110` supplies an explicit empty title/description or structured recent/local destinations inside a bounded body. | `browser.rs:1332–1343` returns a padded sentence for no-thread/no-browser state. The screenshot must distinguish those cases from a selected blank new tab; they are not the same render branch. |
| Page identity | Unified page tab uses actual title, host fallback, favicon/globe. | Nested tab uses `host_str()`, which excludes port, and no page-title branch in this render loop. `browser.rs:1356–1361`. Two local apps on different ports can therefore look identical in the strip. |

This comparison does not demand that Ferrite copy T3's colors, corner radii, or brand. It demands coherent panel/browser hierarchy, a deliberate space budget, stable controls across loading states, identifiable pages, and complete empty/failure handling using Ferrite's own system.

## 6. Capture criteria that can actually confirm or refute this report

Use identical window dimensions, a recorded nav state, the same simple page, and a fresh thread plus a thread with saved widths. Record device scale; measure logical pixels rather than mixing screenshot physical pixels with source CSS/GPUI values.

| Capture/interaction | Measurements and pass evidence |
| --- | --- |
| Fresh Browser vs Files→Browser | Panel left edge, total panel width, conversation width before/after, and stored width before/after. Confirm whether opening Files alone changes later Browser width. A single wide screenshot cannot prove cause. |
| Drag smaller then reopen | Drag to a chosen compact width, release, close/open, switch surfaces and threads. Record each width and whether the same saved choice survives. |
| Close/open both sidebars | Frame sequence or video with timestamps. Show start, intermediate geometry, and final state. A still cannot prove smoothness or absence of motion. Measure left nav and right panel separately. |
| Interrupt transition | Close then reopen before completion, reverse again, and navigate thread mid-transition. No stale content, stuck width, transparent click interception, or focus landing in a hidden panel. |
| Resize while another surface is active | Cursor and edge track immediately; no easing lag; release outside still ends resizing; resulting width stays with the originating thread. |
| Browser loaded/loading/error | Measure top of actual web content in all states; annotate header/tab/toolbar bounds. Control centers and address-field rectangle should remain stable except intentionally designed changes. |
| Browser narrow/wide | Measure card margins, border layers, number of tab/header rows, text clipping, and proportion of width consumed by labels/buttons. Check long title/URL and two localhost ports. |
| Hover/focus/disabled | Capture Back disabled, Back focused, address focused, active/inactive page tabs, annotation unavailable/available. Verify click and keyboard behavior alongside appearance. |
| Small viewport | Capture just above/below responsive thresholds, with panel and nav both open. Record conversation usable width, not merely whether components remain visible. |

For the browser appearance claim, the most persuasive evidence is one annotated native capture showing the exact stacked rows and card insets, followed by loading/loaded captures with the same window/page. For the oversized-panel claim, use a short interaction sequence with measured widths. For motion, use time-resolved evidence. Do not substitute screenshots from an unrelated app or label a source-based reconstruction as T3 runtime.

## 7. T3 live-reference feasibility

Read-only inspection found no `node_modules` at clone root or `apps/web`, and no built `apps/web/dist`, `apps/server/dist`, or `apps/desktop/dist`. The root package declares Node26.12.1 and pnpm12.14.0, `dev` via `scripts/dev-runner.ts`, and Vite Plus. Web depends on local workspace packages plus runtime/backend integration. This clone is not immediately runnable as an already-built visual reference; installing dependencies and selecting a backend/data setup would be a separate task. No install, build, app launch, or process was performed by this report's author.

## Result of this corrective comparison

The source supports three concrete priorities: the forced width-floor persistence that explains repeated oversized panels; the abrupt/sidebar-panel lifecycle that the user explicitly rejects; and the browser's extra reader-card/header/tab nesting plus unstable loading controls/rows. These are failures of existing surfaces to form a finished interaction. Finding more optional features is not a substitute for demonstrating that those paths feel and behave correctly.

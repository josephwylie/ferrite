# Browser presentation reconciliation

The user wants a complete browser presentation map after a floating browser preview was confused with the separate Thread details card. This report traces rendered children, placement, state handoffs, Ferrite counterparts, and the evidence each journey still needs.

Throughput checkpoint. Read-only source investigation with two report outputs.

## Revision and evidence rules

Installed T3 Nightly `0.1.10-nightly.20261111.4544` maps to `1264c206aee0dce805d6279809118bc091f29d2f`. All T3 source links below use that commit. The newer checkout `74573fc29b901bafbf8f1dbb2eb29606da1fa448` is a byte-comparison target only. Ferrite source is `62947f91d5cd6ee494aaae9ed09d794a853e5bab`; the inspected product paths have no working-tree diff. [Source provenance](t3-installed-source-match.md) supplies the installed identity.

The [native ledger](../evidence/t3-live/observations.md) belongs to the parent's UI run. This worker used no UI or models. The [JSON companion](surface-reconciliation-browser.json) fingerprints the ledger and each source file, records byte matches to the newer checkout, and stores the map. Source-only branches remain source-only. A successful local-native flow does not verify a remote streamed branch.

Thread details is a separate shell component. This browser map names that boundary and its layout interaction. It does not substitute browser mini-player coverage for the upper-right Thread details card. BP identifiers below are map rows, not new findings or an increase to the existing issue tally.

## Root composition

- AppRoot mounts RouterProvider and ElectronBrowserHost as siblings inside AppAtomRegistryProvider. ElectronBrowserHost owns authorized HostedBrowserWebview guests, their fixed native pages, and optional device wrappers.
- ChatView -> ChatRightPanel -> shared RightPanelTabs -> RightPanelSurfaceContent preview -> embedded PreviewPanel -> PreviewView chrome and conditional body.
- ChatView chat canvas -> ThreadPreviewMiniPlayer -> BrowserMiniPlayer -> MiniPlayerShell -> local BrowserSurfaceSlot or ServerBrowserSurface.
- ChatView chat canvas -> ThreadDetailsPanel is a distinct shell component. It is only a layout obstacle in this browser map.
- PreviewView -> guest PickPreload annotation overlay -> composer context detail -> sent context detail -> optional ExpandedImageDialog.
- PreviewView or mini-player action -> native mirror BrowserWindow or remote video PiP. More -> detached DevTools is a different window.

## Presentation inventory

| Map row | Presentation | Existing native evidence |
| --- | --- | --- |
| BP-01 | Root ownership and retained browser host | native-partial |
| BP-02 | Docked Workspace browser and responsive sheet | native-partial |
| BP-03 | Browser chrome and address editing | native-partial |
| BP-04 | Local guest, remote stream, and control overlays | source-only-for-stream |
| BP-05 | Interactive in-app browser mini-player | native-partial |
| BP-06 | Automatic browser reveal and presentation selection | source-only |
| BP-07 | Native picture-in-picture preview mirror and remote video PiP | source-only |
| BP-08 | Detached DevTools window | native-completed-bounded |
| BP-09 | Browser device viewport inside the selected presentation | native-partial |
| BP-10 | Annotation overlay and movable annotation editor | native-partial |
| BP-11 | Annotation detail cards and expanded image preview | native-partial |
| BP-12 | Direct screenshot export and recording feedback | native-partial |
| BP-13 | Empty, unavailable, and failed page presentations | native-partial |
| BP-14 | Loading, Find, zoom, cursor, and local feedback layers | native-partial |
| BP-15 | Link menus, browser menus, and website popups | native-partial |
| BP-16 | Browser permission sheets and JavaScript dialogs | native-partial |
| BP-17 | Cross-presentation focus, stacking, and motion | native-partial |

## BP-01. Root ownership and retained browser host

Render path. AppRoot mounts ElectronBrowserHost outside RouterProvider and inside AppAtomRegistryProvider, so a route change does not itself unmount the native guests. ChatView separately mounts the selected Workspace surface, thread mini-player, and Thread details card. A server-owned tab can render natively or as a remote stream; server-owned does not always mean streamed.

Visual composition. The docked and mini-player components describe presentation rectangles. BrowserSurfaceSlot acquires an owner lease and presents the same runtimeTabId at its measured bounds. The fixed HostedBrowserWebview supplies the actual native page, viewport wrapper, and optional device controls. Remote ServerBrowserSurface supplies a canvas, input bridge, and control UI instead.

State and handoff. Only the current surface owner can update or release a local tab rectangle. Hosted guest lifetime is distinct from a slot lifetime. Desktop-tab reference count reaching zero schedules tab closure; this is not a promise to preserve every hidden or restarted session.

Native evidence. Local browser navigation, form input, hide/reopen, target-blank, scripted popup, tab close/reopen, and mini-player coexistence were observed. No remote-stream run was performed.

Native limit. The native run does not reveal every lease, mount, ownership, or reference-count transition.

Ferrite counterpart. Ferrite open_browser attaches the thread-owned service, selects its saved active tab, calls preview.show_browser, and upserts one SurfaceId::Browser. The outer panel routes Browser through panel_preview, then Body::Browser through browser_body. Native CEF WebView entities live in a per-thread page map, separate from persisted tab identity.

Current audit coverage. Browser ownership and page lifetime were traced in browser.md. The original findings count does not enumerate this complete presentation graph.

Omitted or unverified contracts follow.

- Exercise same-tab dock-to-float-to-dock handoff while preserving form values, scroll, history, selection, and the unsent chat draft.
- Measure or inspect which presentation owns pointer and keyboard input during a handoff, route switch, and guest remount.
- Keep runtimeTabId identity, local native rendering, remote streaming, and source-tab session lifetime distinct in evidence.

Source anchors. [Native guests live outside route-owned components](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/AppRoot.tsx#L8), [Selected Workspace content](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3179), [Independent mini-player root](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3815), [Separate Thread details boundary](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3832), [Native versus streamed runtime selection](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L206), [Retained native guest host](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/ElectronBrowserHost.tsx#L128), [Presentation lease](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/BrowserSurfaceSlot.tsx#L29), [Exclusive local presentation owner](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/browserSurfaceStore.ts#L210), [Native tab lifetime](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/desktopTabLifetime.ts#L44), [Ferrite browser entry](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L195), [Ferrite preview route](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L685), [Ferrite Browser body](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/preview.rs#L383).

## BP-02. Docked Workspace browser and responsive sheet

Render path. ChatView renders ChatRightPanel when rightPanelPresent, an active thread, and a non-embedded chat are present. RightPanelSurfaceContent selects preview, which mounts PreviewPanel in embedded mode inside the shared panel.

Visual composition. ChatRightPanel chooses an inline shared tab host or RightPanelSheet. The shared strip owns each T3 browser tab alongside Files, Terminal, Diff, PR, and Side chat. Embedded PreviewPanel adds the browser body without another shared strip. PreviewPanelShell owns collapsible width and retained content.

State and handoff. Panel visibility, active surface, and browser-session closure are separate operations. Inline collapse returns focus and sets inert before painting. Closing a browser tab remembers a successful snapshot; reopening creates a session from URL, viewport, and profile. It does not restore a captured DOM.

Native evidence. The local browser opened in the right panel. Command-Option-B hid and reopened it with its active tab and draft retained. Command-W and Command-Shift-T closed and restored its title and URL.

Native limit. The native browser run did not establish narrow-sheet behavior, divider dragging, first-frame width, or browser-specific maximize geometry.

Ferrite counterpart. Ferrite has one outer Browser surface with a second browser tab strip inside its Reader-style card. The outer panel has fixed-column/minimum-width rules and a maximized state. Browser pages are not independent peers of Files and Terminal in the outer strip. Existing runtime evidence reproduces sticky Overview width after visiting wide tools.

Current audit coverage. panel-tools.md explicitly mentions the internal browser strip, and panel-tools-018 plus panel-runtime-correction.md cover sizing. No original browser finding owns the entire nested chrome and close-command hierarchy.

Omitted or unverified contracts follow.

- Compare outer tabs, inner tabs, Reader header, navigation row, viewport origin, and consumed vertical space at ordinary laptop widths.
- Verify independent hide, close-tab, close-panel, maximize, restore, and narrow-sheet transitions. Record which control each shortcut targets.
- Closing and restoring browser presentation must preserve or clearly reload page state; closing a tab must not close its conversation.

Source anchors. [Workspace panel root gate](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3931), [Inline versus sheet branch](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatRightPanel.tsx#L34), [Preview surface dispatch](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L85), [Embedded browser shell](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanel.tsx#L50), [Shared Workspace strip host](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/RightPanelTabs.tsx#L1324), [Focus and inert on collapse](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L132), [Remember successful browser close](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/closePreviewSession.ts#L22), [Recreate browser session](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/reopenClosedView.ts#L104), [Ferrite internal browser strip](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1492), [Ferrite Reader-style browser card](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/preview.rs#L498), [Ferrite tool width floor](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel.rs#L149).

## BP-03. Browser chrome and address editing

Render path. PreviewView places PreviewChromeRow before its relative browser body. Its trailing action is PreviewMoreMenu. Availability follows the current tab, runtime, rendered page, and control ownership.

Visual composition. T3 has one compact navigation row containing Back, Forward, Refresh or loading status, an expanding address field, annotate/capture/float controls, and More. Open in system browser appears inside the address area on hover/focus when the address is not being edited. The loading track is absolutely positioned under this row. The active address draft is separate from the committed URL.

State and handoff. Submit navigates the displayed target. Escape cancels address editing. Annotation, capture, and float remain recognizable controls but can be disabled on a failed page. Menu actions are capability-filtered instead of assuming every runtime exposes local DevTools or Find.

Native evidence. Browser chrome, menus, error-disabled controls, navigation, and committed URL changes were observed. Form input left the chat draft intact.

Native limit. Address-edit survival through redirects, focus order, narrow overflow, hover-only discoverability, and every menu action were not verified.

Ferrite counterpart. Ferrite renders its internal browser tabs, then a toolbar with Back/Forward/Reload, conditionally inserted Annotate, Address composer, and Go. A Reader header remains above it. Loading and errors add rows. It has no browser-specific More menu or explicit system-browser escape in that toolbar.

Current audit coverage. BROWSER-05 through BROWSER-08, BROWSER-10, BROWSER-15, and BROWSER-16 cover selected behavior. They do not establish complete chrome density, hierarchy, focus order, collision, or stable hit targets.

Omitted or unverified contracts follow.

- Keep the page origin and address bounds stable across loading, errors, annotation availability, and narrow layouts.
- Make external opening and other primary actions discoverable without relying solely on a hover state.
- Resolve address input focus, committed URL, and submitted draft separately, including redirects and Escape.

Source anchors. [Browser chrome before body](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L948), [Navigation row structure](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewChromeRow.tsx#L91), [Address edit draft](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewChromeRow.tsx#L159), [Conditional external-open reveal](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewChromeRow.tsx#L188), [Reserved loading track](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewChromeRow.tsx#L292), [Runtime-specific More actions](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L478), [Ferrite browser toolbar](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1406), [Ferrite conditional annotation control](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1452), [Ferrite loading row](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1522).

## BP-04. Local guest, remote stream, and control overlays

Render path. PreviewView chooses ServerBrowserSurface only for server-owned tabs that do not render natively. Otherwise it presents a local BrowserSurfaceSlot when a desktop bridge exists. A missing bridge has its own explanation.

Visual composition. The local guest receives native form and page input. The remote branch renders a control bar, canvas, hidden text-input bridge, file chooser, JavaScript-dialog card, host setup, and reconnect/denied states. The control bar can sit at the bottom in the mini-player. The remote page owns a server session and streamed input rather than a local Electron DOM.

State and handoff. Control ownership gates input and the main browser toolbar. Take control and Release control are explicit server messages. Local and streamed branches cannot share a blanket assertion about prompts, permissions, IME, or popup implementation.

Native evidence. Native local input and form behavior were exercised.

Native limit. Remote control transfer, stream reconnect, file chooser, streamed dialogs, and denied access were not exercised.

Ferrite counterpart. Ferrite exposes manual-intervention ownership and a local CEF host through browser.rs and visual/element.rs. The inspected Browser surface has no equivalent remotely streamed canvas/control-bar branch. This does not classify all Ferrite remote execution features as missing.

Current audit coverage. Input ownership is partly covered by input-focus-001 and input-focus-006. No original browser finding maps every remote-stream child or presentation state.

Omitted or unverified contracts follow.

- Record local-native and streamed browser observations under separate capability gates.
- Verify control ownership is visible, inputs are rejected or delivered to the correct owner, and reconnect does not act on a stale page.
- Do not generalize the observed local Electron Prompt exception to the remote stream, whose installed source has a Prompt input.

Source anchors. [Native server-tab versus streamed branch](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L207), [Remote input gating](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L225), [Remote body conditional](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L1019), [Local guest or missing-bridge conditional](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L1066), [Remote control and canvas root](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/ServerBrowserSurface.tsx#L743), [Remote text input bridge](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/ServerBrowserSurface.tsx#L797), [Remote file chooser](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/ServerBrowserSurface.tsx#L825), [Remote JavaScript dialog card](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/ServerBrowserSurface.tsx#L856), [Remote denied/reconnect state](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/ServerBrowserSurface.tsx#L911), [Ferrite native input forwarding](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/element.rs#L270).

## BP-05. Interactive in-app browser mini-player

Render path. ChatView renders ThreadPreviewMiniPlayer separately within the chat canvas when the active thread has a visible mini-player and preview access. This component can also host a device stream; that device branch is not browser device emulation. The visibility helper suppresses the mini-player while the rendered right-panel surface is that same source tab, so one page is not shown in two interactive positions.

Visual composition. The browser mini-player presents the same local runtime tab through BrowserSurfaceSlot with fitSourceContent, rounded corners, and z-index 48. Its shell has a shadow layer at 47, ring and controls at 49, eight resize handles, and a small top-right handle. Hover/focus reveals an action pill with Open in right panel, a separate-window action when available, and Close. Touch tap reveals the pill. Remote streams use their own control bar below the content.

State and handoff. Float from docked chrome opens the mini-player and hides the whole right panel. Open in right panel closes the mini-player and opens that same browser tab. Close deletes mini-player presentation state, not the browser session. A separate source switch while still open can retain position and width. Store state is not persisted through a persistence middleware here. The actual chat-canvas caller initially places a new player near the lower right, then adjusts around chat, composer, Find, and the separate Thread details card.

Native evidence. A floating preview existed alongside another right-panel tab. Its dedicated Close control worked.

Native limit. Initial activation was confounded. Drag, eight resize directions, page interaction while floating, dock return, touch pill, focus, auto-placement, retained geometry, and state continuity were not certified.

Ferrite counterpart. No mini-player entry or matching render branch exists in the inspected Ferrite Browser toolbar, Reader browser body, or SurfaceId browser route. Ferrite generic float windows are not proof of an interactive browser mini-player.

Current audit coverage. Original browser.md mentions floating preview in its supported-capabilities paragraph. Checklist BROWSER-26 plans the lifecycle. No standalone original finding or acceptance suite captures its complete placement and interaction contract.

Omitted or unverified contracts follow.

- Treat the mini-player as a distinct presentation, not as Thread details or a resized docked panel.
- Exercise explicit float, automatic appearance, drag, aspect-preserving resize, pill reveal, interaction, dock return, Close, and reopen with the same page sentinel.
- Verify no overlap blocks composer, Find, details controls, dialogs, or sidebar menus, including short and narrow windows.
- Verify whether Close/reopen intentionally resets geometry and whether switching browser sources retains it.

Source anchors. [Mini-player visibility and access gate](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L1921), [Independent mini-player root](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L3815), [Float hides docked panel](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L465), [Return same tab to dock](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L230), [Browser mini-player composition](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L270), [Interactive native surface slot](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L336), [Presentation-only Close](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L460), [Drag and resize updates](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L483), [Mini-player shell and controls](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L526), [Eight resize handles](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L616), [Mini-player state lifecycle](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/previewMiniPlayerStore.ts#L64), [Geometry and stacking constants](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/previewMiniPlayerLayout.ts#L12), [Actual initial bottom placement](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/chatCanvasLayout.ts#L66), [Details-card overlap output](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/chatCanvasLayout.ts#L172), [Inspected Ferrite browser entry controls](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1406), [Suppress duplicate same-source presentation](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.logic.ts#L134).

## BP-06. Automatic browser reveal and presentation selection

Render path. agentOpenedPreviews watches preview-session changes beyond the current thread. A fresh revealed server session can open a floating preview, while a forced reveal can focus an existing panel tab. This is a different entry path from clicking Float.

Visual composition. An automatic reveal is gated by browserAutoShowFloatingPreview unless force is set. Freshness is computed after the first loaded baseline, so an initial reload does not automatically resurrect dismissed previews. Session reconciliation closes a mini-player whose source session disappeared. Popup routing preserves whether a streamed popup originated in the panel or floating presentation.

State and handoff. Only one mini-player source is stored per thread. Automatic source changes can therefore replace which preview appears without creating another independent mini-player frame.

Native evidence. Browser settings exposed auto-floating preview configuration.

Native limit. No agent-opened browser auto-reveal, forced reveal, baseline reload, or automatic source replacement was exercised.

Ferrite counterpart. Ferrite browser requests can reveal its Browser panel while checking the surface selected since the request. This is a docked reveal path, not floating presentation. Browser source state remains thread-owned.

Current audit coverage. Original reports mention broader agent browser capabilities, but do not independently verify this automatic presentation workflow. The browser mini-player lifecycle has no dedicated original finding.

Omitted or unverified contracts follow.

- Test explicit float and auto-reveal separately. Respect disabled auto-preview and distinguish a forced operator reveal.
- Do not resurrect dismissed previews on a baseline reload. Replace or retain the intended source when another session appears.
- Verify background-thread reveals do not steal the currently focused conversation or draft.

Source anchors. [Auto-floating and forced reveal](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/agentOpenedPreviews.ts#L37), [Session reconciliation](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/agentOpenedPreviews.ts#L64), [Fresh-reveal baseline](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/agentOpenedPreviews.ts#L87), [Watched reveal changes](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/agentOpenedPreviews.ts#L101), [All-thread reveal lifecycle](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/agentOpenedPreviews.ts#L240), [One mini-player source per thread](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/previewMiniPlayerStore.ts#L64), [Ferrite guarded docked reveal](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L637).

## BP-07. Native picture-in-picture preview mirror and remote video PiP

Render path. The docked More menu and mini-player pill can request a separate window. This is distinct from the in-app mini-player and from a website-script popup.

Visual composition. For native guests Manager creates an always-on-top BrowserWindow, with native resizing, minimum dimensions, no maximize or minimize, skipped taskbar, and macOS panel behavior. Its document contains an image that receives captured JPEG frames. The inspected separate-window document does not forward website input. It is a visual mirror, not another interactive browser tab. Remote server picture-in-picture instead uses canvas-to-video capture and an interactive:false stream, subject to browser support.

State and handoff. Opening a native mirror validates the source webContents and reuses an existing mirror for that tab. Closing releases its mirror session and updates pictureInPicture without navigating or closing the source tab. Frame delivery adjusts the native window aspect ratio. The remote stream survives React panel unmounting through module-level ownership.

Native evidence. Separate-window actions were visible.

Native limit. No separate preview window was opened or verified during the recorded browser sequence. Native geometry, stacking, other-workspace visibility, source continuity, failure, and closure remain untested.

Ferrite counterpart. No corresponding separate-preview action appears in the inspected Ferrite Browser toolbar or browser SurfaceId dispatch. Generic app windows and the image lightbox are different presentation types.

Current audit coverage. Original browser.md names separate preview windows and checklist BROWSER-27 plans inspection. There is no standalone original finding or complete lifecycle record.

Omitted or unverified contracts follow.

- Do not describe the native image mirror as an interactive browser window or confuse it with window.open.
- Open from docked and floating presentations, resize, switch threads, close the source, and close the mirror. Verify no orphan window or capture activity remains.
- Distinguish the source-supported disabled minimization from a product requirement to add it. Verify current page, title, aspect ratio, and return route.

Source anchors. [Native separate-window request](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L519), [Mini-player separate-window action](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L235), [Mirror document contains frame image](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L167), [Native mirror lifecycle](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L2933), [Native mirror BrowserWindow shape](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L2961), [Native close and floating level](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L3024), [Mirror release without tab closure](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L2864), [Remote mirror ownership outside React](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/serverPictureInPicture.ts#L25), [Remote video PiP path](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/serverPictureInPicture.ts#L67), [Remote mirror non-interactive stream](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/serverPictureInPicture.ts#L115), [Inspected Ferrite toolbar](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1406).

## BP-08. Detached DevTools window

Render path. More exposes DevTools for a local native tab, including server-owned tabs rendered natively. A pure remote stream without a native guest does not receive that action through this branch.

Visual composition. Manager opens Chromium DevTools in detach mode for the exact guest webContents. This is its own native development window, not a browser tab, in-app floating preview, or image mirror.

State and handoff. An already-open inspector receives focus. Opening detaches the control session; closing restores it. The source tab remains the inspected target.

Native evidence. After separately confirming the Amber page URL, DevTools Console displayed the actual unsupported Prompt error twice. Both inspected DevTools windows were closed.

Native limit. The initial unsettled tab selection targeted Cobalt and is not recorded as a targeting defect. No all-runtime, repeated-open, debug-pause, or control-restoration proof follows from this one Console check.

Ferrite counterpart. BROWSER-10 records no browser DevTools control in Ferrite. Agent browser observation is not a human DevTools UI.

Current audit coverage. BROWSER-10 owns the capability candidate. Earlier menu-only coverage has been upgraded only for the confirmed native Console sequence.

Omitted or unverified contracts follow.

- Match the inspector to a separately verified tab identity. Reopen focuses one inspector rather than multiplying it.
- Close the inspector and verify page input and agent/manual control resume on the same tab.

Source anchors. [Native-server DevTools gate](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L487), [Local native DevTools action](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L513), [Detached inspector ownership and restoration](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L2165), [Detached DevTools window](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L2178), [DevTools menu branch](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewMoreMenu.tsx#L99).

## BP-09. Browser device viewport inside the selected presentation

Render path. A non-fill viewport adds device controls. For native guests HostedBrowserWebview owns the toolbar and resize handles. For remote streams PreviewView inserts the toolbar above the streamed body. Mini-player fitSourceContent suppresses the native device toolbar.

Visual composition. The docked viewport can be a centered/scaled constrained page within a larger browser area, with preset, width, height, aspect lock, rotation, eight resize handles, and a drag-size label. The outer panel width and actual page viewport are separate dimensions. This is browser viewport emulation, not the device-stream mini-player branch.

State and handoff. Choosing a preset updates shared viewport state. Rotation swaps dimensions and adjusts a locked ratio. Closing returns to fill. A mini-player fits the chosen source content without recreating that docked toolbar.

Native evidence. iPhone 12 Pro displayed 390 by 844, rotation displayed 844 by 390, and closing restored normal browser view. Controls for dimensions, lock, and handles were exposed.

Native limit. Actual innerWidth, media-query behavior, touch/user-agent emulation, custom dimensions, aspect-lock behavior, drag resizing, zoom scaling, and handoff were not measured.

Ferrite counterpart. Ferrite native browser bounds follow the rendered WebView area. Browser Action and the toolbar lack an independent page viewport setting. Panel resizing changes page dimensions.

Current audit coverage. BROWSER-09 owns the missing independent viewport capability. Original acceptance did not map every native/stream/floating placement variant.

Omitted or unverified contracts follow.

- Verify actual CSS viewport dimensions independently of displayed controls, including rotation, zoom, and constrained panel fitting.
- Preserve chosen viewport across float/dock and distinguish it from native window size.
- Check toolbar and handles remain reachable near edges and under remote control gating.

Source anchors. [Native toolbar suppressed for fitted mini-player](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/HostedBrowserWebview.tsx#L237), [Native viewport wrapper and toolbar](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/HostedBrowserWebview.tsx#L317), [Native resize handles and drag label](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/HostedBrowserWebview.tsx#L377), [Stream viewport toolbar](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L1021), [Preset application](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/BrowserDeviceToolbar.tsx#L145), [Viewport rotation](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/BrowserDeviceToolbar.tsx#L153), [Mini-player fits source content](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L341), [Ferrite native viewport follows bounds](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/element.rs#L525).

## BP-10. Annotation overlay and movable annotation editor

Render path. The docked annotation control starts the installed guest preload overlay when a usable native page and bridge exist. PreviewView separately coordinates voice and restores prior host focus. A failed page disables the entry.

Visual composition. PickPreload attaches a fixed full-viewport host with a closed shadow root. It paints outlines, regions, and freehand SVG above the webpage. A compact tool strip is fixed near the top center. Selecting targets opens a bounded note editor placed beside the target using candidate positions and viewport clamping. Expanded editing enables a drag handle and style controls. These floating controls live inside the guest, not in the host Thread details card.

State and handoff. Capture-phase page handlers keep annotation input from activating page content. Escape cancels. Plain Enter in the note attaches, Command/Control-Enter requests direct send when allowed, and Shift-Enter remains text. A successful accepted pick outlives later clicks/unmount for draft insertion; screenshot failure retains the note with visible feedback.

Native evidence. Select/Region/Draw/Erase controls were exposed. Selecting h1, writing a note, attaching with Enter, expanding its composer chip, and sending produced real Sonnet receipt of metadata and screenshot.

Native limit. Editor drag/expand geometry, region/draw/erase execution, edge collision, cancellation, voice, direct-send shortcut, screenshot failure, and transition interruption were not certified.

Ferrite counterpart. Ferrite browser annotation first adds Region/Select element/Cancel controls and an occluding selection layer above the WebView. After capture, Phase::Editing replaces the browser body with AnnotationEditor. It does not use T3 guest-local floating note/style-editor composition. Its editor already has drawing tools, comment text, undo/redo, and retained capture evidence.

Current audit coverage. Original browser report described annotation as supported Ferrite behavior. BROWSER-14 concerns direct export, not absence of annotation. The original tally does not independently cover overlay placement, Editor-phase body replacement, or annotation keyboard/geometry contracts.

Omitted or unverified contracts follow.

- Compare selection chrome, target visibility, editor placement, viewport consumption, cancel/return, and keyboard ownership as explicit visual contracts.
- Verify overlay input cannot click the underlying website or send the unrelated chat draft.
- Exercise the same target/note/screenshot through draft inspection and actual provider receipt, including cancel and failed capture.

Source anchors. [Annotation entry and coordination](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L771), [Focus restoration after pick](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L793), [Guest-local fixed annotation overlay](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/PickPreload.ts#L564), [Top-center tool strip and note editor](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/PickPreload.ts#L604), [Editor clamping and target-adjacent placement](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/PickPreload.ts#L1260), [Expanded editor and dragging](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/PickPreload.ts#L1335), [Annotation keyboard ownership](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/PickPreload.ts#L1657), [Attach versus direct-send Enter policy](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/AnnotationKeyboard.ts#L11), [Accepted annotation draft and image](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L828), [Ferrite selection/editor body phases](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser/annotations.rs#L556), [Ferrite occluding selection overlay](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser/annotations.rs#L629), [Ferrite full annotation editor](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/annotation_editor.rs#L205).

## BP-11. Annotation detail cards and expanded image preview

Render path. An attached annotation becomes a composer context chip with an expandable detail view and optional screenshot. Sent context uses its own detail card. Clicking its image can open the shared ExpandedImageDialogHost mounted by ChatView.

Visual composition. Composer and sent cards show page identity, note, target counts, selected element details, and a bounded image crop. The expanded image is a modal media viewer with close, zoom/pan, and gallery navigation when applicable. This image dialog is distinct from browser presentation, separate-window frame mirroring, and the live annotation editor.

State and handoff. The annotation record and screenshot attachment are related but separate payloads. Missing screenshots receive explicit text. Expanding and closing media must return to the same draft/history state.

Native evidence. The annotation chip detail card showed title, note, and element count. A prior ordinary composer attachment preview opened a centered image at 100% and Escape preserved the draft. Real annotation send and screenshot receipt were observed.

Native limit. The ordinary attachment preview is not proof that the annotation screenshot itself was expanded. Gallery, zoom/pan, failed image loading, and modal coexistence with an active mini-player were not exercised.

Ferrite counterpart. Ferrite retained BrowserAnnotation previews render capture URL, comment, and annotation stage through the Reader card. Ordinary image links/attachments open the whole-window lightbox, which has filename, zoom, Save a copy, Open in app, close, and gallery navigation. The browser annotation editor and retained review stage are distinct from that lightbox.

Current audit coverage. Attachment and quote reports cover some modal/draft behavior. Original browser.md did not fully map composer detail, sent detail, retained annotation review, and lightbox as separate presentations.

Omitted or unverified contracts follow.

- Identify exactly which image was expanded. Do not count an ordinary attachment as annotation-preview coverage.
- Verify detail expansion, image modal, Escape, remove, send, and reopen preserve note/image association and draft contents.
- Keep the modal above local webview and mini-player input layers and restore focus without transcript jump.

Source anchors. [Composer annotation context detail](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/composerContextPresentation.tsx#L442), [Sent annotation card and image expansion](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/UserMessageContextChips.tsx#L142), [Expanded media modal](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ExpandedImageDialog.tsx#L161), [Thread-scoped expanded image host](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ExpandedImageDialog.tsx#L329), [Ferrite retained annotation review](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/preview.rs#L384), [Ferrite whole-window lightbox](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/lightbox.rs#L27), [Ferrite media viewer controls](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/lightbox.rs#L60).

## BP-12. Direct screenshot export and recording feedback

Render path. The browser capture button is available on supported native tabs. Plain click captures a still image; Shift-click follows a distinct recording path. The mini-player shell mirrors recording status but does not add the full docked capture toolbar.

Visual composition. A still capture writes a PNG artifact and produces a toast with Copy image, Copy path, and Reveal in Finder actions. This artifact is page content, not app chrome. Recording adds an active camera/indicator state and separate start/stop/export handling. The two output types cannot be verified by the same screenshot result.

State and handoff. The saved artifact has its own durable path. Capturing does not imply attaching to the composer. Annotation capture is a separate operation with different crop and context ownership.

Native evidence. A direct screenshot was saved, its path was copied and pasted, and the exported PNG was independently retained and inspected as browser content only.

Native limit. Copy image and Reveal were visible but not exercised. Recording start/stop, active-state continuity, playback, and export failure were not verified.

Ferrite counterpart. Ferrite Browser toolbar enters annotation-oriented capture. Retained captures and its lightbox already support later image viewing/export actions, but the inspected browser chrome has no direct plain screenshot or recording command. Existing screenshot-capable agent operations are not equivalent operator entry points.

Current audit coverage. BROWSER-14 owns direct export/recording capability. It does not fully map capture feedback placement, ordinary versus annotation output, or recording across presentations.

Omitted or unverified contracts follow.

- Verify a direct screenshot without entering annotation, its exact artifact path, clipboard image data, and recovery from failure.
- Treat recording as an independent flow with visible active state, stop, playable output, and cleanup across dock/float/window transitions.

Source anchors. [Capture and recording button state](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewChromeRow.tsx#L239), [Separate recording versus still path](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L533), [PNG capture and artifact ownership](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L2587), [Saved screenshot toast actions](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L735), [Floating recording indicator](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L556), [Ferrite annotation-only toolbar entry](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1452), [Ferrite existing Save a copy route](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/lightbox.rs#L107).

## BP-13. Empty, unavailable, and failed page presentations

Render path. PreviewPanel first checks preview access and availability. PreviewView then distinguishes an empty destination, active local or streamed page, missing bridge, load failure, and downloaded content that cannot render.

Visual composition. T3 empty content offers URL guidance and, when present, recent destinations and discovered local servers. An unauthorized or unavailable preview gets a centered explanation. A LoadFailed overlay replaces meaningful page presentation with friendly text, expandable diagnostics, and Reload. File/download failures have a separate not-shown state. Page-only tools are disabled when content did not render.

State and handoff. Back/Reload/URL entry remain recovery routes. Empty/new-tab state is not equivalent to leaving the old page as an active toolbar target. Error rendering and resource lifetime remain separate concerns.

Native evidence. Refused connection showed themed message, ERR_CONNECTION_REFUSED, Details, and Reload. Back recovered the fixture. Annotation, capture, and float were disabled.

Native limit. Empty recent/local-server variants, denied access, unavailable bridge, download-not-shown, TLS, profile failure, browser crash, and Reload recovery were not exercised.

Ferrite counterpart. Ferrite has no-thread and address-entry guidance branches, a blank new-tab body, a CEF-disabled explanation in headless builds, and raw error text appended above the page. Its annotations can remain eligible on a non-loading errored page. BROWSER-07 traces old-page controls behind new-tab presentation.

Current audit coverage. BROWSER-07, BROWSER-12, and BROWSER-15 own parts of this. The original audit did not enumerate every branch and its full visual composition or floating error behavior.

Omitted or unverified contracts follow.

- Map every failure and empty state to one concise explanation and a reachable recovery action.
- Verify no hidden old page receives navigation actions from an empty new tab.
- Inspect error propagation while floated or mirrored separately from the docked error overlay.

Source anchors. [Capability/access fallback](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanel.tsx#L34), [Empty/failure derivation](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L237), [Empty guidance branch](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewEmptyState.tsx#L35), [Recent and local-server cards](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewEmptyState.tsx#L50), [Missing desktop bridge explanation](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L1074), [Download and page failure overlays](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L1114), [Failure content composition](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewUnreachable.tsx#L54), [Page-only action gating](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L962), [Ferrite initial guidance](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1332), [Ferrite selected body and new-tab filter](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1479), [Ferrite raw error row](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1533).

## BP-14. Loading, Find, zoom, cursor, and local feedback layers

Render path. PreviewChromeRow owns loading progress. PreviewView layers the empty state, ZoomIndicator, AgentBrowserCursor, active Find bar, and failure overlays around the content body. Each has a different condition.

Visual composition. The loading track is absolutely positioned and animated synthetically from 4% to 90% over a fixed 5.3 seconds. Find is an upper-right page-local overlay with query, count, previous/next, and close. Zoom is transient feedback. Agent cursor visibility excludes empty, failed, and active-recording states. These are not the Thread details card.

State and handoff. Find is bound to one runtime tab and resets on tab change. Enter/Shift-Enter cycle matches and Escape closes and returns page focus. Source progress and pulse animations respect reduced-motion branches; the native captures do not establish frame rate or exact timing.

Native evidence. Browser Find showed 1/60, Enter advanced to 2/60, and Escape closed it. Three delayed-refresh captures showed the old page retained under an advancing blue bar.

Native limit. Previous/zero-match/tab-switch Find, zoom feedback, agent cursor behavior, exact stable geometry, reduced motion, and intermediate panel animation were not verified.

Ferrite counterpart. Ferrite inserts a Loading text row, conditionally removes Annotate, and appends error text. Browser-native Find/zoom/cursor feedback controls are absent from the inspected toolbar. Existing Reader Find is a different dispatch path and cannot establish browser Find parity.

Current audit coverage. BROWSER-08 and BROWSER-10 cover loading geometry and browser utilities. motion-010 addresses feedback more generally. Original coverage did not map stacking among Find, cursor, device controls, annotation editor, and floating presentations.

Omitted or unverified contracts follow.

- Measure bounds and layer ownership during loading, Find, viewport resizing, errors, and annotation.
- Keep Find scoped to the active page and distinguish it from transcript Find and file-editor Find.
- Use timed native frames for motion claims. A moving synthetic activity bar is not network percentage or smoothness proof.

Source anchors. [Fixed loading layer](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewChromeRow.tsx#L292), [Installed loading animation](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/index.css#L2390), [Zoom/cursor/Find conditions](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L1092), [Tab-scoped Find subscription](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewFindBar.tsx#L26), [Find keyboard ownership](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewFindBar.tsx#L63), [Find overlay placement](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewFindBar.tsx#L83), [Ferrite loading row](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser.rs#L1522), [Ferrite Reader Find placement](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/panel/preview.rs#L434).

## BP-15. Link menus, browser menus, and website popups

Render path. Chat Markdown links use a custom app menu. Guest webpage context menus come from DesktopWindow. More is host browser chrome. Website window.open and target-blank destinations are handled by Manager. Native HTML select menus are another distinct page-owned popup type.

Visual composition. The chat menu exposes integrated opening, system opening, and Copy Link. The native webpage menu adds link-copy and editing roles without those Open actions. More exposes browser operations and nested appearance/zoom/clear menus. Manager classifies new-tab versus popup-window versus in-place routing. Native select controls do not share the website window-open handler.

State and handoff. Menu dismissal, right-click focus, target URL, hit region, and popup destinations require explicit checks. Right-clicking a native guest focuses that guest before editing roles execute. A T3 omission does not excuse Ferrite missing both Open destinations on webpage links.

Native evidence. Chat integrated opening, webpage Copy Link menu visibility, More visibility, same-tab navigation, target-blank, and scripted popup creating a correct Cobalt second.html tab were observed.

Native limit. Middle-click, popup OAuth opener, nested popup policy, clipboard menu actions, select dropdown geometry, menu collision, and keyboard selection were not fully exercised.

Ferrite counterpart. Ferrite clears CEF context menus, denies website new-window requests, and has no replacement browser More menu. It does composite external HTML select popup pixels into the bounded WebView. Therefore blocked window.open does not mean every select control is absent. The browser internal tab bar remains distinct from outer panel tabs.

Current audit coverage. BROWSER-01, BROWSER-03, BROWSER-10, BROWSER-16, and link-affordances-01 cover selected failures. Native source/reference behavior does not replace the user requirement for basic link actions.

Omitted or unverified contracts follow.

- Offer Open in integrated browser, Open in system browser, and Copy Link for both chat and webpage links.
- Test native menu focus, editing roles, pointer and keyboard dismissal, edge flipping, and exact copied/opened URL.
- Keep HTML select popup, website script popup, separate preview mirror, and in-app mini-player separate in the coverage matrix.

Source anchors. [Chat destination menu](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/externalLinkContextMenu.ts#L26), [Generic guest menu](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/window/DesktopWindow.ts#L567), [Context-menu guest focus](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/window/DesktopWindow.ts#L576), [Host browser More branches](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewMoreMenu.tsx#L90), [Website new-window destination policy](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L488), [Native popup and new-tab dispatch](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L1632), [Ferrite clears native context menu](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L589), [Ferrite denies website popup](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L384), [Ferrite composites HTML select popup](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L233).

## BP-16. Browser permission sheets and JavaScript dialogs

Render path. Native guest session permissions are handled by BrowserSession; local alert/confirm use Electron native UI. Remote ServerBrowserSurface has an app-rendered dialog branch, including Prompt. These are distinct from agent approval cards in the conversation.

Visual composition. Installed native permissions explicitly allow a bounded set, prompt for clipboard reads and external protocols, and deny other permission requests through the allowlist. Native permission sheets are parented to the preview host, default to deny, and allow only one concurrent question. The remote dialog card requires control to respond and can collect Prompt text. Local Electron Prompt instead throws an unsupported error.

State and handoff. Origin, native-sheet parenting, keyboard defaults, remembered clipboard decisions, control ownership, and dialog return value determine whether the operator can safely continue the website flow.

Native evidence. Native Alert displayed Amber alert and Enter dismissed it. Confirm Escape returned false. Prompt recheck on confirmed Amber left status unchanged and DevTools Console showed the unsupported error twice.

Native limit. Confirm OK, Prompt input/cancel, clipboard permission, external-protocol permission, camera/microphone/geolocation, remembered decisions, concurrent sheets, and remote dialogs were not exercised.

Ferrite counterpart. Ferrite NoDialogs suppresses alert/confirm/prompt and refuses external beforeunload navigation while reporting unfinished work. The inspected browser renderer supplies no native-origin permission sheet. Other CEF permission plumbing was not fully traced, so this is not a blanket claim that every permission is denied.

Current audit coverage. BROWSER-02 covers suppressed JavaScript dialogs. Original audit listed native permissions as unverified and did not map local versus streamed dialog presentation. Agent command approvals are a separate audit area.

Omitted or unverified contracts follow.

- Test positive and negative website dialog branches and distinguish local guest versus remote streamed behavior.
- Verify requesting origin, correct parent window, deny-by-default keyboard behavior, explicit consent, cancellation, and recovery.
- Do not copy the local T3 unsupported Prompt failure as a Ferrite requirement.

Source anchors. [Native permission allowlist](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/BrowserSession.ts#L22), [Native permission sheet serialization](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/BrowserSession.ts#L198), [Clipboard permission question](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/BrowserSession.ts#L309), [Native request/check policy](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/BrowserSession.ts#L322), [Remote Prompt-capable dialog card](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/ServerBrowserSurface.tsx#L856), [Ferrite JavaScript suppression](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L643), [Ferrite beforeunload handling](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/web/cef/handlers.rs#L664), [Installed native Electron dependency](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/package.json#L25), [Native Electron Prompt compatibility limit](https://github.com/electron/electron/blob/v44.10.1/lib/renderer/window-setup.ts#L17).

## BP-17. Cross-presentation focus, stacking, and motion

Render path. Browser slot placement follows host layout changes. Panel shells own collapse focus/inert state. Mini-player gesture controls and annotation controls own their separate pointer streams. Image dialogs, native sheets, DevTools, and native mirrors have separate modal or window lifecycles.

Visual composition. Docked native content defaults to z-index 30. Mini-player content uses 48 with its chrome at 49; the layout constant explicitly places it below dialogs. Host sheets, popovers, tooltip/menu portals, guest-local overlays, and native windows do not form one interchangeable overlay system. BrowserSurfaceSlot observes shell and sidebar movement so native content follows their rectangles.

State and handoff. Panel width animation is enabled for open/close and intentionally suppressed for direct resize/maximize changes. Closing returns focus before inert. Guest editing shortcuts and page-first forwarding differ from workspace shortcuts. No animation guarantee follows merely from a CSS transition or an endpoint screenshot.

Native evidence. Browser form input preserved the draft, Find closed, browser/sidebar toggles retained state, image Escape preserved a draft, and floating Close worked. Separate shell evidence observed left-sidebar interpolation only after temporarily selecting 400 ms and restoring 0 ms.

Native limit. No complete browser handoff focus trace, modal/mini-player collision, panel timing, reduced-motion run, rapid reversal, or IME composition test was performed.

Ferrite counterpart. Ferrite WebView raw event forwarding sits inside a Reader action context, which existing source findings identify as an input-dispatch risk. Its annotation selection layer has an explicit capture context. Its image lightbox has priority above floats and returns focus. Generic motion helpers elsewhere do not prove browser handoff motion.

Current audit coverage. input-focus-001, input-focus-005, input-focus-006, panel-tools-018, and motion findings cover related contracts. They do not establish every pairwise browser presentation combination.

Omitted or unverified contracts follow.

- Verify focused page, URL field, Find, annotation note, image modal, mini-player, and native window each consume only their intended keys.
- Test overlay ordering and click-through with active menu, sheet, dialog, annotation, mini-player, and full Workspace panel combinations.
- Use intermediate native captures for animations and rapid reversals; verify hidden native pages cannot intercept input.

Source anchors. [Presentation tracks moving panel shell](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/BrowserSurfaceSlot.tsx#L63), [Only open/close width transitions](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L78), [Collapse focus before inert](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L132), [Mini-player stacking contract](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/previewMiniPlayerLayout.ts#L14), [Gesture handle layer](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/ThreadPreviewMiniPlayer.tsx#L544), [Local guest key forwarding](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L1535), [Modal image layer](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ExpandedImageDialog.tsx#L161), [Ferrite lightbox priority](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/lightbox.rs#L27), [Ferrite annotation input context](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-cockpit/src/cockpit/browser/annotations.rs#L693), [Ferrite raw native input](https://github.com/josephwylie/ferrite/blob/62947f91d5cd6ee494aaae9ed09d794a853e5bab/crates/ferrite-pane/src/visual/element.rs#L270).

## Coexistence still needs independent evidence

- Different docked browser tab and in-app mini-player. Supported by independent roots and surface leases. Existence/coexistence observed; handoff and interactive state continuity not certified.
- Same native browser page in docked and floating positions. One presentation owner per runtime tab; do not describe two independently interactive copies. Handoff not fully certified.
- Thread details and browser mini-player. Separate roots with chat-canvas obstacle/overlap calculations. No complete overlap/avoidance lifecycle evidence in this worker.
- Docked/floating browser and separate native mirror. Mirror captures the source guest while separate window state owns its presentation. Not exercised.
- Image modal or permission sheet over local guest/mini-player. Different host/dialog/native window systems; local stacking constants express intended ordering. No full click-through/focus verification.
- Guest annotation overlay and host menus/modals. Guest-local shadow-root overlay plus host-side annotation lifecycle. Single successful h1 note/send observed; interruption and combined overlay cases not certified.

## What the original tally did and did not capture

Selected browser failures and capability candidates are covered by BROWSER-01 through BROWSER-16 plus input/link/panel/motion findings. That inventory was not a complete rendered-surface map.

The original [browser report](browser.md) only named floating preview and separate windows in its supported-capabilities paragraph. The [shell checklist](t3-live-shell-checklist.md) contains planned BROWSER-26 and BROWSER-27 lifecycles. The [live browser correspondence](t3-browser-live-source-match.md) accurately limits floating evidence to existence, coexistence, and Close. None of those statements is a completed mini-player or separate-window acceptance suite.

Known defects and capability candidates remain in the existing [finding inventory](../findings.md). This map identifies missing presentation and interaction contracts without manufacturing new runtime defects or silently changing the tally. The parent owns any resulting prioritization and native verification.

## Verification

Every source anchor was inspected against the exact installed fetch or the clean Ferrite source file. The JSON records source hashes and comparison identity. All 17 map rows have an explicit native evidence status and unverified contracts. The observations snapshot SHA-256 is `ee2739e94e1d8d6b0fa3c924debebf8b6228f2c96d969af347bf33c80eb9630b`.

Only `surface-reconciliation-browser.md` and `surface-reconciliation-browser.json` were written. No product source, previous report, fixture, settings, model session, tests, typecheck, or build was changed or run. Prove It Works shaped the separation of source branches, visible controls, bounded native observations, and missing complete journeys.

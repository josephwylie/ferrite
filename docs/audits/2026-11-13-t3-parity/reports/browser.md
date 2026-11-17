# Browser audit

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants every missing or broken browser interaction and the concrete reasons the browser presents poorly exposed before remediation. This report investigates the browser itself, including its page lifecycle, chrome, navigation, annotation tools, input boundary and recovery.

## Overview and method

Result: ISSUES. The issue inventory contains 16 findings: 2 P1, 13 P2, and 1 P3. This is a source investigation of Ferrite `62947f91d5cd6ee494aaae9ed09d794a853e5bab` and T3 Code `74573fc29b901bafbf8f1dbb2eb29606da1fa448`. Source files were read from the working repository and `/tmp/ferrite-t3-audit-source-20261113`. No application code, user data, configuration, branches, or dependencies were changed. No live UI, browser, test, or build was run by this worker. Source-proven means the implementation path is explicit; it does not assert reproduction in the installed binary. Each finding below supplies an observable acceptance check that also serves as a reproduction request.

The strongest defects are silent refusal of new-window links, automatic dismissal of website dialogs, overwriting address edits, and controls that act on the old page during new-tab entry. Concrete presentation problems include hostname-only labels, a blank empty page, and changes to both page height and address width while loading. The large T3 development-tool set is present in current source; it is not merely promised by the older comparison documents. Those differences remain product decisions rather than automatic mandates to clone T3.

## Key concepts and traced flows

- The service owns a Thread's `State`, ordered tabs, active ID, document generation, owner and protection leases. The cockpit owns the address Composer, native page map, temporary new-tab boolean and annotation session. `crates/ferrite-core/src/browser.rs:18-101`; `crates/ferrite-cockpit/src/cockpit/browser.rs:79-96`.
- Open browser attaches the Thread to the service, reselects its saved active tab, reveals the preview and upserts one Browser panel entry. An address submission becomes Open or Navigate depending on the active document and new-tab flag. `crates/ferrite-cockpit/src/cockpit/browser.rs:195-215,1304-1323`.
- A service-live tab creates a `WebView::browser` using its URL. CEF callbacks become PageState events with URL/loading/history/error/dirty data. The GPUI host resizes CEF to the painted bounds and forwards local mouse/wheel/key events. `crates/ferrite-cockpit/src/cockpit/browser.rs:650-775`; `crates/ferrite-pane/src/visual/element.rs:248-287,525-535,597-689`.
- CEF permits HTTP(S) navigation for external pages, while retaining strict isolation for sealed visualization content. Popup, context-menu, and JavaScript-dialog restrictions remain unconditional. `crates/ferrite-pane/src/visual/web/cef/handlers.rs:384-450,589-679`.
- Every manual gesture pauses peer agent control immediately. Pointer/key holds and 350 ms quiet settlement distinguish active intervention from finished interaction. Dirty, loading, visible and protected tabs are excluded from reclaim. `crates/ferrite-cockpit/src/cockpit/browser/manual.rs:10,81-110,129-147`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1222-1255`.
- Annotation starts a protected region/element selection, retains capture evidence, then opens a drawing editor and conversation attachment workflow. Region and element selection are real controls, and the editor has pen, line, arrow, rectangle, ellipse, steps, text, undo and redo. `crates/ferrite-cockpit/src/cockpit/browser/annotations.rs:73-164,556-724`; `crates/ferrite-pane/src/annotation_editor.rs:16-35,414-426`.
- Native pages may be destroyed when clean and hidden. Open tab identity, URL, generation and selection persist independently of native page lifetime. Website cookies have a durable profile directory with an exclusive process lock. `crates/ferrite-core/src/browser.rs:635-785`; `crates/ferrite-pane/src/visual/web/cef/profile.rs:17-77`.

## Source map

| Concern | Ferrite | Current T3 |
| --- | --- | --- |
| Chrome and navigation | `crates/ferrite-cockpit/src/cockpit/browser.rs` | `apps/web/src/components/preview/PreviewChromeRow.tsx`, `PreviewView.tsx` |
| Service and persistence | `crates/ferrite-core/src/browser.rs`, `src/server/browser/disk.rs` | `apps/web/src/browserHistoryStore.ts`, `browser/desktopTabLifetime.ts` |
| Native page policy | `crates/ferrite-pane/src/visual/web/cef/handlers.rs` | `apps/desktop/src/preview/Manager.ts` |
| Bounds and input | `crates/ferrite-pane/src/visual/element.rs`, `web/cef/input.rs` | `apps/web/src/browser/BrowserSurfaceSlot.tsx`, `BrowserDeviceToolbar.tsx` |
| Annotation | `crates/ferrite-cockpit/src/cockpit/browser/annotations.rs`, `crates/ferrite-pane/src/annotation_editor.rs` | `apps/web/src/components/preview/PreviewView.tsx`, `PreviewAnnotationSpeech.tsx` |
| Profiles | `crates/ferrite-pane/src/visual/web/cef/profile.rs` | `apps/web/src/components/settings/IntegrationsSettings.tsx`, `BrowserImportWizard.tsx` |

## Detailed findings

### BROWSER-01: New-window links and scripted popups are silently discarded

Priority P1. Category `defect`. Evidence `source-proven`.

Symptom and impact: Ordinary website links that request another tab do nothing. Popup-based authentication cannot complete through this handler.

Trigger: Click an HTTP link with target=_blank, middle-click a link, or activate a script that calls window.open.

Expected behavior: Route allowed destinations into a new tab, preserve a legitimate popup opener when needed, or show a visible explanation and escape route.

Actual behavior: The native client refuses every popup and open-URL-from-tab request without inspecting the URL or user gesture.

Cause: The same sealed-content client is also used for external websites. ViewLifeSpan::on_before_popup and ViewRequests::on_open_urlfrom_tab return 1 unconditionally; neither records an error or sends a tab-open event.

Ferrite evidence: `crates/ferrite-pane/src/visual/web/cef/handlers.rs:151-165`; `crates/ferrite-pane/src/visual/web/cef/handlers.rs:384-401`; `crates/ferrite-pane/src/visual/web/cef/handlers.rs:442-450`.

T3 evidence: [apps/desktop/src/preview/Manager.ts:488-504](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L488-L504); [apps/desktop/src/preview/Manager.ts:1637-1658](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L1637-L1658).

Recommendation: Separate external website navigation policy from sealed visualization policy. Dispatch ordinary new-tab requests through the service and explicitly support or explain authentication popups.

Observable acceptance check: In one local fixture, same-tab navigation, target=_blank, middle-click, and a scripted popup each produce the documented destination or a visible actionable refusal. The opener-based fixture can report a successful callback.

### BROWSER-02: Website JavaScript dialogs are always dismissed

Priority P1. Category `defect`. Evidence `source-proven`.

Symptom and impact: An action gated by confirm cannot receive affirmative consent; prompt cannot collect input, and alert messages never reach the operator.

Trigger: Use a website action implemented with confirm(), prompt(), or alert().

Expected behavior: External pages should expose an operator-controlled dialog with the correct return value and origin.

Actual behavior: NoDialogs sets suppress_message=1 for every JavaScript dialog; its comment explicitly specifies dismissal.

Cause: The client installs NoDialogs for both external browser pages and sealed visualizations. There is no external-page exception in on_jsdialog.

Ferrite evidence: `crates/ferrite-pane/src/visual/web/cef/handlers.rs:151-165`; `crates/ferrite-pane/src/visual/web/cef/handlers.rs:643-661`.

T3 comparison: no affirmative T3 parity claim is needed for this defect or limitation. This finding follows Ferrite's explicit implementation.

Recommendation: Keep sealed visualization restrictions, but supply a native or GPUI dialog bridge for external pages, with a deliberate suppression state when appropriate.

Observable acceptance check: A local confirm fixture executes its positive branch after the user chooses OK; Cancel returns false. Prompt returns entered text, and alert is visible. No website can replace the app window.

### BROWSER-03: Right-click has no browser context menu

Priority P2. Category `interaction`. Evidence `source-proven`.

Symptom and impact: Operators cannot discover normal link, image, selection, and editable-field actions through right-click.

Trigger: Right-click a link, selected text, image, or input in the browser.

Expected behavior: Offer supported context actions, including copying a link and normal editing actions, with unavailable actions omitted.

Actual behavior: The native menu model is cleared on every context-menu request. There is no replacement in the WebView mouse path.

Cause: NoContextMenu::on_before_context_menu unconditionally clears the CEF menu; the GPUI host forwards right-button events but does not build a menu.

Ferrite evidence: `crates/ferrite-pane/src/visual/web/cef/handlers.rs:589-603`; `crates/ferrite-pane/src/visual/element.rs:643-689`; `crates/ferrite-pane/src/visual/element.rs:713-719`.

T3 comparison: no affirmative T3 parity claim is needed for this defect or limitation. This finding follows Ferrite's explicit implementation.

Recommendation: Introduce a browser-specific context menu using native hit-test details. Do not apply arbitrary website menus to sealed content.

Observable acceptance check: Right-click a link and copy its destination; right-click selected page text and copy it; paste into an input through the menu. Each action operates on the clicked page.

### BROWSER-04: Browser tabs discard page titles and show only hostnames

Priority P2. Category `interaction`. Evidence `source-proven`.

Symptom and impact: Two pages on the same host, including two localhost apps on different ports, have indistinguishable tab labels. Favicons are absent.

Trigger: Open two distinct pages on the same host or two localhost ports.

Expected behavior: Use the document title with a useful URL fallback and a stable site icon; expose the full address when needed.

Actual behavior: The renderer uses Url::host_str, which discards path and port, for every title. Core Tab and PageState have no title or favicon field although native BrowserState receives a title.

Cause: Title metadata stops at the CEF-to-cockpit state conversion. The tab renderer independently derives a hostname.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/browser.rs:1355-1388`; `crates/ferrite-cockpit/src/cockpit/browser.rs:744-763`; `crates/ferrite-core/src/browser.rs:44-56`; `crates/ferrite-pane/src/visual/web/mod.rs:69-77`; `crates/ferrite-pane/src/visual/web/cef/handlers.rs:320-321`.

T3 evidence: [apps/web/src/components/RightPanelTabs.tsx:599-619](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L599-L619); [apps/web/src/components/RightPanelTabs.tsx:631-641](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L631-L641).

Recommendation: Carry page identity metadata through service state and render title, favicon, and a full-address tooltip. Keep port in the fallback.

Observable acceptance check: Tabs titled Docs and Dashboard on one origin remain distinguishable; two untitled localhost ports show distinct fallback labels. Title changes update the correct tab.

### BROWSER-05: Address entry rejects normal schemeless hostnames

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact: Common development addresses such as localhost:3000 require manual http:// entry.

Trigger: Enter localhost:3000 or example.com in the address field and submit.

Expected behavior: Normalize a schemeless loopback host to HTTP and a public hostname to HTTPS before applying the strict network policy.

Actual behavior: browser_go trims the input and sends it unchanged. validate_url accepts only an already parsed HTTP or HTTPS URL with a host.

Cause: The UI exposes strict service validation directly and has no address normalization stage.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/browser.rs:1304-1323`; `crates/ferrite-core/src/browser.rs:817-829`.

T3 evidence: [packages/shared/src/preview.ts:183-218](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/preview.ts#L183-L218); [apps/web/src/components/preview/PreviewView.tsx:310-327](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L310-L327).

Recommendation: Normalize human address input separately from the service authorization policy. Do not weaken scheme restrictions or silently turn arbitrary text into a network destination.

Observable acceptance check: localhost:3000 opens http://localhost:3000; example.com opens https://example.com. Explicit unsupported schemes still produce clear errors.

### BROWSER-06: Page navigation can overwrite an unfinished address edit

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact: A redirect or same-page URL change can replace the address the user is currently typing.

Trigger: Focus the address field, type a replacement, then let the current page change its URL before submission.

Expected behavior: Keep an edit draft while the address field is focused and resolve it only on submit or cancel.

Actual behavior: The browser pump sets the Composer text to tab.url whenever the observed tab identity or URL differs. It does not inspect address focus or edit state.

Cause: address_shown is a last-synchronized URL, not a separate operator draft. Native navigation updates and text entry share the same mutable field.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/browser.rs:667-672`; `crates/ferrite-cockpit/src/cockpit/browser.rs:180-186`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1304-1323`.

T3 evidence: [apps/web/src/components/preview/PreviewChromeRow.tsx:79-88](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L79-L88); [apps/web/src/components/preview/PreviewChromeRow.tsx:157-180](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L157-L180).

Recommendation: Introduce explicit address editing state. Show committed URL while idle; preserve and validate the draft while editing; Escape should restore the committed URL.

Observable acceptance check: Type a new URL during a delayed redirect and a history.pushState update. The draft survives both. Escape restores the final page URL, and Enter navigates to the draft.

### BROWSER-07: New-tab mode leaves navigation controls pointed at the hidden old page

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact: Clicking Reload or Back while an empty new tab is shown unexpectedly acts on the previous tab and abandons new-tab entry.

Trigger: Open a page with history, click the new-tab plus button, then click Reload, Back, or Forward.

Expected behavior: An empty new tab should have no old-page navigation target. Unsupported controls should be disabled.

Actual behavior: new_tab hides the old body but leaves state.active unchanged. Toolbar enablement and document references use active without filtering new_tab. Successful browser_action resets new_tab=false.

Cause: New-tab presentation is a boolean laid over the old active document rather than a distinct active browser destination.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/browser.rs:275-276`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1407-1445`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1479-1481`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1505-1515`.

T3 comparison: no affirmative T3 parity claim is needed for this defect or limitation. This finding follows Ferrite's explicit implementation.

Recommendation: Model the displayed target explicitly or consistently derive toolbar state from the displayed page. Keep draft new-tab state independent of the previous document.

Observable acceptance check: After clicking plus, Back, Forward, and Reload cannot affect the old page. The address draft remains until explicit submission, cancel, or tab selection.

### BROWSER-08: Loading changes browser geometry and shifts the address field

Priority P2. Category `interaction`. Evidence `source-proven`.

Symptom and impact: Loading moves the page and changes its actual viewport size. The Annotate button also appears and disappears, changing address width.

Trigger: Reload or navigate repeatedly, especially in a narrow right panel.

Expected behavior: Keep browser chrome height and primary control positions stable; indicate loading within reserved space.

Actual behavior: A Loading text row is conditionally inserted above the page. Annotate is conditionally added only when loading is false. WebView then resizes the native browser to the new bounds.

Cause: Progress and availability are represented by structural insertion/removal rather than fixed-size status presentation and disabled controls.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/browser.rs:1452-1466`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1521-1530`; `crates/ferrite-pane/src/visual/element.rs:525-532`.

T3 evidence: [apps/web/src/components/preview/PreviewChromeRow.tsx:94-95](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L94-L95); [apps/web/src/components/preview/PreviewChromeRow.tsx:132-147](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L132-L147); [apps/web/src/components/preview/PreviewChromeRow.tsx:295-300](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L295-L300).

Recommendation: Reserve one compact navigation row; keep annotation visible but disabled during loading and place progress in an overlay or fixed-height track.

Observable acceptance check: Record the page origin, page viewport size, and address bounds before and during a slow navigation. All remain stable, and loading remains visible. Narrow-width controls stay operable.

### BROWSER-09: Viewport size is inseparable from panel size

Priority P2. Category `missing-capability`. Evidence `source-proven`.

Symptom and impact: The operator cannot verify a site at a precise mobile or desktop viewport, rotate a preset, or compare a large layout while keeping the chat readable.

Trigger: Try to preview the same app at a fixed 390 by 844 viewport or another reproducible breakpoint.

Expected behavior: Offer a fill mode plus explicit viewport dimensions or presets, with scaling independent of the right-panel width.

Actual behavior: The browser always adopts the GPUI bounds. Browser chrome exposes no viewport or page zoom control.

Cause: BrowserViewport carries physical host dimensions and device scale, but no persisted user-selected layout size. The UI action set has navigation only.

Ferrite evidence: `crates/ferrite-pane/src/visual/element.rs:504-505`; `crates/ferrite-pane/src/visual/element.rs:525-532`; `crates/ferrite-core/src/browser.rs:77-101`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1406-1478`.

T3 evidence: [apps/web/src/browser/BrowserDeviceToolbar.tsx:29-84](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/browser/BrowserDeviceToolbar.tsx#L29-L84); [apps/web/src/components/preview/PreviewView.tsx:1049-1054](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L1049-L1054); [apps/web/src/components/preview/PreviewMoreMenu.tsx:112-173](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewMoreMenu.tsx#L112-L173).

Recommendation: Add a user-selected viewport mode distinct from host size and device pixel ratio, plus explicit page zoom. Validate dimensions and preserve selection per browser tab.

Observable acceptance check: A 390 by 844 preset reports those CSS dimensions in the page while the right panel is resized; rotate swaps dimensions; Fill restores host-sized layout. Zoom has a visible reset.

### BROWSER-10: Find, DevTools, hard reload, and page appearance controls are absent

Priority P2. Category `missing-capability`. Evidence `source-proven`.

Symptom and impact: Long-page search and normal web development inspection require leaving the embedded browser. Cache bypass and theme checks have no operator controls.

Trigger: Try to find text in a page, inspect a DOM or network issue, force a cache-bypassing reload, or test prefers-color-scheme.

Expected behavior: Expose supported development tools through a discoverable menu and route shortcuts to the active page.

Actual behavior: Browser Action and BrowserView support Open/Select/Close/Navigate/Back/Forward/Reload only. The toolbar has no more-menu or find UI. Agent observation is not a user DevTools UI.

Cause: The browser was modeled around service-owned navigation and agent capture, without separate operator page tooling.

Ferrite evidence: `crates/ferrite-core/src/browser.rs:77-101`; `crates/ferrite-pane/src/visual/web/mod.rs:82-103`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1406-1478`.

T3 evidence: [apps/web/src/components/preview/PreviewMoreMenu.tsx:90-128](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewMoreMenu.tsx#L90-L128); [apps/web/src/components/preview/PreviewView.tsx:494-532](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L494-L532); [apps/web/src/components/preview/PreviewFindBar.tsx:1-40](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewFindBar.tsx#L1-L40).

Recommendation: Add typed page operations and a browser menu. Keep find scoped to the website rather than transcript search; surface supported runtime distinctions.

Observable acceptance check: Find highlights matches and cycles next/previous in the current page; DevTools targets the correct tab; hard reload bypasses a fixture cache; light/dark setting updates a media-query fixture.

### BROWSER-11: Inactive-page reclamation restores only a URL, losing navigation and page position

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact: A clean tab can lose its Back/Forward stack, scroll position, and transient UI state merely because it was hidden. Restart also restores only URLs and selection.

Trigger: Navigate A to B, scroll B, switch to another tab or thread until reclamation completes, then return.

Expected behavior: Routine tab switching should preserve reading position and useful navigation state, or make suspension and its limits explicit.

Actual behavior: Eligible hidden clean pages are released immediately after state checks. try_release closes the native browser. Restoration opens a fresh browser using tab.url; saved state contains only IDs, URLs, generations and selection.

Cause: Reclamation equates clean form state with disposable browsing state. No browser history or scroll snapshot travels through the restoration model.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/browser.rs:1222-1259`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1284-1294`; `crates/ferrite-cockpit/src/cockpit/browser.rs:708-715`; `crates/ferrite-pane/src/visual/web/cef.rs:1397-1433`; `crates/ferrite-core/src/browser.rs:728-785`.

T3 comparison: no affirmative T3 parity claim is needed for this defect or limitation. This finding follows Ferrite's explicit implementation.

Recommendation: Preserve native tabs for normal switching or retain a navigation/session snapshot before reclaiming. Define the resource policy explicitly instead of immediate hidden-state disposal. Do not claim full T3 session restoration without checking its host lifetime.

Observable acceptance check: Visit A then B, scroll B, switch away and back through actual reclamation, and verify Back returns to A and B restores its scroll. Repeat after restart according to the documented persistence contract.

### BROWSER-12: An empty browser provides neither useful guidance nor recent/local destinations

Priority P2. Category `missing-capability`. Evidence `source-proven`.

Symptom and impact: Opening the browser or a new tab leaves the user to recover a URL manually from terminal output or memory.

Trigger: Open a browser with no tabs, close the last tab, or click the plus button.

Expected behavior: Show concise address guidance and useful recent or running development-server destinations.

Actual behavior: The initialized empty browser renders its chrome and an empty body. The textual entry hint is only in the missing-browser-map branch, which open_browser avoids by calling ensure_browser. There is no recent-URL or server list in browser_body.

Cause: The empty state is an absence of a live page rather than a designed browser state. Persisted open tabs do not supply a visit-history model.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/browser.rs:195-196`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1338-1342`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1479-1545`; `crates/ferrite-cockpit/src/cockpit/browser/annotations.rs:563-570`; `crates/ferrite-core/src/browser.rs:728-745`.

T3 evidence: [apps/web/src/components/preview/PreviewEmptyState.tsx:29-45](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewEmptyState.tsx#L29-L45); [apps/web/src/components/preview/PreviewEmptyState.tsx:50-89](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewEmptyState.tsx#L50-L89).

Recommendation: Render an actual empty/new-tab page with address guidance, scoped recent destinations, and discovered local servers. Reuse existing terminal/server discovery if available rather than introducing another scanner.

Observable acceptance check: First open and last-tab close show guidance. Starting a local development server yields a clearly labeled destination, and a recently visited URL can be reopened and removed.

### BROWSER-13: Persistent website profiles have no operator management controls

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact: Cookies survive sessions, but the operator cannot clear broken authentication/cache or choose isolated browser identities from the browser UI.

Trigger: Try to clear site cookies/cache, switch work/personal identities, or use a temporary browser session.

Expected behavior: Expose the active profile and supported cleanup or isolation actions with an explicit scope.

Actual behavior: Ferrite allocates one durable websites directory under an environment-selected profile root. Browser toolbar and action types have no profile, cookie, or cache operation.

Cause: Profile lifecycle is process configuration, not a browser-domain choice exposed to the operator.

Ferrite evidence: `crates/ferrite-pane/src/visual/web/cef/profile.rs:17-28`; `crates/ferrite-pane/src/visual/web/cef/profile.rs:55-77`; `crates/ferrite-core/src/browser.rs:77-101`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1406-1478`.

T3 evidence: [apps/web/src/components/preview/PreviewMoreMenu.tsx:175-182](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewMoreMenu.tsx#L175-L182); [apps/web/src/components/preview/PreviewView.tsx:521-528](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L521-L528); [apps/web/src/components/settings/IntegrationsSettings.tsx:1111-1185](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/IntegrationsSettings.tsx#L1111-L1185); [apps/web/src/components/settings/BrowserImportWizard.tsx:464](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/BrowserImportWizard.tsx#L464).

Recommendation: First add visible active-profile identity and scoped clear-cookie/cache operations. Treat multi-profile, private sessions, and cookie import as separate product choices; do not import credentials without an explicit user action.

Observable acceptance check: A fixture cookie persists normally, clears through the named profile action, and stays isolated from another chosen profile if multi-profile support is adopted. The UI states which data will be cleared.

### BROWSER-14: Browser capture is annotation-oriented and lacks direct recording/export affordances

Priority P2. Category `missing-capability`. Evidence `source-proven`.

Symptom and impact: An operator cannot directly record a browser interaction or capture a plain screenshot through the browser chrome for external sharing.

Trigger: Try to capture the full viewport to a file/clipboard or record a short interaction without starting annotation.

Expected behavior: Provide a direct screenshot action and, if parity is desired, explicit recording start/stop with an observable output.

Actual behavior: The toolbar provides Annotate, whose capture workflow enters region/element selection and the annotation editor. It has no direct screenshot/export or video-recording action.

Cause: Capture is implemented as retained evidence for a conversation annotation rather than as an operator media tool.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/browser.rs:1452-1463`; `crates/ferrite-cockpit/src/cockpit/browser/annotations.rs:556-627`; `crates/ferrite-core/src/browser.rs:77-101`.

T3 evidence: [apps/web/src/components/preview/PreviewChromeRow.tsx:244-265](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L244-L265); [apps/web/src/components/preview/PreviewView.tsx:549-780](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L549-L780).

Recommendation: Reuse the existing capture pipeline for a plain screenshot command with output feedback. Decide recording scope separately; show active recording and its destination rather than hiding it behind a modifier-only gesture.

Observable acceptance check: A direct screenshot contains the current page at the chosen scale and can be opened from the reported destination. If recording is added, start/stop yields a playable file with a visible active-state indicator.

### BROWSER-15: Errors occupy an inline text row with repetitive recovery copy

Priority P3. Category `interaction`. Evidence `source-proven`.

Symptom and impact: Failure details compete with the page and can wrap through much of a narrow panel. The recovery action is described in text rather than placed beside the message.

Trigger: Navigate to an unreachable host, receive HTTP 404, or fail to attach the browser profile.

Expected behavior: Present an unmistakable failure state with a concise summary, local recovery controls, and expandable technical detail.

Actual behavior: The renderer appends the raw error plus a generic edit/reload sentence above the same page body. Native load errors already contain address/connection/reload instructions, duplicating advice. Annotate can remain eligible on a non-loading errored page if its native page exists.

Cause: Error states are strings added to the normal page layout rather than typed browser presentation states.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/browser.rs:1452-1453`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1533-1545`; `crates/ferrite-pane/src/visual/web/cef/handlers.rs:628-638`.

T3 evidence: [apps/web/src/components/preview/PreviewUnreachable.tsx:54-106](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewUnreachable.tsx#L54-L106).

Recommendation: Classify load, HTTP, engine, and profile errors. Give each a short summary and relevant action, with details behind disclosure. Keep layout stable and decide whether the failed page is meaningfully annotatable.

Observable acceptance check: Offline, refused connection, HTTP error, and unavailable engine each show one concise explanation and the correct retry/repair action at narrow panel widths, without repeated recovery sentences.

### BROWSER-16: No browser-chrome escape to the system browser

Priority P2. Category `missing-capability`. Evidence `source-proven`.

Symptom and impact: When a website needs unsupported authentication, downloads, permissions, or development tools, the operator must manually copy the address and leave the app.

Trigger: Try to open the current website in the default system browser.

Expected behavior: Offer a clearly labeled action for the current committed URL.

Actual behavior: The full browser toolbar contains no external-open command. Its actions are navigation, annotation, address submission, and tab creation/close.

Cause: The browser chrome lacks a normal recovery path for capabilities outside its embedded runtime.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/browser.rs:1406-1478`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1490-1519`.

T3 evidence: [apps/web/src/components/preview/PreviewChromeRow.tsx:193-213](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L193-L213); [apps/web/src/components/preview/PreviewView.tsx:437-440](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L437-L440).

Recommendation: Add Open in system browser for the committed page URL, with an error if launching fails. Do not send the unsubmitted address draft.

Observable acceptance check: The action opens the current page in the configured browser; an unfinished address draft does not change the launched destination.

## Supported T3 capabilities and comparison limits

Current T3 supports separate browser-tab titles and favicons, normalized addresses, an edit draft separate from committed URL, browser find, viewport presets/custom dimensions/rotation, zoom, appearance control, DevTools, hard reload, profile cleanup, recent URLs, local-server discovery, external-browser opening, screenshots, recording, floating preview and separate preview windows. These are source-backed controls and handlers, not runtime verification. See the pinned citations in BROWSER-01 and BROWSER-04 through BROWSER-16. Floating preview is wired in [PreviewChromeRow.tsx:269-290](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L269-L290), and the separate-window action is in [PreviewMoreMenu.tsx:104-109](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewMoreMenu.tsx#L104-L109). Panel placement differences belong to the panel-tools report.

T3 is not uniformly complete. Its refresh button has the accessible label Stop while loading, but PreviewView.handleRefresh still calls refresh, and desktop Manager.refresh reloads. Therefore this report does not claim T3 has a working Stop-loading capability. [PreviewChromeRow.tsx:138-147](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L138-L147), [PreviewView.tsx:332-335](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewView.tsx#L332-L335), [Manager.ts:2150](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L2150).

T3 also closes a desktop tab when its reference count reaches zero, after a zero-delay timer. Its broader mounting policy needs inspection before claiming it preserves every hidden or restarted browsing session. BROWSER-11 consequently makes no superiority claim. [desktopTabLifetime.ts:44-81](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/browser/desktopTabLifetime.ts#L44-L81).

## Positive Ferrite coverage

Ferrite already has real per-Thread tabs, back/forward/reload, explicit load errors, persisted open URLs and selection, a durable website cookie profile, region and element capture, a substantial drawing editor, retained annotation evidence, and coordination that pauses agent commands during manual interaction. The browser is not a static screenshot mockup. The state paths are cited above.

CEF native select dropdowns receive explicit off-screen compositing support, unlike the blocked new-window popups. `crates/ferrite-pane/src/visual/web/cef/handlers.rs:233-289`. The source smoke covers a composited dropdown at device scale 2 and independently checks a selected value through an HTTP fixture. `crates/ferrite-pane/src/visual/smoke/browser.rs:429-509`. Do not conflate an HTML select popup with window.open when reproducing BROWSER-01.

Protected/dirty page reclamation guards, stale-document generations, exclusive profile ownership and isolated smoke profiles are deliberate correctness work. They should remain while browser interaction is improved. `crates/ferrite-core/src/browser.rs:635-725`; `crates/ferrite-pane/src/visual/web/cef/profile.rs:17-28,55-77`; `crates/ferrite-cockpit/src/cockpit/browser.rs:740-743`.

## Motion and presentation

The browser has hover/press styling on its tabs, so absence of all motion would be an overclaim. `crates/ferrite-cockpit/src/cockpit/browser.rs:1373-1380`. The concrete problem in this slice is that loading changes layout, while progress is plain text, and tab/new-tab/error/annotation body replacement has no browser-local transition in the inspected render path. `crates/ferrite-cockpit/src/cockpit/browser.rs:1483-1545`; `crates/ferrite-cockpit/src/cockpit/browser/annotations.rs:563-577`. The motion owner should assess shared animation helpers and reduced-motion handling before classifying additional gaps.

T3 reserves a loading progress track and has a motion-safe recording pulse. The user's requested useful motion is independent of whether a particular T3 control animates. Stable geometry, a progress indicator, brief selection feedback and an interruptible annotation transition would address specific state changes; adding unrelated decorative animation would not resolve the identified defects. [PreviewChromeRow.tsx:244-300](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L244-L300).

## Verification gaps and reproduction requests

The parent attempted runtime inspection after this worker's source review. CUA exposed only the Ferrite window controls and app menu; screenshot capture timed out and its kernel reset. Subsequent CUA calls aborted before dispatch. The Ferrite browser tool opened the parent's local fixture as tab 52, generation 1, with loading=true and visible=false, then observation and wait requests failed with "owned browser document or operator revision changed before dispatch" while fresh state remained unchanged. These attempts do not reproduce a popup, dialog, address, or layout defect. The fixture is `docs/audits/2026-11-13-t3-parity/evidence/browser-fixture.html`. All 16 findings retain source-proven classification; the observed automation failure belongs to the parent's verification record.

The parent owns further runtime verification. Exercise the acceptance checks above in the installed binary and record build identity. Prioritize new-window links, JavaScript dialogs, stale new-tab controls and address-edit overwrite. A small local fixture should expose a normal link, target=_blank link, window.open button, confirm/prompt/alert controls, a delayed redirect, a history.pushState button, an input, long scroll content and two titled pages on one origin. This report did not create or launch that fixture.

Check native page lifetime by navigating A to B, scrolling, switching away and back, and observing the actual reclamation event. This settles the timing and perceived severity of BROWSER-11, although source already proves URL-only restoration. The existing smoke explicitly checks clean inactive reclaim/restore, but does not establish preserved navigation history or scroll. `crates/ferrite-cockpit/src/cockpit/browser_smoke.rs:97`; `crates/ferrite-pane/src/visual/smoke/browser.rs:235-247`.

Keyboard ownership and IME are assigned to input-focus. The browser host tracks a GPUI focus handle and forwards key events, but GPUI action dispatch precedes those callbacks. Do not infer that all host shortcuts are swallowed merely because the raw-key callback stops propagation. `crates/ferrite-pane/src/visual/element.rs:257-286`; `vendor/gpui-pre/src/window.rs:6152-6168`. The input-focus report owns the actual Enter, Tab, Escape, clipboard and composition consequences. Test them in a focused webpage and after clicking the toolbar/composer boundary.

Other browser cases remain unverified: file chooser/upload, file downloads, drag/drop, camera/microphone/geolocation permission prompts, TLS certificate errors, renderer crash recovery, mouse side buttons, native clipboard behavior, page accessibility exposure, zoom on multiple displays, and website beforeunload flows. The client installs no explicit crash recovery handler in the inspected request handler; this is an investigation gap, not a reproduced crash defect. Beforeunload is explicitly refused for external pages and reports unfinished work, with no browser-level discard decision. Verify a fixture whose page has no own discard control. `crates/ferrite-pane/src/visual/web/cef/handlers.rs:414-465,664-677`.

Narrow tests were not run because no application code changed and tests cannot establish click-to-handler reachability for this read-only audit. Existing source tests and smokes provide implementation coverage but do not resolve the installed UI claims. No measured performance claim, numeric visual quality score, or exhaustive runtime parity claim is made.

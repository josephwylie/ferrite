# Live T3 browser source correspondence

The user wants actual T3 browser behavior matched to implementation and complete Ferrite journeys. This report connects the parent's native observations to the exact installed source and defines acceptance against existing Ferrite findings.

Throughput checkpoint. Not applicable. This is a read-only source investigation with two report outputs.

## Evidence and revision boundary

The [native observations](../evidence/t3-live/observations.md) describe installed Nightly `0.1.10-nightly.20261111.4544`. The [provenance report](t3-installed-source-match.md) identifies its source as `1264c206aee0dce805d6279809118bc091f29d2f`. Every T3 source anchor below uses that commit. Checkout `74573fc29b901bafbf8f1dbb2eb29606da1fa448` is only a byte-comparison target. This worker did not operate either app or send model prompts.

The [machine-readable report](t3-browser-live-source-match.json) records source hashes, comparison results, an observation fingerprint, anchors, acceptance requirements, and boundaries. Native results below are parent-recorded. Source establishes the corresponding implementation path. These are separate evidence types.

This adds no Ferrite finding IDs and does not promote Ferrite source findings into runtime reproductions. Existing [browser](browser.md), [input and focus](input-focus.md), [link affordance](link-affordances.md), and [panel correction](panel-runtime-correction.md) reports retain their scoped evidence. The [consolidated inventory](../findings.md) owns final dispositions.

## Chat links and webpage links have different menus

Observed. Right-clicking a chat Markdown link exposed integrated-browser, system-browser, and copy actions. The integrated action opened the local fixture in the right panel. A webpage link exposed Copy Link plus editing and system-service entries. The toolbar independently exposed Open in system browser.

Source. [ChatMarkdown's handler](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatMarkdown.tsx#L1266) invokes a [custom link menu](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/externalLinkContextMenu.ts#L26) with explicit destinations. [DesktopWindow's generic menu](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/window/DesktopWindow.ts#L567) adds link copying and editing roles without either Open action. It [installs that menu on guests](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/window/DesktopWindow.ts#L656). The [toolbar external action](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewChromeRow.tsx#L199) is separate. This explains the two observed menus.

Ferrite mapping. `link-affordances-01`, `BROWSER-03`, and `BROWSER-16`.

Acceptance. Both transcript and webpage link menus expose Open in integrated browser, Open in system browser, and Copy Link. Each action uses the clicked link's resolved URL. Page editing actions operate on the clicked guest. T3's copy-only webpage menu describes the reference and does not excuse Ferrite's reported missing Open actions. The active-page external action launches the committed URL even when the address field contains an unfinished draft.

Limit. The live record proves integrated opening. System launching and menu copying were visible but not activated in this sequence. Ferrite `link-affordances-01` already has separate runtime evidence in the consolidated inventory.

## Website form input stays within the browser

Observed. An unsent chat sentinel survived typing into Name, Tab into Notes, multiline paste, Enter in Notes, and Save locally. The page reported Saved: AMBER_INPUT_TAB. No chat message was sent.

Source. Manager [classifies guest editing shortcuts](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L511), checks [actual guest focus](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L1504), and controls host menu shortcuts. Its [app forwarding branch](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L1535) excludes composing input and requires guest focus. [Page-first commands](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L1555) have a separate [unhandled-key fallback](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L1580). This policy corresponds to the observed separation of page input and composer submission.

Ferrite mapping. `input-focus-001` and `input-focus-006`.

Acceptance. Leave an unsent chat sentinel, then exercise text, Tab, Shift-Tab, multiline paste, Enter, Escape, copy, and paste in a local browser form. The page receives its editing actions and no provider send occurs. Exercise explicit app shortcuts separately. Verify IME with actual composed text before closing the composition finding.

Limit. The native sequence did not test every listed acceptance key, IME composition, or focused iframes. It does not close Ferrite findings.

## Find cycles real matches in the active page

Observed. Find reported 1/60 for Browser passage. Enter advanced to 2/60, and Escape dismissed the bar. Search stayed inside the browser.

Source. [PreviewView gates Find](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L357) on a live local desktop tab. PreviewFindBar [filters results by tab ID](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewFindBar.tsx#L27), retains partial Chromium counts, and [handles Enter and Escape locally](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewFindBar.tsx#L63). Manager [forwards every result update](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L1476) and [maps the query to Chromium](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L2148). Closing returns focus to the page.

Ferrite mapping. `BROWSER-10`.

Acceptance. Search a fixture with 60 repeated phrases and show the active ordinal and total. Enter and Shift-Enter cycle that page. Escape clears the search and returns browser focus. Switching tabs must not search the prior page or transcript. Cover empty queries and zero results.

Limit. Previous match, zero results, and tab switching were not observed.

## Navigation, close, and reopen preserve conversation ownership

Observed. Same-tab navigation updated page, URL, and history controls. Back restored the form values. A target-blank link opened a second tab. Command-W closed the active browser tab while retaining the parent thread and chat draft. Command-Shift-T restored its title and URL. In a later check on the confirmed Amber page, Scripted popup created a new Cobalt tab with the correct second.html URL. Command-W closed that extra tab.

Source. Manager [classifies new-window destinations](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L488) and [emits new-tab requests](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L1632). [Surface close releases the exact resource](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/hooks/useRightPanelSurfaceClose.ts#L108). [Successful preview close remembers a snapshot](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/closePreviewSession.ts#L22). [Reopen creates a session](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/reopenClosedView.ts#L104) using its URL, viewport, and profile. [Guest shortcut forwarding](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ReopenClosedViewShortcut.tsx#L207) makes reopening available from browser focus.

Ferrite mapping. `BROWSER-01`, `BROWSER-04`, `BROWSER-11`, and the cross-surface close contract in `input-focus-004`.

Acceptance. Same-tab navigation changes only that page and Back restores fixture state. Target-blank and a user-activated scripted popup open the correct destination with a useful title. Closing the focused tab preserves the conversation and draft. Reopen restores a usable destination and identifies which browsing state survives. Measure Ferrite reclamation separately before closing `BROWSER-11`.

Limit. Target-blank and the user-activated scripted popup were exercised. Middle-click, OAuth opener behavior, downloads, and POSTs remain unverified. The scripted popup success does not establish authentication callback compatibility. T3 reopen explicitly creates a new session. Restoring title and URL does not establish retained DOM, scroll, or complete history. Back restored form state before closure. `input-focus-004` concerns Ferrite PR and terminal close ownership, so this browser sequence is a regression requirement, not a reproduction of that finding.

## Hiding the panel differs from closing its browser tab

Observed. Command-Option-B hid and reopened the panel with its active browser tab and unsent draft retained. Command-B hid and restored the left sidebar while the composer had focus.

Source. [Panel close](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L1102) and [visibility toggle](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L1116) change `isOpen` without deleting surfaces or the active ID. PreviewPanelShell [releases focus and sets inert](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L132) while [retaining children](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewPanelShell.tsx#L206). This differs from explicit preview-session close.

Ferrite mapping. `panel-tools-018`, `input-focus-005`, and `BROWSER-11` have related acceptance boundaries.

Acceptance. Toggle a panel containing a browser and unsent draft, then restore the same active resource and draft. Hidden browser input cannot consume later typing. Keep operator width separate from temporary tool minimums. Overview to Files or Changes to Overview must recover the prior compact width when the operator did not resize it.

Limit. This live record establishes resource and draft retention. It does not measure animation, widths, reclamation, or restart. The separate Ferrite panel correction establishes sticky-width runtime evidence. T3's successful visibility toggle does not resolve that defect.

## Failed navigation has local recovery and page-tool gating

Observed. An unavailable local port produced a themed Connection refused page with ERR_CONNECTION_REFUSED, Details, and Reload. Back recovered the fixture. Annotation, capture, and floating controls were disabled on the failed page.

Source. [PreviewView's LoadFailed overlay](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L1123) passes error information and a reload handler into [PreviewUnreachable](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewUnreachable.tsx#L54). That component separates friendly text, details, and Reload. PreviewView gates [capture](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L962), [floating preview](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L968), and [annotation](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L973) on `isUnreachable`.

Ferrite mapping. `BROWSER-15`.

Acceptance. Refused connections show one concise explanation with expandable diagnostics and Reload. Back returns to the working page. Disable capture and annotation when no meaningful page rendered. Test offline, TLS failure, HTTP error, and engine/profile failure separately.

Limit. Only refused connection was exercised. Details and Reload were present. Back was the verified recovery. This does not prove every error class or Reload recovery.

## The advancing loading bar reports activity rather than network percentage

Observed. Three consecutive native captures during refresh of a delayed route retained the existing page beneath a narrow blue bar whose length advanced.

Source. [PreviewChromeRow positions the bar absolutely](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewChromeRow.tsx#L292) at its bottom edge. Installed CSS [defines its keyframes](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/index.css#L2390) and [animates scaleX from 0.04 to 0.9 over a fixed 5.3 seconds](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/index.css#L2408). This is indeterminate activity, not measured download completion. The [reduced-motion branch](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/index.css#L2413) removes the animation.

Ferrite mapping. `BROWSER-08`.

Acceptance. Record page origin, actual viewport size, and address bounds before and during a slow navigation. Keep geometry stable and retain meaningful content. Show loading without inserting a row or removing controls. Respect reduced motion and do not label synthetic activity as a completion percentage.

Limit. The captures establish advancing activity and retained content. They do not measure latency, frame rate, smoothness, or exact pixel stability. The wrapped Slow load anchor click was an automation limitation. Direct entry of its observed URL established navigation.

## Screenshot export creates a page artifact with recovery actions

Observed. Capture screenshot saved an artifact and exposed Copy path, Reveal in Finder, and Copy image. Copy path followed by paste exposed its exact path. The parent copied and inspected [browser-content-export.png](../evidence/t3-live/browser-content-export.png). It contains page content without app chrome.

Source. [Manager captures the guest](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L2587), converts the image to PNG, writes an artifact, and returns its path and metadata. [PreviewView requests the screenshot](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L640) and builds the [toast with image, path, and reveal actions](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L735). This differs from the annotation pipeline.

Ferrite mapping. `BROWSER-14`.

Acceptance. A direct screenshot action creates an openable PNG of the browser viewport without app chrome and reports the destination. Copy path pastes the saved path. Reveal selects the artifact. Copy image pastes actual image data. Visible failures preserve the working page.

Limit. Copy path and the exported file were verified. Reveal and Copy image were only visible controls. Recording was not exercised and needs separate verification of start, stop, and playable output.

## Element annotation reaches the real model with a note and screenshot

Observed. Annotation exposed Select, Region, Draw, and Erase. Selecting the heading identified an h1 and opened an editor. Enter attached the note to the existing draft without sending. The expanded chip showed page title, note, and element count. After Send, real Sonnet described the h1 selector, page URL, dimensions, font, and screenshot selection overlay. The composer cleared.

Source. PickPreload [captures element and page context](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/PickPreload.ts#L441), [defines the four tools](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/PickPreload.ts#L1244), and [snapshots the note and selected geometry](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/PickPreload.ts#L1708). Manager [attaches a cropped screenshot](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/src/preview/Manager.ts#L2316). [PreviewView inserts annotation context and an optional image](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewView.tsx#L828), then calls the immediate-send callback only for a send submission. [useComposerSend snapshots annotation context](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useComposerSend.ts#L743) alongside image attachments. Shared formatting serializes [element metadata](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/shared/src/composerContextReferences.ts#L278) and [page, URL, and comment context](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/shared/src/composerContextReferences.ts#L310) for provider expansion.

Ferrite mapping. `BROWSER-14` concerns separate direct export. Ferrite already has element and region capture, a drawing editor, and retained evidence. This successful T3 annotation baseline adds no missing-annotation finding.

Acceptance. Select an h1, add a note, attach it to an unrelated draft, inspect the context, and send once. An actual provider receives element identity, URL, dimensions, relevant style context, note, and screenshot. The composer clears only after successful submission. Cancel preserves the prior draft. Screenshot failure is visible and cannot imply an image reached the model.

Limit. Only element selection and one attachment/send were exercised. Region, Draw, Erase, style edits, voice, cancellation, and screenshot failure remain source-supported or acceptance-only.

## Viewport presets rotate independently of the panel

Observed. The toolbar exposed preset, width, height, ratio lock, rotation, and resize handles. iPhone 12 Pro set 390 by 844. Rotate changed the controls to 844 by 390. Closing the toolbar restored normal view.

Source. The [shared preset table](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/shared/src/previewViewport.ts#L36) defines those dimensions. BrowserDeviceToolbar [resolves the preset](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/BrowserDeviceToolbar.tsx#L145). Its [rotation handler](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/browser/BrowserDeviceToolbar.tsx#L153) swaps dimensions and inverts a locked aspect ratio.

Ferrite mapping. `BROWSER-09`.

Acceptance. Choose 390 by 844 independently of panel width. Rotate to 844 by 390 and verify the actual page viewport and a media-query fixture match the displayed controls. Keep chat readable, restore fill mode on close, and cover custom dimensions, bounds, ratio lock, and resize handles.

Limit. The live record verifies displayed dimensions and view restoration. It does not independently measure window.innerWidth, device identity, touch emulation, user agent, custom dimensions, or drag handles.

## Alert and confirm work while Prompt throws an observed unsupported error

Observed. Alert opened a native Amber alert sheet and Enter dismissed it. Confirm opened Cancel and OK. Escape dismissed it, and the page displayed Confirm: false. Rechecking Prompt on the verified Amber URL left that status unchanged. The parent opened DevTools for the confirmed Amber page and selected Console. Its native console displayed Uncaught Error: prompt() is not supported. The error was repeated 2 times. The DevTools windows were closed with Command-W.

Source. Installed T3 [pins Electron 44.10.1](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/desktop/package.json#L25). That exact [Electron implementation](https://github.com/electron/electron/blob/v44.10.1/lib/renderer/window-setup.ts#L17) replaces window.prompt with a function that throws an unsupported error before webview-specific setup. No prompt replacement or custom JavaScript-dialog bridge appeared in the bounded installed Manager, DesktopWindow, or PickPreload files inspected here. The [recorded fixture location](../evidence/t3-live/fixture.json) resolves to an inspected `index.html`. Its Prompt handler evaluates `prompt()` before assigning the status text. The observed unsupported exception prevents that assignment and explains the unchanged status. The native Console result now confirms the unsupported-prompt failure that the exact Electron source explained.

Ferrite mapping. `BROWSER-02`.

Acceptance. Ferrite alert shows the message. Confirm OK returns true, while Cancel or Escape returns false. Verify both branches. Prompt returns entered text and null on cancel, or the app states its deliberate compatibility limitation and offers a system-browser route. Do not copy T3's unsupported-prompt failure as desired parity.

Limit. The runtime exception is now observed on the confirmed Amber page. It establishes the unsupported-prompt failure, not a successful prompt return value. An earlier DevTools attempt targeted Cobalt during an unsettled tab action. The later separate-call URL verification and Amber Console check establish the correct target and do not support a targeting-defect finding. Native alert and confirm were actually observed, so do not group all T3 JavaScript dialogs as unsupported. Ferrite's dialog finding remains source-proven in this slice.

## Menu visibility and floating preview remain bounded

The live menu exposed Find, hard reload, DevTools, separate window, device toolbar, appearance, zoom, and cookie/cache controls. The [installed menu](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/preview/PreviewMoreMenu.tsx#L90) defines those branches. Visibility proves discoverability. Find, preset rotation, and opening DevTools on the confirmed Amber page have separate completed observations above. The native DevTools Console exposed the actual Prompt error. Cookie and cache deletion were not exercised. This run does not establish that hard reload, appearance, zoom, or separate-window actions completed.

The parent observed floating-preview coexistence with another panel tab and verified its dedicated close action. Activation involved a pointer-coordinate retry and is not certified. This is neither a confirmed activation failure nor a fully verified floating journey.

File chooser, downloads, web permissions, TLS failures, renderer recovery, authentication popups, beforeunload, and IME remain outside this bounded sequence. The 107-item [shell checklist](t3-live-shell-checklist.md) remains a coverage plan, not 107 completed live checks.

## Verification

All cited raw sources were fetched from their exact public revisions. The JSON records their SHA-256 values, anchor lines, and whether each T3 file matches the newer checkout. It also fingerprints the observations and exported PNG. The native screenshots outside that PNG remain conversation evidence.

Only these two reports were written. No app code, fixture, source checkout, settings, model session, test, typecheck, or build was changed or run. Prove It Works shaped the upgrade of Prompt from a source inference to an observed runtime failure only after the correctly targeted native Console exposed the error. Visible actions remain distinct from completed journeys.

# Link affordances audit

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants a detailed Ferrite versus T3 Code audit of broken everyday workflows. Their latest example is right-clicking a link and finding Copy without Open in browser. This report traces the actual link targets, menu verbs, pointer events and keyboard paths rather than inferring behavior from underline styling.

## Overview and method

Result: ISSUES. Seven additional P2 findings. All are source-proven implementation findings; no UI behavior was reproduced in this slice. The installed user's exact copy-only menu remains unidentified. At the pinned Ferrite source, an ordinary transcript web link has **two** rows, Open link and Copy link, but no system-browser choice. Do not erase that contradiction or claim this audit saw the user's menu.

Ferrite baseline and inspected HEAD: `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. T3 baseline and inspected HEAD: `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, from `/tmp/ferrite-t3-audit-source-20261113`. Method: read-only source and existing-test inspection, broad symbol searches, event-to-dispatch tracing and overlap checks against the other reports. No app changes, settings changes, browser launches, tests, typecheck or E2E were run. Report files are the only writes. Existing tests are cited as test intent, not as green results. The harness adapter, investigation instructions and Unslop writing instructions were read.

The user's belief about T3 is correct for app-rendered web links. T3's chat Markdown context menu explicitly offers Open in integrated browser, Open in system browser and Copy Link. T3's generic Electron context menu for a web page is narrower: it offers Copy Link, editing roles and image/email copying, without an Open item. Therefore, "T3 has Open everywhere" is not supported.

## Key concepts

A display label is not a destination. A destination can be a public URL, a repository issue reference, a file with a line number, an app thread, or a document fragment. The current Ferrite paths treat several of these as strings but resolve them in different handlers.

Plain activation, modifier activation, context-menu actions and clipboard actions must agree on the target. Choosing an external browser is different from requesting a new integrated tab or a background tab. Copying selected label text is different from copying an address. A callback accepting a keyboard event does not establish that a keyboard event can reach it.

## Traced flows and source map

| Link context | Ferrite path and source evidence | T3 path and evidence | Coverage outcome |
| --- | --- | --- | --- |
| Transcript HTTP(S), including bare URL links | `rich.rs:605-645` activates left/middle; `preview.rs:324-331` queues the URL; `cockpit/browser.rs:342-355` sends Action::Open with the pane's thread | ChatMarkdown `<a>` and context menu; browserLinkTarget chooses app/system | Findings 01 and 02 |
| Transcript right-click | `transcript.rs:1515-1520,1614-1627` intercepts right mouse-down; `message_menu.rs:278-282` reads pressed_link; rows and verbs at 132-139,192-203 | External-link menu has separate integrated/system/copy verbs | Findings 01 and 03 |
| File/path chip | `rich.rs:567-575` replaces link text; `inline_flow.rs:193-233` retains selection text but loses URL metadata; `attachments.rs:804-842` opens preview and supplies a keyboard button | MarkdownFileLink has editor/reveal/browser/path-copy menu and preserves location | Finding 04 |
| Generated issue reference | `file_links.rs:607-614` writes issue:; `rich.rs:617-625` resolves only the primary click | ChatMarkdown stores a real href and passes it into the menu | Finding 03 |
| Thread chip | `link_chip.rs:119-134` opens scoped thread through raw clickable Div | Context chips use anchors or native buttons, depending on kind | Finding 06; thread routing belongs to subagents-background |
| Citation/source chip | Existing report `transcript-006` owns lost source provenance | Source-message navigation already traced by transcript worker | Cross-reference only |
| Sidebar PR badge | `cockpit.rs:12806-12819` handles left-down, platform modifier opens externally; `nav_list.rs:339-344` owns right-down at the card | useOpenPrLink handles modified anchors/buttons; PR row and header menus have explicit URL identity | Finding 07; destination visibility and native PR routes stay in pr-navigation |
| Native PR header | `panel/pull_request.rs:516-533` is an underlined clickable Div; tooltip says Open on GitHub but open_pr opens integrated Browser | PullRequestDetailCrumbs has a button, explicit system open, middle-click and target menu | PR route differences belong to pr-navigation; raw-control accessibility belongs to input-focus-003 |
| PR body, guide and comments | `panel/pull_request.rs:727-733,752-755,832-836` append strings, not Markdown components | PullRequestMarkdown.tsx:116-127 delegates to ChatMarkdown | `pr-operations-009` owns noninteractive Markdown |
| PR checks | `panel/pull_request.rs:702-716` only name/icon rows | PullRequestChecksPopover.tsx:96-109 supplies Details using useOpenLink | `pr-operations-008` owns missing detail navigation. T3's button does not forward modifiers, so do not claim full gesture parity here |
| Browser website links | CEF NoContextMenu clears native menu; popup/new-tab handlers suppress requests | Generic Electron menu supports Copy Link but no Open item | `BROWSER-03` and `BROWSER-01`, no duplicate findings |
| Browser address/chrome | `cockpit/browser.rs:1410-1545` provides navigation/address/new-tab/annotation controls, no external-open control | PreviewChromeRow has Open in system browser | `BROWSER-16`, no duplicate finding |

Paths shortened in the table are under `crates/ferrite-pane/src/`, `crates/ferrite-cockpit/src/`, or `vendor/gpui-base/src/text/` as expanded in findings below. T3 references are pinned in each finding and the comparison notes below.

## Detailed findings

### link-affordances-01: Transcript web-link menus cannot explicitly open the system browser

Priority: P2. Category: missing-capability. Evidence: source-proven.

Symptom and impact: Users cannot choose their normal browser, authenticated profile or extensions from the link itself. The only Open action requests Ferrite Browser. Copying and manually pasting the address is the available escape.

Trigger: Right-click an ordinary HTTP(S) Markdown link in an answer and inspect the menu.

Expected: The link offers separately named integrated-browser and system-browser destinations, plus Copy link address.

Actual: The source offers Open link and Copy link only. Open link uses the integrated browser for HTTP(S). The source does not prove the installed copy-only symptom: its test explicitly expects two rows.

Cause: The transcript menu models one OpenLink verb. HTTP(S) is always accepted by Preview.open_url, so its OS fallback does not provide a user choice.

Ferrite evidence:

- `crates/ferrite-pane/src/transcript/message_menu.rs:67-84`
- `crates/ferrite-pane/src/transcript/message_menu.rs:132-139`
- `crates/ferrite-pane/src/transcript/message_menu.rs:192-198`
- `crates/ferrite-pane/src/preview.rs:322-331`
- `crates/ferrite-cockpit/src/cockpit/tests/link_clicks.rs:267-289`

T3 evidence:

- [apps/web/src/components/chat/externalLinkContextMenu.ts#L26-L30](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/externalLinkContextMenu.ts#L26-L30)
- [apps/web/src/components/chat/externalLinkContextMenu.ts#L111-L124](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/externalLinkContextMenu.ts#L111-L124)
- [apps/web/src/components/ChatMarkdown.tsx#L1299-L1328](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1299-L1328)

Recommendation: Add an explicit system-browser verb to the shared link interaction, preserving the resolved address. Name browser destinations consistently. This is separate from BROWSER-16, which owns opening the already active browser page externally.

Acceptance: Right-click [Example](https://example.com/a?b=c#d). Open in system browser launches that exact URL without creating or selecting a Ferrite tab. Open in integrated browser opens it in the owning thread. Copy link address copies the URL, not Example. Repeat while text is selected and while streaming.

### link-affordances-02: Modified and middle clicks on transcript web links ignore destination intent

Priority: P2. Category: interaction. Evidence: source-proven.

Symptom and impact: Cmd/Ctrl-click and middle-click cannot bypass Ferrite Browser. The same gesture works differently on a sidebar PR badge, creating an unpredictable everyday navigation rule.

Trigger: Cmd/Ctrl-click or middle-click an HTTP(S) transcript link.

Expected: A documented override selects the system browser, or a distinct foreground/background tab operation if that is the chosen product convention.

Actual: Both left and middle clicks are accepted, but modifiers are never read. HTTP(S) becomes the same integrated-browser Action::Open as a plain click. The sidebar PR handler explicitly checks the platform modifier and opens externally.

Cause: Inline forwards the actual button and modifiers. Rich checks only which button may activate, then always calls the same preview route. The preview queue has only URLs, with no destination or background-tab intent.

Ferrite evidence:

- `crates/ferrite-pane/src/rich.rs:605-645`
- `vendor/gpui-base/src/text/inline.rs:1200-1241`
- `crates/ferrite-pane/src/preview.rs:324-330`
- `crates/ferrite-cockpit/src/cockpit/browser.rs:342-355`
- `crates/ferrite-cockpit/src/cockpit.rs:12806-12819`

T3 evidence:

- [apps/web/src/browser/browserLinkTarget.ts#L11-L27](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/browser/browserLinkTarget.ts#L11-L27)
- [apps/web/src/components/ChatMarkdown.tsx#L1263-L1297](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1263-L1297)
- [apps/desktop/src/window/DesktopWindow.ts#L742-L760](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L742-L760)

Recommendation: Resolve link activation once from target, button, modifiers and user preference. Keep the gesture distinct from browser-page target=_blank handling, already owned by BROWSER-01. T3 supports a saved app/system preference; Ferrite can choose a different default without ignoring overrides.

Acceptance: Exercise plain, Cmd/Ctrl, Shift and middle clicks on the same transcript URL and sidebar PR. Assert the documented destination and whether selection changes. Dragging still selects without opening, and right-click never navigates.

### link-affordances-03: Issue-reference context menus open and copy an internal scheme instead of the real URL

Priority: P2. Category: defect. Evidence: source-proven.

Symptom and impact: A generated #212 reference can work on left click but fail through Open link. Copy link yields issue:212 rather than a shareable repository issue URL.

Trigger: In a checkout with an origin, receive an answer containing an auto-linked #212. Compare plain click, menu Open link and menu Copy link.

Expected: Every activation and clipboard action resolves the same issue identity to the checkout origin URL.

Actual: Plain click calls issue_url; the menu passes or copies the literal issue:212. The OS result is platform-dependent, but the incorrect requested target and clipboard text are source-proven.

Cause: Link target decoding exists inside the left-click closure but not the menu. The menu stores the renderer URL verbatim; FileLink rejects issue: as a non-file scheme, Preview rejects it as non-HTTP, then the menu calls the OS URL opener with issue:212.

Ferrite evidence:

- `crates/ferrite-pane/src/file_links.rs:607-614`
- `crates/ferrite-pane/src/file_links.rs:653-664`
- `crates/ferrite-pane/src/rich.rs:617-625`
- `crates/ferrite-pane/src/transcript/message_menu.rs:278-282`
- `crates/ferrite-pane/src/transcript/message_menu.rs:192-198`
- `crates/ferrite-pane/src/file_links.rs:38-42`

T3 evidence:

- [apps/web/src/components/ChatMarkdown.tsx#L1192-L1214](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1192-L1214)
- [apps/web/src/components/ChatMarkdown.tsx#L1221-L1228](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1221-L1228)
- [apps/web/src/components/ChatMarkdown.tsx#L1312-L1328](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1312-L1328)

Recommendation: Represent resolved target identity separately from display text and internal renderer schemes. Share decoding between clicks, menus and copy. Audit ferrite-path:, ferrite-thread:, ferrite-chip: and ferrite-skill: at the same boundary; do not advertise OS actions for app-only targets.

Acceptance: For origin git@github.com:owner/repo.git and #212, plain click and both browser menu actions use https://github.com/owner/repo/issues/212; Copy link address copies that URL. Missing origin produces an unavailable reason rather than issuing issue:212 to the OS.

### link-affordances-04: Custom file chips lose target-aware context actions

Priority: P2. Category: partial-capability. Evidence: source-proven.

Symptom and impact: An inline source-file chip supports opening but lacks reliable right-click actions for its full path, relative path, editor or file manager. A user cannot copy the actual destination from an abbreviated label with the confidence provided by a normal URL link.

Trigger: Right-click a chip rendered from `[short name](src/main.rs:42)` in an answer.

Expected: The clicked file owns the context menu and exposes valid path/editor actions, preserving any source location.

Actual: The custom chip has no context-menu binding and does not retain the native link URL in its Element. The exact inherited menu or stale-target interaction requires runtime observation. A source-defined file-menu Open in Files action drops line information if reached.

Cause: InlineFlow replaces the linked text with an Element containing size and selection state, but no URL metadata. The custom chip installs a primary button and modifier-click handler, without a right-click target handler. The parent menu relies on pressed_link from native text. Its fallback OpenInFiles branch also hardcodes line: None.

Ferrite evidence:

- `crates/ferrite-pane/src/rich.rs:567-575`
- `vendor/gpui-base/src/text/inline_flow.rs:193-233`
- `vendor/gpui-base/src/text/inline_flow.rs:470-481`
- `crates/ferrite-pane/src/attachments.rs:795-855`
- `crates/ferrite-pane/src/file_links.rs:354-380`
- `crates/ferrite-pane/src/transcript/message_menu.rs:269-284`
- `crates/ferrite-pane/src/transcript/message_menu.rs:199-203`

T3 evidence:

- [apps/web/src/components/chat/MarkdownFileLink.tsx#L247-L309](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownFileLink.tsx#L247-L309)
- [apps/web/src/components/chat/MarkdownFileLink.logic.ts#L32-L49](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownFileLink.logic.ts#L32-L49)
- [apps/web/src/components/ChatMarkdown.tsx#L835-L840](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L835-L840)

Recommendation: Pass a typed file target through custom rendering and install its own context menu. Offer Copy full path and Copy relative path where meaningful, Open in Files at the referenced line, and supported editor/reveal actions. Keep display-label copying as text selection.

Acceptance: Right-click a chip whose visible label differs from its path. Copy full/relative path returns the intended path with documented position suffix. Open in Files reveals line 42. Repeat after right-clicking an unrelated URL to ensure no stale target is reused, and for missing files and image chips.

### link-affordances-05: Markdown section links are sent to the OS instead of scrolling within the document

Priority: P2. Category: defect. Evidence: source-proven.

Symptom and impact: Tables of contents and section references in generated explanations or plans do not navigate to their headings.

Trigger: Click [Details](#details) in the same rendered Markdown document as a Details heading.

Expected: The transcript reveals that heading in its own scroll container without changing thread or browser state.

Actual: The handler requests OS opening of #details rather than resolving a heading. The resulting OS error or no-op was not observed.

Cause: No fragment branch exists in Rich activation. FileLink explicitly rejects #..., Preview rejects it as not an absolute HTTP(S) URL, and the final fallback invokes open_url on the fragment string.

Ferrite evidence:

- `crates/ferrite-pane/src/rich.rs:628-644`
- `crates/ferrite-pane/src/file_links.rs:25-28`
- `crates/ferrite-pane/src/preview.rs:324-326`
- `vendor/gpui-base/src/text/text_view.rs:95-112`

T3 evidence:

- [apps/web/src/components/ChatMarkdown.tsx#L1215-L1233](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1215-L1233)
- [apps/web/src/components/chat/markdownFragments.ts#L24-L61](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/markdownFragments.ts#L24-L61)
- [apps/web/src/components/chat/markdownFragments.ts#L83-L105](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/markdownFragments.ts#L83-L105)

Recommendation: Add document-scoped heading identity and fragment navigation before file or external URL dispatch. Preserve a stable heading target across streaming updates and avoid selecting a heading in another answer with the same name.

Acceptance: A local fixture with two answers containing Details headings scrolls to the clicked answer's heading. Percent-encoded fragments and duplicate heading suffixes work. Missing anchors give a defined result and never launch an external application.

### link-affordances-06: Ordinary Markdown links and their transcript menus have no keyboard activation path

Priority: P2. Category: interaction. Evidence: source-proven.

Symptom and impact: Keyboard users cannot traverse and activate the same web links available to pointer users, or choose a link menu action after opening the pointer-only transcript menu.

Trigger: Try Tab/Shift-Tab and Enter on a web link in an answer, then attempt arrow/Enter operation of its right-click menu. Compare an inline file chip.

Expected: Links are reachable as named controls with a visible focus indicator. A keyboard menu invocation identifies the focused link and arrow/Enter/Escape operate the menu.

Actual: Ordinary links are pointer hit regions, and the menu installs mouse-only rows. Thread chips are also raw clickable Divs. File chips are a positive exception with tab_stop(true), a label and a real button.

Cause: TextView owns a text-selection focus handle and Copy/SelectAll actions. Inline links register mouse events, without per-link focus targets or Enter actions. The Keyboard arm of the Rich callback is not a producer of keyboard events. Transcript menu rows only bind left mouse-down, with no focus or menu navigation wiring.

Ferrite evidence:

- `vendor/gpui-base/src/text/inline.rs:1132-1241`
- `vendor/gpui-base/src/text/state.rs:39-49`
- `vendor/gpui-base/src/text/text_view.rs:701-718`
- `crates/ferrite-pane/src/rich.rs:605-613`
- `crates/ferrite-pane/src/transcript/message_menu.rs:210-243`
- `crates/ferrite-pane/src/link_chip.rs:119-134`

T3 evidence:

- [apps/web/src/components/ChatMarkdown.tsx#L1221-L1229](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1221-L1229)
- [apps/web/src/components/chat/MarkdownFileLink.tsx#L329-L377](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MarkdownFileLink.tsx#L329-L377)
- [apps/web/src/components/pullRequest/PullRequestDetailCrumbs.tsx#L69-L85](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailCrumbs.tsx#L69-L85)

Recommendation: Give inline links stable focusable semantic targets and reuse the app's accessible menu implementation. Keep this scoped to transcript links; input-focus-003 already owns right-panel control accessibility.

Acceptance: Without a pointer, focus a link, activate it with Enter, open its context menu and choose Open in system browser or Copy address. Escape closes the menu and restores link focus. Re-rendering streaming text does not lose the focused target or submit the composer.

### link-affordances-07: Right-clicking a sidebar PR badge can offer Copy link for the thread instead of the PR

Priority: P2. Category: defect. Evidence: source-proven.

Symptom and impact: A user who right-clicks an underlined PR number and selects Copy link can copy a Ferrite thread link rather than the forge PR address. The menu does not represent the clicked link.

Trigger: Right-click directly on a sidebar PR number, then choose the menu's Copy link.

Expected: A link-specific menu copies that exact PR URL and offers explicit native/integrated/system destinations where supported. The enclosing thread menu remains available on the rest of the row.

Actual: The source event path reaches the thread menu, whose Copy link means thread identity. This handler-level conclusion is source-proven; the precise installed hit-test path remains a runtime check.

Cause: The PR badge handles left mouse-down and click but not right mouse-down. Its enclosing navigation card opens MenuTarget::Thread. The resulting Copy link action explicitly serializes thread_link(thread), while Open PR in browser is a different row.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:12806-12819`
- `crates/ferrite-cockpit/src/cockpit/nav_list.rs:339-344`
- `crates/ferrite-cockpit/src/cockpit/context_menu.rs:457-462`
- `crates/ferrite-cockpit/src/cockpit/context_menu.rs:828-829`

T3 evidence:

- [apps/web/src/components/pullRequest/PullRequestRow.tsx#L143-L154](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestRow.tsx#L143-L154)
- [apps/web/src/components/pullRequest/PullRequestDetailCrumbs.tsx#L69-L76](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestDetailCrumbs.tsx#L69-L76)
- [apps/web/src/components/pullRequest/pullRequestLinkContextMenu.ts#L17-L23](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/pullRequestLinkContextMenu.ts#L17-L23)
- [apps/web/src/components/pullRequest/pullRequestLinkContextMenu.ts#L39-L45](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/pullRequestLinkContextMenu.ts#L39-L45)

Recommendation: Handle the PR badge context gesture and stop propagation after capturing its explicit PR target. Label any thread action Copy thread link. Existing pr-navigation findings own the primary route and visibility behavior; this finding only adds wrong clipboard/context identity.

Acceptance: Right-click B's PR while A is focused. Copy PR link returns B's exact forge URL without changing focus. Right-click the remainder of B's row and Copy thread link returns B's Ferrite link. A missing PR URL disables URL actions with a reason.

## Supported T3 capabilities and limits

Chat links explicitly offer both browser destinations and copy the href. The custom menu stops event propagation before showing, so the message menu does not take over. [apps/web/src/components/ChatMarkdown.tsx:1299-1328](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ChatMarkdown.tsx#L1299-L1328).

Cmd/Ctrl or middle-click selects the system target. Plain clicks use the app/system preference and capability check. This establishes destination policy, not a promise that every T3 component forwards an event. [apps/web/src/browser/browserLinkTarget.ts:11-27](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/browser/browserLinkTarget.ts#L11-L27).

The common opener waits for the preference, supports an explicit target thread, records successful in-app visits, and falls back to the system opener on eligible failures. [apps/web/src/browser/useOpenLink.ts:32-54](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/browser/useOpenLink.ts#L32-L54).

Desktop host-renderer new-window links go to the system opener. Browser-preview guest navigation has its own policy and must not be conflated with host anchors. [apps/desktop/src/window/DesktopWindow.ts:742-760](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L742-L760).

The generic Electron context menu offers Copy Link and email/image/edit operations, not Open in system browser. It is installed for attached webviews as well as the main window. [apps/desktop/src/window/DesktopWindow.ts:647-739](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L647-L739).

PR Markdown reuses ChatMarkdown, so ordinary web links there inherit its link menu. Attachment fallback anchors have native _blank behavior. [apps/web/src/components/pullRequest/PullRequestMarkdown.tsx:116-178](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestMarkdown.tsx#L116-L178).

PR Details buttons open check URLs through useOpenLink but do not pass the click event. Do not infer that Cmd-click works on those particular buttons from the shared helper alone. [apps/web/src/components/pullRequest/PullRequestChecksPopover.tsx:96-109](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/pullRequest/PullRequestChecksPopover.tsx#L96-L109).

The browser chrome provides a named Open in system browser button, supporting BROWSER-16. [apps/web/src/components/preview/PreviewChromeRow.tsx:193-211](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L193-L211).

## Positive Ferrite behavior

- Normal URL links do have Open and Copy handlers in this baseline. `crates/ferrite-cockpit/src/cockpit/tests/link_clicks.rs:267-289` explicitly asserts the two-row menu and URL clipboard value. The test never activates Open and cannot establish external-browser behavior.
- Existing event-level tests cover plain click, one-pixel movement, selection, same-frame trackpad tap, streaming reparse and drag selection. See `crates/ferrite-cockpit/src/cockpit/tests/link_clicks.rs:56-114,144-188`. The implementation retains the pressed URL across streaming reparse at `vendor/gpui-base/src/text/inline.rs:64-87,1176-1241`.
- The native link callback does receive button and modifier information. The missing override is in application policy, not loss of the input upstream. See `vendor/gpui-base/src/text/inline.rs:1231-1241` and the test at `vendor/gpui-base/src/text/text_view.rs:1531-1590`.
- File chips use real labeled keyboard buttons and preserve their referenced line for normal preview opening. `crates/ferrite-pane/src/attachments.rs:795-842`. Missing files produce a visible notification on the OS-open fallback at `crates/ferrite-pane/src/file_links.rs:76-86`.
- The sidebar PR platform-modifier path explicitly opens the URL externally at `crates/ferrite-cockpit/src/cockpit.rs:12814`. This is useful behavior to retain when unifying link dispatch.
- Browser requests preserve the owning pane's thread when drained at `crates/ferrite-cockpit/src/cockpit/browser.rs:342-355`. This does not prove that the correct thread becomes visible; pr-navigation owns that distinction.

## Verification gaps and recommended runtime fixture

The parent could not capture the running app. Nothing here establishes which exact link, menu, installed build or context produced the user's copy-only observation. Possibilities include a different link context, an inherited menu, an older binary or a hit-test failure. These are alternatives to investigate, not established causes.

Use one transcript fixture containing a labeled URL, a bare URL, a generated issue reference, a file chip with a shortened label and line, a thread chip, and a local heading fragment. Give two sidebar threads different PR URLs. Keep a selected range while repeating the actions. Record the installed build identity, clicked target, actual menu labels, resulting clipboard value, OS-open request, visible thread and visible destination.

For each link type, cover plain left click, right click, middle click, platform modifier, Control where distinct, Shift, keyboard focus/Enter and keyboard context invocation. A shared route test is insufficient; events must hit the rendered target. For a custom chip, first open a different link's menu and then right-click the chip to detect stale pressed-link state.

Separately exercise an actual website link, _blank link and scripted popup. Those results belong to BROWSER-01 and BROWSER-03. Exercise the PR body/check/header entry points under the existing PR findings, and the active page's external-open escape under BROWSER-16. T3's generic webpage menu should be captured too, because it is deliberately narrower than its custom Markdown menu.

New-tab semantics need an explicit product decision and observable tests. Ferrite source queues Action::Open for all accepted transcript web clicks, without foreground/background intent. T3's explicit integrated-browser menu item does not itself prove a background-tab option. This report does not invent one for either product.

No numeric interaction score or runtime parity claim is made. Remediation should first share typed target resolution and menu identity, then add the external destination and modifier policy, then complete keyboard and fragment behavior. Preserve the existing selection/streaming click protections throughout.

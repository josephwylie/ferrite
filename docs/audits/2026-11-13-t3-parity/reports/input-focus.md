# Input and focus investigation

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user's goal is to understand why natural keyboard and mouse interactions fail in Ferrite, especially around the right panel, and which current T3 behaviors would address those failures.

## Overview

The strongest source-proven problem is action dispatch order. Ferrite's embedded browser and empty-panel launcher implement raw key handlers, but GPUI runs key-bound actions first. The cockpit consumes keys these children expect to own. In the browser, this can turn Enter into a conversation submit and paste into a main-composer edit. Files edits notify a cockpit redraw that has no Files focus exemption, so the main composer reclaims focus. Find in Files routes to the transcript. The right panel also has many mouse-only controls and no shared focus-area model. These are separate from the reported underlined PR link that does not open. This slice does not establish that link's runtime cause.

This report contains nine source-proven findings and one inferred resize risk. None is runtime-reproduced by this worker. Priority reflects the reachable behavior and user impact, not a measured failure rate.

Ferrite baseline is `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. T3 baseline is `74573fc29b901bafbf8f1dbb2eb29606da1fa448`. Both working-copy HEADs were checked. The investigation read current Rust/TypeScript sources, vendored GPUI dispatch and focus code, and existing test bodies. It did not change application code, run tests, build the app or drive a live UI. Existing tests mentioned below were inspected, not rerun. Throughput checkpoint is not applicable to this read-only investigation.

## Key concepts

- A Ferrite pane is the conversation owner. A right-panel surface is a tab owned by a thread. Selected pane, selected surface and keyboard focus are distinct state.
- GPUI keyboard dispatch has an action phase before its raw key listener phase. Consuming an action ends processing. A later `on_key_down` propagation stop cannot protect a child from an earlier action. The implementation is in `vendor/gpui-pre/src/window.rs:6152-6168` and `vendor/gpui-pre/src/window.rs:6539-6557`.
- Mouse dispatch uses a paint-ordered hitbox list. Occlusion stops hit testing at a blocking hitbox. Ordinary div move/up handlers require that their own hitbox is hovered. This differs from assuming every ancestor receives a DOM-style bubble. See `vendor/gpui-pre/src/window.rs:1187-1208`, `vendor/gpui-pre/src/window.rs:5973-5993` and `vendor/gpui-pre/src/elements/div.rs:215-228,304-317`.
- A GPUI `div().on_click(...)` does not become a focusable button. Focus handles require explicit opt-in. Keyboard click synthesis runs only while the element is focused. Accessibility roles require a role override. See `vendor/gpui-pre/src/elements/div.rs:1876-1882,2222-2241,2992-3057`.
- Ferrite's render function actively reconciles focus. Its default target at transcript level is the composer, subject to named exceptions. This means fixing an individual button also requires preserving its focus during future renders. See `crates/ferrite-cockpit/src/cockpit.rs:10109-10274`.

## Traced flows

### A browser input key

A click in browser content focuses `WebView.inner.focus` in `crates/ferrite-pane/src/visual/element.rs:645-663`. The browser is a child of the preview's Reader context in `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:383,498-520`. The nested focus is preserved by the preview exemption in `crates/ferrite-cockpit/src/cockpit.rs:10215-10223` and `crates/ferrite-pane/src/preview.rs:486-494`. Thus a browser key need not first lose focus to fail. The matching cockpit or Reader action consumes it before WebView's raw forwarding handler. This distinction matters when reproducing the problem.

### A right-panel tab click

The tab's left press starts a potential reorder at `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:544-548`. Its click activates the surface at lines 541-543. `activate_surface` updates the surface store and notifies at `crates/ferrite-cockpit/src/cockpit/panel.rs:282-284`. No focus handoff occurs there. The newly displayed body may supply its own focus logic. Middle-click closes the tab, right-click opens its menu, and the close child stops propagation so it does not also activate the parent tab. Those handlers are present at `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:501-504,561-573`.

### Tab, Shift-Tab and Escape

Ferrite binds Tab to the local composer/tool walk and Shift-Tab to permission-mode cycling, except the ToolDisclosure reverse binding. See `crates/ferrite-ui/src/keymap.rs:304-315`. The walk traverses attachments and transcript actions and returns to the composer at `crates/ferrite-cockpit/src/cockpit.rs:5811-5914`. Escape is a cockpit interrupt unless a deeper context owns it. The interrupt handler first dismisses several transient states, then interrupts the selected main agent or subagent at `crates/ferrite-cockpit/src/cockpit.rs:5638-5704`. Reader Escape closes its preview. This is useful for ordinary readers but inappropriate for a web page's own Escape behavior.

### Selection, copy and overlays

Native transcript selection uses the retained `TextSelection` document and is copied by `crates/ferrite-cockpit/src/cockpit.rs:9292-9297`. The focus pump explicitly preserves native text selection and code actions at lines 10187-10196 and reader text at lines 10215-10223. Terminal selection has its own copy, paste, select-all and Enter/Tab/Escape actions at `crates/ferrite-terminal/src/view.rs:324-344`. Browser DOM selection is a third independent owner. The global native-copy action does not read DOM selection.

The cockpit paints titlebar chrome before the panel, then menus/settings/sheets/lightbox/dialog layers after it at `crates/ferrite-cockpit/src/cockpit.rs:10642-10673`. The plane seams are paint-only divs and do not install event handlers at lines 5458-5475. There is no source basis here to blame those seams for a universal dead click region. Particular live overlays and platform titlebar hit testing still require inspection in the installed app.

## Source map

| Responsibility | Ferrite source |
| --- | --- |
| Contexts and default/user keybindings | `crates/ferrite-ui/src/keymap.rs:47-52,83-617,661-675` |
| Root action registration | `crates/ferrite-cockpit/src/cockpit.rs:10371-10500` |
| Render-time focus ownership | `crates/ferrite-cockpit/src/cockpit.rs:10109-10274` |
| Panel geometry, activation and close | `crates/ferrite-cockpit/src/cockpit/panel.rs:282-318,480-620` |
| Panel tabs, launcher and pointer gestures | `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:290-742` |
| PR action and segmented controls | `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:492-643` |
| Browser raw event bridge | `crates/ferrite-pane/src/visual/element.rs:246-289,597-675` |
| Browser input contract and CEF conversion | `crates/ferrite-pane/src/visual/web/mod.rs:121-162`; `crates/ferrite-pane/src/visual/web/cef/input.rs:60-205` |
| Terminal focus, selection and event bridge | `crates/ferrite-terminal/src/view.rs:247-344,565-623` |
| Native focusable controls and menu restoration | `crates/ferrite-ui/src/components.rs:29-39,137-158`; `crates/ferrite-ui/src/menu.rs:897-910,1025-1043` |
| GPUI keyboard and pointer dispatch | `vendor/gpui-pre/src/window.rs:1187-1208,5959-5993,6017-6168,6539-6557` |

## Findings
### input-focus-001 Host actions take browser Enter, Tab, Escape and clipboard shortcuts before Chromium receives them

P1. defect. Evidence is source-proven.

Symptom and impact. Browser forms and keyboard navigation cannot rely on normal page input. Enter can send or queue the main conversation draft while the user is editing a web page. Paste targets that draft and takes focus away from the page.

Trigger. Focus an input in the embedded Browser tab. Press Enter, Tab, Shift-Tab, Escape, Cmd+C or Cmd+V on macOS, or the primary-modifier equivalents on Windows.

Expected behavior. The page owns its form and editing keys. Only explicit host shortcuts leave the page, according to a documented context policy.

Actual behavior. GPUI matches actions before invoking raw key-down listeners. WebView supplies only raw forwarding and no key context or consuming browser actions. Enter therefore resolves to Cockpit Submit, Tab to BandCycle, Shift-Tab to permission-mode cycling for supported providers or the tool-walk fallback, and Escape to the enclosing Reader Close. Cmd+C reaches native TextSelection rather than the DOM selection. Cmd+V matches the no-text-field fallback and pastes into the main composer. The precise installed-binary effects remain untested in this slice.

Cause. The browser is mounted inside the Reader preview. Reader provides an Escape action but is not a browser editing context. An on_key_down handler that stops propagation is too late to block an already-consumed action. Submit checks several modal states but never checks that the composer owns focus before taking and sending its text. This is the same dispatch-order error as input-focus-002.

Ferrite evidence. `vendor/gpui-pre/src/window.rs:6152-6168`; `vendor/gpui-pre/src/window.rs:6539-6557`; `crates/ferrite-pane/src/visual/element.rs:265-287`; `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:383`; `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:498-520`; `crates/ferrite-ui/src/keymap.rs:47`; `crates/ferrite-ui/src/keymap.rs:90-93`; `crates/ferrite-ui/src/keymap.rs:179-192`; `crates/ferrite-ui/src/keymap.rs:304-315`; `crates/ferrite-cockpit/src/cockpit/palette.rs:1378-1402`; `crates/ferrite-ui/src/keymap.rs:537`; `crates/ferrite-cockpit/src/cockpit.rs:5486-5596`; `crates/ferrite-cockpit/src/cockpit.rs:9292-9335`.

T3 comparison. [apps/desktop/src/preview/Manager.ts#L514-L534](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L514-L534); [apps/desktop/src/preview/Manager.ts#L1509-L1583](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L1509-L1583).

Recommendation. Give browser content an explicit input context and route its editing actions to the browser before the cockpit acts. Keep page keys and host navigation shortcuts separate. Guard Submit with the actual sending control or explicit submit intent, so unrelated focused content cannot send its draft.

Observable acceptance check. With a fake provider and a local form, leave a sentinel unsent in the main composer. Click the page input and exercise Enter, Tab, Shift-Tab, Escape, copy and paste. The form receives intended keys, the draft remains unchanged, no provider send occurs, and only explicit host commands navigate away.

### input-focus-002 Enter in the empty panel invokes the conversation submit action instead of the highlighted opener

P1. defect. Evidence is source-proven.

Symptom and impact. The empty panel advertises a keyboard launcher that cannot receive its Enter key under the production binding table. An unsent main draft is exposed to the same unintended-submit path as browser content.

Trigger. Close every right-panel tab, click the empty panel, use arrows to highlight an available opener, then press Enter.

Expected behavior. Enter opens the highlighted row while the main conversation remains unchanged.

Actual behavior. The panel takes panel_empty_focus and has an on_key_down handler for Enter. It has no key context or action overriding the Ferrite Enter binding. The root Submit action consumes the event before surface_empty_key can run.

Cause. The implementation treats raw key bubbling as if it precedes GPUI action matching. Its regression test calls surface_empty_key directly and therefore cannot reveal this production dispatch conflict.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:670-691`; `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:716-742`; `crates/ferrite-cockpit/src/cockpit.rs:10239-10248`; `crates/ferrite-ui/src/keymap.rs:185-189`; `vendor/gpui-pre/src/window.rs:6152-6168`; `crates/ferrite-cockpit/src/cockpit/tests/tools.rs:190-220`.

T3 comparison. [apps/web/src/components/RightPanelTabs.tsx#L438-L460](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L438-L460); [apps/web/src/components/RightPanelTabs.tsx#L475-L481](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L475-L481).

Recommendation. Add a panel-launcher context with its own Enter action, or intercept before action dispatch. Verify through production key registration and the focused rendered launcher, rather than invoking its helper.

Observable acceptance check. An integration test uses bind_production_keys, clicks the blank launcher, sends ArrowDown and Enter as real key events, and asserts exactly the chosen surface opens with zero main-composer sends.

### input-focus-003 Right-panel tabs, close buttons and PR controls have no keyboard focus or accessible control roles

P1. interaction. Evidence is source-proven.

Symptom and impact. Keyboard-only users cannot operate much of the right panel. Assistive technology cannot discover the raw controls as named buttons or tabs. A visible hover and an on_click handler do not make a GPUI Div focusable.

Trigger. Try to Tab to the panel tab title, close icon, plus menu, maximize control, PR number, PR Merge button or PR Summary/Guide/Timeline/Code segments.

Expected behavior. Each action has a focus target, visible focus treatment, a semantic role and an accessible name. Enter or Space activates a focused button, and tab selection has an explicit keyboard pattern.

Actual behavior. These controls are raw div elements with mouse handlers, without focusable or track_focus and without role/label metadata. head_door also returns an unfocusable div. GPUI creates automatic focus only for explicitly focusable elements, and synthesizes Enter/Space clicks only for a focused element. Div accessibility roles are not inferred from on_click. The 16-pixel tab-close hit target adds precision demand for pointer users.

Cause. The panel copied control appearance and click handlers without using Ferrite's existing focusable form-control path. The generic compact button helper also defaults tab_stop to false, so substituting it without a policy change would not solve keyboard traversal.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:487-575`; `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:578-619`; `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:314-324`; `crates/ferrite-pane/src/preview.rs:689-707`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:517-533`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:578-603`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:609-633`; `crates/ferrite-ui/src/components.rs:29-39`; `crates/ferrite-ui/src/components.rs:144-158`; `crates/ferrite-ui/src/theme.rs:1019`; `vendor/gpui-pre/src/elements/div.rs:1876-1882`; `vendor/gpui-pre/src/elements/div.rs:2222-2241`; `vendor/gpui-pre/src/elements/div.rs:2992-3057`.

T3 comparison. [apps/web/src/components/RightPanelTabs.tsx#L912-L915](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L912-L915); [apps/web/src/components/RightPanelTabs.tsx#L977-L990](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L977-L990); [apps/web/src/components/RightPanelTabs.tsx#L1408-L1418](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/RightPanelTabs.tsx#L1408-L1418); [apps/web/src/components/preview/RightPanelResizeHandle.tsx#L14-L25](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/RightPanelResizeHandle.tsx#L14-L25).

Recommendation. Use named, focusable native controls for panel actions, with a single tab-order policy. Expose tab state and PR segment selection to accessibility. Preserve focus through redraws rather than just adding handles to controls the focus pump will reclaim. Increase close hit area while retaining the visual icon size.

Observable acceptance check. Operate tab switching, close, add, maximize, PR views and PR link without a pointer. Inspect the accessibility tree for names, roles and selected states. Repeated background updates preserve the focused control and Enter/Space executes only that control.

### input-focus-004 Cmd+W on a PR or terminal can close the parent conversation instead of the active panel surface

P1. interaction. Evidence is source-proven.

Symptom and impact. A familiar close command can remove the working conversation from view when the user intends to close a panel tab. The thread is parked, not deleted, but the navigation change is unrelated to the clicked surface.

Trigger. Work in the right-panel PR, Changes, Files list, Background view or terminal, then use Cmd+W or Ctrl+W. Browser/file preview and review focus are important exceptions.

Expected behavior. Close resolves against the actual focused area. Closing a panel tab does not close its owning conversation.

Actual behavior. CloseThread is globally bound. The handler special-cases nav cursor, lightbox, focused preview and focused review, then closes the focused cockpit pane. It never dispatches through active_surface for other surface types. The terminal has its own Enter/Tab/Escape/clipboard actions, but no Close override. PR raw controls leave focus in the composer or root, which reaches the same fallback.

Cause. The close handler still models older preview/review exceptions rather than the generalized right-panel surface ownership model.

Ferrite evidence. `crates/ferrite-ui/src/keymap.rs:364-367`; `crates/ferrite-cockpit/src/cockpit.rs:9148-9175`; `crates/ferrite-cockpit/src/cockpit/panel.rs:282-293`; `crates/ferrite-terminal/src/view.rs:324-344`; `crates/ferrite-ui/src/keymap.rs:604-617`.

T3 comparison. [packages/shared/src/keybindings.ts#L69-L75](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L69-L75); [apps/web/src/components/chat/useRightPanelActions.ts#L315-L324](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useRightPanelActions.ts#L315-L324).

Recommendation. Route close through a focused-area owner and active surface. Preserve the explicit command for closing a conversation, with a clearly named separate action or context.

Observable acceptance check. For each panel surface, focus its content, press close once and assert only its tab closes. The parent thread stays selected and unparked. Test the browser/file/review exceptions to preserve their existing behavior.

### input-focus-005 No workspace-area focus cycle or per-surface focus restoration connects the composer and panel

P2. missing-capability. Evidence is source-proven.

Symptom and impact. Users who enter the terminal or panel cannot rely on one keyboard command to move among major areas. Opening or switching a panel tab does not establish a consistent focus handoff. Focus behavior depends on the specific body component.

Trigger. Attempt to move from composer to transcript to panel and back with a consistent focus command, or return to a previously focused panel tab after another tab has been used.

Expected behavior. Visible workspace areas expose next/previous focus commands and remember an appropriate control per surface. Modal boundaries constrain the cycle. Closing a surface returns focus to a useful visible origin.

Actual behavior. The key table has nav focus and panel visibility toggles, but no workspace-area cycle. Tab usually calls the custom draft/attachment/tool walk and returns to the composer; Shift-Tab normally cycles permission mode. activate_surface only activates state and notifies. The render-time focus pump chooses a wanted focus, generally the main composer, with a growing list of exceptions for specific widgets. There is no general panel focus owner or per-tab focus memory in the inspected code.

Cause. The focus model is based on a preferred conversation focus plus exceptions. The surface model tracks what is visible independently. Adding another panel body therefore requires another exemption to avoid focus reclamation.

Ferrite evidence. `crates/ferrite-ui/src/keymap.rs:263-310`; `crates/ferrite-cockpit/src/cockpit.rs:5796-5914`; `crates/ferrite-cockpit/src/cockpit.rs:10109-10274`; `crates/ferrite-cockpit/src/cockpit/panel.rs:282-284`; `crates/ferrite-cockpit/src/cockpit/panel.rs:550-620`.

T3 comparison. [packages/shared/src/keybindings.ts#L59-L63](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/shared/src/keybindings.ts#L59-L63); [apps/web/src/components/AppSidebarLayout.tsx#L175-L195](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/AppSidebarLayout.tsx#L175-L195); [apps/web/src/lib/workspaceFocus.ts#L99-L174](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/workspaceFocus.ts#L99-L174); [apps/web/src/lib/workspaceFocus.ts#L175-L257](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/workspaceFocus.ts#L175-L257); [apps/web/src/components/preview/PreviewPanelShell.tsx#L112-L148](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewPanelShell.tsx#L112-L148).

Recommendation. Introduce explicit focus ownership for major visible areas and a remembered focus target per panel surface. Route keyboard focus deliberately on open, switch and close. Keep the composer's useful local tool walk, but provide a separate area-navigation command.

Observable acceptance check. F6 and Shift-F6, or documented equivalents, visit visible nav, conversation, composer and panel areas. They leave a terminal and browser without typing into them. Reopening a tab restores its last valid control. A closed or hidden tab never retains focus.

### input-focus-006 Embedded browser input has no composition or native text-input bridge

P2. partial-capability. Evidence is source-proven.

Symptom and impact. Users of input methods, composed text and native text services lack an implemented path equivalent to Ferrite's composer. Sending raw key_char values is not a composition protocol. Exact failures for each operating-system input method need runtime checks.

Trigger. Use a Japanese or Chinese input method, an accent composition sequence, or another native composition workflow in a browser text field.

Expected behavior. The host accepts composition updates, supplies caret geometry, commits or cancels marked text correctly, and leaves composition keys to the input method.

Actual behavior. WebView renders a bitmap and forwards key-down/up values. Input contains mouse, wheel and Key variants only. There is no marked-text, composition range, committed-text, candidate-geometry or native EntityInputHandler path in the visual implementation. The CEF adapter builds RAWKEYDOWN and CHAR events from the raw key text. The main composer, in contrast, implements EntityInputHandler and registers ElementInputHandler.

Cause. The offscreen browser integration implements keyboard events without the native text-input contract needed to host an editor. This is a source-defined missing bridge, not a claim that every non-ASCII character fails.

Ferrite evidence. `crates/ferrite-pane/src/visual/element.rs:265-289`; `crates/ferrite-pane/src/visual/web/mod.rs:121-162`; `crates/ferrite-pane/src/visual/web/cef/input.rs:60-110`; `crates/ferrite-pane/src/composer.rs:984-1021`; `crates/ferrite-pane/src/composer.rs:1852-1854`.

T3 comparison. [apps/desktop/src/preview/Manager.ts#L1540-L1548](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/preview/Manager.ts#L1540-L1548).

Recommendation. Implement native text-input and composition for the offscreen browser, including caret bounds and commit/cancel handling. Do not treat synthetic per-key CHAR events as sufficient. Audit the custom terminal input path separately because it also reads raw key events.

Observable acceptance check. Compose and commit a Japanese phrase and Chinese candidate sequence in a local browser textarea on each supported OS. Cancel a composition with Escape, move the candidate cursor, and confirm no host command runs and the candidate window follows the caret.

### input-focus-007 Typing y, n or a as the first character of an empty approval composer performs a permission action

P1. interaction. Evidence is source-proven.

Symptom and impact. A user who starts a normal follow-up with "also", "no" or "yes" can answer a pending approval before sending a message. The a shortcut means Always. This is an intentional current interaction contract, but it conflicts with the live text-field affordance.

Trigger. A non-question approval is pending at transcript level. The composer is focused and empty. Begin typing a sentence whose first character is y, n or a.

Expected behavior. A focused editable composer accepts ordinary letters. Permission actions require a deliberate control activation or modified shortcut.

Actual behavior. The enclosing field adds Decision context. The keymap binds bare y/n/a. answer_or_type inserts these letters only when a question is pending or the composer already contains text. Otherwise it answers the approval. The current test explicitly avoids these letters in the initial sentence, then proves y answers when the composer is emptied.

Cause. An empty editable field doubles as an approval command mode without a separate focus transition. The exemption for nonempty text solves mid-word interception but does not solve first-character ambiguity.

Ferrite evidence. `crates/ferrite-pane/src/pane/composer.rs:504-517`; `crates/ferrite-ui/src/keymap.rs:193-207`; `crates/ferrite-cockpit/src/cockpit.rs:5984-5993`; `crates/ferrite-cockpit/src/cockpit.rs:6012-6040`; `crates/ferrite-cockpit/src/cockpit/tests/composer_menus.rs:957-1027`.

T3 comparison. [apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L26-L30](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L26-L30); [apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L51-L86](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerPendingApprovalActions.tsx#L51-L86).

Recommendation. Keep bare approval letters in an explicitly focused approval region or non-editing mode. Use modified chords or ordinary focused buttons while the composer is editable. Preserve question and nonempty-draft behavior.

Observable acceptance check. With an approval pending and the composer empty, type "also check this", "no, change direction" and "yes, after tests" as ordinary key events. All text remains unsent and no approval response occurs. Explicit approval shortcuts and buttons still work.

### input-focus-008 Panel resizing relies on root hover events that occluding children can block

P2. interaction. Evidence is inferred.

Symptom and impact. A resize drag may stall over the panel edge or another occluding control and may miss its release. This is a source-backed risk, not a measured or runtime-reproduced failure.

Trigger. Grab the four-pixel panel edge and move slowly within it, then across a tab, browser content, or another control with an occluding hitbox. Release while over that control.

Expected behavior. The drag owner receives move, release and cancellation regardless of hit-test changes under the pointer.

Actual behavior. The edge sets panel.resizing and occludes mouse input but registers no move or up listener. Resizing and release primarily use the root on_mouse_move/on_mouse_up handlers. GPUI only calls those handlers when their hitbox is hovered, and hit testing stops at BlockMouse. Tabs contain a local forwarding workaround for moves, but the edge does not. The exact visible stall depends on hitboxes and movement coordinates.

Cause. The implementation assumes the root sees every mouse move and release. Ordinary hover-gated bubble listeners do not provide pointer capture. Occlusion is a paint-order hit-test boundary rather than DOM-like ancestor bubbling.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/panel.rs:480-484`; `crates/ferrite-cockpit/src/cockpit/panel.rs:500-542`; `crates/ferrite-cockpit/src/cockpit/panel.rs:586-604`; `crates/ferrite-cockpit/src/cockpit.rs:10506-10514`; `crates/ferrite-cockpit/src/cockpit.rs:10534-10549`; `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:550-559`; `vendor/gpui-pre/src/elements/div.rs:215-228`; `vendor/gpui-pre/src/elements/div.rs:304-317`; `vendor/gpui-pre/src/window.rs:1187-1208`.

T3 comparison. [apps/web/src/hooks/useResizeDrag.ts#L47-L71](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useResizeDrag.ts#L47-L71); [apps/web/src/hooks/useResizeDrag.ts#L82-L125](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useResizeDrag.ts#L82-L125).

Recommendation. Use an explicit captured drag session or capture-phase window listeners with an active-drag guard. End it on release, cancellation, window blur and owner disappearance. Confirm the actual failure before selecting the smallest repair.

Observable acceptance check. Resize continuously across the edge, tabs, browser and terminal. Release outside the original edge and outside the window. Width follows continuously and never changes after release. Include slow movement smaller than the edge width.

### input-focus-009 Find in a Files editor searches the conversation instead of the file

P2. defect. Evidence is source-proven.

Symptom and impact. Users searching the code or prose currently visible in Files get a transcript search. The shortcut changes the wrong reading context and does not offer file-local matching.

Trigger. Open a checkout source file or Markdown file in Files, click its contents, and press Cmd+F or Ctrl+F.

Expected behavior. Find searches the focused file and presents matches in that file. Escape returns focus to it.

Actual behavior. The global find action routes only to a legacy preview when it is shown and considered pressed/focused, otherwise to the transcript. Checkout text previews are redirected into Files and the old preview is closed. Editor registers editing, save and clipboard actions, but no Find action. Files content has no alternate find owner.

Cause. The find dispatcher was not migrated with the checkout-file redirect and generalized panel surfaces.

Ferrite evidence. `crates/ferrite-ui/src/keymap.rs:263-265`; `crates/ferrite-cockpit/src/cockpit.rs:6144-6164`; `crates/ferrite-cockpit/src/cockpit/tools/files.rs:271-293`; `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:426-504`; `crates/ferrite-cockpit/src/cockpit/tools/files.rs:663-687`.

T3 comparison. [apps/web/src/components/files/FilePreviewPanel.browser.test.tsx#L189-L225](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.browser.test.tsx#L189-L225).

Recommendation. Route Find through the focused surface and implement a file-local search owner for code and prose. Keep transcript search scoped to conversation focus.

Observable acceptance check. Use a file containing a sentinel absent from the transcript. Focus the file and invoke Find. The sentinel matches and scrolls into view in the file; the transcript search remains closed. Repeat for Markdown and after returning from another panel tab.

### input-focus-010 Files editor and filter lose keyboard focus when their edits redraw the cockpit

P1. defect. Evidence is source-proven.

Symptom and impact. Code editing and file filtering can redirect subsequent typing into the main conversation. This is a concrete missing exemption in the current focus reconciliation, beyond the broader area-navigation gap in input-focus-005.

Trigger. Click the Files editor or file filter and type. An edit or any background event that redraws the cockpit is sufficient to reach focus reconciliation.

Expected behavior. The editor or filter keeps focus through edits and background updates until the user moves focus. Subsequent characters remain in that control.

Actual behavior. The editor click focuses its handle. An edit emits Edited, and the cockpit subscription calls notify. The filter subscription likewise notifies the cockpit. At transcript level the cockpit wants its main composer. None of its focus-preservation cases covers Files editor or Files filter, and Files is rendered outside the conversation controls and legacy preview. On that root render it focuses the main composer. The exact visible timing in the installed binary is unverified, but the focus reassignment path is explicit.

Cause. Files added independent text controls without extending the centralized focus ownership model. The existing file-edit test invokes Editor.type_text directly, so it tests file mutation and autosave without requiring input to stay focused.

Ferrite evidence. `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:301-312`; `crates/ferrite-cockpit/src/cockpit/tools/editor.rs:201-216`; `crates/ferrite-cockpit/src/cockpit/tools/files.rs:125-135`; `crates/ferrite-cockpit/src/cockpit/tools/files.rs:228-239`; `crates/ferrite-cockpit/src/cockpit/tools/files.rs:446-464`; `crates/ferrite-cockpit/src/cockpit.rs:10159-10162`; `crates/ferrite-cockpit/src/cockpit.rs:10220-10274`; `crates/ferrite-cockpit/src/cockpit/tests/files.rs:74-89`.

T3 comparison. [apps/web/src/components/files/FilePreviewPanel.browser.test.tsx#L189-L225](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/files/FilePreviewPanel.browser.test.tsx#L189-L225); [apps/web/src/lib/workspaceFocus.ts#L120-L138](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/lib/workspaceFocus.ts#L120-L138).

Recommendation. Preserve the active Files editor and filter as real focus owners. Prefer a general active-surface ownership check over another loosely related global exception. Replace the editing proof with a rendered click-and-type integration check in addition to the autosave test.

Observable acceptance check. In a scratch checkout, click a source line and type several separate key events across cockpit updates. Assert every character lands in the file, none enters the composer, and focus remains in the editor. Repeat in the file filter and while a transcript streams. No live user file is needed for this test.

## Supported T3 capabilities

T3 implements a named workspace region for the panel, a per-surface focus key, native buttons for tab activation and close, a named add-surface button, and an accessible empty launcher. These are executable source features, not claims about every runtime edge case. See the commit-pinned sources in input-focus-002, input-focus-003 and input-focus-005.

T3's focus controller remembers the last focused control per area, filters hidden and inert content, attempts restoration after remount, and picks a preferred panel-content control before the tab strip. F6 and Shift-F6 have capture-phase handlers so they can leave widgets that otherwise consume keys. The controller respects active modal boundaries. See the sources in input-focus-005.

T3 deliberately handles the browser boundary. It recognizes editing shortcuts, checks the guest's actual focused frame, excludes composing input from host forwarding, leaves selected find/zoom commands to the page first, and explicitly returns native focus to the host for workspace navigation or URL focus. See the Manager sources in input-focus-001 and input-focus-006. This source architecture supports the expected ownership rule. It does not prove that every IME, embedded frame or web application works.

T3 uses pointer capture for resize, tracks the pointer ID, flushes pending width on completion, and handles blur, cancel and lost capture. See input-focus-008. Ferrite does not need to duplicate the React implementation, but it needs the same observable drag ownership.

## Positive Ferrite behavior

- Terminal text has a dedicated context and overrides clipboard, Enter, Tab, Shift-Tab and Escape before cockpit actions. This is a local pattern the browser can follow. `crates/ferrite-terminal/src/view.rs:324-344` and `crates/ferrite-ui/src/keymap.rs:604-617`.
- Native selection is preserved rather than universally stolen by the focus pump. Transcript, preview and review text all have explicit exemptions. `crates/ferrite-cockpit/src/cockpit.rs:10187-10223`.
- Menus have a focused PopupMenu context and restore their supplied origin on close. `crates/ferrite-ui/src/menu.rs:897-910,1025-1043`.
- The right-panel tabs already support middle-click close, a context menu, drag reorder, overflow scrolling and mouse-safe close propagation. `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:325-385,487-573`. This is incomplete keyboard coverage, not a wholly inert panel.
- The main composer implements the native input-handler path absent from WebView. `crates/ferrite-pane/src/composer.rs:984-1021,1852-1854`.
- Ferrite has real focusable form controls with accessible labels in its shared chooser, and form_button enables tab stops. `crates/ferrite-ui/src/controls.rs:43-62` and `crates/ferrite-ui/src/components.rs:144-158`.
- Existing tests exercise actual pointer clicks for several PR controls, not only helper calls. `crates/ferrite-cockpit/src/cockpit/tests/pr.rs:220-222,264-290`. They do not establish installed-binary hit testing, and the empty-panel keyboard test specifically bypasses dispatch.

## Verification gaps and follow-up checks

1. Reproduce input-focus-001 and input-focus-002 with a fake provider or isolated test conversation. Never use a real unsent user draft to probe accidental submit. Record focused handle, context stack, selected action and whether Chromium received the key.
2. Inspect the actual installed binary's PR hitboxes, mouse-down/up targets and URL-open outcome. Hover underline proves a hover handler ran. It does not prove the click completed or the URL-open command succeeded. The PR-navigation owner traces that operation.
3. Exercise keyboard traversal and inspect the live accessibility tree. The raw-div omissions are source-proven. The exact spoken output, platform settings and installed build still require runtime validation.
4. Check the resize inference using slow movement over the edge and release over occluding children. Include window exit and re-entry. Do not upgrade input-focus-008 to runtime-reproduced without observing it.
5. Probe terminal interaction under overlays. Its raw handlers check rectangle bounds, not GPUI hitbox occlusion, at `crates/ferrite-terminal/src/view.rs:568-623`. A later overlay that does not consume the matching event could allow a hidden terminal press or scroll. The browser's scroll bridge checks its hitbox and stops propagation at `crates/ferrite-pane/src/visual/element.rs:597-614`. This is a separate unverified overlap risk, not an extra defect count in the JSON.
6. Exercise selection across boundaries, including browser DOM copy, native transcript copy, code-block copy and terminal copy. Existing native selection coverage does not cover the CEF boundary. The transcript owner reports transcript-specific issues.
7. Exercise composition with operating-system input methods. Browser key-conversion unit tests only cover letters, digits, named keys and a simple CHAR sequence at `crates/ferrite-pane/src/visual/web/cef/input.rs:224-243`.
8. Verify focus after repeated background redraws, tab changes, closing a tab and collapsing the panel. Generic focus preservation is not established by a test that sets a handle once and immediately invokes a helper.

## Recommended sequence

First fix action ownership for browser content and the empty launcher, including a guard against unrelated focus submitting the main composer. Preserve Files editor and filter focus through edits, and route Find to the active file. Then introduce a focused-area contract and migrate close routing and panel controls to it. Add keyboard reachability and accessibility together so the focus pump does not undo new controls. Address the approval first-character ambiguity as a product interaction correction. Implement composition support and validate resize capture separately. Keep native selection, terminal overrides and menu return-focus behavior intact.

This slice does not assert exhaustive runtime parity. It identifies dispatch and ownership failures from the current implementations and specifies the runtime checks that would settle the remaining uncertainty.

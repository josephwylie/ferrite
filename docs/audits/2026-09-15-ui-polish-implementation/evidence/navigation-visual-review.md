# Navigation / image inspection verification

Product commits:2892d3b and4b4aa93. Integrated native frame source2f5df5f with only disposable visual_reference fixture changes. Capture succeeded in the QA checkout; all25frames written to `/tmp/ferrite-polish-after`. No provider Sessions or operator store were used.

## UI-07 — Main and selected child visibility

Pass. `subagents-1000x800.png` shows a readable Main label and an aligned selected underline in the same tab grammar as Navigation, Error states and Forms. `selected-child-1000x800.png` shows Main first and Rowan selected with all five children in order. `selected-child-640x800.png` shows Main, Atlas, Cedar, Finch, selected Rowan and +1 without collision; Juniper is correctly the overflow item. Main and the selected transcript stay visible at the narrow size. The real native strip remains compact.

Functional verification:14subagent tests pass, including measured Main label width, selected-child retention under resize/status reorder, pointer press cancellation across reorder/resize and keyboard held-Space identity. The existing held-Space assertion was preserved. Its original failure was caused by Composer Tab wrapping to sidebar focus and the cockpit returning it to Composer. The bounded fix enters Main from Composer before native traversal continues.

## UI-13 — Contextual creation

Pass. `subagents-1000x800.png` and both selected-child frames show + New Group for a loose Thread. `preview-group-1200x800.png` shows + Add Thread for the Group. `draft-1000x800.png` shows + New Thread for the draft. All fit the titlebar, and each label is visibly distinct from the sidebar's existing standalone plus. The label and captured placement come from the same source match, preserving actual creation semantics. No extra titlebar tab stop was added.

Functional verification:both real titlebar click tests pass (loose Thread creates a new Group draft; existing Group keeps its draft in the Group).

## UI-11 — Original image inspection

Pass. `preview-1000x800.png` and `preview-group-1200x800.png` show readable Open Original between the image title and Close without collision. Image containment is preserved; the Group preview remains within its owning pane and neighboring panes remain visible. The image in this capture is the committed app-icon PNG, so the framebuffer establishes geometry and affordance rather than detailed screenshot readability.

Functional verification:the real Open Original click receives an unresolved path with Unicode, spaces, # and %. The test proves the OS receives the correctly encoded canonical file URL, with no fragment/query; no prompt is sent, the dialog remains open, and Escape still dismisses it. The existing cross-pane/native-file-drop test also passes. Actual OS image-viewer launch and Windows/Linux native drawing were not exercised.

## Capture helper notes

The initial helper retained a CockpitView handle through HeadlessAppContext teardown, causing its entity leak assertion. Dropping the helper's retained handle first fixed that; this was not a product retain cycle. The final batch exits0. Native code-copy action and exact clipboard assertion pass during capture.

`settings-hover-compact.png` visibly hovers Close Settings. `settings-keyboard-compact.png` visibly focuses the native search field, not the initially intended Close button: native key dispatch advances once and the helper's explicit focus_next advances again. Use this image only as search-focus evidence. Choice-chip keyboard/hover styling is not verified by that helper frame.

Current-source targeted logs: `/tmp/ferrite-navigation-subagents-tests.log` (14passed), `/tmp/ferrite-navigation-titlebar-tests.log` (2passed), `/tmp/ferrite-navigation-preview-tests.log` (1passed), `/tmp/ferrite-navigation-drops-tests.log` (1passed). App and changed vendor source mtimes were forced before compilation to avoid shared-target false freshness. `git diff --check` passed; worktree clean after commits.

## Final-suite transient file-drop result

On final integrated source ba6ffd3, the first full app suite had363passed/4failed/2ignored. The existing native-file-drop test failed at the visual assertion that sent files remain attachment cards. No source or assertion changed. Its immediate isolated rerun passed (1.14s), and the full-suite repeat also passed that test, finishing364passed/3failed/2ignored; the remaining three failures exactly match baseline failures.

Source inspection found no demonstrated interaction regression. Fake Session.send and core prompt insertion are synchronous. The assertion observes the retained virtual transcript after a single32ms test-clock tick; native layout may publish deferred refreshes, so timing is a possible lead, not a proven cause. No production change, sleep, retry loop, or weakened assertion was introduced for this transient failure. Investigation stopped after the unchanged isolated and full-suite reruns passed, as directed by root.

Logs: `/tmp/ferrite-polish-final-app-tests.log` (initial full-suite failure), `/tmp/ferrite-polish-native-drops-rerun.log` (isolated pass). Root retains the full-suite repeat log.

Final Question capture helper also settles two extra native draw/park passes before each question-* frame so the screenshot records measured layout after deferred prepaint notifications. This affects the disposable harness only.

## Final native confirmation on ba6ffd3

The final28-frame batch completed successfully (exit0) in the disposable QA checkout, using `--locked` and the committed audit Settings screenshot as the preview image. Artifacts: `/tmp/ferrite-polish-confirmed`; log: `/tmp/ferrite-polish-final-visual-capture.log`. A helper-only missing `gpui::Focusable` import was added before this successful run. Cargo slot released.

Verified actual focus placement, closing the earlier capture limitation:

- `settings-choice-keyboard-compact.png` visibly places the light inset keyboard outline around the selected Claude provider chip, preserving the checkmark and selected fill.
- `new-project-create-keyboard-1000x800.png` visibly places the dark inset keyboard outline around the enabled Create button. The Name field now has a visible input surface. Create was never activated.

Rechecked `preview-1000x800.png` and `preview-group-1200x800.png` with a real, high-resolution Settings screenshot instead of the icon fixture. Both preserve aspect ratio and containment; Open Original, title and Close remain distinct. The Group preview stays inside its owner while adjacent panes remain visible. The fitted screenshot is naturally too small for detailed text inspection in the Group pane, and the visible Open Original action provides the requested escape hatch to the full-resolution file. No external viewer was launched by the capture.

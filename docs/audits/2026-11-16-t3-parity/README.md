# Scoped T3 alignment

> Current status, 2026-11-17. Native annotation acceptance now passes after Keychain resolution and two native-input corrections. Whole-surface parity remains open because the PR and context differences listed in the acceptance record remain. No 1:1 or numerical 90% claim is supported.

The [current acceptance record](acceptance-current.md) separates verified behavior from remaining differences. Reports are [PR](pr-current.md), [context](context.md), [sidebar](sidebar.md), [Cite](cite-complete-parity.md), and [live annotations](live-annotation-current.md). The [native completion record](evidence/live-browser/native-completion.md) links actual captures, fixture-provider payloads, and measurements against pinned T3 rules.

After the final source edit, five bootstrap tests, the app typecheck, the bundle build, and the strengthened browser smoke passed. Normal native startup then loaded the requested page, kept private bootstrap history hidden, and opened live annotation controls. Earlier native passes verify CSS editing, Attach, receipt reopening, Draw, Region, Erase, direct Send, Cite, contextual PR loading, sidebar controls, and responsive context placement. Exact commands and source hashes are in the [verification manifest](evidence/live-browser/bootstrap-history-verification.json).

Implementation remains in the warmed `ferrite-t3-parity` worktree. Cleanup removed disposable compiler state and completed test artifacts, leaving 4.7 GiB free at the final check. Warm dependency outputs, the source, both audits, the main checkout, and the final verification app remain. No task commit, PR, merge, or release installation has occurred.

Everything below records earlier iterations. Historical claims about Keychain blocking or retained-screenshot annotation parity are superseded by the current records above.

## Historical implementation and verification

This change aligns Ferrite's sidebar, contextual PR panel, responsive context card, browser annotation controls, and selected-text citation comment popup with T3's implementation. The source authority is commit `4da2bc520c38cd6b36014d2888774168fbf4c77f` in `/tmp/ferrite-t3-reference`. The old `2026-11-13-t3-parity` directory remains unchanged.

## Live annotation acceptance correction

The first completion report did not establish the requested live T3 annotation comparison. Its claim that a full-panel retained editor was outside scope was wrong. The acceptance check was reopened. Actual T3 and native CEF Ferrite were then captured on the same local page. The correction keeps the full canvas, floats the toolbar, anchors the compact composer beside the selection and drawings, and expands existing options without resizing the image.

See [the paired native comparison](evidence/live-browser/paired-annotation-after.png), [the source and behavior report](live-annotation-correction.md), and [measurable rules](evidence/live-browser/measurements.json). The old fixture image remains historical evidence, not the final annotation result.

## Contracts and implementation

| Area | Source-backed contract | Evidence |
| --- | --- | --- |
| Sidebar | 256px column;52px header;search center76px;78/60px thread cards;14/12px text;grouped28px controls;active/selected washes | [Source comparison](sidebar.md) |
| PR | Conversation remains visible;explicit requested number;repository identity;identity above tabs on every view;black ground | [Source comparison](pr-context.md) |
| Context | 280px card;12px edge gap;32px conversation clearance;default-open inline;separate default-closed popover;measured density and Find clearance | [Source comparison](pr-context.md) |
| Annotation | 32px drawing tools;28px options;4px group padding;2px gaps;selected10% primary fill;framed inputs | [Source comparison](annotations.md) |
| Citation | 320px comment popup anchored to selection;64px minimum field;28px actions;chip fallback;save/cancel/dismiss | [Source comparison](citation.md) |

## Captures

- [Installed Ferrite before](evidence/native-before.png) and [context before](evidence/native-context-before.png).
- [Installed T3](evidence/t3-native.png). Its binary commit was not established, so it supplements the pinned-source rules.
- [User's broken citation screenshot](evidence/citation-user-before.png) and [actual T3 Cite popup](evidence/t3-citation-native.png). The T3 quote was cancelled after capture, the empty draft was restored, and no message was submitted.
- `evidence/baseline-built` uses original Ferrite commit `f7884cff` with only opt-in scenario setup added. It was built in a disposable worktree after `scripts/git-hooks/post-checkout` completed `scripts/warm-worktree.sh`. The worktree and its build directory were removed after capture.
- `evidence/first-built`, `evidence/final-built`, and `evidence/review-built` record correction iterations. The first annotation capture was rejected because an empty-browser notice hid the editor. The notice was fixed in production code and covered by a regression assertion.

The visual-reference captures use Ferrite's real Cockpit renderer and native GPUI framebuffer with isolated fixture stores. They do not use the operator's provider sessions or send prompts to a remote model. Captures are 2x device images; files suffixed `-1x` are reduced viewing copies.

## Review and boundaries

Independent review caught the duplicate sidebar header row, annotation occlusion, PR title/tab order, PR ground, bare-number routing, and Find height clearance. These were corrected rather than accepted as native rendering differences. The initial test failures and correction decisions are retained in [the decision trail](decisions.tsv).

Exact pixel parity and a numerical 90% similarity score are not claimed. T3's dynamic glass, palette choices, SVG fractal grain, existing-only context capabilities, PR wire-data differences, and native range anchoring differences are explicit in the per-area reports. The changes preserve Ferrite's supported data and repository boundaries. No release installation or merge is part of this work.

Performance review found no added polling, forge reads per frame, or unbounded redraw loop. Measurement callbacks refresh only after a changed measurement. Citation focus is requested once for each newly opened field. This is source review, not a measured speed claim.

Laziness Protocol kept the fixes local to the affected controls and state owners. Model the Domain separated inline/popover state and preserved typed PR identity. Prove It Works required inspecting actual rendered captures and exercising interactions, which exposed the hidden annotation editor.

## Earlier scoped verification

The final source passed one typecheck with `cargo check --locked -p ferrite --features headless,visual-reference`. Both the screenshot build and the headless-only production-reader build passed. The full suite was not run. Exact commands, results and the source diff hash are in [verification.json](verification.json).

The initial scoped run exposed nine failures. Incorrect legacy expectations and real defects were investigated, followed by narrow green checks for PR surfaces, PR read identity, all 17 citation interactions, and the final four changed UI flows. Sidebar geometry, navigation, selected state, panel focus, PR actions and retained annotations passed in the scoped run. The final four checks also exercised wide-to-narrow-to-wide context placement and a short canvas.

The real headless reader loaded [PR #308 beside the conversation](evidence/headless/e2e-loaded-308-1x.png). Its title remained visible after choosing Timeline. This was a read-only GitHub request, with a local fixture provider supplying the transcript link. A synthetic #7 attempt was not accepted as loader evidence because the screenshot feature suppresses nonfixture reads and the reader intentionally resolves host binaries before inherited PATH shims.

The same driven window selected assistant text, pressed Cite, typed a comment, saved it, and sent the citation to the local fixture provider. [The popup capture](evidence/headless/e2e-cite-comment-1x.png) and [recorded result](evidence/headless/e2e-result.json) confirm the interaction. The fixture's recorded user input contains both `amber` and `Please clarify this source.` No remote model was called. All fixture sessions, the headless window and its server were stopped afterward.

## Final images

- [Sidebar states](evidence/verified-built/nav-states-app-1x.png).
- [Persistent context](evidence/verified-built/conversation-app-1x.png).
- [Contextual PR](evidence/verified-built/panel-pr-app-1x.png).
- [Corrected native annotation comparison](evidence/live-browser/paired-annotation-after.png).
- [Citation comment popup](evidence/verified-built/cite-comment-app-1x.png).

The latest captures were opened and inspected. The later live comparison rejected the initial annotation placement despite passing element-bound checks. That correction is recorded above. The implementation and evidence are local. No release was installed and no PR was merged. About 12 GiB remained free after the final checks. The main warm build cache was preserved.


## Latest native verification

The six annotation checks passed with `cargo nextest run -p ferrite-pane -p ferrite-cockpit -E 'test(annotation_editor_) | test(browser_annotation_)'`. Run ID is `adbab413-6cc5-4b0a-97f1-1612aa296ea8`. The final typecheck passed with `cargo check --locked -p ferrite --features headless,visual-reference`. `scripts/install-app.sh --bundle-only` built the native CEF app without installing it. No production source changed after these checks.

The actual native flow selected the heading, drew a blue stroke, expanded options, entered a comment, attached the result, and reopened the retained evidence. Undo visibly removed the stroke in a separate attempt. Redo is covered by the narrow pointer test; the manual redo attempt used a stale coordinate after the composer moved and is not counted as a native pass. Escape cancelled that attempt before a clean final capture. The saved clean annotation contains exactly one pen drawing, the selected h1, the entered comment, and a verified original image hash. See [saved data](evidence/live-browser/saved-annotation.json) and [native result](evidence/live-browser/native-e2e-result.json).

The final send encountered a verification routing mistake. The GUI automatically restarted the isolated server before the manually launched server acquired its lock. That server replaced the fixture PATH and used host Claude. The turn was interrupted when detected and cost about $0.41. It is not counted as fixture-provider delivery. The earlier PR and citation fixture results remain separate. No additional repository changes appeared from that turn.

Both verification apps and owned fixture servers were stopped. T3's annotation was cancelled, its empty draft retained no attachment or submitted prompt, and its disposable preview was closed. The existing untracked 2026-11-13 audit remains untouched. The source and evidence remain local and uncommitted. The final native bundle is under `target/local/bundle/macos/Ferrite.app`. No release was installed and no merge occurred. About 9.6 GiB remained free at the final check.

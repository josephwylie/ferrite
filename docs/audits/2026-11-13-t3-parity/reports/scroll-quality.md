# Scroll quality investigation

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants Ferrite's scrolling to behave reliably through complete workflows, including the jump scrolling they report as sometimes buggy and weird. This pass tests the source-level interaction contracts behind those workflows. A scroll animation existing, or an endpoint test passing, does not establish that the reader keeps control while content, focus, disclosure, and layout change.

Throughput checkpoint: n/a, read-only investigation.

## Overview

Status: ISSUES. Ferrite source is pinned to `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. T3 source is pinned to `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, inspected locally at `/tmp/ferrite-t3-audit-source-20261113`. Both commits were verified with `git rev-parse HEAD`.

There are nine additional P2 findings in this report. Seven have source-proven control-flow or arithmetic defects. Two describe source-backed risks whose visible outcomes remain inferred. No finding is runtime-reproduced. No app code, tests, settings, dependencies, branches, or user data were changed. The parent owns live UI verification.

The most consequential pattern is split scroll ownership. Native tail following, an explicit tween, a sent-prompt anchor, synthetic bottom space, Find reveal, and resize correction each write the viewport. They do not all receive the same cancellation or identity-change signal. That makes individual working mechanisms insufficient evidence of production quality.

The existing `motion-008` and `motion-009` already cover stepped live following and far/unmeasured jump fallbacks. They are not duplicated in this report's JSON. `transcript-002` owns missing older-history paging. `transcript-001` owns search coverage of collapsed content. This report adds defects in the interactions between existing controls.

## Key concepts

`TranscriptScroll` owns the GPUI `ListState`, a resize anchor, a top mask, and an optional `ScrollEase`. The ease target is either a numeric `ListOffset` or the end. The native list independently tracks whether it follows the tail. See `crates/ferrite-pane/src/transcript/scroll.rs:14-43`, `:78-86`, and `:140-145`.

`TranscriptView` adds `send_anchor`, a stable `RowId`, and `Runway`, which stores a numeric row index, a pending jump, bottom room, and the measured height below that index. New prose consumes that room. Tool or reasoning activity releases the anchor by immediately scrolling to the bottom. See `crates/ferrite-pane/src/transcript.rs:450-456`, `:487-497`, `:1002-1055`, and `:3030-3078`.

The list virtualizes by row. Its measured height sum excludes unknown rows and excludes the list's bottom padding. Its logical top is a row index plus an offset within that row. List splices adjust that logical top, but Ferrite's other saved numeric indices are separate state. See `vendor/gpui-pre/src/elements/list.rs:537-547`, `:770-795`, `:878-883`, and `:1647-1662`.

T3 explicitly models navigation and follow state, ties positioning work to the current thread visit, and uses stable row/message identity when restoring or checking a pending anchor. That is evidence of a stronger stated transition contract, not proof that every T3 runtime path is flawless. See [navigation and follow state](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineViewport.ts#L35-L63), [visit state](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L48-L97), and [position restore](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelinePositionRestore.ts#L84-L147).

## Traced flows and transition contracts

| Flow | Current source behavior | Quality assessment |
| --- | --- | --- |
| Send an idle-thread prompt | Send adds the prompt, arms `jump_to_new_prompt`, then reconciliation chooses the last prompt and creates its runway and row ease. `cockpit.rs:5585-5596`; `transcript.rs:728-731,993-1025`. | Basic path is explicit. The target is a numeric row index after this point, and first work can terminate the movement abruptly. |
| Send while a turn runs | Queue path bypasses `scroll_transcript_to_new_prompt`. `cockpit.rs:5585-5596`. | Useful coverage. Existing test verifies the queued send does not move a reader in history. |
| First tool or reasoning during send motion | Every content reconciliation calls `release_send_anchor`; a work row immediately clears runway and calls `scroll_to_bottom`, which clears the ease. `transcript.rs:731,1033-1055`; `scroll.rs:111-114`. | No wait for the send tween to finish. The existing test's comment describes a combined landing after the ease, but the assertion checks only the later endpoint. This requires a frame-level runtime check. T3 also releases the prompt for work, so release itself is not a missing capability. |
| Jump to latest | Pill clears runway, hides itself, pauses native follow and starts an End ease. `transcript.rs:2323-2331`. | Correct intended handoff. Track clicks cannot reliably cancel it, and the pill's visibility metric is wrong with runway. Findings 001 and 003. |
| Wheel or trackpad while easing | The transcript's bubbled wheel handler clears the ease and sent-prompt anchor. Native upward scrolling stops follow. `transcript.rs:3198-3200`; `list.rs:938-940`. | Covered for one synthetic pixel event. The handler ignores event axis, delta and phase; continuous trackpad and nested-scroll behavior remain runtime gaps. |
| Thumb drag while easing | `step_ease` sees the drag flag and later defers `operator_scrolled`. `scroll.rs:164-169`; `transcript.rs:3017-3023`. | Covered through direct mutation of the drag flag, not the real thumb gesture. After the ease, the same check is unreachable. Finding 001. |
| Scrollbar track click | Sets offset without setting the dragging flag. `scrollbar.rs:879-905`. | It neither cancels the ease nor uses the inverse of the thumb's coordinate mapping. Findings 001 and 002. |
| Page Up/Down, Home, End | Page Up/Down clear runway and ease; Home clears runway; End calls a separate bottom path that keeps runway. `transcript.rs:1073-1102`. | Keyboard routing exists, but End and the latest pill do not have the same state transition. Finding 005. |
| Stream while following | Native tail follow resets the list to its end on each render/layout. `transcript.rs:2932-2933`; `list.rs:1043-1048`. | Functional following is present. Continuous motion is already `motion-008`; no measured quality or frame-time claim is made here. |
| Expand finished work at the tail | Toggle changes disclosures; reconciliation invalidates rows; follow remains active and the render requests the tail again. `transcript.rs:1364-1370,724-754,2932-2933`. | The clicked heading can move out of view. Finding 004. |
| Find while following or jumping | Opening Find takes keyboard focus. It only cancels scroll when the result's row does not overlap the viewport. `transcript/find.rs:40-51,136-172`. | A visible tall row lets Find and automatic scrolling compete. Finding 006. |
| Open/close right panel or resize | Width invalidates row heights. Ferrite restores a saved fully visible row's inset if it has one. `list.rs:1549-1563`; `scroll.rs:238-270`. | Protects row-level reading position in some layouts. A viewport inside one tall row has no saved fully visible row. Finding 007. |
| Switch subject/thread during movement | Each retained subject has its own transcript entity; only visible transcripts are synchronized. `pane.rs:378-436`; `cockpit.rs:2028-2039`. | Avoids directly applying thread A's offset to thread B. Pending movement is retained with A, and returning after its clock expires has no explicit cancellation/restoration contract. Treat this as a runtime gap, not a proven cross-thread leak. |
| Replace history or change row projection | History replay rebuilds activity; generation changes update namespace; row splices rebase native logical top. Only `send_anchor` is explicitly cleared for a namespace change. `cockpit/mirror.rs:313-324`; `cockpit.rs:2493-2499`; `transcript.rs:675-727`. | Ease target, runway index and resize index have no corresponding rebase/reset here. Finding 008. |

Ferrite paths in this table are abbreviated only within the table. `transcript.rs`, `transcript/find.rs`, `transcript/scroll.rs` and `pane.rs` are under `crates/ferrite-pane/src/`; `cockpit.rs` is under `crates/ferrite-cockpit/src/`; `cockpit/mirror.rs` is under `crates/ferrite-core/src/`; `scrollbar.rs` is under `crates/ferrite-ui/src/`; `list.rs` is `vendor/gpui-pre/src/elements/list.rs`.

## Source map

| Responsibility | Ferrite | T3 |
| --- | --- | --- |
| Send anchor and latest jump | `crates/ferrite-pane/src/transcript.rs:993-1075,2323-2331` | [useTimelineFollow.ts:305-329,374-427,498-530](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L305-L427) |
| Interruption | `crates/ferrite-pane/src/transcript/scroll.rs:155-169`; `crates/ferrite-pane/src/transcript.rs:1063-1070,3198-3200` | [timelineViewport.ts:86-155,221-240](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineViewport.ts#L86-L155) |
| Native geometry and reflow | `vendor/gpui-pre/src/elements/list.rs:537-547,878-883,1027-1217,1549-1563` | [useTimelinePositionRestore.ts:16-46,84-147](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelinePositionRestore.ts#L16-L147) |
| Disclosure positioning | `crates/ferrite-pane/src/transcript.rs:1364-1370`; `crates/ferrite-cockpit/src/cockpit.rs:2108-2114` | [MessagesTimeline.tsx:483-578](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L483-L578) |
| Scrollbar input | `crates/ferrite-ui/src/scrollbar.rs:571-575,867-1008`; `vendor/gpui-base/src/scrollbar.rs:120-142` | [timelineViewport.ts:91-100](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineViewport.ts#L91-L100) |
| Existing test intent | `crates/ferrite-cockpit/src/cockpit/tests/send_scroll.rs`; `tests/transcript_pointer.rs`; `tests/find.rs`; `tests/subagents.rs:150-253` | [MessagesTimeline.browser.test.tsx](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.browser.test.tsx), [timelineScrollAnchoring.test.tsx](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineScrollAnchoring.test.tsx) |

## Detailed findings

### scroll-quality-001. Scrollbar navigation does not consistently take ownership from automatic scrolling

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact. A scrollbar action can be overwritten by the existing jump or by the first tool event of a sent turn. This invalidates a broad claim that any manual scroll interrupts the programmatic movement.

Trigger. Click the scrollbar track while a send or latest jump is easing. Separately, let a send ease finish with runway remaining, drag the thumb a short distance into history, then let the turn produce its first tool/reasoning row before the runway has been consumed.

Expected behavior. Both track clicks and thumb movement transfer viewport ownership to the reader immediately. Later provider events must not restore automatic scrolling without the reader reaching or requesting the end.

Actual behavior and cause. The track-click branch calls `set_offset` but never calls `start_drag`. The ease cancels only when `is_scrollbar_dragging()` is true. Consequently the next ease frame can overwrite the clicked offset. After an ease has finished, `step_ease` returns Idle before checking the drag flag. A subsequent thumb drag stops native following but leaves `send_anchor` intact. If runway still remains, `release_send_anchor` sees the first work row and unconditionally calls `scroll_to_bottom`; it does not check that the native list is no longer following. This second case is conditional on runway remaining, not a claim that every thumb drag later snaps.

Ferrite evidence. `crates/ferrite-ui/src/scrollbar.rs:871-905`; `vendor/gpui-base/src/scrollbar.rs:125-142`; `crates/ferrite-pane/src/transcript/scroll.rs:163-169`; `vendor/gpui-pre/src/elements/list.rs:1371-1416`; `crates/ferrite-pane/src/transcript.rs:1033-1055,3017-3023`.

T3 evidence. Scrollbar presses enter its common navigation signal at [timelineViewport.ts:91-100](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineViewport.ts#L91-L100). Navigation stops anchor positioning and pending restores at [useTimelineFollow.ts:238-248](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L238-L248). Its browser test distinguishes a scrollbar press from empty padding at [timelineViewport.browser.test.tsx:83](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineViewport.browser.test.tsx#L83).

Recommendation. Make manual scrollbar positioning emit the same reader-navigation transition as wheel/page input, independently of whether an ease exists. Cancel pending send anchoring and positioning before applying the user's offset. Preserve the intended rule for deliberately dragging to the end.

Acceptance check. Use real pointer down/move/up and a real track click at several points of a running jump. Then repeat the thumb gesture after the tween finishes while a short prose response leaves runway. Deliver a tool event afterward. In every interrupted case, the viewport stays at the reader's chosen content through later frames and events. Dragging to the bottom must still resume following.

Duplicate relationship. Extends `motion.md`'s manual-input inventory, which correctly identifies a cancellation path but overgeneralizes its coverage. This is not the live-follow smoothness issue `motion-008`.

### scroll-quality-002. Scrollbar track clicks use the wrong scroll distance

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact. Clicking a position on the track lands farther down than the thumb's corresponding position, and lower track clicks clamp to the end early. Repeated clicks can feel inconsistent with dragging the same thumb.

Trigger. On a transcript whose content exceeds one viewport, click the middle or lower part of the scrollbar track, outside the thumb.

Expected behavior. The chosen track policy must have a consistent geometric mapping. The current comment promises to center the thumb on the click; that requires the inverse of the mapping used to paint the thumb.

Actual behavior and cause. The thumb position divides scroll position by `scroll_area_size - container_size`. Dragging uses that same scrollable distance. Track clicking instead multiplies the track fraction by the full `scroll_area_size`, then clamps to the smaller valid range. For a 2,000px content extent and 500px viewport, a computed track fraction of 0.5 produces a 1,000px offset instead of 750px. The discrepancy follows directly from the formulas; the example is arithmetic, not a measured UI trace.

Ferrite evidence. `crates/ferrite-ui/src/scrollbar.rs:544-554,571-575,860-903,959-978`; `vendor/gpui-base/src/scrollbar.rs:133-134`; `crates/ferrite-ui/src/components.rs:1904-1908`.

T3 evidence. T3 gives the timeline a native overflow scroller at [MessagesTimeline.tsx:1471-1476](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L1471-L1476). Its exact track-click policy belongs to the browser and OS; no claim of matching a particular platform policy is needed to establish Ferrite's inconsistent inverse mapping.

Recommendation. Use one coordinate conversion for painted thumb position, thumb drag, and track click, with explicit handling of inset and minimum thumb size. If the product wants page-by-page track clicks instead, implement and test that policy explicitly.

Acceptance check. With known content and viewport extents, click at 25%, 50%, and 75% of the effective thumb-travel range. The resulting thumb center matches the click within the specified pixel tolerance, except when an end clamp is necessary. Repeat with the minimum-size thumb and horizontal scrollbars, which use the same faulty branch.

### scroll-quality-003. The latest button undercounts hidden content when send runway remains

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact. Real answer content can remain below the viewport while the latest button and lower-edge cue are absent. A reader can reasonably conclude they have reached the latest visible content when they have not.

Trigger. Send into a long thread, let a short prose response leave synthetic bottom runway, then wheel upward far enough to put part of that response below the viewport while its rows remain measured.

Expected behavior. The latest button's threshold and bottom fade use actual hidden content distance. Artificial blank room must not reduce that distance twice.

Actual behavior and cause. GPUI's `max_offset_for_scrollbar()` is `max(measured_row_height - viewport_height, 0)` and does not include bottom padding. `rows_below` adds the negative current scroll offset, then subtracts both normal bottom padding and runway again. For fully measured overflowing rows with height H, viewport V, scroll position S and foot space P, the real row distance below is `H - V - S`; Ferrite uses `H - V - S - P`. With H=2,000, V=600, S=1,300, and P=200, 100px of rows are below the viewport, but Ferrite reports -100px. These are a source-derived example's inputs, not runtime measurements.

The runway can survive the user's wheel action by design: `operator_scrolled` clears only its pending jump, and the room remains until growth consumes it. The pill needs `rows_below >= two lines`; the bottom fade needs `rows_below > 0`. Both inherit the undercount.

Ferrite evidence. `vendor/gpui-pre/src/elements/list.rs:772-795,878-883`; `crates/ferrite-pane/src/transcript.rs:1063-1070,2204-2229,2980-2984,3069-3078,3219-3222`.

T3 evidence. T3 derives whether it is at the end from content length, scroll offset and viewport length at [MessagesTimeline.logic.ts:319-336](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.logic.ts#L319-L336), and removes anchored space before a requested end scroll at [useTimelineFollow.ts:305-325](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L305-L325).

Recommendation. Separate the real content-end metric from the padded scrollable-end metric. Audit every consumer against GPUI's actual inclusion of padding and measurement estimates.

Acceptance check. Use a short response with at least 200px of runway, then position it so more than two lines of real content lie below the viewport. The latest button appears after its configured delay and the lower fade is present. Clicking latest reaches the actual final row and removes runway. Repeat with no runway and with rows whose heights are not yet measured.

### scroll-quality-004. Expanding finished work at the tail does not preserve the clicked heading

Priority P2. Category `interaction`. Evidence `source-proven`.

Symptom and impact. Opening a completed turn or large tool detail can move the heading upward by the height added below it, potentially out of the viewport. The reader has to find the control again to understand or collapse what they just opened.

Trigger. At the followed tail of a finished thread, click a visible Worked fold that hides more than a viewport of intermediate work. The same ownership problem applies to a large expanding tool body.

Expected behavior. For a finished turn, preserve the clicked heading at its visible inset while revealing its content. If a smaller reveal near the bottom needs a scroll, move only enough to show the new content and keep the heading visible. Define a separate policy for expanding a currently streaming turn.

Actual behavior and cause. The click only clears output selection and emits a disclosure toggle. The cockpit changes disclosure state. Reconciliation remeasures/splices rows without pausing follow or recording the clicked heading's position. The next render/layout follows the tail. A generic top-row anchor while detached is not an anchor on the control the user clicked.

Ferrite evidence. `crates/ferrite-pane/src/transcript.rs:1364-1370,711-754,2932-2933`; `crates/ferrite-cockpit/src/cockpit.rs:2108-2114`; `vendor/gpui-pre/src/elements/list.rs:1043-1048`; `crates/ferrite-pane/src/transcript/scroll.rs:311-324`.

T3 evidence. [MessagesTimeline.tsx:535-573](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L535-L573) cancels movement and anchors the toggled row, with an explicit exception for active streaming follow. [MessagesTimeline.browser.test.tsx:298-340](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.browser.test.tsx#L298-L340) checks that expanding more than 500px of hidden work does not move its heading. Reveal correction lives at [MessagesTimeline.tsx:492-523](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L492-L523).

Recommendation. Give disclosure actions a pre-change anchor and a post-measure reveal contract. Apply it to turn folds, groups and individual details. Retain live following only when that is the intended working-turn behavior.

Acceptance check. At an idle tail, expand and collapse a fold hiding several viewport heights. Its heading remains at the same screen position. Repeat for a long tool result near the composer and during a pending latest jump. Rapid reversal must leave neither an old animation nor a previous reveal command in control. Test live streaming separately.

Duplicate relationship. `motion.md` inventory row 37 records the abrupt work-group and turn-fold body reveal. `transcript-004` concerns the nested work-list height cap. This finding concerns viewport ownership and remains a defect even if the reveal is instantaneous and the cap is correct.

### scroll-quality-005. Keyboard End retains the send spacer that the latest button removes

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact. End and the latest button can produce different resting positions. End can preserve a large blank area below the newest message. If a runway jump is still being sized, its layout callback can reassert the prompt position after End.

Trigger. Send a prompt with normal or reduced motion and press End while artificial bottom room remains. Include the first frames of runway sizing, then repeat after the send animation has settled.

Expected behavior. Every explicit command to return to the live tail clears send-specific positioning and reaches the same content position. Reduced motion changes timing, not the destination or retained state.

Actual behavior and cause. `Page::Bottom` invokes `TranscriptView::scroll_to_bottom`. That clears `send_anchor` and the ease but leaves `Runway` untouched. The end pill explicitly replaces `Runway` with its default before it moves. Bottom padding still contains runway, and a pending runway jump is allowed to call native `scroll_to` whenever no ease exists.

Ferrite evidence. `crates/ferrite-pane/src/transcript.rs:1073-1091,2323-2331,2980-2984,3042-3062`; `crates/ferrite-pane/src/transcript/scroll.rs:111-114`; `crates/ferrite-cockpit/src/cockpit.rs:2069-2076`.

T3 evidence. Its explicit end command releases the timeline anchor before measuring the destination at [useTimelineFollow.ts:305-325](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L305-L325). This is a destination-state comparison, not a claim that T3 maps every keyboard key identically.

Recommendation. Route all explicit return-to-end actions through one state transition. Release pending prompt positioning and synthetic room before measuring or landing; let callers separately choose animated or immediate motion.

Acceptance check. From identical send states, compare End and the end pill after layout settles. Both leave zero runway, put the same last real row at the same inset, and follow subsequent text. Press End during the first sizing frame; later layout callbacks must not restore the old prompt anchor.

### scroll-quality-006. Find can compete with an existing jump or live follow inside a visible row

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact. A highlighted result can move away while the reader searches it. During an existing ease, Find can move to the occurrence and the ease can immediately move back toward its old target.

Trigger. While following a long streamed answer, open Find for text within its currently intersecting row. Also start a latest jump, open Find before it settles, and search for an occurrence in a tall row that still intersects the viewport.

Expected behavior. Opening Find or initiating a result reveal explicitly claims the viewport. Older jump, sent-prompt, and live-follow work must not override the active occurrence. Later content changes may update counts without moving away from the hit.

Actual behavior and cause. `open_find` creates the bar and takes keyboard focus without ending follow or ease. `reveal_found` calls `operator_scrolled` only if the row has no overlap with the viewport. A tall row can overlap even when the occurrence lies far outside the visible part. `settle_found` directly calls the native list's `scroll_by`; it never clears the ease or send anchor. A currently visible occurrence can leave native follow active too. Render subsequently advances the old ease or forces the tail.

Ferrite evidence. `crates/ferrite-pane/src/transcript/find.rs:40-51,136-172`; `crates/ferrite-pane/src/transcript/scroll.rs:229-233`; `crates/ferrite-pane/src/transcript.rs:2921-2933,3003-3009`.

T3 evidence. Find participates in navigation and position ownership, with browser tests for releasing follow and choosing Find's match over a restored row at [MessagesTimeline.browser.test.tsx:1737-1819](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.browser.test.tsx#L1737-L1819). Restore also treats Find as a reason to release follow at [useTimelinePositionRestore.ts:84-99](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelinePositionRestore.ts#L84-L99).

Recommendation. Make Find navigation revoke earlier viewport ownership regardless of whole-row visibility. Keep the occurrence reveal and its later measurement callbacks scoped to the same search request and transcript identity. User scrolling must be able to cancel the reveal too.

Acceptance check. With motion active, search within a tall visible row halfway through a latest jump. The hit remains clear of the pinned band after the old tween's maximum duration and after additional streamed content. Repeat when the match is already visible at the live tail. Manual wheel, scrollbar and page input after searching must take control without a delayed Find correction.

Duplicate relationship. `transcript-001` owns hidden-content search coverage. This finding assumes the match is searchable and concerns whether reaching it remains stable.

### scroll-quality-007. Width changes lack a reading anchor inside a tall row

Priority P2. Category `partial-capability`. Evidence `inferred`.

Symptom and impact. Opening the right panel or narrowing the window while reading the middle of a long answer can change which sentence occupies the reading position. A preserved row index and pixel offset can conceal this loss of place in endpoint tests.

Trigger. Detach from the tail and position the viewport entirely within one answer row whose top is above and bottom below the viewport. Then toggle the right panel or resize enough to change wrapping.

Expected behavior. Preserve a stable visible passage or text position through reflow, or specify and test a deliberate fallback for long rows.

Actual behavior and cause. `did_layout` only saves an anchor when a row's top lies inside the viewport. In this trigger it saves None. GPUI invalidates all row heights when width changes but keeps the logical row offset; it does not map that offset to a text position. Ferrite's inset-triggered remeasure also uses an absolute offset. No source path in this trace preserves the same character/paragraph within a tall row. The actual amount of displacement depends on the text and widths, so this is inferred rather than a reproduced defect.

Ferrite evidence. `crates/ferrite-pane/src/transcript/scroll.rs:238-281`; `vendor/gpui-pre/src/elements/list.rs:412-425,1041-1048,1082-1099,1549-1563`; `crates/ferrite-pane/src/transcript.rs:3147-3158`.

T3 evidence. T3 has size-change visible-content maintenance at [MessagesTimeline.tsx:909-912](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L909-L912), and row/offset restoration at [useTimelinePositionRestore.ts:105-144](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelinePositionRestore.ts#L105-L144). Those sources do not prove that T3 preserves a sentence within a rewrapped tall row either. This is a production-quality requirement to verify in both apps, not a claimed T3 advantage.

Recommendation. Preserve a text-level or sub-row anchor for tall prose, and a suitable local anchor for code/output/visual content. Keep the existing row-inset strategy for layouts where it is sufficient. Do not introduce full-history measurement to solve this local case.

Acceptance check. Put a distinctive sentence in the middle of a multi-viewport answer. After panel open, close, maximize, restore and window-width changes, that sentence remains at the same readable inset within a documented tolerance. Repeat with a code fence and with a fully visible row boundary to retain existing behavior. Record the actual installed-build result before promoting this to a reproduced defect.

### scroll-quality-008. Saved jump, runway and resize indices are not rebased when rows change

Priority P2. Category `defect`. Evidence `inferred`.

Symptom and impact. A row insertion/removal or history replacement can leave pending positioning aimed at the wrong row or at an obsolete row index. The risk is greatest when a fold changes or retention removes earlier rows during a send, or when width reflow and structural changes share a frame.

Trigger. While a row ease or runway placement is pending, insert/remove rows before its target through a disclosure change, history replacement or retention. Separately, resize while an earlier disclosure changes the saved resize row's numeric position.

Expected behavior. Pending operations identify their semantic target, resolve its current row index after reconciliation, and cancel if the target or transcript generation disappears. Every stored anchor must agree on the current projection.

Actual behavior and cause. The native list rebases its logical top in `splice`, but `TranscriptScroll::reconcile` does not rebase `EaseTarget::Row` or `resize_anchor`. `Runway.index` is also separate. `send_anchor` is a stable ID, but that ID does not drive the ease target or runway's later measurements. A namespace change clears `send_anchor` only; it does not clear ease, runway or resize state. These omissions are source-proven. A particular user-visible wrong-row landing requires a concurrent transition, so the finding's evidence level remains inferred.

Ferrite evidence. `crates/ferrite-pane/src/transcript/scroll.rs:18-24,30-42,172-179,245-267,316-324`; `crates/ferrite-pane/src/transcript.rs:492-497,721-731,1013-1025,3035-3062`; `vendor/gpui-pre/src/elements/list.rs:537-547`; `crates/ferrite-pane/src/pane.rs:355-361,403-436`; `crates/ferrite-cockpit/src/cockpit.rs:2493-2499`.

T3 evidence. T3 scopes follow state to a visit and verifies an anchored message is still present at [useTimelineFollow.ts:48-97,374-423](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L374-L423). Its restore resolves a stable row ID again while settling at [useTimelinePositionRestore.ts:124-144](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelinePositionRestore.ts#L124-L144). Hydration tests explicitly distinguish a newly started run from loaded history at [timelineScrollAnchoring.test.tsx:19-66](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineScrollAnchoring.test.tsx#L19-L66).

Recommendation. Represent pending targets by `RowId` plus transcript generation. Resolve them after each projection change. Reset or deliberately translate all saved geometry together on history replacement. Define what should happen if the target folds away or is evicted; silently clamping a stale index to the end is not a sufficient contract.

Acceptance check. Insert and remove rows before the target midway through a send ease, then confirm it still lands on the same prompt ID. Repeat with a width change and with generation replacement. An absent target cancels cleanly. Returning from a different thread must never revive an operation against a different history generation.

### scroll-quality-009. Page navigation does not subtract the pinned prompt's covered area

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact. Page Down can skip lines concealed by the pinned prompt after the move. Page Up has the corresponding overlap problem. The user cannot read the transcript continuously by paging even though the offset changes by the expected nominal page size.

Trigger. Read a long answer under a pinned prompt whose covered height exceeds one transcript line, then press Page Down repeatedly.

Expected behavior. Each page retains a readable overlap with the previous page after accounting for the pinned prompt. No sentence is hidden on both sides of a page transition.

Actual behavior and cause. `scroll_page` moves by the full viewport height minus `keep`, and its caller passes one text line. It does not subtract `top_mask`, although that mask is already measured for Find. The pinned prompt is an absolute overlay. With viewport height V, pinned coverage M and a retained line L, the readable range starts M pixels below the scroll origin; a page step of V-L skips M-L pixels when M exceeds L. This follows from the source geometry, not a measured reproduction.

Ferrite evidence. `crates/ferrite-pane/src/transcript/scroll.rs:128-133,284-297`; `crates/ferrite-pane/src/transcript.rs:1081-1087,2158-2173,3082-3085`; `crates/ferrite-ui/src/theme.rs:1928`; `crates/ferrite-pane/src/transcript/find.rs:167-171`.

T3 evidence. Its page distance explicitly deducts the composer overlay from usable height at [pageScrollController.ts:115-120](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/pageScrollController.ts#L115-L120). This supports the usable-viewport contract; it is not a claim that T3 has Ferrite's exact pinned-prompt geometry.

Recommendation. Compute page travel from the currently unobscured viewport and retain at least one readable line. Reuse the measured top mask and define behavior when the pinned prompt changes height during the move.

Acceptance check. Use numbered answer lines under a two-line pinned prompt. Page down and up through the whole answer. Every line appears in an unobscured viewport at least once and each adjacent page has a visible overlap. Repeat with a short prompt, no pinned prompt, and a prompt that changes while paging. The current page-key test checks offset and follow state, not this content-coverage property.

## Existing tests and what they do not establish

| Existing coverage | Actual assertion boundary | Missing adversarial check |
| --- | --- | --- |
| `crates/ferrite-cockpit/src/cockpit/tests/send_scroll.rs:78-108` | At least one intermediate offset, eventual prompt near the top, and later stability. | Far unmeasured target, direction reversals, streaming growth during the movement, first-frame teleport, actual frame pacing. |
| `send_scroll.rs:112-171` | One wheel event, direct `page()` call, or direct `scrollbar_drag_started()` ends a running ease. | Real scrollbar hit testing, track click, quick press/release between frames, drag after the ease, first tool after the reader's drag. |
| `send_scroll.rs:176-191,196-228` | Reduced-motion landing and bubble starts after send movement. | End while runway is pending; cancellation before the bubble has its first visible frame; dynamic reduced-motion changes. |
| `send_scroll.rs:233-275` | First call releases a settled prompt to the tail. | Reader navigation between settlement and first call. |
| `send_scroll.rs:278-312` | A call arrives mid-ease; after 60 frames, the anchor is gone and tail follow is true. | Its comment claims a landing after the ease. No intermediate assertion establishes that choreography or forbids an immediate snap. |
| `send_scroll.rs:329-377` | Button delay, no button while at the tail, click eases and preserves composer focus. | Button metrics with retained runway; repeated jump requests; interruption by Find, track click, resize or disclosure. |
| `send_scroll.rs:383-429` | Queued send stays put; Find hides the pill. | Queued run starting after completion; Find taking ownership from an already-running ease or from live follow. Hiding a button does not establish cancellation. |
| `crates/ferrite-cockpit/src/cockpit/tests/transcript_pointer.rs:46-123,755-794` | Wheel detach/reattach and production page key bindings with an empty composer. | Trackpad momentum phases, diagonal/horizontal gestures, IME/composer boundaries during motion, synthetic runway on End. |
| `crates/ferrite-cockpit/src/cockpit/tests/find.rs:67-127,266-323` | Offscreen result and occurrence inside a tall row can be reached. | Same reveal while automatic scrolling is active, followed by late stream/tool events. |
| `crates/ferrite-cockpit/src/cockpit/tests/transcript_rows.rs:1314-1382` | Fold renders, opens, persists through re-projection, and closes. | Clicked-heading position when a large fold opens at the tail. |
| `crates/ferrite-cockpit/src/cockpit/tests/subagents.rs:150-253` | A manually detached child's offset, disclosure and text entity survive a switch away/back. | Switch during ease, data growth while hidden, generation replacement, or new viewport width on return. |
| `crates/ferrite-cockpit/src/cockpit/tests/render_performance.rs:660-732,864-947,1079-1126` | Selection/entity and layout survive reflow. | The same visible sentence remains at the reading inset inside a tall row. |
| `crates/ferrite-ui/src/scrollbar.rs:1080-1114` | Thumb padding starts a drag and hit bounds stay in the gutter. | The track-click offset is the inverse of thumb painting. |

T3's corresponding coverage includes browser assertions for keeping the completed-work heading still, retaining live follow for an expanding working turn, very different row heights, row ordering through switches, and finding after restore. See [MessagesTimeline.browser.test.tsx:298-456](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.browser.test.tsx#L298-L456), [row order tests](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.browser.test.tsx#L708-L798), and [Find/restore tests](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.browser.test.tsx#L1737-L1819). These were inspected, not executed.

The original narrow existing-test command completed successfully: `cargo nextest run -p ferrite-cockpit -E 'test(cockpit::tests::send_scroll::)'`. Nextest run `1f15cb61-d17c-4e99-b1ce-2ebbda99dfae` ran 12 tests, all passed, with 938 tests skipped and exit code 0. The build took 12m 01s; the test summary reports 3.217s. The original agent-owned exec session `58140` was collected on followup, without rerunning. [The evidence log](../evidence/scroll-nextest.log) preserves the full final output chunk, including every test result and summary; earlier dependency-compilation progress is not repeated there. The existing test boundaries above still apply. No second test command, typecheck, E2E or live UI run was performed by this worker. The report-only edits require no Rust typecheck.

## Positive Ferrite behavior and supported T3 capabilities

Ferrite preserves row identity through projection reconciliation, maintains native tail follow, allows detached history reading, reserves answer room after sends, delays the latest pill, preserves composer focus when it is clicked, and retains separate subject transcript entities. These mechanisms are valuable. The tests above establish specific intended invariants; they do not establish combinations absent from their fixtures.

T3 adds explicit shared navigation signals, visit-scoped positioning state, a bounded measurement settle loop, disclosure-specific anchors, and tests that assert the screen position of a clicked heading. It also distinguishes hydration from a new run and the initially mounted list from a later thread visit. See [navigation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineViewport.ts#L35-L63), [settling](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelinePositionRestore.ts#L6-L46), and [hydration tests](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/timelineScrollAnchoring.test.tsx#L19-L66). T3 uses smooth active following in its list configuration at [MessagesTimeline.tsx:1449-1458](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L1449-L1458).

Ferrite's native list also already rebases pending measurements after manual scroll and freezes scrollbar geometry during a drag. Those mechanisms should be retained. See `vendor/gpui-pre/src/elements/list.rs:739-752,931-935,1377-1416`. The defects above arise partly because that native input state and Ferrite's higher-level send/ease state are not one transition.

## Runtime verification gaps and handoff

The user's reported weirdness is not yet attributed to one installed-build cause. The following checks can discriminate the source findings without modifying application code or existing data beyond an approved test thread.

1. Record a send from at least five viewports back, including its first frame. Repeat with a new prompt that has not been measured. Separate the initial far jump from the final ease. This verifies `motion-009`, rather than counting any intermediate frame as smooth scrolling.
2. Interrupt the movement with a real wheel, trackpad with momentum, thumb drag and track click. Repeat after the movement settles but before the first tool arrives. Verify the same visible passage stays put after several later tool/text events.
3. Create a short prose response with runway. Scroll upward only enough to hide several real answer lines while keeping the prompt close to view. Check the latest button and lower fade. Compare End with the pill.
4. Expand a large finished Worked fold at the tail. Record the heading before and after. Repeat with a work group, a long nested tool result, and a collapse while a latest jump is active. Keep live-turn expansion separate from completed-turn reading.
5. Open Find during a jump and while following a growing tall answer. Test a match inside a row already intersecting the viewport. Continue streaming after the occurrence appears.
6. While detached in a tall answer, toggle the right panel, maximize it, restore it, drag its divider, and resize the window. Track a distinctive sentence, not only the aggregate scroll offset or the presence of the same row entity.
7. Start movement in thread A, switch to B, and return before and after the original tween duration. Repeat with A streaming while hidden. Assert that B never moves from A's callbacks and that A resumes according to an explicit reading-position policy. Existing entity isolation is positive evidence; it is not a runtime result for this sequence.
8. Exercise late hydration and replacement of history during a pending jump, including changed row counts. Initial empty hydration should still open at the tail. A previously detached reading position should resolve by identity or show an explicit fallback, without reviving an old numeric target.
9. Cross the retention limit while reading older content and while a jump is active. Test row insertion/removal above the anchor and mixed short/tall rows. Older-history retrieval remains `transcript-002`; this check concerns stability of the retained viewport.

An acceptance contract should assign exactly one owner to the viewport at a time: reader navigation, a requested jump, Find, disclosure reveal, or restoration. Tail following should be an explicit mode that those transitions can suspend or resume. Row identity and transcript generation should qualify every delayed positioning command. That change can reuse the native list and existing motion code; a new animation layer alone would leave these conflicts intact.

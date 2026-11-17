# Motion investigation

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants useful, coherent motion across every Ferrite interface, especially sidebars that currently snap. This report inventories current source behavior, compares it with current T3 Code, and identifies what needs runtime confirmation. T3's default of zero panel animation does not override that goal.

## Overview

Ferrite has a substantial motion implementation. The strongest gap is structural coverage, not missing animation machinery. Its main sidebar and right panel explicitly snap. Most panel bodies also switch directly, despite the motion catalog saying tab bodies fade. Menus, shelves and several exits are intentionally immediate. The operating-system reduced-motion bridge reads macOS only, once at startup.

The historical comparison is stale in several important places. Current Ferrite already implements sidebar row movement with retained exits, smooth send scrolling, smooth jump-to-end, bubble-after-scroll choreography, attachment entrances, find-bar fades and delayed scroll-pill display. These must not be filed as missing features. Current T3 has also changed: right-panel width animation is limited to open/close and explicitly suppressed on resize and maximize.

This investigation inspected Ferrite commit `62947f91d5cd6ee494aaae9ed09d794a853e5bab` and T3 commit `74573fc29b901bafbf8f1dbb2eb29606da1fa448`. `git rev-parse HEAD` confirmed both. T3's origin is `https://github.com/pingdotgg/t3code.git`. Historical `docs/design/t3-reference/motion.md` targets `bcee0f5d` and is evidence of old intent only. ADR 0038 keeps Ferrite's stronger bubble and text motion; ADRs 0039 and 0040 establish the shared panel and flat inbox navigation that the matrix covers.

Method: broad source searches for animation, transition, Tween, Reveal, scroll, reduced-motion and their callers; then reads of state changes and rendering branches. No app code, settings or data changed. No runtime UI was driven and no benchmark or test was run. Durations below are source constants, not measured response times, frame rates or network timings. Source-proven behavior describes this checkout, not the installed binary.

## Key concepts

- A transition needs a state change, an animation clock, a drawn intermediate state and a final state. A token or unused helper alone does not establish coverage.
- `Tween::retarget` samples the displayed value before choosing a new target. `Reveal` combines enter and exit specs, supports reversal, and lets callers retain a closing view. `Exits<T>` keeps outgoing dialog or toast content until its exit completes. See `crates/ferrite-ui/src/motion.rs:923-947`, `:1004-1042` and `:1335-1365`.
- Ferrite's painted scale does not change layout or hit bounds. The motion catalog explicitly records this at `crates/ferrite-ui/src/motion.rs:84-89`. Animated entry/exit must therefore be checked for input ownership, not judged from screenshots alone.
- The native application flag controls most Ferrite one-shots, loops, tweens and hover blends. The missing part is live, cross-platform adoption of the OS preference, rather than the end-state implementation. See `crates/ferrite-ui/src/motion.rs:2078-2082` and `:2152-2177`.
- T3's panel duration setting accepts 0 to 400ms and defaults to zero. Its provider suppresses animation through two animation frames after navigation; closing panels retain their last content and clear the removal timer if reopened. These are separate concerns from panel width and tab-content motion. See T1 and T2 below.

## Source map

All Ferrite citations are repository-relative. Every T3 source below is pinned to the inspected commit. Matrix references resolve through this map.

- F1: `crates/ferrite-ui/src/motion.rs:17-70, 285-374, 923-1042, 1335-1365`.
- F2: `crates/ferrite-ui/src/theme.rs:3531-3574, 3578-3790`.
- F3: `crates/ferrite-cockpit/src/cockpit.rs:2829-2838, 2841-2891`.
- F4: `crates/ferrite-cockpit/src/cockpit/panel.rs:399-456, 498-522, 550-694`.
- F5: `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:244-267, 337-449`.
- F6: `crates/ferrite-cockpit/src/nav/flip.rs:137-255, 360-367, 435; crates/ferrite-cockpit/src/cockpit/nav_list.rs:69-84, 190-217`.
- F7: `crates/ferrite-cockpit/src/nav/list.rs:114-117, 168-179, 238-263; crates/ferrite-cockpit/src/nav/chrome.rs:50-66`.
- F8: `crates/ferrite-cockpit/src/cockpit/beside.rs:14-27, 606-622; crates/ferrite-cockpit/src/cockpit/tests/reader_motion.rs:104-294`.
- F9: `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:331, 406-421`.
- F10: `crates/ferrite-pane/src/transcript/scroll.rs:62-70, 78-80, 111-221; crates/ferrite-pane/src/transcript.rs:999-1068, 2320-2332, 2932-2933`.
- F11: `crates/ferrite-pane/src/transcript.rs:815-899, 2340-2381, 2432-2437, 2973; crates/ferrite-pane/src/rich.rs:583-588`.
- F12: `crates/ferrite-pane/src/composer.rs:376-425, 557-579, 694-707, 1704-1743; crates/ferrite-pane/src/attachments.rs:140-146; crates/ferrite-pane/src/pane/composer.rs:257-260, 548-558`.
- F13: `crates/ferrite-pane/src/find.rs:322-325; crates/ferrite-pane/src/pane/background_work.rs:167-170; crates/ferrite-pane/src/decision.rs:688-730`.
- F14: `crates/ferrite-cockpit/src/cockpit/browser.rs:1491-1540; crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:460-486`.
- F15: `crates/ferrite-cockpit/src/cockpit.rs:4467-4469, 5296-5313, 5334-5336; crates/ferrite-cockpit/src/cockpit/palette.rs:1461; crates/ferrite-cockpit/src/cockpit/context_menu.rs:1001`.
- F16: `crates/ferrite-pane/src/image_viewer.rs:402-407, 671-743, 884; crates/ferrite-pane/src/pane/usage.rs:214-217`.
- F17: `crates/ferrite-ui/src/scrollbar.rs:42-70; crates/ferrite-ui/src/motion.rs:1424-1477, 1658-1659, 1958-1993, 2078-2177`.
- F18: `crates/ferrite-cockpit/src/cockpit/panel/sections.rs:249-278; crates/ferrite-pane/src/pane/work_row.rs:226-229; crates/ferrite-pane/src/pane/working.rs:285-328`.
- F19: `crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:1-16, 90-116; crates/ferrite-ui/src/motion.rs:57-70`.
- F20: `crates/ferrite-cockpit/src/cockpit/tests/send_scroll.rs:167-196, 329-419; crates/ferrite-cockpit/src/cockpit/tests/render_performance.rs:1203; crates/ferrite-ui/src/motion.rs:2085-2098`.
- T1: [Panel duration range and default](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/settings.ts#L155).
- T2: [Navigation suppression, settings and retained panel presence](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/panelAnimations.ts#L11).
- T3: [Sidebar widths, side position and resize suppression](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/sidebar.tsx#L388).
- T4: [Panel width suppression and opening/closing transition](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewPanelShell.tsx#L81).
- T5: [Terminal grid-height transition](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/PersistentThreadTerminals.tsx#L245).
- T6: [List movement, interruption and inert cloned exits](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.motion.ts#L1).
- T7: [End scrolling, anchor positioning and release](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L305).
- T8: [Conditional smooth live follow](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L1449).
- T9: [Popover geometry and crossfade](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/popover.tsx#L82).
- T10: [Menu popup without transition](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/menu.tsx#L56).
- T11: [Tooltip positioning, scale and opacity](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/tooltip.tsx#L109).
- T12: [Dialog entry, exit and nested stack](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/dialog-styles.ts#L1).
- T13: [Toast translation, opacity and height](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/toast.tsx#L614).
- T14: [Optional collapsible height transition](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/collapsible.tsx#L21).
- T15: [Attachment entrance and hover controls](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ComposerAttachmentStrip.tsx#L127).
- T16: [Reduced-motion guarded streamed block fade](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/index.css#L2246).
- T17: [Browser loading and reduced-motion fallback](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/index.css#L2424).
- T18: [Visible shimmer and loop policy](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/index.css#L565).
- T19: [Scrollbar opacity and delay](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/scroll-area.tsx#L87).
- T20: [Reactive media-query subscription](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useMediaQuery.ts#L63).
- T21: [Message timestamp and action opacity](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineUserMessageRow.tsx#L1069).
- T22: [Citation highlight pulse](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/AssistantCitationSource.tsx#L245).
- T23: [Branch-label movement](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/BranchToolbar.tsx#L464).
- T24: [History swipe release and arrow feedback](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/HistorySwipeNavigation.tsx#L175).
- T25: [Agent cursor movement and click ping](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/AgentBrowserCursor.tsx#L77).
- T26: [Browser chrome hover controls](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L196).
- T27: [Thread status-line entrance](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadNow.tsx#L24).
- T28: [Button press scale](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/button.tsx#L9).
- T29: [Switch track and thumb transitions](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/menu.tsx#L191).
- T30: [Connection ping](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ConnectionStatusDot.tsx#L40).
- T31: [Reduced-motion guarded skeleton](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/skeleton.tsx#L5).
- T32: [Reduced-motion guarded visible spinner](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/spinner.tsx#L7).
- T33: [Queue chevron and hover actions](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/QueuedRunsControl.tsx#L392).
- T34: [Explicitly unanimated event body](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineV2EventRow.tsx#L145).
- T35: [Shelf changes suppress list movement](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L2040).
- T36: [Zoom feedback](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/ZoomIndicator.tsx#L41).
- T37: [Settings search pulse and slider transitions](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/index.css#L923).
- T38: [Combobox popup scale and opacity](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/combobox.tsx#L170).

## Traced flows

1. Sidebar toggle changes folded state. `nav_width` immediately returns zero or the resting width, and board geometry consumes that result. There is no open/close clock. Row FLIP is an independent mechanism and cannot smooth the column itself. F3, F6.
2. A panel action sets the per-thread surface state. `panel_pane` decides whether a panel exists; `panel_column_w` computes its final width; `panel_element` paints it at that width; `surface_body` selects one body. A closing panel has no retained presence stage. F4. Preview documents separately opt into `swap_in` after an observed opening. F5.
3. Sending sets `jump_to_new_prompt`. The next reconciliation creates a runway and an anchor, then calls `ease_to` unless reduced motion is enabled. Prepaint steps the scroll tween. The bubble's entrance waits for landing. Wheel/page navigation clears the ease and anchor; scrollbar dragging is checked inside the scroll step. Tool or reasoning activity releases the anchor immediately to native tail following. F10, F11, F20.
4. A nav-list update snapshots row positions, carries current offsets across interrupted movement, and retains noninteractive ghost rows for exits. First paint, drag, viewport change, shelf changes and reduced motion skip the movement. F6, F7. This is present today.
5. A dialog close retains its outgoing face and fades the veil. `Exits` removes it once time expires, or immediately with reduced motion. The same lifecycle machinery is available for other controls, but an entrance helper does not automatically provide an exit. F1, F15.
6. Startup checks `FERRITE_REDUCE_MOTION`; otherwise macOS runs one background `defaults` query. The resulting application flag reaches native clocks. There is no OS preference subscription, and non-macOS returns without a query. F17. T3 subscribes to browser media-query changes through T20.

## Interface motion matrix

This is an inventory of inspected transition families and their named consumers, including static behavior. It is not a claim that every runtime frame or every provider-specific dialog has been exercised. `Kit` means Ferrite's shared motion helpers honor its application reduced-motion flag. Direct manipulations should continue to track the hand even when discrete transitions gain motion.

| # | Interface and trigger | Current Ferrite motion and lifecycle | Current T3 comparison | Disposition and evidence |
|---|---|---|---|---|
| 1 | Main sidebar open/close | Width changes immediately; no retained close. | Configurable width and position transition; default 0ms. | Gap motion-001. F3; T1-T3. |
| 2 | Automatic sidebar folding on window width | Same direct folded-width branch; no separate reveal. | Responsive sidebar uses inline/offcanvas/sheet variants. | Apply discrete-transition policy without delaying resize. F3; T3. |
| 3 | Sidebar rows reordered by state | 150ms ease-out position movement; interrupted offsets continue. | 150ms ease-out list movement. | Present. F6; T6. |
| 4 | Sidebar row insertion | Opacity plus travel capped at 40px; skips initial paint. | Fade plus capped travel. | Present. F6; T6. |
| 5 | Sidebar row removal | Retained ghost fades and travels out. | Cloned inert exit row. | Present; verify native hit ownership. F6; T6. |
| 6 | Shelf open/close | Body snaps; chevron turns 150ms; row FLIP suppressed. | List animation suppressed on shelf changes. | User preference gap motion-005. F6-F7; T35. |
| 7 | Sidebar hover and selection | Row and selection ground snap; receded card opacity eases 150ms. | Selection and list transitions are separate. | Keep immediate semantic selection; polish hover only if useful. F1, F7; T6. |
| 8 | Slim shelf actions | Time/action opacity and badge grayscale blend 150ms. | Reference sidebar uses hover disclosure. | Present in Ferrite. F7; T35 is structural comparison only. |
| 9 | Undo notice in nav foot | 200ms ease-out entrance and retained fade out. | T3 sidebar has list-state feedback; no exact value asserted here. | Present. F1-F2. |
| 10 | Right panel open/close | Instant width and presence change. | Configurable ease-out width, retained closing content. | Gap motion-002. F4; T2, T4. |
| 11 | Right panel maximize or width-floor change | Final geometry applied immediately. | Current T3 explicitly disables these width transitions. | User preference beyond T3 default. F4; T4. |
| 12 | Panel seam drag | Width follows pointer directly. | Resize disables transitions. | Correct direct manipulation; retain when adding motion. F4; T3-T4. |
| 13 | Panel tab-strip overflow chevrons | 200ms standard Tween; retargets; reduced motion snaps. | Native smooth tab scrolling is reference intent; exact current consumer not traced. | Ferrite coverage present, comparator gap. F9. |
| 14 | Overview tab activation | Body direct match; no shared activation fade. | Shell animation does not establish tab-body animation. | Gap motion-003. F4; T4. |
| 15 | Background tab activation | Body direct match; no shared activation fade. | No exact T3 body transition asserted. | Gap motion-003. F4. |
| 16 | Files and Changes tab activation | Bodies direct match; no shared activation fade. | No exact T3 body transition asserted. | Gap motion-003. F4. |
| 17 | Terminal tab activation | Body direct match; panel opens instantly if hidden. | T3 terminal drawer can animate grid height. | Shared panel gap; no separate Ferrite drawer remains. F4, F19; T5. |
| 18 | Side chat and agent tab activation | Bodies direct match. | No exact T3 body transition asserted. | Gap motion-003. F4. |
| 19 | PR tab activation and internal view switch | Direct body match, including Summary/Guide/Timeline/Code. | No exact T3 PR subview transition asserted. | Gap motion-003. F4, F14. |
| 20 | File/browser preview document changes | 180ms opacity from 0.3 when observed opening marks fade; initial tending skips it. | Panel shell transitions independently. | Present but not universal tab motion. F5; T4. |
| 21 | Connector disclosure | 60ms chevron; conditional body snaps. | Generic collapsible can tween height 200ms. | Gap motion-005. F18; T14. |
| 22 | Review slot beside a pane | 240ms enter, 200ms exit, snap curve; retained close; reversible; seam drag lands. | No exact equivalent asserted. | Strong existing structural pattern. F8, F1-F2. |
| 23 | Browser tab addition/switch | Tab list and page selection update directly. | Browser chrome provides hover transitions. | Covered by structural policy; inspect separately from outer panel. F14; T26. |
| 24 | Browser navigation loading | Plain loading row inserted and removed. | Progress scale, completion and delayed fade. | Gap motion-010. F14; T17. |
| 25 | Browser annotation capture arriving | Loading capture text replaced with stage; outer document may fade. | No exact T3 capture transition asserted. | Content readiness gap to verify; no invented animation. F5. |
| 26 | Embedded page startup | Starting branch can be an empty body until live view is ready. | Preview loading feedback exists. | Runtime loading-state check needed. F5; T17. |
| 27 | New prompt scroll | Distance-based ease up to 200ms, standard curve; target remeasured. | Animated LegendList anchor positioning. | Present, with long-distance caveat motion-009. F10; T7. |
| 28 | New prompt bubble | 450ms overshoot, 0.92 scale, opacity and 6px rise; begins on scroll landing. | No corresponding bubble entrance asserted. | Ferrite intentionally exceeds historical reference. F1-F2, F11, F20. |
| 29 | Tool/reasoning begins after send | Anchor releases and native tail resumes immediately. | Also releases to nonanimated end scroll. | Parity; review choreography under user preference. F10; T7. |
| 30 | Streaming prose and appended rows | 400ms linear fade; streamed runs land within 120ms of completion. | 600ms ease-out block fade with reduced-motion guard. | Present; different granularity. F2, F11; T16. |
| 31 | Active streaming tail follow | Native tail position jumps with layout growth. | Animated maintain-at-end while working. | Gap motion-008. F10; T8. |
| 32 | Scroll to end visibility | Wait 150ms, then show; hides immediately; no fade. | Separate show state and animated click. | Intentional present behavior; user may want fade. F1-F2, F20; T7. |
| 33 | Scroll to end click | Same distance ease; resumes native following; retains composer focus in test intent. | Explicit animated end scroll. | Present; distant jumps partially snap. F10, F20; T7. |
| 34 | History/thread-position restoration | Immediate position restore, no content entrance replay. | Restoration disables live follow animation. | Correct continuity policy. F1, F10; T8. |
| 35 | Page keys/minimap/scene jumps | scroll_page and scroll_to clear ease and apply position directly. | Different navigation paths have distinct follow policy. | Explicit policy decision; not all jumps currently smooth. F10; T7. |
| 36 | Manual wheel and scrollbar input | Wheel/navigation cancels programmatic motion; scrollbar checked during step. | Navigation controller releases programmatic holds. | Present. F10, F20; T7. |
| 37 | Work groups and finished-turn folds | Bodies snap; 60ms or 200ms chevrons by control. | Event rows can explicitly disable collapsible animation. | Preference gap; virtualization makes blind height tween unsafe. F1, F18; T14, T34. |
| 38 | Subagent and output disclosures | Use transcript disclosure machinery; no separate universal retained-body transition found. | No exact agent-body timing asserted. | Verify each virtualized consumer before changing shared fold. F1, F18. |
| 39 | Message actions and timestamps | 200ms standard hover opacity, no old 100ms entry delay. | 200ms opacity. | Present; historical catalog timing is stale. F11; T21. |
| 40 | Transcript find opening/closing | 150ms entrance; removal immediate. | No exact current find timing asserted. | Exit preference gap motion-011. F13. |
| 41 | Composer typing adds/removes line | 200ms quart Tween, interruptible; first layout and reduced motion land. | No exact desktop-height transition asserted. | Present. F12, F2. |
| 42 | Composer clears after send | Height clear snaps by explicit policy. | Historical desktop comparison also snaps. | User policy decision; do not accidentally replay growth. F1, F12. |
| 43 | Attachment added to draft | 300ms snap-curve opacity/0.95 scale; only newly added files. | 300ms drawer-curve opacity/scale. | Present. F12, F2; T15. |
| 44 | Attachment removal | The remove handler deletes the file from the current collection immediately. | Entrance and hover actions specified; no removal timing asserted. | Immediate removal is source-proven; verify layout continuity. F12; T15. |
| 45 | Queue strip | No strip entrance per catalog; queue changes stay immediate. | Queue chevron rotates 150ms; actions fade 200ms. | Preference gap; detailed queue ownership stays in composer audit. F1; T33. |
| 46 | Send/steer icon change | Shared FADE_QUICK settled transition. | Shared button press transition. | Present, exact T3 icon morph not asserted. F12; T28. |
| 47 | Composer notices and failed-start message | 150ms entrance, immediate exit. | Thread/status line uses 150ms guarded entrance. | Gap motion-011. F12-F13; T27. |
| 48 | Approval/decision card | 220ms cubic entrance, opacity from 0.75, 6px shift and 0.97 scale. | No exact current decision timing asserted. | Present; resolve/replace runtime check remains. F13, F1-F2. |
| 49 | Background-work status line | 150ms ease-out entrance; immediate disappearance. | 150ms ease-out status entrance. | Present entry; exit gap motion-011. F13; T27. |
| 50 | Menus/context menus | 0ms global MENU_IN; no exit helper. | Menu popup also snaps. | User preference beyond parity, motion-004. F1-F2, F15; T10. |
| 51 | Model/attachment/project popovers | Same global zero-duration menu helper. | Popover scale/opacity/size and viewport crossfade. | Partial parity, motion-004. F1-F2; T9, T38. |
| 52 | Slash and mention menus | Global snap policy. | Menu components generally snap; exact consumer not traced. | User preference gap, not a fabricated T3 transition. F1. |
| 53 | Tooltips | 120ms entrance from 0.98; nav 150ms; exit dropped immediately. | Scale/opacity plus position transitions; instant reuse path. | Present entry; no native exit. F1; T11. |
| 54 | Settings/project sheets | 200ms quart scale/opacity and veil; retained noninteractive exit. | 200ms ease-in-out, nested stack transforms. | Present; nested parity not established. F15; T12. |
| 55 | Command palette and shortcut sheet | 200ms entry, 120ms exit; 50ms veil. | Dialog-based motion is the shared comparator. | Present; runtime focus on exit still needs checking. F1, F15; T12. |
| 56 | Toasts and needs-you feedback | 200ms enter/exit; slides own height; exit scales to 0.85; retained face. | 500ms transform/opacity, 150ms height; stacked layout. | Present with different policy. F1-F2; T13. |
| 57 | Image zoom | 240ms logarithmic Tween around held point; pinch/pan direct. | Zoom indicator has 200ms feedback; zoom mechanism not fully traced. | Present and interruptible. F16; T36. |
| 58 | Loaded image replaces preview | 150ms arrival fade/crossfade. | No exact current image replacement timing asserted. | Present. F16. |
| 59 | Button hover and press | 60ms hover; 60ms press toward 0.975; 450ms overshoot release. | Shared button active scale 0.97 and transition. | Present; validate painted scale hit bounds. F1-F2, F17; T28. |
| 60 | Settings switch | 120ms overshoot knob slide; track snaps. | Track 200ms, thumb translate 150ms plus press shape. | Present with intentional parameter differences. F1-F2; T29. |
| 61 | Plan usage ring | 300ms arc and 200ms ink; first draw lands. | No exact current counterpart asserted. | Present. F16. |
| 62 | Working label, count and progress | 180ms label roll; 120ms changing digit crossfade; test-output progress uses retargeted Tween. | Live label shimmer and state feedback. | Present; old single-line consumers should be checked after transcript regrouping. F1-F2; crates/ferrite-pane/src/pane/output.rs:379-400; T18. |
| 63 | Live activity shimmer and working mark | 2200ms stepped shimmer token; working orbs 2500ms; loops scoped to visible views. | 2200ms steps(30), visibility pause; status pulse/ping separate. | Present; frame cost not measured. F1-F2, F17; T18, T30. |
| 64 | Spinners and skeleton placeholders | Shared spinner clock exists; no claim of animated skeleton coverage in every loading view. | Visible spin and motion-safe skeleton. | Loading-state coverage remains per consumer. F1, F17; T31-T32. |
| 65 | Scrollbar | Immediate appearance, 1400ms linger, 100ms fade; 4px to 6px widen. | Delayed opacity hide and 100ms show rule. | Present, timings differ. F17; T19. |
| 66 | Composer caret | 1000ms blink; reduced motion holds solid and declares no tick. | Native/editor caret behavior not compared. | Present. F12. |
| 67 | Terminal cursor | Terminal is its own native view with blink preference. | Persistent terminal drawer is separate from cursor implementation. | Cursor reduced-motion and viewport visibility behavior unverified. F19; T5. |
| 68 | Connection and provider state | Global policy says status dots never pulse; working mark supplies live motion. | Connection ping uses motion-safe animation. | Intentional alternative; static state must remain sufficient. F1-F2; T30. |
| 69 | Settings search/slider feedback | No source-backed Ferrite equivalent established in this partition. | 650ms search pulse twice; slider transform/shadow transitions. | Explicit comparison gap; settings owner should assess need. T37. |
| 70 | History swipe, citation focus, browser agent cursor | No equivalent motion path established here; absence not asserted. | 100-200ms swipe feedback, citation WAAPI highlight, 150ms agent cursor plus ping. | Capability ownership belongs to navigation/transcript/browser workers. T22, T24-T25. |
| 71 | Branch-label change and mobile composer morph | No mobile requirement for native Ferrite; branch-label motion not established here. | Branch label uses WAAPI; historical mobile morph must not be assumed current without tracing. | Native applicability and verification gap, not automatic missing feature. T23. |
| 72 | Reduced motion and idle scheduling | Kit lands one-shots, removes exits, freezes loops; startup-only macOS bridge. | Reactive media query; most bespoke motion guards, but not every generic popup class has an explicit guard. | Defects motion-006/007; retain Ferrite end-state handling. F17, F20; T2, T9, T20. |

## Detailed findings

The P2 motion items describe material usability gaps relative to the user's requested behavior. They do not imply that a working control is inaccessible. There are seven P2 findings and four P3 findings. No P0 or P1 defect was established in this motion-only investigation.

### motion-001: The main sidebar opens and closes in a single layout change

Priority P2. Category `motion`. Evidence `source-proven`.

Symptom and impact: The entire workspace changes width without an intermediate visual state. This directly matches the user's snapping-sidebar complaint.

Trigger: Toggle the sidebar with Cmd-B or the UI; automatic folding uses the same width path.

Expected: A short, useful width transition preserving spatial continuity.

Actual: nav_width returns either zero or COLUMN_WIDTH; no tween or retained closing state participates.

Cause: The implementation and design tokens explicitly prescribe snapping. T3 also defaults to zero duration, but supports configurable width transitions. Its default does not satisfy this user's preference.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit.rs:2829-2838`; `crates/ferrite-ui/src/motion.rs:33`; `crates/ferrite-ui/src/theme.rs:3531-3537`.

T3 evidence: [source 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/sidebar.tsx#L388); [source 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/panelAnimations.ts#L31); [source 3](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/settings.ts#L155).

Recommendation: Give sidebar open and close a shared interruptible layout transition. Keep pointer resize immediate, skip restored layouts, retain a closing face until it finishes, and honor reduced motion. Replace the documented prohibition on sidebar motion with the user's requested policy.

Acceptance: With motion enabled, record sidebar toggles from toolbar and shortcut. At least one intermediate width must appear, the board must follow without stale hit regions, and reversing before completion must continue from the displayed width. Reduced motion and restored startup layouts must settle immediately.

### motion-002: The right panel snaps open, closed, maximized and between width floors

Priority P2. Category `motion`. Evidence `source-proven`.

Symptom and impact: Files, browser, terminal, PR, agents and other panel tools abruptly take or return workspace area.

Trigger: Open or close any panel surface, select a surface with a different minimum width, or maximize the panel.

Expected: A coherent panel transition shared by all panel tools.

Actual: panel_element disappears when panel_pane is absent and otherwise takes panel_column_w directly. There is no panel-level exit retention.

Cause: Panel geometry reads persisted state directly. T3 retains closing content, but current T3 deliberately suppresses transitions for maximize and width-only changes. Animating those is an explicit user preference beyond T3 parity.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/panel.rs:25-27`; `crates/ferrite-cockpit/src/cockpit/panel.rs:399-420`; `crates/ferrite-cockpit/src/cockpit/panel.rs:433-456`; `crates/ferrite-cockpit/src/cockpit/panel.rs:550-620`.

T3 evidence: [source 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewPanelShell.tsx#L81); [source 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewPanelShell.tsx#L169); [source 3](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/panelAnimations.ts#L41).

Recommendation: Use an interruptible panel presence state and a visual width separate from the saved resting width. Animate open/close and deliberate maximize changes under the user's policy. Clip content at its final reading width where appropriate; do not animate hand-driven resize.

Acceptance: Open and close each panel kind, reverse mid-transition, switch to a wider kind, maximize, then drag its seam. Intermediate geometry must be visible only for discrete transitions, outgoing content must lose input ownership at once, and browser/terminal dimensions must stay correct.

### motion-003: Most panel tab bodies have no transition despite the motion catalog

Priority P2. Category `motion`. Evidence `source-proven`.

Symptom and impact: Tab changes replace large bodies instantly and differ from document previews. The catalog incorrectly promises broader coverage.

Trigger: Select a different panel tab or PR Summary, Guide, Timeline or Code.

Expected: A deliberate content change with consistent feedback across the shared tab host.

Actual: surface_body directly renders its match arm. Only panel_preview wraps its body in swap_in when preview_tab.fade is set. PR subviews also use a direct match.

Cause: The unified panel host does not apply the old generic tab-body fade described by the catalog. T3's panel-shell width transition is not evidence of a tab crossfade; this finding is about Ferrite's current implementation and the user's requirement.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/panel.rs:657-694`; `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:244-257`; `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:444-449`; `crates/ferrite-ui/src/motion.rs:53`; `crates/ferrite-cockpit/src/cockpit/panel/pull_request.rs:460-486`.

T3 evidence: [source 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewPanelShell.tsx#L169).

Recommendation: Give surface activation one transition owner keyed by thread and surface identity. Apply the agreed body transition to Overview, Background, Files, Changes, Terminal, SideChat, Agent and PR, and coordinate document preview fades to avoid double animation. Correct the catalog.

Acceptance: Switch through every panel kind and PR subview using both mouse and keyboard. Confirm one visible transition per activation, no replay on unrelated updates, no outgoing input target, and no lost terminal/browser focus.

### motion-004: Popover and menu entrances are globally disabled

Priority P3. Category `motion`. Evidence `source-proven`.

Symptom and impact: Model and other popovers, context menus, slash commands and mention menus appear and disappear instantly.

Trigger: Open or dismiss any Ferrite-drawn menu or popover.

Expected: Subtle opening and closing feedback consistent with the user's preference.

Actual: MENU_IN is zero and menu_in returns the element without animation state. No exit mechanism is provided by this helper.

Cause: One snap token serves both menus and popovers. T3 menus also snap, but T3 popovers and comboboxes expose scale/opacity transitions.

Ferrite evidence: `crates/ferrite-ui/src/theme.rs:3662`; `crates/ferrite-ui/src/motion.rs:473-489`; `crates/ferrite-cockpit/src/cockpit/context_menu.rs:1001`; `crates/ferrite-cockpit/src/cockpit.rs:3333-3336`; `crates/ferrite-pane/src/transcript.rs:2392-2395`.

T3 evidence: [source 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/popover.tsx#L95); [source 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/popover.tsx#L114); [source 3](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/menu.tsx#L56); [source 4](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/combobox.tsx#L170).

Recommendation: Separate menus and popovers in the policy, add a short origin-aware entrance and retained noninteractive exit where requested, and preserve immediate keyboard selection. A global MENU_IN duration change alone does not implement exits.

Acceptance: Open and close model, attachment, slash, mention, project-filter and context menus at every window edge. Verify the entrance originates at the anchor, closing immediately releases input, and rapid reopen does not show duplicate faces.

### motion-005: Shelves and connector details reveal their bodies instantly

Priority P2. Category `motion`. Evidence `source-proven`.

Symptom and impact: Expanding a shelf or connector section moves neighboring content without showing the relationship between the header and revealed rows.

Trigger: Expand Working, Snoozed or Parked, or toggle Connectors in Overview.

Expected: The body and its disclosure control communicate the same transition.

Actual: Shelf toggling sets nav_snap and updates settings immediately. Connectors conditionally includes children. Chevrons animate independently.

Cause: These paths intentionally bypass list movement and body reveal. T3 also suppresses sidebar list motion on shelf changes, although its generic collapsible supports a 200ms height transition. Existing parity is not the requested motion policy.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/nav_list.rs:69-84`; `crates/ferrite-cockpit/src/nav/list.rs:114-117`; `crates/ferrite-cockpit/src/cockpit/panel/sections.rs:249-278`; `crates/ferrite-ui/src/motion.rs:57-70`.

T3 evidence: [source 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/ui/collapsible.tsx#L21); [source 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/Sidebar.tsx#L2040); [source 3](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/TimelineV2EventRow.tsx#L145).

Recommendation: Add measured, clipped body reveal for nonvirtualized shelves and connector sections. Treat virtualized transcript disclosures separately so visible bounds, scroll anchoring and hit testing agree throughout the transition. Keep chevron timing coordinated with body motion.

Acceptance: Expand and collapse every shelf and Connectors, including rapid reversal and a long list. Rows must remain clipped to the displayed body, adjacent headings must move continuously, and reduced motion must reveal the final content immediately.

### motion-006: System reduced-motion changes are not observed while Ferrite runs

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact: A user who enables Reduce Motion after launch keeps the previous animation policy until restarting. Turning it off also is not observed.

Trigger: Change macOS Reduce Motion after Ferrite has launched.

Expected: OS accessibility preference changes apply to the running application.

Actual: init performs one background defaults read. No change subscription exists in the inspected platform integration. GPUI initially has reduce_motion false.

Cause: Reduced-motion handling is strong after the application flag is set, but the platform bridge only samples at launch. The asynchronous initial read also leaves a source-visible window before the preference arrives; its perceptibility is unmeasured.

Ferrite evidence: `crates/ferrite-ui/src/motion.rs:2152-2177`; `vendor/gpui-pre/src/app.rs:864`; `vendor/gpui-pre/src/app.rs:1065-1067`.

T3 evidence: [source 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useMediaQuery.ts#L63); [source 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/panelAnimations.ts#L35).

Recommendation: Subscribe to platform accessibility preference changes and route them through set_reduce_motion. Resolve the initial preference before animated content is presented, or adopt a conservative initial state until it is known. Ensure active scroll and layout transitions land when the setting changes.

Acceptance: While a loop, panel reveal and smooth scroll are active, toggle the OS preference without restarting. All animation must reach a static state promptly; toggling back must affect subsequent transitions. Repeat with the environment override.

### motion-007: Non-macOS builds do not read the system reduced-motion preference

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact: Windows and other supported non-macOS users cannot rely on their system animation preference reaching Ferrite through this integration.

Trigger: Run a non-macOS Ferrite build without FERRITE_REDUCE_MOTION.

Expected: The system accessibility preference controls native application motion.

Actual: init returns before any system query on non-macOS targets. Only an explicit environment override sets the flag through this path.

Cause: The current platform adapter implements a macOS-only defaults query. This is a source-level integration gap; no non-macOS runtime was exercised.

Ferrite evidence: `crates/ferrite-ui/src/motion.rs:2155-2165`; `vendor/gpui-pre/src/app.rs:864`; `crates/ferrite-ui/src/motion.rs:2081-2082`.

T3 evidence: [source 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useMediaQuery.ts#L63); [source 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/panelAnimations.ts#L35).

Recommendation: Implement the relevant platform setting bridge and change observation on supported operating systems. Preserve the explicit environment override for test and operator control.

Acceptance: On each supported non-macOS platform, launch with the system animation setting disabled and then change it while running. Built-in transitions and continuous indicators must follow the setting without an environment variable.

### motion-008: Streaming tail follow still jumps with content growth

Priority P2. Category `motion`. Evidence `source-proven`.

Symptom and impact: The answer can move in provider-update increments even though its new text fades in. Send scrolling and jump-to-end already animate, which makes continuous follow inconsistent.

Trigger: Follow an active answer after it grows beyond the viewport.

Expected: Continuous follow movement that absorbs uneven streamed content updates.

Actual: GPUI FollowMode::Tail and scroll_to_end own tail updates. The explicit Tween path is used for sends and the end pill, not ongoing growth.

Cause: The implementation deliberately distinguishes smooth programmatic jumps from native tail following. Current T3 selects animated follow when working and reduced motion is off.

Ferrite evidence: `crates/ferrite-pane/src/transcript/scroll.rs:78-80`; `crates/ferrite-pane/src/transcript/scroll.rs:111-114`; `crates/ferrite-pane/src/transcript.rs:2932-2933`; `crates/ferrite-ui/src/motion.rs:64-70`.

T3 evidence: [source 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L195); [source 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/MessagesTimeline.tsx#L1449).

Recommendation: Introduce a dedicated interruptible live-follow policy while work is running. Keep idle growth, history restoration and user-driven scroll separate. Preserve the reading anchor and stop following immediately on wheel, scrollbar or page navigation.

Acceptance: With a long answer visible at the tail, stream uneven chunks and tool rows. Record intermediate scroll positions, then interrupt with wheel and scrollbar. The transcript must stop exactly where the user takes control and resume only when requested.

### motion-009: Long programmatic scrolls can snap before their final ease

Priority P3. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact: A distant jump to the latest content can teleport across most history and animate only the last viewport. A send target that has no measured bounds can land immediately.

Trigger: Use Scroll to end from distant history, or send when the target row has not been measured.

Expected: A predictable, explicitly specified long-distance transition.

Actual: When target bounds are unavailable, step_ease lands immediately. When distance exceeds a viewport, it scrolls by the excess before constructing the tween.

Cause: The lazy list cannot measure every intervening row. These explicit fallbacks trade continuity for bounded work; the visual impact has not been measured.

Ferrite evidence: `crates/ferrite-pane/src/transcript/scroll.rs:159-203`.

T3 evidence: [source 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/useTimelineFollow.ts#L408).

Recommendation: Decide and document a long-distance policy. If full spatial continuity is required, use measured estimates or staged movement that respects virtualization. If a shortened jump is desired, make that intentional and verify it visually instead of describing every jump as smooth.

Acceptance: From several screens above the tail, activate Scroll to end and record the first frame through completion. Repeat a send with an unmeasured target. The observed path must match the chosen policy and remain interruptible.

### motion-010: Browser navigation feedback is static and changes the content stack

Priority P3. Category `motion`. Evidence `source-proven`.

Symptom and impact: A loading page adds and removes a plain text row rather than showing persistent progress feedback in the browser chrome.

Trigger: Navigate or reload an embedded browser page.

Expected: Consistent loading and completion feedback that does not rearrange the page.

Actual: When loading is true, browser_body inserts a text child reading Loading. Browser tab content is also switched directly.

Cause: The browser has load state but does not use the T3 progress sequence. T3 specifies progress travel over 5.3 seconds plus a completion transition; that is an illustrative animation, not measured network progress.

Ferrite evidence: `crates/ferrite-cockpit/src/cockpit/browser.rs:1521-1531`; `crates/ferrite-cockpit/src/cockpit/browser.rs:1491-1497`.

T3 evidence: [source 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/index.css#L2424); [source 2](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/preview/PreviewChromeRow.tsx#L196).

Recommendation: Give navigation a stable loading indicator in the chrome with an explicit loading, completed and failed sequence. Use a static equivalent under reduced motion and avoid displacing the page viewport merely to show status.

Acceptance: Navigate a deliberately slow page, then a fast page, failure, reload and back/forward. Loading feedback must remain visible, finish once, and not shift the page bounds. Under reduced motion status must remain legible without travel.

### motion-011: Several transient elements animate in but disappear immediately

Priority P3. Category `motion`. Evidence `source-proven`.

Symptom and impact: Find bars, notices and background-work lines have asymmetric lifecycle feedback; layout can change abruptly on dismissal or completion.

Trigger: Dismiss find or notices, or finish background activity.

Expected: Intentional exit behavior for transient information, with a static reduced-motion equivalent.

Actual: The catalog and callers provide entrance fades and immediate removal. Exits<T> exists for dialogs and toasts but is not shared by these paths.

Cause: The current motion policy intentionally treats these as entrance-only. This is a lower-priority preference gap, not evidence that the underlying workflow is broken.

Ferrite evidence: `crates/ferrite-ui/src/motion.rs:31-32`; `crates/ferrite-ui/src/motion.rs:48-51`; `crates/ferrite-pane/src/find.rs:322-325`; `crates/ferrite-pane/src/pane/background_work.rs:167-170`; `crates/ferrite-pane/src/pane/composer.rs:548-558`.

T3 evidence: [source 1](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadNow.tsx#L24).

Recommendation: Use a small retained exit where it helps the user track completion or dismissal. Release hit testing immediately, and collapse occupied space in the same transition. Keep high-frequency text updates and direct manipulation immediate.

Acceptance: Dismiss find, resolve a notice, finish background work, and remove an attachment while the composer is populated. Verify stable focus and intentional exit behavior without duplicate controls or hidden clickable remnants.

## Supported T3 capabilities and current differences

Current T3 exposes a panel duration preference, retains closing panels, suppresses navigation entrances, cancels resize transitions, animates list changes with inert exit clones, uses smooth active tail-follow, animates popover size/content, and provides browser progress feedback. T1-T9 and T17 establish those mechanisms. Menus and several transcript event bodies intentionally snap. T10 and T34 establish those limits. T3 is not a blanket motion specification.

Ferrite already supports several equally useful behaviors: reversible review-slot motion, navigation list FLIP, scroll interruption, send/bubble sequencing, per-run stream fades, source-aware attachment entrances, retained dialog/toast exits, usage-ring changes, hover/press feedback and reduced-motion static states. F6, F8, F10-F13, F15-F17 establish these paths. Reuse and extend their lifecycle patterns rather than adding an unrelated animation layer per feature.

## Historical comparison corrections

| Historical motion.md statement | Current evidence |
|---|---|
| Send scroll and jump-to-latest snap | Now ease through `transcript/scroll.rs:140-214`; far/unmeasured cases remain qualified by motion-009. |
| Bubble and scroll need choreography | Current send-scroll tests explicitly cover bubble starting after landing at `cockpit/tests/send_scroll.rs:196`; implementation gates arrivals in `transcript.rs:815-899`. |
| Anchor release on tool/reasoning is missing | Implemented at `transcript.rs:1029-1055`. |
| Sidebar FLIP is missing | Implemented in `nav/flip.rs:137-255` and wired at `cockpit/nav_list.rs:190-217`. |
| Attachment entrance is missing or uncertain | Added-file timestamps and CHIP_IN exist at `composer.rs:557-579`; the spec is 300ms at `theme.rs:3726-3727`. |
| Find and notice entrance is uncertain | Callers use FADE_QUICK at `find.rs:322-325` and `pane/composer.rs:548-558`. |
| Message hover actions are 120ms after a 100ms delay | Current ACTIONS_IN/OUT use 200ms and the current caller has no entry delay at `motion.rs:332-333`, `transcript.rs:2432-2437`. |
| Shimmer is 2400ms | Current token is 2200ms with 30 steps at `theme.rs:3764-3765`. |
| Panel tab bodies fade | Current catalog repeats this, but the unified surface host directly matches most bodies. Only document preview has the observed swap wrapper. F4-F5. |
| Panel widths snap like T3 default | Still true for Ferrite. Current T3 also supports configurable opening/closing; its newest shell excludes other width changes. T1-T4. |

## Verification gaps and runtime checks

There is no runtime reproduction in this partition. The parent owns the live UI. No claim about dropped frames, perceived speed, GPU work or exact on-screen timing is made. The source durations are enough to identify disabled transitions, not enough to certify visual quality.

Existing tests provide useful intent, but they were not run in this read-only audit. `cockpit/tests/reader_motion.rs` covers entry, exit, reversal, seam interruption and reduced motion. `cockpit/tests/send_scroll.rs` covers scrollbar cancellation, reduced motion, bubble sequencing and scroll-pill behavior. `nav/flip.rs:497-540` tests row movement and ghost lifecycle. `cockpit/tests/render_performance.rs:1203` contains an idle scheduling check. Ordinary tests default motion to rest unless `testing::drive()` is called, as `motion.rs:2085-2098` explains. A green endpoint test therefore cannot establish animated input safety or intermediate geometry.

The parent should record these focused flows with normal motion and reduced motion. Use timestamped frame capture if timings are to be reported; otherwise describe only visible behavior.

1. Toggle the sidebar and panel with both mouse and shortcuts. Open the browser, terminal, PR and files; change width floor and maximize. Confirm that the source snap is what the installed app displays.
2. Switch through every panel kind and PR subview. Confirm whether any external wrapper adds motion absent from the inspected body renderer. Check stale hit targets and focus while switching quickly.
3. Send at the tail and from distant history; interrupt with wheel, trackpad, page keys and scrollbar; stream uneven chunks while following; jump to the end from several viewports away. Capture the initial snap versus final ease separately.
4. Toggle shelves, connector details, work groups, finished turns, subagent details, image zoom, queue disclosure and attachment removal. Check clipping and input ownership throughout each change.
5. Open each menu category near a window edge; dismiss/reopen quickly; close a dialog while pressing the underlying control. Check retained outgoing faces cannot accept input.
6. Trigger slow and failed browser navigation, PR loading, provider connection and reconnect, queued work, approvals, stream completion and transient errors. Distinguish a static but clear status from absent feedback.
7. Change OS Reduce Motion while the application is open. Repeat at launch, with the environment override, and on supported non-macOS platforms. Native terminal cursor and embedded web animations require separate checks because they have independent rendering engines.
8. Record an idle window and a busy window with off-screen indicators only before making a resource-use claim. The shared clock design is promising; this audit did not measure its cost.

Suggested implementation order is shared sidebar/panel presence and geometry, consistent surface activation, reactive reduced-motion integration, live tail-follow, then lower-impact popup/disclosure/loading/exit coverage. Preserve direct resize, immediate semantic focus, first-paint suppression and stable history restoration. Update the motion catalog alongside implementation so it describes actual callers.

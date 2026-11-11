# T3 Code motion catalogue, compared with Ferrite's motion kit

Sources: T3 Code (pingdotgg/t3code at `bcee0f5d`), `apps/web/src` (paths below are relative to it unless they start with `packages/`).
Ferrite at `crates/ferrite-ui/src/motion.rs`, `crates/ferrite-ui/src/theme.rs` (motion section, lines ~2972–3210),
`crates/ferrite-pane/src/transcript.rs`, and `crates/ferrite-pane/src/transcript/scroll.rs`.
LegendList 3.7.2's source was pulled with `npm pack` (`package/react.js`) to check how it scrolls.

Tailwind v4 defaults used everywhere below: a bare `transition-*` gets **150ms** at `cubic-bezier(0.4, 0, 0.2, 1)`.
`ease-out` is `cubic-bezier(0, 0, 0.2, 1)` and `ease-in-out` is `cubic-bezier(0.4, 0, 0.2, 1)`.
Base UI popups use `data-starting-style` / `data-ending-style`. These are CSS transitions, so they can be interrupted: a reversal turns around from the current value, and the element unmounts only after its exit transition ends.

## 0. Main finding

T3's motion is mostly **restraint plus one or two smooth scrolls**:
- **Panel animation is off by default.** The `panelAnimationDurationMs` setting defaults to **0** (range 0–400, step 25). A comment at `packages/contracts/src/settings.ts:404` explains: "Zero avoids per-frame layout work". It gates the sidebar, the right panel, the terminal drawer, the draft-hero FLIP and the mobile composer morph.
- **Menus snap**, the slash menu snaps, and the right-panel tab switch snaps.
- **The user bubble has no entrance.** It just appears.
- What reads as polish when you send is mostly this:
  1. A spacer at the end of the list ("anchored end space") reserves room below the new message, so the message can sit **24px from the top**.
  2. A **native smooth scroll** (`behavior: "smooth"`) brings the message there.
  3. Streamed paragraphs fade in over 600ms.
  4. While the agent works, follow mode keeps the end in view with smooth scrolling.
  None of it jumps twice (they patched LegendList to stop a double jump).

## 1. Sending a prompt, step by step

| # | Step | Detail | File:line |
|---|---|---|---|
| 1 | (Draft thread only) hero docks | When the empty-thread hero ("What should we build in X?") submits, `captureDraftHeroComposerRect()` runs and the dock state is set inside `flushSync`. On mobile with View Transitions, `runMobileComposerTransition` wraps the swap. | `chat/useComposerSend.ts:855-875` |
| 1a | Desktop FLIP of the composer | `useLayoutEffect` measures old vs new composer rect and calls `transitionGroup.animate([{transform: translate3d(dx,dy,0)}, {transform: none}], {duration: panelAnimationDurationMs, easing: "cubic-bezier(0.4, 0, 0.2, 1)"})`. Skipped if the move is under 0.5px, if animation is inactive (default 0ms, or reduced motion, or navigation suppression), or during the mobile view transition. Any running animation is cancelled first; there is no retargeting from the current position. | `chat/useDraftHeroLayoutTransition.ts:23-66`, `chat/draftHeroTransition.ts:2` |
| 1b | Mobile morph (≤639px) | `document.startViewTransition`. Root has `animation: none`. The composer group moves over `--mobile-composer-transition-duration` at `cubic-bezier(0.4,0,0.2,1)`. Old and new composer crossfade with linear keyframes: old opacity 1 until 35%, 0 from 65%; new is the reverse. The headline exits to opacity 0, translateY(-6px), at `cubic-bezier(0.4,0,1,1)`. | `index.css:19-90`, `chat/draftHeroTransition.ts:38-100` |
| 1c | Draft promotion waits | Swapping the draft for the server thread waits for `t3-draft-hero-transition` animations to finish, so the swap can't cut the FLIP. | `composerDraftStore.ts:4576` |
| 2 | Anchor armed before the run exists | `anchorNewTurn(threadKey, messageId)` sets follow to FOLLOWING, cancels the 150ms pill debounce, hides the pill and sets `timelineAnchor = {messageId}`. Skipped when the message is queued behind an active run. | `chat/useComposerSend.ts:1080-1084`, `chat/useTimelineFollow.ts:526-531` |
| 3 | Optimistic user message | Appended to `optimisticUserMessages` and rendered at once. **No entrance animation** on `TimelineUserMessageRow`; the only transitions in that file are hover reveals (`opacity-0 transition-opacity duration-200 group-hover:opacity-100`). When the server acknowledges the message, it is swapped for the server row in the same render, so nothing is shown twice. | `chat/useComposerSend.ts:1109`, `chat/optimisticUserMessages.ts:24-45`, `chat/TimelineUserMessageRow.tsx:330,1047` |
| 4 | Composer clears | Prompt cleared (`promptRef.current = ""`). On desktop the composer's height snaps; the resting-transition hook animates only when collapse state changes, which is mobile only. | `chat/useComposerSend.ts:1128`, `chat/ChatComposer.tsx:2362` |
| 5 | Room reserved below the message ("anchored end space") | `MessagesTimeline` passes `anchoredEndSpace = {anchorIndex, anchorOffset: 24}` to LegendList. LegendList sizes a spacer at the end of the list to `scrollLength − (height of rows from the anchor down + footer + padding) − 24`, so the user message can scroll to 24px below the top. The spacer **shrinks as the reply grows**, and holds its last value while the tail is still unmeasured. While the anchor is active, `maintainScrollAtEnd` is false. | `chat/MessagesTimeline.tsx:807-818,1303-1316`, `chat/timelineScrollAnchoring.ts:4` (CHAT_TIMELINE_ANCHOR_OFFSET=24), legend `react.js:5301-5362` |
| 6 | Smooth scroll to the anchor | When LegendList reports `onReady`: two rAFs, then `list.scrollToIndex({index, animated: !prefersReducedMotion, viewPosition: 0, viewOffset: CHAT_LIST_ANCHOR_OFFSET (16)})`, retried for up to 12 frames if the list isn't mounted yet. `holdProgrammaticScroll()` marks the upward move as programmatic so it doesn't release follow. LegendList's animated scroll is the browser's `scrollTo({behavior: "smooth"})`. In Chromium (Electron), as I remember its source (unverified): duration ≈ `min(√|Δpx|, 12)/60` s, which caps at **200ms** for any jump of 144px or more, on ease-in-out. | `chat/useTimelineFollow.ts:373-427`, legend `react.js:6948` |
| 7 | Anchor stays put while the message's own size changes | `onAnchorSizeChanged`: if the scroll offset is within 2px of where it was, re-apply that offset without animation on the next rAF. | `chat/useTimelineFollow.ts:430-462` |
| 8 | Streamed text | Each new top-level markdown block (paragraph, code block, mermaid) fades in over **600ms, ease-out, opacity only**, using `@starting-style` gated on `[data-streaming]`, so a finished thread never replays it. Under reduced motion it is removed. | `index.css:2203-2215` |
| 9 | Anchor released, follow takes over | When the running turn first shows reasoning or tool work, `scrollToEnd()` runs (not animated). It drops the anchor, waits one rAF so the spacer is gone, calls `list.scrollToEnd`, then `settleListScroll` corrects for up to 60 frames. | `chat/useTimelineFollow.ts:500-524,290-315`, `ChatView.logic.ts:266-289`, `chat/useTimelinePositionRestore.ts:8` |
| 10 | Following while working | `maintainScrollAtEnd = {animated: isWorking && !reducedMotion, on: {dataChange, itemLayout, layout}}`, so each chunk is a native smooth scroll to the end. Not animated when idle or under reduced motion. A T3 patch to LegendList holds the end in the same pass as a size-only change, "so the feed visibly jumps [only once]". | `chat/MessagesTimeline.tsx:192-205,1306-1316`, `patches/@legendapp__list@3.7.2.patch` |
| 11 | Releasing follow | Wheel up (if the content overflows), Page/Arrow up, scrollbar drag, touch away from the end, or an unexplained upward scroll releases follow. Downward input inside the end band never releases, so streaming jitter can't break follow. Reaching the end again re-arms follow, except after keyboard navigation at the end. | `chat/timelineViewport.ts:184-252` |
| 12 | "Scroll to end" pill | Shown via a **150ms debounce** (to avoid flashing during thread switches) and hidden immediately. It mounts and unmounts with **no fade**: a glass `Button size="xs"` with a chevron, "Scroll to end · {summary}", placed `bottom = composer clearance + 4px`. Clicking calls `scrollToEnd(true)`: drop the anchor, one rAF, then a native smooth `scrollToEnd` (unless reduced motion), then settle. | `chat/TimelineOverlays.tsx:26-55`, `ChatView.tsx:3470-3476`, `chat/useTimelineFollow.ts:70,290-315` |
| 13 | Composer growth follows the end | If the composer inset grows while following, `scrollToEnd()` runs (not animated). Footer growth (a queued turn, a status line) does the same. | `chat/useTimelineFollow.ts:318-324,466-469` |

## 2. Global motion tokens (`index.css`)

| Token / keyframe | Value | Line |
|---|---|---|
| `--ease-drawer` | `cubic-bezier(0.32, 0.72, 0, 1)` (iOS sheet curve; the same as Ferrite's `MOTION_EASE_SNAP`) | 195 |
| `--animate-skeleton` | `skeleton 2.4s infinite`. Opacity 1 → 0.55 → 1 with `steps(4)` (stepped to cut compositor frames on 120Hz) | 223, 295-313 |
| `--animate-status-pulse` | 2s infinite. Opacity 1 → 0.5 with `steps(6)` | 226, 332-346 |
| `--animate-status-ping` | 2s. Scale 0.75 → 2, opacity 0.9 → 0 over the first 40% with `steps(8)`, then holds | 227, 347-360 |
| stash pulses | 1400ms. A color hold for 85.7%, then the row eases back (~200ms); the count snaps (`step-start`) | 230-232, 314-331 |
| `live-tool-shine` | 2.2s `steps(30)` infinite. A 4.5rem gradient highlight sweeps across `background-clip:text`. Only under `prefers-reduced-motion: no-preference` and no forced colors. Paused off-screen (`--visible-animation-state`, `lib/visibleAnimation.ts`) | 551-578 |
| `live-activity-focus` (+counter) | 2.2s `steps(30)`. A masked 4.5rem light band translating 0 → 100% (counter-translated content). Reduced motion: `animation: none; opacity: 0` | 586-660 |
| `visible-animate-spin` | Tailwind `spin` (1s linear), paused off-screen | 580-584 |
| `@property --assistant-citation-highlight-opacity` | Number, animated by WAAPI: pulse 650ms ×1.5, hold 1575ms, fade 450ms, peak 0.45 | 662-666, `chat/AssistantCitationSource.tsx:11-17,245` |
| settings search pulse | 650ms ease-in-out ×2, inset ring 0 → 2px → 0 | 923-936 |
| settings slider thumb | transform and box-shadow 120ms `ease`; hover scale 1.08 | 1009-1027 |
| streamed block fade | 600ms ease-out, `@starting-style` opacity 0 | 2203-2215 |
| preview loading bar | scaleX 0.04 → 0.9 over 5.3s `cubic-bezier(0.1,0.5,0.2,1)`; on finish, transform 150ms ease-out, opacity 150ms ease-out after a 220ms delay | 2386-2420 |
| sidebar drag label | 200ms `step-end` visibility reveal; rows keep dnd-kit's 200ms transitions | 2422-2433 |
| theme probe | `transition: none !important` while probing | 1393-1398 |
| top scroll fade | `topbar-scroll-fade` mask gradient (static) | 469 |

Animation libraries: **none**. No framer-motion, motion, auto-animate or react-spring. Motion comes from Tailwind classes, Base UI starting/ending styles, raw WAAPI `element.animate` (5 files) and native smooth scroll.
`startViewTransition` is used only for the mobile composer morph.

Reduced motion is handled four ways:
- Tailwind `motion-reduce:transition-none` / `motion-safe:` on most hand-made transitions.
- `usePanelAnimationSettings` turns all panel motion off.
- JS checks before `animated:` smooth scrolls.
- The sidebar list motion checks `matchMedia`.

## 3. Panel animation setting (`panelAnimations.ts`, `components/settings/PanelAnimationsPreview.tsx`)

- `usePanelAnimationSettings()` is active when `durationMs > 0 && !prefersReducedMotion && !suppressed` (`panelAnimations.ts:31-39`).
- Suppression: after a route or navigation change, animation stays off for **two rAFs**, so a new route paints before panels can animate (`panelAnimations.ts:11-28`).
- `usePanelPresence`: keeps a closing panel mounted for `durationMs` (setTimeout) with its last value retained, then unmounts (`:41-69`).
- The preview in settings is a 3-pane mock with `transition-[width,height,border-width] duration-(--preview-duration) ease-out`. Clicking it replays the animation.
- Consumers, all `ease-out` at `var(--panel-animation-duration)` and only when `[data-panel-animations=true]`:
  - Sidebar width, and its left/right/width offcanvas transition (`ui/sidebar.tsx:364,376,557`).
  - Right/preview panel width with `@starting w-0` (`preview/PreviewPanelShell.tsx:132-133`).
  - Terminal drawer `grid-template-rows` 0fr → 1fr (`PersistentThreadTerminals.tsx:247-248`).
  - Chat `padding-left` (`ChatView.tsx:3287`).
  - Panel layout controls' opacity (`chat/PanelLayoutControls.tsx:198`).
  - Workspace header.
  - Mobile right-panel `Sheet` (`RightPanelSheet.tsx:21`, `ui/sheet.tsx:62-78`; at 0 the sheet is "instant").
  - The draft hero FLIP and the mobile composer morph.
- Sidebar resize drag forces `transition-duration: 0ms` while dragging (`ui/sidebar.tsx:474,513`).

## 4. Everything else

| Surface | Trigger | Properties | Duration / easing / delay | Origin | Interruptible | Reduced motion | File:line |
|---|---|---|---|---|---|---|---|
| Popover (model picker, etc.) | open/close | scale 0.98 ↔ 1, opacity 0 ↔ 1; also width/height while content resizes | 150ms, Tailwind default curve | `--transform-origin` (anchor side) | yes (CSS) | not explicit (no motion-reduce) | `ui/popover.tsx:95` |
| Popover positioner | reposition | `top` | 150ms ease-out | — | yes | `motion-reduce:transition-none` | `ui/popover.tsx:85` |
| Popover viewport content swap | content change | current/previous opacity crossfade | 150ms | — | yes | — | `ui/popover.tsx:114` |
| **Menu / context menu** | open/close | **none (snap)** | — | origin set but no transition | — | — | `ui/menu.tsx:58` |
| Menu checkbox switch thumb | toggle/press | translate .15s, radius .15s, scale-x 1.1 (.1s after .1s) | per `[transition:...]` | left / thumb | yes | — | `ui/menu.tsx:194` |
| Combobox popup | open/close | scale, opacity | 150ms | `--transform-origin` | yes | — | `ui/combobox.tsx:170` |
| Tooltip | hover | scale 0.98 ↔ 1, opacity | 150ms; `data-instant` = 0 (when moving between grouped tooltips) | `--transform-origin` | yes | — | `ui/tooltip.tsx:141` |
| Tooltip positioner | moving between triggers | top/left/right/bottom/transform | 150ms | — | yes | — | `ui/tooltip.tsx:107` |
| Tooltip delay | — | — | Base UI default 600ms open, 0 close; sidebar uses `delay=150 closeDelay=0` | — | — | — | `Sidebar.tsx:3235-3238` |
| Preview card | hover | scale 0.98, opacity | 150ms | `--transform-origin` | yes | — | `ui/preview-card.tsx:33` |
| Dialog popup | open/close | scale 0.98 ↔ 1, opacity, translate; nested: parent scales `1−0.1·n`, lifts 1.25rem·n, child from translate-y-8 | 200ms ease-in-out | top when nested | yes | — | `ui/dialog-styles.ts:7` |
| Dialog backdrop | open/close | opacity (`transition-all`) | 200ms | — | yes | — | `ui/dialog-styles.ts:1` |
| Mobile sheet dialog | open/close | translate-y 1rem | 200ms | — | yes | — | `ui/dialog-styles.ts:13` |
| Sheet (right panel on narrow screens) | open/close | opacity, translate ±2rem by side; backdrop opacity + blur | 200ms ease-in-out (or the setting's duration; 0 = instant) | — | yes | — | `ui/sheet.tsx:22,93-105` |
| Collapsible (settings, markdown `<details>`, V2 event rows, pending input) | toggle | height 0 ↔ `--collapsible-panel-height` | 200ms default curve | — | yes | `motion-reduce` | `ui/collapsible.tsx:29` |
| AnimatedHeight / AnimatedWidth (dialogs, labels) | content resize | height / width | 200ms / 150ms ease-out | — | yes | `motion-reduce` | `AnimatedHeight.tsx:86`, `AnimatedWidth.tsx:30` |
| Timeline work groups / tool rows | toggle | **snap** (no collapsible); chevron `transition-transform duration-200` | 200ms | — | — | — | `chat/TimelineWorkEntryRow.tsx:506` |
| Toasts | add/remove/stack | transform .5s `cubic-bezier(.22,1,.36,1)`, opacity .5s, height .15s; stacked scale `1 − index·0.1`, peek 0.75rem | 500ms | top/bottom | yes | — | `ui/toast.tsx:602-622` |
| Sidebar thread list | reorder, insert, remove, project fold (not shelf folds) | FLIP translateY; entering rows opacity 0 → 1 from `−min(h,40)px` (or riding a neighbour's delta); exiting rows are **cloned** absolutely, fade to 0 and travel | 150ms ease-out (WAAPI) | — | yes (offset carried from progress) | `matchMedia`; skipped if more than 40 rows fade or while dragging | `Sidebar.motion.ts:1-246`, `Sidebar.tsx:1990-2012` |
| Sidebar width toggle | ⌘B | width / left / right | setting (default 0 = snap) ease-out | — | yes | via setting | `ui/sidebar.tsx:364,376` |
| Right / preview panel | open/close | width from `@starting w-0` | setting (default 0) ease-out | — | yes | via setting | `preview/PreviewPanelShell.tsx:132` |
| Right panel tab switch | click tab | **none (snap)** | — | — | — | — | `chat/RightPanelSurfaceContent.tsx` |
| Terminal drawer | open | grid-rows 0fr → 1fr | setting | — | yes | via setting | `PersistentThreadTerminals.tsx:247` |
| Composer slash / mention menu | trigger | **none (snap)**; inline `Command` list | — | — | — | — | `chat/ComposerCommandMenu.tsx` |
| Model picker sidebar indicator | selection | `top` of a 3px primary bar | 200ms ease-out | — | yes | — | `chat/ModelPickerSidebar.tsx:90` |
| Composer surface | focus / drag states | background-color | 200ms | — | yes | — | `chat/ChatComposer.tsx:3305` |
| Send button | hover / press | `transition-all`, `hover:scale-105`, inset-shadow pressed | 150ms | centre | yes | — | `chat/ComposerPrimaryActions.tsx:257` |
| Stop button | hover | colors | 150ms | — | yes | — | `chat/ComposerPrimaryActions.tsx:132` |
| Attachment chips | add | scale 0.95 → 1, opacity 0 → 1 (`@starting`) | 300ms `--ease-drawer` | centre | yes | `motion-reduce` | `chat/ComposerAttachmentStrip.tsx:102` |
| Composer banners | mount | opacity from 0 | 150ms ease-out | — | yes | `motion-reduce` | `chat/ComposerBannerStack.tsx:31` |
| ThreadNow / find bar / undo notice | mount | opacity from 0 | 150ms / 150ms / 200ms ease-out | — | yes | `motion-reduce` | `chat/ThreadNow.tsx:24`, `chat/ThreadFindBar.tsx:65`, `sidebar/SidebarThreadUndoNotice.tsx:46` |
| Stash badge | count change | translate-y 2px + opacity from 0; count pulse 1.4s | 180ms ease-out | — | yes | `motion-reduce` | `chat/ComposerStashBadge.tsx:23-49` |
| Composer context (branch) label | width change | width FLIP (WAAPI) | 180ms `cubic-bezier(0.32,0.72,0,1)` | — | yes | — | `BranchToolbar.tsx:434-475` |
| Composer context label compact | compact toggle | opacity | 180ms `--ease-drawer` | — | yes | `motion-reduce` | `ComposerContextLabel.tsx:29` |
| Resting composer (mobile collapse) | collapse/expand | WAAPI height, prompt/action translate FLIP, prompt `clip-path` reveal, controls opacity + 4px drift (half duration, after half duration when expanding), image previews fade | setting duration, `cubic-bezier(0.32,0.72,0,1)`, cleanup timer +50ms | — | **yes: retargets from current rect and keeps the remaining time if the target is unchanged** | via setting | `chat/useComposerRestingTransition.ts:1-460` |
| Status dots (thread working, terminal) | running | stepped opacity pulse | 2s | — | — | `motion-safe:` | `ThreadStatusIndicators.tsx:563-785` |
| Connection status dot | connect | ping 2s, color 2000ms | — | — | — | — | `ConnectionStatusDot.tsx` |
| Spinners | loading | rotate | 1s linear | centre | — | — | `ui/spinner.tsx:7` |
| Skeletons | loading | stepped opacity | 2.4s | — | — | `motion-safe:` | `ui/skeleton.tsx:5` |
| Active tool label shimmer | tool running | text gradient sweep | 2.2s `steps(30)` | — | — | yes | `index.css:551`, `chat/TimelineWorkGroupRows.tsx:404-417` |
| Message hover actions / timestamps | hover | opacity 0 → 1 | 200ms, no delay | — | yes | — | `chat/TimelineUserMessageRow.tsx:330`, `chat/TimelineAssistantRows.tsx:171` |
| Timeline minimap | hover / in-view | opacity, transform | 150ms | left | yes | — | `chat/TimelineMinimap.tsx:121,220,309` |
| Scroll-area thumb | hover / scroll | opacity, shown at once, hidden after a 300ms delay | 100ms | — | yes | — | `ui/scroll-area.tsx:87` |
| Switch | toggle | bg/shadow 200ms | 200ms | — | yes | — | `ui/switch.tsx` |
| History swipe nav | swipe | 100–200ms | — | — | — | — | `HistorySwipeNavigation.tsx` |
| Fork icon busy | busy | `animate-pulse` (2s) | — | — | — | — | `chat/TimelineAssistantRows.tsx:125` |

## 5. Comparison with Ferrite

Ferrite's kit was modelled on Claude Desktop. Its machinery:
- `Played`: one-shots on the executor clock.
- `Reveal`: open/shut with enter and exit specs.
- `Tween`: retargets from its current value.
- `settle` / `settled`: per-id values that ease toward a target.
- `frames_through`: requests frames through the cheapest view.
- The pulse clock, `frame_loop`, and hover blends.
- Painted-only `scale` with `scale_origin`.

| T3 motion | Ferrite | Notes / feasibility in GPUI |
|---|---|---|
| Room reserved below the anchored message, shrinking as the reply grows | **Has it**: `Runway` (`transcript.rs:442-480, 2613-2690`) sizes `into + view_h − below − pad_b` and uses the room up as rows grow. `scroll_to_new_prompt` sets `jump_to_new_prompt` and the next reconcile calls `scroll_to(TurnBand{newest, 0})`. | Same idea. Ferrite's anchor is the turn band's gap (no separate 24px). |
| **Smooth scroll to the anchored message** (native, ~≤200ms ease-in-out) | **Differs: Ferrite snaps.** `scroll.scroll_to` → `ListState::scroll_to` jumps in one frame. | Feasible. Drive a `Tween` on a pixel offset (from the current `logical_scroll_top` to the target), and each frame call `list.scroll_to(ListOffset{item_ix, offset_in_item})` or `scroll_by(delta)` from `on_children_prepainted`, asking for frames through the transcript view. Complications: (a) GPUI's list measures lazily, so the target's absolute offset is only known once rows from the current top to the anchor are measured. Tween in item space (fixed item_ix, offset going from −distance to `into`), or tween a `scroll_by` remainder each frame like T3's settle loop does. (b) Runway sizing currently re-issues `scroll_to` on each prepaint until settled; that must hand over to the tween. (c) Under reduced motion, snap. (d) Any wheel input cancels the tween. Use a Chromium-like duration: `min(√Δ,12)/60` s on `EASE_STANDARD` (0.42,0,0.58,1 for true ease-in-out), or `MOTION_DUR_BASE_MS` on the quart. |
| Follow while working with smooth scrolling to the end | **Differs**: `FollowMode::Tail` snaps to the end each layout. | Feasible but riskier. A per-frame smoothed tail (exponential approach or a short Tween retargeted on each growth) means GPUI's own tail follow can no longer own the offset, and it must not flicker the "following" state. Medium effort. Ferrite's 400ms stream fade already softens growth, so this is lower priority than the send scroll. |
| Anchor released to the end when tool/reasoning work starts | **Missing / differs**: Ferrite keeps the Runway until rows outgrow the viewport. | Policy choice, easy to add (clear the Runway and `scroll_to_bottom` when the first tool row of the new turn arrives). Worth weighing: T3 jumps to the end the moment tools start. |
| Optimistic message, no entrance | **Differs, deliberately**: Ferrite pops the bubble (`BUBBLE_IN` 450ms overshoot, scale 0.92 → 1 from its right edge, 6px rise, opacity). | Ferrite is already "more animated" here. Combining the pop with a smooth scroll needs care: the bubble should pop as it arrives at the top, not while travelling. Consider starting the pop when the scroll tween ends, or shortening it. |
| Streamed blocks fade in, 600ms ease-out, per block | **Has it, differs**: `STREAM_FADE` 400ms linear per streamed run (finer grained), with landing at turn end. | Fine as is. |
| "Scroll to end" pill: 150ms show debounce, no fade, label + summary, smooth scroll on click | **Has it, differs**: round jump button fading over `JUMP_FADE` 200ms, shown 2+ lines from the end; the click **snaps** (`scroll_to_bottom`). | Add the smooth scroll on click with the same tween as the send scroll. A show debounce is a cheap win against flashing during thread switches. |
| Draft hero → thread composer FLIP (translate3d, 0.4,0,0.2,1, setting duration; default off) | **Missing** (Ferrite has no hero layout that moves the Composer). | Feasible with painted-only offset: GPUI has no transform-translate on layout, but `relative().top(px)` / `left(px)` with a `Played` or `Tween` can stand in. Measure the old bounds with `on_children_prepainted` / `canvas`, then animate the offset to 0. Since T3 ships it at 0ms by default, it is low priority. |
| Mobile View Transition morph | N/A | Desktop only. |
| Resting composer collapse (mobile) | N/A / Ferrite has `COMPOSER_GROW` 200ms quart for its box height | T3's desktop composer height snaps. |
| Popover / tooltip / preview-card scale 0.98 + fade 150ms from origin | Tooltip **has it** (`TOOLTIP_IN` 120ms ease-snap, 0.98). Popovers **differ**: `MENU_IN` 0ms snap. | If wanted, `MENU_IN` could be set to 150ms with scale 0.98 via `grow_from` + `scale_origin`; the machinery exists (`menu_in` reads the token). T3 snaps menus too, so this only matters for popovers like the model picker. |
| Menus snap | **Has it (same)** | — |
| Dialog 200ms ease-in-out, scale 0.98, backdrop fade, nested stacking | **Has it** (`DIALOG_IN/OUT` 200ms quart, 0.98, veil) | Nested stacking missing; rarely matters. |
| Toasts .5s `(.22,1,.36,1)`, stacked scale | **Has it, differs** (200ms quart, slide in, shrink to 0.85 on exit) | Fine. |
| Collapsible height 200ms (settings, details) | Nav folds `FOLD` 150ms ease-out; transcript tool folds snap (as T3's work groups do) | Same split as T3. |
| Sidebar list FLIP: reorder, insert, remove with cloned exits, 150ms ease-out, ±40px travel | **Missing** (nav rows move at once; only fold bodies animate) | Feasible. Keep last-frame row tops per thread id (a `HashMap<ThreadId, f32>` sampled in prepaint), and on change give each row a `Tween` of a painted y-offset (`relative().top(px)`) from Δ → 0 over 150ms `EASE_OUT`. Entering rows fade from −min(h,40). Exits need a retained "ghost" row drawn absolutely (Ferrite's `Exits` machinery for toasts is the pattern). Redraw the nav part only (`frames_through`). Probably the biggest visible gap after the send scroll, since threads re-sort as agents work. |
| Sidebar / right panel width (setting, default 0) | Snap (same as T3's default) | Ferrite has the slot slide machinery (`SLOT_OPEN` 240ms) if ever wanted. |
| Right panel tab switch snaps | Ferrite **fades** tab bodies (`ROW_IN` 180ms from 0.3) | Ferrite is more animated. |
| Attachment chip add: scale 0.95 + fade 300ms drawer curve | **Missing?** (not in catalog) | Trivial: `grow_from(id, chip, FROM_CENTRE)` with a 300ms `EASE_SNAP` spec and from-scale 0.95; play only on add, never first paint. |
| Banners / find bar / undo notice fade in 150–200ms | Partially (toasts, approval promote). Find bar: not listed | Trivial with `fade_in` (`FADE_QUICK` 150ms). |
| Branch label width FLIP 180ms snap curve | Missing | Feasible via a width `Tween`; low value. |
| Send button `hover:scale-105` 150ms | Differs: Ferrite buttons blend ground 60ms and squish on press 0.975 → overshoot back | Taste; Ferrite's is Claude's. |
| Stepped status pulse (2s, steps(6)) on working dots | **Differs deliberately**: no dot breathes; Ferrite's working mark orbs run at display rate | — |
| Tool label shimmer 2.2s steps(30), paused off-screen | **Has it**: `MOTION_SHIMMER_MS` 2.4s on the working line | T3's is stepped to ~14fps worth of frames; Ferrite rides `frame_loop`. |
| Skeleton pulse 2.4s stepped | Unknown / likely N/A | — |
| Model picker indicator bar `top` 200ms ease-out | Missing (selection moves at once, by Ferrite's rule) | Easy with `settle` on a y-offset; conflicts with the "selection is keyboard-rate" rule. |
| Message hover actions 200ms opacity, no delay | Has it (`ACTIONS_IN` 120ms after 100ms, out 60ms) | — |
| Scrollbar fade (300ms delay, 100ms) | Has it (1.4s linger, 100ms fade, widen) | — |
| Citation highlight pulse (custom property) | Unknown | Feasible with `Played` on a highlight alpha. |
| Panel-animation user setting (0–400ms slider) | Missing | Ferrite could expose one duration that scales the slot/panel specs; minor. |

## 6. Top gaps, ranked by how much they buy the "chat moves when I send" feel

1. **Smooth scroll of the sent prompt to the top** (T3: native ~≤200ms ease-in-out to anchor at 24px; Ferrite: one-frame snap). Build a `ScrollTween` on `TranscriptScroll` that owns the offset while running, cancels on any wheel/key/scrollbar input, snaps under reduced motion, and hands off to the Runway's settle loop.
2. **Smooth "jump to latest" click** (the same tween to the end).
3. **Pop/scroll choreography**: once the scroll moves, start the 450ms bubble pop when it lands, or the pop and the travel will compete.
4. **Smooth tail follow while streaming** (T3 animates follow scrolls only while working). This is the medium-risk change because GPUI's `FollowMode::Tail` owns that offset.
5. **Policy: release the anchor when tools start** (T3 jumps to the end on the first tool or reasoning row). Easy; decide by taste.
6. **Sidebar row FLIP on reorder, insert and remove** (150ms ease-out, ±40px, cloned exits).
7. Small pieces: attachment-chip pop (300ms, 0.95), banner/find-bar fades (150ms), pill show debounce (150ms), and optionally a 150ms 0.98 fade for popovers (the model picker).

Things Ferrite already does at least as well: bubble entrance, per-run stream fade, tab body fade, approval promote, label morph, count tick, dialog/toast/tooltip specs, button press.
Like T3, Ferrite snaps menus, panels, sidebar and tool folds.

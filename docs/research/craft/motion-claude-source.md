# Motion, from Claude's source: what Claude Desktop animates, and what Ferrite misses

Source: `/Applications/Claude.app/Contents/Resources/ion-dist.asar`, extracted 2026-11-04 and read only (never run). Every Claude number below comes from the bundle's CSS (`assets/v1/*.css`) or from the class strings and Web Animations calls in its JS. File names carry the build hash, so the names here will not match a future build.

This replaces `motion-measured.md` for Claude wherever the two disagree. The recordings could not see a 60 ms rotation, a 120 ms fade on a dark surface, a scale of 0.92 to 1 on a right-anchored bubble, or any exit that happens while the pointer moves away. The source shows them all.

## Claude's tokens

Defined on `.cds-root` in `c6a992d55-*.css`. The app renders inside `.cds-root`, so these values apply. The `:root` fallback of `--cds-ease-out` is `cubic-bezier(0,0,.2,1)`, but that never applies in the app.

| Token | Value | Tailwind class |
| --- | --- | --- |
| `--cds-dur-fast` | 60 ms | `duration-fast` |
| `--cds-dur-snap` | 120 ms | `duration-snap` |
| `--cds-dur-base` | 200 ms | `duration-base` |
| `--cds-dur-sheet` | 300 ms | `duration-sheet` |
| `--cds-dur-slow` | 450 ms | `duration-slow` |
| `--cds-ease-out` | `cubic-bezier(.165,.84,.44,1)` (ease-out quart) | `ease-out` |
| `--cds-ease-snap` | `cubic-bezier(.32,.72,0,1)` (Ferrite already has it as `MOTION_EASE_SLIDE`) | `ease-snap` |
| `--cds-ease-overshoot` | `cubic-bezier(.34,1.3,.64,1)` | `ease-overshoot` |
| `--cds-btn-spring` | `linear(0,.2459,.6526,.9468,1.0764,1.0915,1.0585,1.0219,.9993,.9914,.9921,.9957,.9988,1.0004,1)` | button release |
| Tailwind default | 150 ms, `cubic-bezier(.4,0,.2,1)` | a bare `transition-*` with no duration or ease class |
| Shell (`--df-*`, `shared-styles-*.css`) | `--df-ease-out` `cubic-bezier(.32,.72,0,1)`; `--df-dur-fast` 120 ms; `--df-dur-slow` 300 ms | sidebar and panes |

A bare `ease-out` in a keyframe shorthand (for example `.animate-transcript-in{animation:.3s ease-out …}`) is the CSS keyword, `cubic-bezier(0,0,.58,1)`, not the token.

**Reduced motion.** A global rule sets `*{animation-duration:.01ms!important;animation-iteration-count:1!important}` under `prefers-reduced-motion:reduce` or `[data-reduce-motion=true]`. Each component also adds `motion-reduce:transition-none`, so transitions snap too. Streaming fades are skipped in JS (`shared-3-*.js`, `Wl()`). The timeline's `timeline-block-enter` falls back to a plain fade.

## Table: element, Claude from source, Ferrite now, gap

"Snap" means no transition or animation exists for that property.

| Element | Claude (source) | Ferrite now | Gap |
| --- | --- | --- | --- |
| **Button hover** (cds `Button`) | The fill layer transitions `background-color, box-shadow, color` over 60 ms on the ease-out quart (`nc`, `shared-frame-*.js`). | 100 ms on `cubic-bezier(.4,0,.2,1)` (`HOVER_FADE`, `components::faded_button`, `pointer.rs`). | Ferrite is **slower**. Use 60 ms on the quart. |
| **Button press** | `.cds-btn-squish`: scale 0.975 over 60 ms `ease-out` on `:active`, released over 450 ms on `--cds-btn-spring` (a slight overshoot). The same holds for `Card` with a `CardLink`. | The press lands on its frame; nothing moves. | **Missing.** |
| **Sidebar row hover** (session list, user menu, nav buttons) | Snap. `hover:bg-[var(--df-hover)]` with no transition (`shared-18`, `shared-19`, `shared-21-*.js`). `.df-chrome-btn{transition-duration:0s}`. | 100 ms blend (`nav.rs`, `hover_listener`). | Ferrite **over-animates**. Snap. |
| **Menu item highlight** | Snap. `data-[highlighted]:bg-fill-ghost-hover` with no transition (`mx`, `shared-frame`). | 100 ms blend (`pointer.rs` `FLOAT_HOVER`). | Ferrite over-animates. Snap. |
| **Menus and dropdowns** (cds `Menu`, `ContextMenu`, `ModelSelector`) | Snap open and snap close. The popup class `aS` has no transition, and the portal (`oh`) and layer (`rf`) add none. The sheet transitions are for mobile only. | Snap (`MOTION_MENU_IN_MS = 0`). | None. The recordings were right. |
| **Popover** (cds `Popover`, class `Cg`; covers the usage popover and pickers) | Snap. | Snap. | None. |
| **Tooltip** | Opacity 0 → 1 and scale 0.98 → 1 from `--transform-origin`, over 120 ms on `ease-snap`, the same on exit (`Qg`). Inside a group: open delay 300 ms (`wy`), close delay 200 ms, and moving between tooltips is instant (`data-[instant]:duration-0`). | gpui tooltip: a 500 ms show delay (`gpui-base/src/tooltip.rs`), then a snap in and out. | **Missing fade and scale.** The delay is 200 ms longer than Claude's. |
| **CoachMark / InfoTip card** | Opacity and scale 0.97, 200 ms on the quart, enter and exit (`gM`). | n/a | n/a |
| **Dialog / modal** (cds `Dialog`, `ConfirmationDialog`) | Popup: `opacity, scale, filter`, from 0 and 0.98, over 200 ms on the quart, **both enter and exit** (`GE`). Backdrop: opacity over 200 ms, both ways (`KE`). With a nested dialog open, the parent dims through an `::after` scrim over 200 ms (`eD`). | `dialog_in`: 180 ms on CSS `ease`, scale 0.96 plus a 2 px rise. **No exit.** Veil: 150 ms in, no exit (`motion.rs` `dialog_in`, `veil_in`; `cockpit.rs` 5740). | **Exit missing.** The scale and duration differ slightly. |
| **Command palette** | "dialog" variant: opacity and scale 0.98 from `origin-top`, enter 200 ms (`duration-base`), exit 120 ms (`duration-snap`), on the quart. Backdrop: a 50 ms fade (`shared-19-*.js`, `oj`). | `dialog_in_casting`: 180 ms `ease`, scale 0.96, no exit (`palette.rs:654`). | **Exit missing.** Origin and values differ. |
| **Settings** | A routed page. No transition found. | A sheet on `dialog_in`, 180 ms. | Ferrite animates more. Fine either way. |
| **Sidebar collapse / expand (⌘B)** | **Snap.** Neither `.dframe-sidebar` nor `.dframe-content-inner` has a width or translate transition, and `[data-collapsed] .dframe-sidebar{transition:none}`. The collapsed sidebar's hover peek is a popover: `opacity .14s ease` plus `transform .18s cubic-bezier(.32,.72,0,1)`, from `translate(0,-6px) scale(.98)`. | The width rides 200 ms CSS `ease` (`RESIZE`), the content fades over 150 ms (`MOTION_NAV_FADE_MS`), and **every part is rebuilt each frame** (`cockpit.rs` `nav_tween`, `nav_fade`). | Ferrite **over-animates**, and pays for it with a relayout of the whole board on every frame. Snap. |
| **Sidebar group (project) fold** | Expand: `grid-template-rows 0fr→1fr` plus a fade, 150 ms `ease-out`. Collapse: framer `{height:0, opacity:0}`, 150 ms `easeOut` (`shared-20-*.js`, `animate-sidebar-group-expand`). | Snap. | **Missing.** |
| **Right side panel** | No width transition on the panes. Snap, as `motion-measured.md` found. A secondary split pane fades in through `@starting-style{opacity:0}` and `.dframe-pane-extra{transition:opacity .18s var(--df-ease-out)}` (180 ms, `cubic-bezier(.32,.72,0,1)`). | Snap (`cockpit/panel.rs`). The Review slot slides 240 ms / 200 ms (Ferrite's own). | None for the panel. |
| **Thread switch** | Warm: content lands at once. Slow load (`long-delayed` mode, `cacf551c6-*.js`): rows, the trailing slot and the tail chrome fade in over 300 ms `ease-out` (`animate-transcript-in`, keyword ease-out). A body that first appears after a placeholder: `df-body-fade-in`, 300 ms `ease-out` from opacity 0.01. | Lands at once. | Small. Matters only on slow loads. |
| **Tool group / step expand** (Code `TurnStatus`, `TurnStatusStep`) | **Snap**, explicitly: the panel is `h-auto overflow-visible transition-none duration-0 data-[starting-style]:h-auto data-[ending-style]:h-auto` (`c734f318c-*.js`). | Snap. | None. |
| **Disclosure chevron** (`TurnStatus` `CaretRightFilled`) | Rotates 90° over **60 ms** (`transition-transform duration-fast`, Tailwind default curve `cubic-bezier(.4,0,.2,1)`). The cds `Collapsible` caret uses the bare `transition-transform` (150 ms on the same curve). | Snap: the chevron turns on the frame its fold flips. | **Missing.** Nearly invisible at 60 ms, and it is why the recordings saw a snap. |
| **Generic disclosure panel** (cds `Collapsible`) | Height to `--collapsible-panel-height` over 200 ms on the quart, both ways, clip-path inset during (`uT`). Not used by the Code tool steps. | Output folds snap. | Only if a Ferrite fold maps to a plain `Collapsible`. |
| **Claude.ai timeline "N steps"** (shared folding, `c05b96c7f`, `c5ac695a3`) | Expand: `grid-rows 0fr→1fr` plus opacity, 200 ms `cubic-bezier(.19,1,.22,1)`. Collapse: framer height and opacity over 200 ms on the same curve. A new block enters with `timeline-block-enter`: 430 ms, blur 4 px, translateY 5 px, scale 0.9, invisible for the first 30%. An evicted block glides `translateY` over 250 ms `cubic-bezier(.19,1,.22,1)`. | n/a | Reference only. This is the chat surface, not Code. |
| **Working status label** (`TurnStatus` face morph) | When the label changes, the old face leaves (opacity → 0, y → −3 px) and the new one enters (`cds-morph-in`: from opacity 0, y +3 px). Each takes 180 ms on `cubic-bezier(.2,0,0,1)` (`shared-frame` `yL`, `bL`, `xL`). The step indent hang eases `margin-inline-start` over 200 ms `ease-out`. | The caption changes in place (snap). The shimmer loops. | **Missing.** It is visible every time the working line changes its words. |
| **Composer grow** | Snap. No height transition on `ChatComposer`. A JS 200 ms height ease exists only on the comment card's envelope (`frame-shell-chrome`). | Snap. | None. |
| **Composer focus ring** | No transition on the composer shell's focus shadow. Inputs and buttons use `transition-shadow duration-fast` (60 ms). | Snap. | Negligible. |
| **Send / stop button** | `AnimatePresence mode="wait"`: the leaving button fades out over 80 ms, then the new one fades in over 120 ms `ease-out` (`animate-primary-action-in`, `shared-8-*.js` `BR`, `VR`). Composer controls fade in over 75 ms after a 50 ms delay (`animate-composer-control-in`). | The glyph crossfades over 150 ms `ease`, scaling 0.5 ↔ 1 (`pane/composer.rs` `settled`, `MOTION_SEND_GLYPH_FROM`). | Close enough. Optional retune to 80 ms out then 120 ms in, with no scale. |
| **Attachment chips** | The container rises from translateY −8 px and opacity 0, 200 ms `cubic-bezier(.215,.61,.355,1)`. Each chip enters from scale 0.92 and y +4 px over 180 ms on the same curve. A chip leaves over 130 ms `cubic-bezier(.55,.055,.675,.19)` (a view transition). | (no chips) | n/a |
| **User message enter** | `animate-code-user-bubble-enter`: from `opacity:0; transform:scale(.92) translateY(6px)` to none, over **450 ms** on **`cubic-bezier(.34,1.3,.64,1)`** (overshoot). Origin: right in LTR (`c96d752d0-*.js` `_8`, `Lft`). | 250 ms opacity only on CSS `ease-out` (`BUBBLE_FADE`, `transcript.rs:1641`). | **Missing the pop.** This is the most visible gap: it plays on every send. |
| **Streaming text** | Each new delta span fades 0 → 1 over **400 ms linear** (`e.animate([{opacity:0},{opacity:1,offset:400/a},{opacity:1}],{duration:a,easing:"linear",id:"delta-fade"})`). Its timeline is padded to end on a 1 s grid, so many spans finish together. When a turn's ramps end, a fade with more than 120 ms left is sped up (`playbackRate = remaining/120`) so it lands within 120 ms (`shared-3-*.js`, `Rl=120`). A legacy per-word path uses `._wordAnimating`: 100 ms linear. | Each run fades over 300 ms CSS `ease-out` (`STREAM_FADE`, `rich.rs` `TextFresh`). Fades still running land at full at turn end. | Curve and length differ. Claude's linear 400 ms reads softer; Ferrite's ease-out front-loads the ink. The turn-end behaviour differs too: Claude compresses to 120 ms, Ferrite jumps. |
| **Message actions** (copy, retry and others under a message) | Hover reveal of `--cds-message-actions-opacity`: in over 120 ms (`ease-snap`) after a 100 ms delay; out over 60 ms (`ease-snap`) with no delay. Instant under touch or `data-reveal=instant`. | Snap (`group_hover` opacity 0 → 1, `transcript.rs:1661`). | **Missing.** Visible on every message hover. |
| **Approval card** | `animate-code-approval-promote`: from `opacity:.75; transform:translateY(-6px) scale(.97)` to none, over **220 ms** `cubic-bezier(.215,.61,.355,1)`, `origin-top` (`c2d1aeb56-*.js`). | Fades from 0 over 150 ms CSS `ease` (`decision.rs:715`). | Wrong shape: Claude drops down and settles; Ferrite only fades. |
| **Scroll-to-bottom button** (`jump-to-latest`, `cf854d0e8-*.js`) | Shows and hides with an opacity fade, 200 ms on the default curve `cubic-bezier(.4,0,.2,1)`, and is inert while hidden. The hover ground fades over 200 ms. While streaming, the button shows the working mark (fading in over 200 ms), and hovering swaps it back to the arrow. | Snap in and out. Hover 100 ms. The jump itself snaps. | **Fade missing.** The working mark inside is also missing (Codex's "•••" idea, already in Claude). |
| **Toasts** (cds toast stack, `shared-18-*.js`) | Enter: from opacity 0 and `--ty:100%` (rising its own height) over 200 ms on the quart. Exit: opacity 0 and scale 0.85 over 200 ms. Stack: each toast behind sits −14 px and scales by −0.04 per index, transitioning over 200 ms. The body of a toast behind fades out over 200 ms. | Needs-you toast: 180 ms `ease` 4 px drop in, 100 ms out (`notifications.rs`). Kit toasts: the kit's 400 ms slide. | Shorter travel and a softer exit than Claude. No stack. |
| **Selection reply / quote button** | No motion found near Code's "Reply" control (a cds popover, so it snaps). The prose review bar rises 8 px and fades over 200 ms on the quart, with its icon 60 ms later. | Hover blend only. | None for Code. |
| **Usage ring** (`.urm`, `c403c4b83-*.css`) | The arc's `stroke-dashoffset` animates over 300 ms and its stroke colour over 200 ms, both on the quart. The track's colour also takes 200 ms. Entrance "draw": the arc sweeps over 900 ms (`2×slow`) on the overshoot curve, plus a 120 ms linear fade, with the track fading in 300 ms after a 150 ms delay. "Quiet" entrance: the sweep and fade take 200 ms. Hover: a 60 ms scale and glow. Warning, critical and limit states play one-off beats of 2.3 s, 1.45 s and 2.45 s. All of it is gated by reduced motion. | Static (`pane/usage.rs`). | **Missing.** The value change matters most. The entrance is decoration. |
| **Usage meter bars** | Fill `transform` and background over 200 ms on the quart (`[data-usage-bars-crossfade]`). | Static. | Small. |
| **Switch** | Knob `transition-transform` over **120 ms** on **overshoot** (`PA`). The track colour snaps (`NA` has no transition). | The knob slides and the track blends over 150 ms `ease` (`components::switch`, `settled`). | Retune the knob. Snap the track. |
| **Checkbox** | The checkmark wipes in by `clip-path`: 120 ms `ease-out` unchecking, 200 ms checking (`transition-[clip-path] duration-snap … group-data-[checked]:duration-base`). The box fill snaps. | (no Ferrite checkbox found) | n/a |
| **Links** | `color, text-decoration-color, box-shadow` over 60 ms on the quart. | Hover blend. | Small. |

## Gaps, ranked by how visible they are

Visibility here is how often the operator sees it multiplied by how big the change is.

1. **User bubble pop on send.** Claude: 450 ms `cubic-bezier(.34,1.3,.64,1)`, opacity 0 → 1, scale 0.92 → 1 from the right edge, translateY 6 → 0. Ferrite: 250 ms, opacity only. It plays on every send.
2. **Message actions fade.** Claude: in over 120 ms `cubic-bezier(.32,.72,0,1)` after a 100 ms delay; out over 60 ms. Ferrite snaps. It shows on every hover over any message.
3. **Tooltip fade.** Claude: opacity and scale 0.98 → 1 over 120 ms `cubic-bezier(.32,.72,0,1)`, both ways, after a 300 ms delay, and instant between tooltips in a group. Ferrite: a 500 ms delay, then a snap.
4. **Streaming fade shape.** Claude: 400 ms linear per delta, sped up to land within 120 ms at turn end. Ferrite: 300 ms ease-out, jumping to full at turn end.
5. **Working label morph.** Claude: the old face goes to opacity 0 and y −3 px, the new one comes from opacity 0 and y +3 px, each over 180 ms `cubic-bezier(.2,0,0,1)`. Ferrite snaps the caption.
6. **Button press and hover.** Claude: press squish to scale 0.975 over 60 ms `ease-out`, released over 450 ms on `--cds-btn-spring`; hover fill over 60 ms on `cubic-bezier(.165,.84,.44,1)`. Ferrite has no press motion, and its hover is 100 ms, slower than Claude's.
7. **Dialog and palette exits.** Claude: a dialog leaves to opacity 0 and scale 0.98 over 200 ms on the quart, its backdrop fading over 200 ms; the palette leaves over 120 ms. Ferrite has no exits.
8. **Approval card promote.** Claude: from opacity 0.75, y −6 px and scale 0.97 to rest over 220 ms `cubic-bezier(.215,.61,.355,1)`, origin top. Ferrite: a 150 ms fade from 0.
9. **Jump-to-latest fade and its working mark.** Claude: 200 ms opacity on `cubic-bezier(.4,0,.2,1)`, with the working mark inside while streaming. Ferrite snaps.
10. **Sidebar project fold.** Claude: height and opacity over 150 ms `ease-out` (`cubic-bezier(0,0,.58,1)`), both ways. Ferrite snaps.
11. **Toasts.** Claude: in over 200 ms on the quart, rising its own height; out to opacity 0 and scale 0.85 over 200 ms; a stack offset of −14 px and −0.04 scale. Ferrite: in over 180 ms with a 4 px drop, out over 100 ms.
12. **Usage ring value change.** Claude: arc over 300 ms and colour over 200 ms, both on the quart. Ferrite is static.
13. **Switch knob.** Claude: 120 ms `cubic-bezier(.34,1.3,.64,1)`, with the track snapping. Ferrite: 150 ms `ease` with the track blending.
14. **Chevron rotate.** Claude: 60 ms `cubic-bezier(.4,0,.2,1)`. Ferrite snaps.

These run the other way: Ferrite animates more than Claude.

- **⌘B sidebar.** Claude snaps; Ferrite rides 200 ms and relays out the whole board every frame.
- **Nav row and menu row hover.** Claude snaps; Ferrite blends over 100 ms.

Fixing both makes Ferrite match Claude and also removes frames.

## Corrections to `motion-measured.md`

| `motion-measured.md` said | Source says |
| --- | --- |
| Menus snap open and closed. | **Correct.** `Menu`, `ContextMenu`, `Popover` and menu-item highlight have no transition. |
| Tool group expand: "Snap, both ways." | **Correct for Code** (`TurnStatusStep` panels are `transition-none duration-0` on purpose). |
| Chevrons should snap: "Snap everything: group, output fold and chevron". | **Wrong.** Code's chevron rotates over 60 ms; a `Collapsible` caret over 150 ms. |
| Right panel: snap. | **Correct.** No pane width transition. A secondary split pane fades in over 180 ms. |
| User bubble: "fades in over about 230 ms, ease-out. No slide." The centroid drift was called "fade bias". | **Wrong.** 450 ms with an overshoot: scale 0.92 → 1 from the right edge plus a 6 px rise. The measured centroid moving 23 px left and 5 px up *was* that scale and rise. The overshoot curve gets opacity near 1 by about 200 ms, which is why the take read as "≈230 ms". |
| Approval card: "≈155 ms, near-linear, with no translate." | **Wrong.** 220 ms `cubic-bezier(.215,.61,.355,1)`, from opacity 0.75, y −6 px and scale 0.97, origin top. It starts at 0.75, not 0, so a mean-intensity series looks short and nearly linear. |
| Streaming: "each chunk fades in over about 300–350 ms, ease-out (≈ quad)". | **Partly wrong.** 400 ms *linear* per delta span. Ending on a 1 s grid lets later spans finish early, which reads as a shorter fade. At turn end, fades are sped up to finish within 120 ms; they are not snapped to full. |
| Queue strip removal: snap. | **Unconfirmed.** The floating composer's queued card (`.fc-queued-card`) rises 0.5 rem over 200 ms on the quart, and on leaving fades and drops 0.5 rem over 200 ms. Whether Code's queue strip is that card was not traced. |
| Thread switch (recorded on Codex only). | Claude: warm switches land at once; slow loads fade rows over 300 ms `ease-out`. "Land at once" stands. |
| Sidebar (⌘B): not recorded. | **Snap** (no width or translate transition on the desktop shell). |
| Caret: hard 500/500 blink. | The composer uses the native caret. A `caret-blink 900ms ease-in-out` utility exists for custom carets. Keep the 1 s step. |
| "Animation is reserved for content arriving on its own." | **Too strong.** Claude also animates tooltips, message-action reveals, dialog and palette **exits**, toasts, button presses, the jump button and sidebar group folds. What it snaps: menus, popovers, hover grounds on rows, tool-step folds, the sidebar and the panel. |

The doc says its capture ran at about 90 fps. Even so, a 60 ms change in a dark-on-dark region, an exit during pointer travel, and a 0.92 scale on a right-anchored element do not survive H.264 at those deltas. Read the source before trusting a recording on anything under about 150 ms.

## Implementation plan, by file

The rules that keep repaint cheap still hold. Each animation asks for frames through the cheapest view that draws it (`motion::frames_through`, `draw_frames_through`). One-shots use `Played`, a flip that can reverse uses `settled` or `Reveal`, and hovers use `HoverFades`. No new per-frame relayout of the board.

### `crates/ferrite-ui/src/theme.rs` (motion section)

- Add Claude's scale: `MOTION_DUR_FAST_MS = 60`, `MOTION_DUR_SNAP_MS = 120`, `MOTION_DUR_BASE_MS = 200`, `MOTION_DUR_SLOW_MS = 450`.
- Add curves:
  - `MOTION_EASE_OUT_QUART = [0.165, 0.84, 0.44, 1.0]`, Claude's `ease-out`;
  - `MOTION_EASE_OVERSHOOT = [0.34, 1.3, 0.64, 1.0]`;
  - `MOTION_EASE_MORPH = [0.2, 0.0, 0.0, 1.0]`;
  - `MOTION_EASE_OUT_CUBIC = [0.215, 0.61, 0.355, 1.0]`.
- `MOTION_EASE_SLIDE` already equals `ease-snap`. Rename it, or document that it serves both.
- `CubicBezier` solves x only, so a y above 1 (overshoot) evaluates correctly. Clamp opacity to 1 at the call site.
- Retune:
  - `MOTION_BUBBLE_FADE_MS` 250 → 450 on overshoot, and add `MOTION_BUBBLE_FROM_SCALE = 0.92` and `MOTION_BUBBLE_RISE = 6.0`;
  - `MOTION_STREAM_FADE_MS` 300 → 400 on linear, and add `MOTION_STREAM_LAND_MS = 120`;
  - `MOTION_HOVER_FADE_MS` 100 → 60 on the quart (buttons only, below);
  - `MOTION_DIALOG_IN_MS` 180 → 200 on the quart, `MOTION_FLOAT_FROM_SCALE` 0.96 → 0.98, `MOTION_DIALOG_RISE` → 0;
  - `MOTION_TOAST_IN_MS`, `MOTION_TOAST_OUT_MS` → 200 / 200.
- Delete `MOTION_RESIZE_MS` and `MOTION_NAV_FADE_MS` once ⌘B snaps.
- Rewrite rule 1 ("What the operator dismisses … goes at once") to Claude's split: **menus and popovers snap; dialogs, the palette and toasts fade out.** Rewrite rule 2: **one hover blend for buttons (60 ms); list rows and menu rows snap.**

### `crates/ferrite-ui/src/motion.rs`

- `bubble_in(id, element)`: opacity, painted scale and rise from `Played`, on the overshoot curve, with opacity clamped. Scale is painted about the element's centre (`with_element_scale`). Claude's origin is the right edge, so either extend the gpui patch with an origin, or translate x by `(1 − s)·w/2` from the bubble's known width. The patch is the clean fix: tooltips (origin at the anchor) and the palette (origin top) need it too.
- `promote_in(id, element)` for the approval card: opacity 0.75 → 1, y −6 → 0, scale 0.97 → 1 (origin top) over 220 ms on `EASE_OUT_CUBIC`.
- `float_out(left, element)` and `veil_out(left, element)`: exits on `Origin::At`, the same shape as `toast_out`. They draw a face that answers no pointer, so a closed dialog never holds its hit area. Use them for dialogs (200 ms) and the palette (120 ms).
- `morph(id_key, element)`: an entrance from y +3 px, plus a held exit to y −3 px, each 180 ms on `EASE_MORPH`. It is a keyed one-shot pair, like `toast_in` / `toast_out`.
- `HoverFades::set_with` already takes a spec. Add a delay, or separate in and out specs, so the message actions can run 100 ms delay + 120 ms in and 60 ms out. Keep it one map and one tick.
- The catalog table at the top: rewrite the rows for hover, menu, dialog, sidebar, chevron, stream and bubble to cite this doc.

### `crates/ferrite-pane/src/transcript.rs`

- `arrival_fade`: a prompt row uses `bubble_in` instead of `BUBBLE_FADE` opacity. It already plays only for rows appended live, so first paint and scroll-back stay still.
- `message_actions` (1661): replace `group_hover` opacity with `hover_t(key)` on the message row's key, driven by the delayed-in / quick-out spec above. Frame cost: the blend asks for frames through the transcript view, the path the jump button's hover already uses (`file_links::mark_fading`). Check that this path redraws without rebuilding rich text, as the hover card notes warn. If it rebuilds, hang the actions in the loops overlay.
- `jump_to_latest` (1577): keep the button mounted while it fades. Use a `Reveal` at 200 ms on `EASE_STANDARD`; it ignores the pointer while closing. Optionally draw `components::working_mark` inside it while the turn streams out of view: the mark already rides `frame_loop`, so the cost is zero frames.
- The turn-end path that lands fades at full: compress the remaining time to 120 ms instead, by retargeting the run's tween over `MOTION_STREAM_LAND_MS`.

### `crates/ferrite-pane/src/rich.rs`

- `TextFresh` (around line 525): set `fade` to 400 ms and `ease` to linear. There is still one frame source per transcript.

### `crates/ferrite-pane/src/decision.rs`

- Line 715: replace `fade_in(…, FADE_QUICK, 1.0)` with `promote_in`.

### `crates/ferrite-pane/src/pane/working.rs`

- When the caption changes, run `morph`, keyed by the caption text, holding the old face as a no-hit exit for 180 ms. The working line already lives in the loops overlay, so the morph's frames ride the existing overlay frame. No part rebuilds.

### `crates/ferrite-pane/src/pane/usage.rs`

- The ring's fraction gets a `Tween` held in element state, retargeted when the value changes: 300 ms on the quart for the arc, 200 ms for the colour (the same `with_element_state` pattern as `output.rs` `TestBar`). Skip the 900 ms "draw" entrance (YAGNI).

### `crates/ferrite-ui/src/components.rs` and `pointer.rs`

- `faded_button`: hover over 60 ms on the quart. Press: painted scale 0.975 over 60 ms on mouse down, released to 1 over 450 ms on overshoot (an approximation of `--cds-btn-spring`; add a piecewise-linear easing only if overshoot reads wrong). A `Tween` keyed on the button, with frames through `frames_through`.
- `pointer.rs` menu and float rows (`FLOAT_HOVER`): **snap** (drop the blend). That leaves fewer views fading.
- `switch` (884): knob over 120 ms on overshoot. The track snaps (`settled(...).reveal_only()` for the knob, and the track colour taken straight from `on`).

### `crates/ferrite-ui/src/menu.rs` (tooltips)

- `tooltip`: set `tooltip_show_delay(300 ms)` (gpui-pre `div.rs:717`). Wrap the tooltip view's root in a 120 ms opacity and scale-0.98 entrance on `ease-snap`. The tooltip is its own view, so its frames redraw only itself. Check this before landing, because the hover-card precedent shows a host can be dragged in. Claude's instant hand-off between tooltips in a group: skip the entrance when another tooltip closed less than 200 ms ago (one global timestamp).

### `crates/ferrite-cockpit/src/cockpit.rs`

- ⌘B (`nav_tween`, `nav_fade`, around lines 6544–6590 and 14060–14160): **delete the ride.** Set the column's width at once. This removes the per-frame relayout of the board that the motion catalog itself calls the most expensive ride. Remove `RESIZE`, `CONTENT_FADE` and their tests.
- Settings and Project sheets (5740–5751): add `veil_out` and `float_out` on close (200 ms), and retune `dialog_in` through the tokens.

### `crates/ferrite-cockpit/src/palette.rs`, `shortcuts.rs`

- `dialog_in_casting`: 200 ms in; add a 120 ms `float_out`; the veil fades over 50 ms both ways.

### `crates/ferrite-cockpit/src/nav.rs`

- Project and group folds: expand and collapse over 150 ms `ease-out` (`MOTION_EASE_OUT`, already `[0,0,.58,1]`), on height and opacity, through a `Reveal` per group. Only the nav part redraws (it is its own part). Row hovers (`hover_listener` at 667, 842, 994, 1028, 1819): **snap**.
- The nav's rail-and-column fade goes with the ⌘B ride.

### `crates/ferrite-cockpit/src/notifications.rs`

- `toast_in`: 200 ms on the quart. Ferrite hangs toasts top right, so drop in from −100% of the toast's height (Claude's rise mirrored for the top edge), not 4 px. `toast_out`: opacity → 0 and scale → 0.85 over 200 ms. Add a stack offset only if more than one toast can show at a time.

### Leave alone

Menus, popovers, the hover card, tool-step folds, output folds, the panel and the thread switch all snap already, as Claude does. The chevron's 60 ms rotation is optional: if it is added, drive it through the row's existing `settled` state; never add a frame source to the virtualized list for it.

### Order

1. ⌘B snap, and row and menu hover snap (removes cost and matches Claude).
2. User bubble pop.
3. Message actions fade.
4. Tooltip fade and delay.
5. Streaming at 400 ms linear, with the 120 ms landing.
6. Working label morph.
7. Dialog and palette exits.
8. Approval promote.
9. Button press and 60 ms hover.
10. Jump-button fade and its mark.
11. Nav group folds.
12. Toasts.
13. Usage ring.
14. Switch knob.

Each change is one token edit plus one primitive at most. The gpui scale-origin patch, item 2, unblocks items 4, 7 and 8.

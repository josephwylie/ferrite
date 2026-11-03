# Motion: Ferrite (from code) vs Claude and Codex (measured)

Recovered after the 3 Nov crash. The original out-motion.md, with its file:line table, was lost with /tmp. This file is the summary. Re-derive line numbers from `crates/ferrite-ui/src/motion.rs`, the motion section of `theme.rs`, `menu.rs`, `cockpit.rs` and `transcript.rs`.

## Codex, measured from 120fps recordings (13:04–13:10)
The recorder (ffmpeg avfoundation) captures changed frames only, 11–22ms apart, and drops frames under load. Treat a single-frame event as "within one captured frame".
- **Permission menu:** opens and closes in one frame.
- **+ menu:** opens and closes in one frame.
- **Model popover:** missed by the recorder. Earlier note: about a 100ms fade plus blur, no scale.
- **Thread switch:** clears instantly, shows a centred spinner while a cold thread loads (about 4s), then content appears at once, scrolled to the bottom. No crossfade.
- **Send:** the user bubble pins to the top of the viewport with empty space below, a "Today 13:08" divider appears, "Thinking" shimmers as plain text, and the sidebar row jumps to the top of its project with a spinner.
- **Streaming text:** each word fades in with an ease-out, reaching about 78% ink in about 310ms and full ink at 1.1–1.3s. Words land in small chunks, with several mid-fade at once.
- **Streaming tables:** rows appear as they arrive, and column widths reflow live.
- **Answer hover row:** copy, fork and read aloud, plus a time.

## Claude, from earlier notes
- Permission and model menus open in one frame.
- The queue strip fades in over about 100ms.
- The active tool summary line has a shimmer, and the status glyph is animated.

## Ferrite, from code (nominal; never screen-recorded)
- **Menus:** every Ferrite menu fades in over 140ms (opacity 0.3→1, scale 0.96→1, 2px shift). The choice menus (model, effort, mode, +) also draw one blank frame first. Footer cards can wait up to 3 frames before drawing.
- **Thread switch:** a 120ms veil. There are no skeleton chips.
- **Queue strip:** no motion, and the composer height jumps.
- **Approval card:** no motion.
- **Sidebar collapse (⌘B):** 200ms, and every Pane re-lays out on each frame. The right-panel and Review-slot slides do the same.
- **Streaming:** each row fades in from 0 over 500ms with a 4px rise. The frame request sits inside row render (transcript.rs), so the transcript repaints continuously while streaming. Text growing inside a row snaps.
- **Loops on the shared 33ms (30fps) clock:**
  - The braille spinner's 80ms frames show for 66 or 99ms, so the cadence is uneven.
  - The PR refresh spin alternates 33 and 66ms.
  - The shimmer moves about 8pt per step.
  - The caret's 55ms brightening is effectively a two-step snap.
- **⠿ glyph:** the fallback braille font draws the empty dots as rings, so every frame reads as a full ⠿ grid and the spinner looks static.
- **Tool group expand:** the height snaps open, then the content fades in over 220ms. Output folds ease their height instead.
- **Toasts:** the kit's toasts slide 96px over 400ms and leave over 200ms. Ferrite's own toast takes 180ms in and 100ms out.
- **Hover:** four durations (80, 100, 120 and 150ms).
- **Dead code:** `working_spinner` (the star) is used only by a test. Theme rule 8 is stale.

## Ranked changes
1. Snap the permission and context menus (MOTION_MENU_IN_MS 140→0). Give only the model popover a 100ms opacity fade with no scale. Remove the blank first frame and the 3-frame card wait.
2. Fix the loop cadence: retime the spinner frames to the clock (or change the spinner), drop the outline braille font, retime the PR spin, and draw the shimmer and caret at the display rate.
3. Fade the queue strip in over 100ms and ease the composer height.
4. Remove the thread-switch veil.
5. Streaming: fade new words or chunks in over about 300ms with an ease-out instead of fading whole rows over 500ms, drop the 4px rise, and move the frame request out of row render.
6. Measure the sidebar and panel slides. If they drop frames, freeze Pane layout while they slide.
7. Make tool group expand consistent; snapping is the recommendation.
8. Bring the kit toasts onto the toast tokens.
9. Use one hover duration.
10. Fade the scroll-to-bottom button in and out.
11. Delete the star spinner and fix the stale rule docs.

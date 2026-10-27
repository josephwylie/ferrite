---
status: proposed
date: 2026-10-15
---

# Mermaid diagrams: fences drawn once by the visual engine, kept as images

Extends [ADR 0013](0013-inline-web-visuals.md), whose off-screen Chromium
drew only what an agent's `show_visual` asked for, to a second surface: a
` ```mermaid ` fence in an answer or in the reader. Herdr users ask for
diagrams (a terminal can't draw them), and agents already write Mermaid.

## Decision

1. **The same engine, nothing new to ship.** A fence is drawn by the
   visual engine (`visual/diagram.rs`) from a page holding the vendored
   `mermaid.js` (12.2.0, MIT, `crates/ferrite-pane/assets/mermaid/` with its
   license), inlined: the page fetches nothing and stays as sealed as an
   agent's (ADR 0013 §5). `securityLevel: strict`, no click handlers.
2. **A picture, not a live page.** The page draws once, its frame is cropped
   to what the diagram drew and kept as an image; the page closes. Nothing
   in a diagram is worth Chromium's memory while it is read, and an image
   scrolls, clips and caches like any other.
3. **Lazy, one at a time, and cached by content.** A fence asks for its
   diagram the first time its code is painted on screen; until it lands
   (and whenever it can't: no engine in this build, Settings' visuals
   `off`, a fence mermaid refuses, a fence still streaming) the fence shows
   as code. Diagrams are drawn one page at a time, in the order asked for,
   so a document of many fences never has many pages (each parsing the
   whole bundle) open at once, nor pages starving each other into the
   timeout. They wait apart from agents' screenshots (ADR 0013), which an
   agent is waiting on. Images are kept by a hash of the fence, the reading
   size and the display scale, and the least recently shown go past 64 MB,
   except any shown since the last diagram landed: that one may be on
   screen, and letting it go would draw it again and let another go, a
   loop that never ends. The budget is soft: what's on screen is kept even
   past it.
4. **Ferrite's look.** Mermaid's `base` theme is fed Ferrite's `--ferrite-*`
   values (`tokens.rs`): nodes on the raised band with the strong edge,
   lines and labels in the reading inks, Geist Mono at the reading size,
   no drop shadows (the face is loaded before mermaid measures its labels).
   The page is laid out at the fence's measure and drawn
   on the fence's rule; a narrower column scales it down, never up.
5. **One seam in the Markdown view.** `TextView::code_block_view` (a
   vendored gpui-base patch, `vendor/README.md`) hands the app each fence
   with its native element to draw as is or replace. The fence's text stays
   in the document, so selecting and copying an answer still gives its
   Mermaid source.

## Consequences

- A build with the `cef` feature (the released app) grows by
  `mermaid.min.js` (~5.5 MB); a build without it never draws a page and
  doesn't carry the bundle.
- The page carries the whole bundle, so each diagram's first draw parses
  it (a few hundred ms in the renderer, off the UI thread); a cached
  diagram costs nothing.
- The page tells the host it refused a fence by laying itself out two
  pixels tall (the engine seam carries only frames and heights). The empty
  page, measured before mermaid has drawn, reports one (the engine never
  reports less), and is waited on as still drawing. The page is drawn by
  the agent screenshot's settling loop (`renderer::settle`), told to wait
  past that placeholder height.
- Only a refusal (or no engine in this build) keeps a fence as code for the
  process's life. A draw that fails for now (Chromium slow to start, a page
  that drew nothing within 10 s) is asked for again the next time the fence
  is painted, up to three times in all; then it stays code too.
- A diagram taller than the engine's page cap (`MAX_HEIGHT` in
  `web/cef.rs`, 8000 logical px) is cut off at the cap.
- A fence in a quote or a list item is drawn like any other.
- A drawn diagram has no fence actions (Copy, the language tag), and the
  image can't be selected on its own: its source comes along in a selection
  that spans the diagram (from the text before it to the text after). A
  reading-size change draws it again.

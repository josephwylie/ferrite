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
   `mermaid.js` (12.2.0, MIT, `crates/ferrite/assets/mermaid/` with its
   license), inlined: the page fetches nothing and stays as sealed as an
   agent's (ADR 0013 §5). `securityLevel: strict`, no click handlers.
2. **A picture, not a live page.** The page draws once, its frame is cropped
   to what the diagram drew and kept as an image; the page closes. Nothing
   in a diagram is worth Chromium's memory while it is read, and an image
   scrolls, clips and caches like any other.
3. **Lazy and cached by content.** A fence asks for its diagram the first
   time its code is painted on screen; until it lands (and whenever it
   can't: no engine in this build, Settings' visuals `off`, a fence mermaid
   refuses, a fence still streaming) the fence shows as code. Images are
   kept by a hash of the fence, the reading size and the display scale, and
   the least recently drawn go past 64 MB.
4. **Ferrite's look.** Mermaid's `base` theme is fed Ferrite's `--ferrite-*`
   values (`tokens.rs`): nodes on the raised band with the strong edge,
   lines and labels in the reading inks, Geist Mono at the reading size,
   no drop shadows. The page is laid out at the fence's measure and drawn
   on the fence's rule; a narrower column scales it down, never up.
5. **One seam in the Markdown view.** `TextView::code_block_view` (a
   vendored gpui-base patch, `vendor/README.md`) hands the app each fence
   with its native element to draw as is or replace. The fence's text stays
   in the document, so selecting and copying an answer still gives its
   Mermaid source.

## Consequences

- The binary grows by `mermaid.min.js` (~5.5 MB), in every build; only a
  build with the `cef` feature draws with it.
- The page carries the whole bundle, so each diagram's first draw parses
  it (a few hundred ms in the renderer, off the UI thread); a cached
  diagram costs nothing.
- The page tells the host it refused a fence by laying itself out one pixel
  tall (the engine seam carries only frames and heights). A fence that
  never settles is given up on after 10 s and stays code.
- A drawn diagram has no fence actions (Copy, the language tag); its source
  is copied by selecting it. A reading-size change draws it again.

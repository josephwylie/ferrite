# Dependency patches

## GPUI Base

`gpui-base/` is the crates.io `gpui-base` **0.6.0** source from
[longbridge/gpui-kit](https://github.com/longbridge/gpui-kit/tree/94a313a72a2513aee2780240cd322d552b2395f0/crates/base),
upstream commit `94a313a72a2513aee2780240cd322d552b2395f0`.
Its Apache-2.0 license and upstream README are retained in that directory.

Rendering patches:

- `Inline::text_line_bounds` returns immediately when its
text layout is entirely outside the content mask. This avoids per-character
hitbox calculations for clipped transcript paragraphs during streaming.
The original function already clipped every returned hitbox to that mask;
selection calculation and copying of offscreen text remain unchanged.

- `Inline` and `InlineFlow` drop a highlight that overlaps its predecessor, runs past the text
  or splits a character, instead of handing `StyledText::with_runs` runs that
  abort the app. A retained view re-parses off the main thread, so for a frame
  its text can trail highlights computed for the next revision.

- `TextView::link_renderer` lets Ferrite replace local file links with native
  inline attachment cards. The callback leaves ordinary links alone and never
  rewrites Markdown. InlineFlow measures/wraps each card as one element, scopes
  its interaction identity, and maps fragment selections back to the original
  text run, including wrapped text. Remove this extension when upstream offers
  equivalent inline link rendering and selection support.

Markdown block spacing is centralized in `BlockNode::render_block`. The
configured paragraph gap applies to headings, paragraphs, code, quotes, lists,
tables, rules and custom blocks, including nested siblings and virtualized
documents. Loose list items share that gap; tight items and tight nested
continuations do not add a gap. The parser carries Markdown's list-spread flag
to its items. Ordered-list start values survive parsing, marker rendering and
source reconstruction instead of restarting at one. Final visible children have no trailing padding, and reference
definitions introduce no spacing. Code line spacing and
table cell padding remain independent. The Markdown parser also preserves hard
line breaks instead of dropping them. Ferrite's native geometry tests in
`rich.rs` cover block pairs, nesting, zero-gap overrides and hard breaks.

Quotes use a restrained 1px rail and italic text. Table header refinements reach
individual cells so header centering can override a data column's alignment.
Headings are bold at every level (upstream's H1 italic and underline were
removed). Ferrite opts into the existing adaptive horizontal-scroll table
renderer for narrow overflow; the track carries a debug selector for native
geometry acceptance.

Fenced code actions (language label, `Copy`, html `Preview`) render as a header
row above the code, in flow, rather than absolutely positioned over its first
line. Each list measures one marker column (the widest prefix, at least the
font size; a task checkbox is 1.375rem), so continuations and nested lists
indent by that width and a new digit (9 → 10) never moves an item's body.

`InlineFlow` (paragraphs holding a custom link element) captures the inherited
text style and rem size in `request_layout` and measures under them, with the
line height taken from that style. Upstream read `window.text_style()` inside
the measured-layout closure, which Taffy runs outside the element's style
stack, so such paragraphs were shaped at the root style while selection
hit-tested at the real one. Remove when upstream captures the style itself.

Style knobs on `TextViewStyle`, all additive, every default reproducing the
look above (they join `PartialEq`, so a change re-keys selection layout):

- `with_heading(level, StyleRefinement)`: refined after the default size and
  weight (weight, ink, line height).
- `with_heading_spacing(above, below)`: space above a heading that follows a
  visible sibling (`NodeRenderOptions::after_sibling`, set by the document,
  root and blockquote loops; a first block gets none), and optionally the space
  below it in place of the paragraph gap.
- `with_strong(HighlightStyle)`: `**strong**` runs (default: bold weight).
- `with_link_underline(Option<Hsla>)`: the underline's ink (default: the link's).
- `with_inline_code_font(Option<SharedString>)` and
  `with_inline_code_wash(Option<InlineCodeWash>)`: inline code shaped in its own
  family, on a rounded ground painted per wrapped line under the glyphs (a
  glyph at a soft wrap is placed at the start of the next line, where it is
  drawn, not at the end of the earlier one). The
  code highlight then carries a zero `fade_out` marker (no pixel effect; it
  survives highlight merges) that `Inline` and `InlineFlow` run builders use to
  find code runs; with a wash the square highlight background is dropped.
- `DefaultToastMotion`: a global motion for toast stacks whose builder never
  called `.motion(..)`. The kit's notification list builds its stack without
  one; Ferrite sets a collapsed stack that shows only its front toast.
- `with_blockquote(StyleRefinement)`, `with_rule(StyleRefinement)`: refined after
  the defaults (a quote's `not_italic()` wins over the default italic).
- `with_list_markers(bullet, ordinal)`: marker refinements; their text
  refinement also measures the shared marker column.
- `with_link_underline_on_hover(bool)`: a link's underline is painted only
  while the pointer is on that link (`Inline::paint_underlines`); `<u>`
  underlines are unaffected. The inline keeps the hovered link's ordinal in its
  state and notifies only when the pointer moves onto another link or off one
  (leaving the text included), where upstream notified on every glyph crossed.
- `with_numeric_columns_right(bool)`: a table column left at the default
  alignment whose non-empty body cells all read as numbers (`41`, `-3.5`,
  `12%`, `1,204`, `3.2k`, `$12`) is drawn right-aligned
  (`Table::display_align`); the source and a copy keep the column's own
  alignment.
- `with_table_fit(Option<Pixels>)`: a table sized to its content, as CSS
  lays out `width: auto` with `border-collapse` — each column exactly its
  widest cell plus the pad a side (the first column's left pad dropped), no
  column floor, no stretch to the frame, cells on one line, and every row's
  bottom rule including the last's (`TextNode::render_fit_table`). `None`
  keeps upstream's frame-filling, scrollable layout.
- `with_image(StyleRefinement)`: a block Markdown image is drawn in a frame
  refined by this style (its width, border, ground), the picture filling the
  frame's width at its own proportions. The default draws no frame, exactly as
  upstream does. An image alone on its line inside a paragraph of text (a
  caption over a picture: a line break or the paragraph's edge on each side)
  is split out of the `InlineFlow` and framed the same way; an image with text
  beside it on its line (a badge) stays text-sized in the flow.

Ferrite's `rich.rs` `vendor_knob_tests` pin these through the real renderer
(this crate is not a workspace member, so its own tests do not run there).
Remove each knob when upstream offers an equivalent.

Native selection registration updates one participant at a time, then sweeps
and publishes once after the frame. `TextSelectionDocument` separates retained
logical text membership from viewport geometry; its owner wrapper replays the
visible registrations when a cached view reuses its scene. Viewport unmounting
preserves selection, while logical eviction, source replacement, scope changes
and owner unmounting clear it. Only active endpoints pin native text states
beyond the host cache. Logical inline/UTF-8 positions preserve partial ranges
through reflow; paint and copy share their projection. Copy callbacks supply
never-mounted intermediate text. See
[ADR 0006](../docs/adr/0006-retained-transcript-rendering.md).

Cargo applies this through the root `[patch.crates-io]`. Remove the patch when
an upstream release includes equivalent behavior. Registry cache markers and
the dependency's own lockfile are omitted. Source fixtures and the small test
and benchmark targets named by its unchanged manifest are retained.

`InlineFlow` advances each fragment of a line (a text run, an inline element)
by its width rounded up to the layout unit, 1/64px, as a browser lays out an
inline box, so the fragment after a link card starts where the browser's
would (`layout_unit`).

## GPUI

`gpui-pre/` is the crates.io `gpui-pre` **0.3.3** source (Zed's GPUI,
snapshot of zed@`5b055fa789a8b8d38ac951a6e0cde272f66b4495`); `gpui-pre-macos/`
is the crates.io `gpui-pre-macos` **0.3.3** source, its macOS platform. Their
Apache-2.0 licenses and upstream READMEs are retained. The manifests are
unchanged except that `gpui-pre`'s example targets are dropped with the
`examples/` tree (4.9MB of fixtures); registry cache markers and lockfiles are
omitted.

Ferrite renders to the approved prototype's pixels, which a browser lays out
and paints. The patches make gpui place and round what it paints the way a
browser does, so the same CSS geometry lands on the same device pixels:

- **Layout units** (`taffy.rs`, `elements/text.rs`). Authored lengths are held
  to 1/64 of a logical pixel (truncated, as a `LayoutUnit` holds a CSS length)
  instead of being rounded to the device pixel before layout, and measured
  sizes (a text run's width) are rounded up to the next 1/64 instead of the
  next whole pixel. A text measure wraps and truncates with a layout unit of
  slack, so a run laid out at the width it measured never wraps on float error.
- **Pixel snapping** (`taffy.rs`, `window.rs`). Snapping moved from the layout
  engine to `Window::layout_bounds`, on each node's absolute position after
  any element offset: a box's edges round to the whole *logical* pixel, half
  up (a browser's pixel-snapped rect); a run of text (`request_text_layout`,
  used by every text element) and any other measured leaf keeps its left
  edge's fraction and rounds only its top. Children are placed from their
  parents' unrounded origins. `Window::unsnapped_layout_bounds` exposes the
  unrounded box; a cached view, a `List` and `Anchored` lay out and place
  their separately laid-out children from it, and `Anchored` no longer rounds
  its offset.
- **Glyph quantization** (`window.rs`). A glyph's subpixel position rounds
  half up, as a browser's does (upstream rounds half toward zero).
- **Spans** (`text_system/line.rs`). A run of a line in a new style (ink,
  decorations, face) starts where the previous run's width, rounded up to the
  layout unit, ends — a browser lays each span out as its own inline box —
  and glyphs past a span's first lean a hair left, so a glyph exactly between
  two subpixel positions takes the lower, as a browser's summed advances do.
- **Transforms** (`window.rs`). `Window::with_transform_offset` moves what it
  lays out after its boxes are rounded, a box's edges then rounded again to
  the device pixel and a run of text moved exactly: a CSS `translate(..)`.
- **SVG supersampling** (`svg_renderer.rs`). `SMOOTH_SVG_SCALE_FACTOR` is 1:
  icons rasterize at their device size, as a browser rasterizes inline SVG,
  rather than at twice it and filtered down.
- **Synthetic bold** (`gpui-pre-macos`, `text_system.rs`). A face the
  platform falls back to for a run asking for semibold or more, when the face
  is lighter than that, is drawn stroked and filled (Skia's fake bold: the
  size times 1/24 at 9px easing to 1/32 at 36px), as a browser draws it.
- **Fallback by PostScript name** (`gpui-pre-macos`, `open_type.rs`). A font
  fallback written `postscript:<name>` names one face exactly (the braille
  spinner's `AppleBraille-Outline6Dot`, whose family's faces share one
  weight).
- **Blur material** (`gpui-pre-macos`, `window.rs`). A blurred window's
  `NSVisualEffectView` uses `HeaderView`, not upstream's `Selection`: macOS 27
  removed `Selection`'s backdrop blur (no `CABackdropLayer`, only a vibrancy
  tint), so the window showed the desktop unblurred. `HeaderView` keeps the
  gaussian blur on every supported macOS.
- **Hover listener's view** (`elements/div.rs`, `window.rs`). An `on_hover`
  listener runs with `Window::hover_listener_view` naming the view that
  painted it, so a hover blend can notify that view alone instead of
  `Window::refresh`, which re-renders every cached view in the window (every
  transcript) on each pointer crossing.
- **A cached view that behaves like its parent's redraw** (`view.rs`,
  `window.rs`, `app.rs`). `ViewElement::tracking_reads` (on a cached
  `AnyView`) also redraws the view when any entity it read while it last
  rendered was notified since (`Window::notified_entities`; the reads are
  recorded exactly, so an entity something earlier in the frame had read —
  the kit Root — still counts), and when it is redrawn the cached views
  inside it keep their caches instead of being redrawn with it. The
  Cockpit is cached this way under the loops overlay
  (`crates/ferrite/src/loops_overlay.rs`).
- **Deferred draws and opacity, for an overlay** (`window.rs`,
  `elements/deferred.rs`). `Window::drawing_deferred` says whether the
  element now laid out, prepainted or painted sits in a `deferred` draw
  (it paints after everything else); `Window::element_opacity` and
  `Window::with_element_opacity` are public, so an overlay redraws an
  element under the opacity it was painted with.
- **Debug bounds kept by a reused view** (`window.rs`, `elements/div.rs`,
  test support only). A cached view's reused paint range carries its
  `debug_selector` bounds into the next frame, as a fresh paint would.
- **Debug bounds on a window** (`window.rs`, test support only).
  `Window::debug_bounds` reads the last frame's `debug_selector` bounds,
  which upstream exposes only through `VisualTestContext`: the live
  reference capture drives a headless window that is no test context's and
  clicks what it finds there.

Cargo applies both through the root `[patch.crates-io]`. Remove a patch when
an upstream release offers equivalent behaviour.

## Taffy

`taffy/` is the crates.io `taffy` **0.13.0** source from
[DioxusLabs/taffy](https://github.com/DioxusLabs/taffy/tree/45a56299d366ddb383e593a1f0372158d00e8530),
upstream commit `45a56299d366ddb383e593a1f0372158d00e8530`.
The crate archive matches Cargo.lock SHA-256
`c034e05f6ee85a12daa63863c2245797715075c70649947aa0da54f3f2ab1d0f`.
Its MIT license is copied from that commit because the published archive
omits the license file. Its manifest, source, README, and declared examples
are retained; registry cache markers and its own lockfile are omitted.

One source change: flexbox's `resolved_minimum_main_size` uses
`unwrap_or_else` instead of `unwrap_or`. The fallback recursively measures
minimum content size, so eager evaluation did that work even when an
explicit minimum or scroll-container minimum already supplied the result.
Lazy evaluation preserves the chosen size and avoids the unused measurement.
It also avoids populating measurement-cache entries from that unused traversal;
required later measurements still run through Taffy's normal cache path.

The root `[patch.crates-io]` applies this patch. Remove it when an upstream
release makes the fallback lazy. Layout and application checks must verify
that skipping the unused measurement has no observable layout effect.

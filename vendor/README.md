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

- `TextView::link_renderer` lets Ferrite replace local file links with native
  inline attachment cards. The callback leaves ordinary links alone and never
  rewrites Markdown. InlineFlow measures/wraps each card as one element, scopes
  its interaction identity, and maps fragment selections back to the original
  text run, including wrapped text. Remove this extension when upstream offers
  equivalent inline link rendering and selection support.

Markdown block spacing is centralized in `BlockNode::render_block`. The
configured paragraph gap applies to headings, paragraphs, code, quotes, lists,
tables, rules and custom blocks, including nested siblings and virtualized
documents. List items share that gap; final visible children have no trailing
padding, and reference definitions introduce no spacing. Code line spacing and
table cell padding remain independent. The Markdown parser also preserves hard
line breaks instead of dropping them. Ferrite's native geometry tests in
`rich.rs` cover block pairs, nesting, zero-gap overrides and hard breaks.

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

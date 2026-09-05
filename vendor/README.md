# Dependency patches

## GPUI Base

`gpui-base/` is the crates.io `gpui-base` **0.6.0** source from
[longbridge/gpui-kit](https://github.com/longbridge/gpui-kit/tree/94a313a72a2513aee2780240cd322d552b2395f0/crates/base),
upstream commit `94a313a72a2513aee2780240cd322d552b2395f0`.
Its Apache-2.0 license and upstream README are retained in that directory.

One source change: `Inline::text_line_bounds` returns immediately when its
text layout is entirely outside the content mask. This avoids per-character
hitbox calculations for clipped transcript paragraphs during streaming.
The original function already clipped every returned hitbox to that mask;
selection calculation and copying of offscreen text remain unchanged.

Cargo applies this through the root `[patch.crates-io]`. Remove the patch when
an upstream release includes equivalent clipping. Registry cache markers and
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

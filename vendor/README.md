# GPUI Base patch

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

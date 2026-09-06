# Retained transcript architecture — validation

Implementation worktree: `.worktrees/ui-rendering-architecture`, branch
`refactor/ui-rendering-architecture`. Originally based on `bef4ccd`; the PR
was rebased onto `main` at `54d4861` and verified again on 2026-09-07.

## Implemented boundary

Each Pane Subject owns a retained transcript entity, variable-height native
list and native selection document. Composer edits and unrelated provider
streams reuse cached sibling transcript scenes. A central Cockpit observer
compares input keys before drawing, so child invalidation reaches that frame.

The native list mounts semantic rows in its viewport plus 200px overdraw.
Logical selection membership survives scrolling; viewport geometry is transient.
Selection registration does constant work per participant and publishes once
per frame, with immediate pointer updates. Copy reads retained logical fragments,
including rows never mounted by the list. Only active endpoints pin native text
states beyond the existing bounded cache.

Native endpoints track inline/UTF-8 positions instead of retaining screen
coordinates as the selection's meaning. This preserves the character range
through reflow or native-view recreation. Changing the source of an endpoint's
fragment invalidates that selection, including when it is offscreen. Changes
to other fragments leave it intact. Core Subject eviction releases hidden
transcript snapshots; ordinary switching preserves them and their scroll state.

The refactor removes the old whole-history scrolling flex body, duplicate
follow-tail state and prebuilt disclosure controls. It preserves native Markdown
styling, variable heading sizes, tool disclosure behavior and per-Subject state.
Live progress stays above the Composer; its animation does not invalidate the
transcript entity. For short transcripts this also keeps progress at that fixed
location instead of immediately after the last row.

## Validation record

All Cargo/test runs are serialized by the root agent. Workers make scoped code
changes and write assigned regression cases; they do not run tests or builds.
A dedicated Cargo target prevents interference with other running agents.

| Check | Result |
| --- | --- |
| Full application/core workspace suite | 932 passed, 19 existing ignored tests |
| Full native GPUI Base library suite | 779 passed, none ignored |
| Production `cargo check -p ferrite` | Passed; four existing app dead-code warnings |
| Native selection scaling checker | Passed; 4× participants cost 3.82× |

The workspace includes all 282 application tests. Existing ignored tests cover
authenticated live providers and explicit stress probes. The Windows-only
caption-button geometry fixture is now gated to Windows; macOS uses AppKit
caption buttons outside GPUI's geometry. Its production behavior is unchanged.
One Pane disclosure accessor now exists only in tests, matching its remaining
callers. The final production check includes that compile-only cleanup.

The new application regressions exercise real GPUI windows, input and native
text, checking:

- Steady Composer typing and one-Pane streaming render zero sibling native
  transcript wrappers after initial parsing/input modality settle.
- A fixed viewport mounts **6 native reasoning rows at both 30 and 200 retained
  rows**; the test first verifies a real, nonzero viewport.
- Selection copies across 106 logical rows, including a never-mounted middle
  row, and survives both endpoints scrolling out of view.
- A partial UTF-8 range survives measured native reflow. Replacing its offscreen
  source clears selection; replacing an unrelated source preserves it.
- Switching Subjects retains the old transcript entity; core eviction releases
  it and removes its subscription bookkeeping.
- Relative Markdown file links resolve against the Thread workspace. Exact
  partial selection across a native file card survives measured paragraph
  reflow after resizing from 1200px to 700px. Original parser-run byte offsets
  remain stable when custom links and wrapping split the visible fragments.

Commands (run sequentially with `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2` and
the task's isolated `CARGO_TARGET_DIR`):

```sh
cargo test --workspace --offline -- --nocapture --test-threads=1
cargo test --manifest-path /tmp/ferrite-ui-architecture-gpui-base/Cargo.toml \
  --offline --lib -- --nocapture --test-threads=1
cargo check -p ferrite --offline
```

Native dependency tests use a standalone copy of `vendor/gpui-base`, because it
is excluded from the application workspace. Only the copy's manifest declares
its own workspace and the local Taffy patch; the README test fixture resolves
to the same repository README. Production dependency manifests remain unchanged.

Full logs: [workspace](workspace-tests.log), [native library](native-tests.log),
[production compile](production-check.log), [selection probe](selection-after.log).

## Selection scaling

The same headless native registration probe was linked against the patched
application's GPUI test-support library. It uses persistent participants and
reports medians of nine frames after three warm-up frames.

| Participants | Before, ms | After, ms |
| ---: | ---: | ---: |
| 64 | 12.208 | 0.362 |
| 128 | 51.966 | 0.689 |
| 256 | 202.107 | 1.382 |
| 512 | 794.697 | 2.923 |

Growing from 64 to 256 participants now costs **3.82×**, compared with **16.56×**
in the original baseline. The diagnostic's 6× scaling budget passes. System load
and builds changed between measurements; absolute timings are debug-harness CPU
preparation costs, not release frame times or a claim about installed-app FPS.

```sh
rustc --edition=2021 --test docs/research/ui-lag-2026-09-06/selection_probe.rs \
  -o /tmp/ferrite-ui-architecture-selection-probe \
  --extern gpui=/Users/josephwylie/.cache/ferrite-ui-architecture-target/debug/deps/libgpui_kit-e693bc92c2a58c38.rlib \
  -L dependency=/Users/josephwylie/.cache/ferrite-ui-architecture-target/debug/deps \
  -C debuginfo=0
python3 docs/research/ui-lag-2026-09-06/check-selection.py \
  /tmp/ferrite-ui-architecture-selection-probe
```

## Limits

These are headless GPUI behavior/cost checks, not measured display FPS or
key-to-paint latency from the patched release app. The running installed app
and its provider sessions remain untouched. Group/fullscreen release profiling
is still needed after the operator switches to a build containing this change.

One contiguous Markdown answer remains one natural-height semantic row. An
exceptionally large individual answer can still cost substantial layout;
viewport row bounding applies across distinct answers/events, not inside one
native Markdown document. Lazy measurement also makes the scrollbar's total
pixel extent approximate until rows have been measured.

GPUI deliberately refreshes hover/focus-visible styling on the first transition
from mouse to keyboard. Isolation tests warm that input modality and the native
parser before measuring ordinary typing and streaming.

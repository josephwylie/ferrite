# inline-cef: a web page drawn inline in a GPUI list

A throwaway spike. It answers one question: can an interactive, agent-authored
HTML/CSS/JS page sit **inline** in Ferrite's scrolling transcript, with no
native child window or overlay? Five demo pages cover the kinds of visual
output an agent would show inline: a treemap, UI mockup variants, a stats
dashboard, a dependency diagram, and a CI timeline with expandable failures.

The approach:

- An engine renders the page off-screen.
- The host draws the latest frame as an ordinary GPUI image wherever layout
  puts it. Scrolling, clipping and popovers therefore need no coordination.
- The host forwards mouse input in view coordinates.

Background research: `/tmp/inline-web-views-in-gpui.md` (sections "image +
hit-testing" and "Smallest spike").

## What it shows

- A virtualized `list` of 400 transcript-style rows. Rows 100, 130, 160, 190
  and 220 are live web views (treemap, mockups, stats, diagram, timeline), each
  after a user prompt. A view's height is its page's content height (it grows
  when the page does), and its width follows the window.
- A GPUI popover drawn over the treemap (`p` toggles it). The list clips the
  views like any other row.
- `t` switches between dark and light. Every page is restyled live, using only
  `--ferrite-*` CSS variables.
- A header with live numbers for one view (the one the autopilot is on):
  - views opened so far
  - move→frame latency (p50/p95)
  - frames received
  - idle frames, meaning frames that arrive with no input for more than 1 s
  - this process's RSS

  The numbers are printed again on quit.

## Layout

| Path | What |
|---|---|
| `src/web/mod.rs` | The seam: `Engine`, `View`, `Page`, `Viewport`, `Input`, `Frame`, `Updates`. GPUI-free. |
| `src/web/fake.rs` | `FakeEngine`, a second real adapter that draws synthetic frames (no browser). |
| `src/web/cef.rs` | The Chromium adapter (`--features cef`). |
| `src/element.rs` | `WebView`, the GPUI side: open/resize, frames → `RenderImage`, input, visibility, metrics. Also `pump`. |
| `src/page.rs` | `Demo` (the five pages: `ALL`, `title`, `html`, `page`) and `theme_css(dark)`. |
| `src/pages/*.html` | The pages themselves, as an agent would author them. |
| `src/main.rs` | The spike app. |

## Running with the fake engine

The fake engine needs nothing installed. It draws a background, a few blocks,
and a white square under the pointer, so you can see that hover is being
forwarded.

```sh
cargo run                     # t theme · p popover · q quit; scroll with the wheel
cargo test                    # fake engine + element tests (gpui test-support)
cargo run -- --at 0           # start at the top of the list instead of near row 100
cargo run -- --demo stats     # start at a demo: treemap, mockups, stats, diagram, timeline
                              # (same as --at 99 / 129 / 159 / 189 / 219)
cargo run -- --autopilot --quit
    # drive the demo on screen: sweep a synthetic pointer over it, hover and
    # click what it's about, print first-frame/hover/latency numbers, quit
cargo run -- --tour --quit    # the same for every demo in turn, then memory: all visited,
                              # scrolled away past the release grace period, back to one
INLINE_CEF_ON_TOP=1 cargo run --release -- --scroll-bench --quit
    # fling up and down through all five from a fully released state: frame
    # times, main-thread stalls, pump time, height jumps (INLINE_CEF_FLING=<px/s>,
    # INLINE_CEF_BENCH_COLD=1 to start with nothing opened). Don't use the
    # `shots` build for timing, and keep the window uncovered (see NOTES-host.md)
cargo run --features shots -- --demo timeline --autopilot --quit --shots /tmp/inline-cef-shots/timeline
    # also save renderer frames (loaded/sweep/hover + per-demo extras); no screen-recording grant needed
INLINE_CEF_PREVIEW=/tmp/pv cargo test dump_previews -- --ignored
    # write each page as the engine loads it, to open in a desktop browser
```

The fake engine renders synchronously. Its latency is GPUI's own floor: one
notify, then the next frame.

## Running with Chromium

`--features cef` replaces the fake engine with Chromium (CEF 154 through
[tauri-apps/cef-rs](https://github.com/tauri-apps/cef-rs)), rendering
off-screen. CEF only runs from inside a macOS `.app` that ships its framework
and helper apps, so build and run it through the bundle script:

```sh
brew install cmake ninja                    # needed to build CEF's C++ wrapper
./bundle-macos.sh --release --run           # build, bundle, and run the spike
./bundle-macos.sh --release --open          # ...or launch it with `open`
./bundle-macos.sh --release --features shots --run -- --demo stats --autopilot --quit --shots /tmp/inline-cef-shots/stats
./bundle-macos.sh --release --example cef_smoke --run   # engine check, no GPUI: prints PASS
```

- The first build downloads CEF once: 128 MB, unpacked to about 320 MB in
  `~/.local/share/cef`. Set `CEF_PATH` to put it somewhere else.
- The `.app` is written to `<cargo target dir>/bundle/` and is about 325 MB.
- `cargo run --features cef` outside the bundle exits with an error saying so.
- Views off-screen for 2 s are released (their page is closed and reloaded
  when they come back near the screen); all views share one renderer
  process (`--process-per-site`). See NOTES-cef.md "Memory".
- Environment knobs:
  - `INLINE_CEF_NO_SANDBOX=1` turns off Chromium's sandbox.
  - `INLINE_CEF_PROCESS_PER_VIEW=1` gives each view its own renderer again.
  - `INLINE_CEF_TRACE=1` logs browser creates/releases and long pumps.
  - `INLINE_CEF_EXTRA_SWITCHES="disable-gpu disable-gpu-compositing"` passes
    extra Chromium switches.
- Measurements, design and gotchas: [NOTES-cef.md](NOTES-cef.md).

# CEF engine notes

The engine behind `--features cef`: `src/web/cef.rs` plus `src/web/cef/`
(`handlers.rs` CEF callbacks, `document.rs` page assembly and the height
protocol, `input.rs` input and cursor mapping, `nsapp.rs` the macOS `NSApp`
retrofit). The seam in `src/web/mod.rs` was implemented unchanged.

Measured on an Apple M4, macOS 27.0.1, 2026-10-08.

## Versions

- `cef` crate 154.5.0+154.0.34 (tauri-apps/cef-rs), pinned with `=154.5.0`.
- CEF 154.0.34+g14c5a08, Chromium 154.0.8037.98, macOS arm64 "minimal" build.
- Crate features: only `sandbox`. The defaults also pull in `build-util`
  (clap, cargo_metadata) for `bundle-cef-app`, which isn't used here.

## Results

The proof is `examples/cef_smoke.rs`, run with
`./bundle-macos.sh --release --example cef_smoke --run`. It runs without GPUI:
it boots CEF, opens a page with known colours at 400 px wide and scale 2, and
pumps. It checks pixels, content height, hover, cursor, a theme swap, a resize
and that an idle page sends no frames. Typical release output:

```
boot: 108.629416ms
first frame: 57.873625ms after open (176.645125ms since process start)
sized frame: 75.113459ms after open
pixels ok: red/blue/green at y=10/350/550; wakes so far: 3
hover → pixel: 12.905542ms
frames during 1s idle: 0
PASS in 7.376936916s
```

| | Debug | Release |
|---|---|---|
| `boot()` (warm; the first launch after a rebuild takes 0.5–3 s, likely the system scanning the new binary) | 160–530 ms | 110–190 ms |
| `open()` → first frame | 115–780 ms | 58–105 ms |
| `open()` → frame at the reported content height | 0.4–1.0 s | 75–150 ms |
| Hover: `Input::Move` → pixel changed in a taken frame | ~14 ms | 13–23 ms |
| Frames during 1 s idle | 0 | 0 (once 1, a trailing frame after the resize) |

The browser starts at a placeholder height of 150 px. The page then reports
its height, the view is resized, and the frame follows at full size. Since
the memory work (below), frames painted before that first height report are
dropped, so the host's first frame is always the laid-out page ("first
frame" and "sized frame" in `cef_smoke` now differ only by the resize).

### Size

- `du -sh` of the `.app`: **325 MB** release, 345 MB debug.
  - The framework is 323 MB.
  - The smoke binary is 3.2 MB in release and 23 MB in debug.
  - The five helper apps are hard links to the main binary, so they add nothing.
- Download: 128 MB (`..._macosarm64_minimal.tar.bz2`), fetched once into
  `~/.local/share/cef`.

### Memory with one page open (release, `SMOKE_HOLD`)

Two numbers per process: `footprint` (physical footprint) and `ps` RSS. RSS
counts the shared framework pages again in every process, so summing it
overstates the cost.

| Process | Footprint | RSS |
|---|---|---|
| browser (the app) | 45 MB | 150 MB |
| GPU helper | 55–97 MB | 107 MB |
| storage utility | 20 MB | 67 MB |
| renderer | 29 MB | 109 MB |
| **total** | **~150–190 MB** | ~430 MB |

The defaults were tuned from a first run that had 6 processes and a total
footprint of about 241 MB:

- `disable-features=SpareRendererForSitePerProcess` removes the pre-warmed
  second renderer (about 26 MB).
- `enable-features=NetworkServiceInProcess2` moves the network service into
  the browser process (about 25 MB). The only thing it ever serves is the page
  from memory.
- Optionally, `INLINE_CEF_EXTRA_SWITCHES="disable-gpu disable-gpu-compositing"`
  switches to software compositing. That brings the GPU process from 55–97 MB
  down to 32 MB, for a total of about 130 MB, at the cost of WebGL. Hover
  latency was unchanged.

### In the GPUI spike app

Run with `./bundle-macos.sh --release`, then
`.../inline-cef.app/Contents/MacOS/inline-cef --autopilot --quit`. The page is
the treemap; the pump is `element::pump`, as is.

```
autopilot [chromium (cef)] move->frame p50 26.9ms p95 39.6ms max 500.9ms (n=101)  frames 109  idle frames 2  rss 196MB
autopilot [chromium (cef)] move->frame p50 27.3ms p95 34.9ms max 43.6ms (n=144)  frames 148  idle frames 0  rss 198MB
```

| Process, treemap page open | Footprint | RSS |
|---|---|---|
| browser (GPUI app) | 132 MB | 199 MB |
| GPU helper | 180 MB | 123 MB |
| storage utility | 20 MB | 67 MB |
| renderer | 50 MB | 137 MB |

The `.app` is 334 MB in release.

- CEF initialised before GPUI and GPUI then ran normally, so the `NSApp`
  retrofit works with the real `GPUIApplication`.
- The background-executor timer pump kept up during the autopilot.
- `--shots` printed "render_to_image not implemented for this platform", and
  `screencapture` has no Screen Recording permission here. So there is no
  visual capture: the pixel-level proof is `cef_smoke`, and the app run proves
  that frames, hover and the lifecycle work inside GPUI.

## Memory: only views near the screen stay live

Measured 2026-10-08, release, Apple M4, two 1920×1080 displays at 120 Hz
(scale 1, so a 1042×650 view is a 2.7 MB frame). Footprint is `footprint(1)`
physical footprint summed over the app and every helper. Numbers come from
`--tour` (memory checkpoints at the end) and `--scroll-bench` (see
NOTES-host.md), with the old and new adapter built from the same harness.

| | before | after |
|---|---|---|
| five views opened, all live (bench) | **550 MB**, 8 processes (5 renderers, 32–36 MB each) | **438–447 MB**, 4 processes (1 renderer, 48–50 MB) |
| same, after build with `INLINE_CEF_PROCESS_PER_VIEW=1` | | 561 MB, 8 processes |
| scrolled away from all five, 4 s (bench, bottom of the list) | **464–470 MB**, 8 processes | **196–220 MB**, 3 processes (no renderer at all) |
| end of `--tour` (a): five visited, two still near | 555 MB, 8 processes | 416–422 MB, 4 processes |
| `--tour` (b): scrolled away 4 s | 534 MB | **247–255 MB**, 3 processes |
| `--tour` (c): back to the treemap, 2 s | 532 MB | **307–309 MB**, 4 processes (renderer 39–40 MB) |
| scroll back → treemap's new frame | 12 ms (repaint; it was never released) | 141–148 ms (reload) |

With nothing live, what's left is the app (~150 MB, mostly GPUI and the
host's frame images), the GPU helper (~42 MB) and the storage utility
(20 MB). Each live view costs roughly 10–15 MB of shared renderer plus its
share of the GPU process, instead of a ~35 MB renderer of its own.

### Release after a grace period (behind `View::set_visible`)

- `set_visible(false)` hides the browser at once (`was_hidden`) and stamps
  the time. The pump closes the browser of any view hidden for
  `RELEASE_AFTER` (2 s). The `ViewState` (geometry including the last
  content height, cursor) and the `PageDoc` (page + latest theme) stay,
  so the host's `View` is untouched.
- `set_visible(true)` on a released view: the next pump creates a fresh
  browser at the stored geometry, so it starts at the height the host is
  already showing. Frames are dropped until the reloaded page reports its
  height (then an `invalidate` makes sure a laid-out frame follows), so the
  host keeps its frozen frame until the replacement is the real page: no
  blank frame, no placeholder, no height change unless the page's height
  really differs.
- Each browser's handlers carry a **generation**; the view's current
  generation moves on when a browser is released or replaced, and stale
  callbacks from a closing browser (paint, height, cursor) are ignored.
- A hidden browser's paints are dropped (the seam says a hidden view produces
  nothing). **Gotcha found from the screenshots:** a restyle sent just before
  a hide was never painted — a hidden OSR browser doesn't run the frame — so
  the frozen frame kept the old theme, and the next reload swapped it. Fix:
  a hide that arrives while a `set_theme` is still unpainted is deferred
  until that paint lands (at most 250 ms). `/tmp/inline-cef-shots/release/back-0ms.png`
  (frozen, right after scrolling back) and `back-new-frame.png` (reloaded)
  now match.
- **What is lost on reload:** everything the page's script built up after
  load: the treemap's zoom and Size/Churn tab, the stats sort order and
  range, the timeline's expanded failure (and with it the 940 px height,
  back to 508), the diagram's pinned node, the mockups' dismissed notices,
  hover, focus and any typed text. The theme is not lost (it is in the
  document). Seen in the tour: the treemap had grown 4 px for its hover
  tooltip while hidden (a hidden page doesn't re-measure), and its reload
  reported the real 376 px, a 4 px correction when the new frame landed.
  Ferrite would want pages to persist their own state (e.g. in the
  document's URL fragment or a host-provided store) if that matters; the
  2 s grace and the near margin keep it from happening while the user is
  working with a view.

### Sharing one renderer: `--process-per-site`

Every page is served from `ferrite-view://page/<n>`, i.e. one site
(`ferrite-view://page`). Chromium's default gives each browser (each
BrowsingInstance) its own renderer process even for the same site. From
the Chromium source (`content/public/common/content_switches.cc`,
`content/browser/site_info.cc`, `render_process_host_impl.cc`, main as of
2026-10):

- `--process-per-site`: "consolidates same-site pages so that they share a
  single process". `SiteInfo::ShouldUseProcessPerSite` returns true when
  the switch is present; `SiteInstanceImpl` then uses
  `ProcessReusePolicy::kProcessPerSite`, and
  `GetProcessHostForSiteInstance` reuses `GetSoleProcessHostForSite`.
  Site isolation is unaffected: the process is still locked to that one
  site, and `MayReuseAndIsSuitable` still refuses to put another site in
  it.
- `--renderer-process-limit=N`: only consulted after reuse has failed
  (`IsProcessLimitReached` → `GetExistingProcessHost`), and still subject
  to the same suitability check. It's a global cap that would also apply to
  any other site, so it is the blunter tool. Not used.
- A shared `CefRequestContext` doesn't change process allocation (it is
  about storage partitions); all views already share the global context.

Chosen: `process-per-site`, appended in
`on_before_command_line_processing`. The sandbox is untouched (renderers
still run under the macOS seatbelt sandbox; nothing passes `--no-sandbox`).
`INLINE_CEF_PROCESS_PER_VIEW=1` turns it off for comparison.

Trade-offs:

- **Blast radius.** A renderer crash (or an OOM kill) now takes down every
  live view at once, not one. The adapter doesn't handle
  `OnRenderProcessTerminated` yet, so they'd stay frozen on their last
  frame until dropped. Recommended follow-up: on termination, treat each
  affected view as released, so it reloads when next shown (the release
  path already does the rest).
- **Runaway pages.** Views share one renderer main thread. A page in a busy
  loop stalls the others' rendering and input (their frames freeze), where
  before it only stalled itself. Chromium's hang monitor doesn't help
  off-screen. The pages are agent-authored; a watchdog (no frame or height
  report for N s while visible → reload or kill) would be the fix.
- **Isolation between pages.** Same-origin pages could already reach shared
  storage (`localStorage`, IndexedDB on `ferrite-view://page`), so sharing
  a process adds little. Pages don't get references to each other
  (no opener, no popups, separate browsers). If Ferrite wants pages
  isolated from each other, give each its own origin (e.g.
  `ferrite-view://<id>`), which also brings back one process per view.
- **Memory doesn't drop per page right away.** Closing one browser leaves
  its share of the shared renderer's heap to Chromium's GC and allocator.
  The renderer exits only when its last page goes (the 196–220 MB rows).

### Main-thread cost (scrolling must not hitch)

Everything CEF does on the browser side runs on the main thread inside
`pump()`, so all of it is in the frame budget. Measured with
`INLINE_CEF_TRACE=1` (logs creates, releases and pumps over 4 ms):

- `browser_host_create_browser_sync`: **3.5–7 ms** per browser (10–16 ms
  when the app is in the background and throttled). The async variant
  would only move that work into the next `do_message_loop_work` on the
  same thread, so it isn't used. Instead `pump()` creates **at most one
  browser per call and does nothing else in that call**; CEF's own work
  runs 1 ms later in the next pump. So a scroll that brings three views
  back costs three short slices, not one long one.
- `close_browser` on release: 1–4 ms.
- `do_message_loop_work` after a (re)load: 4–10 ms at worst (navigation
  commit and the first paints; the BGRA copy itself is ~0.3 ms for 2.7 MB).
- Max pump during the fling benchmark: **8.2–10.1 ms** (before: 5–9.7 ms;
  before never reloaded anything while scrolling). One earlier, throttled
  run showed a single 130 ms pump that didn't reproduce in ten later runs.

The benchmark results (frame times, stalls, height jumps) are in
NOTES-host.md.

## What `main` must do

Both of these are already in place in the marked block in `src/main.rs`.

1. **Call `web::cef::boot()` first in `main`, before `gpui::application()`**
   and before anything touches `NSApp`.
   - `Boot::Subprocess(code)` means this process is a CEF helper. Exit with
     `code` immediately and never start GPUI.
   - `Boot::Engine(engine)` gives the engine. Wrap it as `Rc<dyn web::Engine>`.
   - `Err` means the app isn't running inside its bundle, or CEF failed to
     start.
2. **Call `Engine::pump()` repeatedly on the main thread, outside any GPUI
   `update`**, sleeping for the `Duration` it returns.
   - The delay is 1–33 ms while pages are live, and 250 ms when nothing is
     open.
   - `element::pump` (a foreground task that awaits a background-executor
     timer) satisfies this.
   - Do not call `pump()` from inside `cx.update(...)`, a render or an event
     handler. `cef_do_message_loop_work` can spin the run loop, and GPUI
     callbacks could then re-enter a borrowed `App`; bokuweb/gpui-cef hit this
     as "RefCell already borrowed".
   - If the pump stalls when the app is idle (gpui-cef measured GPUI's
     foreground executor ticking every 1–5 s in an idle app), drive it from a
     `CFRunLoopTimer` in `kCFRunLoopCommonModes` instead. That also keeps it
     running during live window resizes.

What the element can rely on:

- `wake` is only ever called from inside `pump()`, after
  `do_message_loop_work` has returned, and never from inside a CEF callback.
  Calling `view.take()`, `open()` or dropping views from within `wake` is safe.
- A frame's size is in device pixels and can differ from the view's current
  layout size for one frame after a resize or a height change. Draw it at the
  frame's own size divided by the scale, or stretch it.
- Frames are **premultiplied** BGRA. The browser background is transparent
  (`background_color: 0`), so pixels the page doesn't paint have alpha 0.
- `content_height` is clamped to 1..=8000 logical px.
- `Input::Wheel` reaches the page only if the host forwards it. `dx`/`dy` use
  GPUI's sign convention, in logical pixels. The injected
  `html,body{overflow:hidden}` means the page never scrolls itself.
- Focus: the first `Input::Down` focuses the browser so that `Input::Key`
  works. The seam has no blur, so focus stays once given; a caret in a text
  field keeps blinking, and the page keeps producing frames.
- The first frame after an open or a reload is the laid-out page (frames
  before the page's first height report are dropped). A hidden view sends no
  frames. A released view sends nothing until its reload is laid out.
- Input sent to a released view is dropped; resize and theme are kept and
  applied by the reload.

## Design decisions

- **The binary is its own helper.** On macOS CEF launches its child processes
  from `Contents/Frameworks/<App> Helper{, (GPU), (Renderer), (Plugin), (Alerts)}.app`.
  - `boot()` sees `--type=` and runs `cef_execute_process`.
  - In a helper, `boot()` first enters the sandbox: cef-dll-sys is built with
    `USE_SANDBOX`, and gpui-cef found that skipping this kills the GPU and
    network children with exit code 5. It then loads the framework from
    `../../..`.
  - The bundle uses hard links, so this costs no disk. GPUI's `#[ctor]` class
    registration runs in helpers too, which is harmless.
  - A separate tiny helper binary would need a `[[bin]]` and a second build,
    and wouldn't save meaningful memory: binary pages are shared and mostly
    never touched.
- **`NSApp`.** `cef_initialize` CHECKs that `NSApp` conforms to
  `CefAppProtocol`.
  - GPUI registers `GPUIApplication` from a `#[ctor]`, before `main`. So
    `boot()` adds `isHandlingSendEvent` and `setHandlingSendEvent:` to that
    class, wraps `-sendEvent:` on that class only (not on `NSApplication`),
    and registers the protocols.
  - It then calls `[GPUIApplication sharedApplication]` itself, so `NSApp` is
    GPUI's class when CEF initialises. GPUI's later `sharedApplication` gets
    the same instance.
  - gpui-cef does the same retrofit but defers `cef_initialize` into GPUI's
    run closure. Doing it at boot means `boot()` returns a fully started
    engine.
  - Without GPUI (the smoke example), `boot()` registers a bare
    `NSApplication` subclass instead.
- **The message loop** is `external_message_pump`.
  - `OnScheduleMessagePumpWork` can arrive on any thread; it records the
    earliest due time atomically.
  - `pump()` finishes deferred hides and releases views hidden past the
    grace period. If a visible view is waiting for a browser it creates one
    and returns 1 ms. Otherwise it resets the due time, runs
    `do_message_loop_work`, wakes dirty views, and returns the time until
    the due time, clamped to 1–33 ms. The 33 ms cap is cefclient's own.
  - `pump()` is guarded against re-entry.
- **Browser creation is deferred** to `pump()`, always: `cef_initialize`
  returns before `OnContextInitialized`, and creates are staggered one per
  pump (see "Main-thread cost"). A hidden view that never had a browser
  gets none until it is shown. In practice `open()` → create is one pump.
- **Pages** load from `ferrite-view://page/<n>`.
  - The scheme is registered `STANDARD | SECURE` in every process via
    `OnRegisterCustomSchemes`; that is why the helpers get the `App` too.
  - The page is served by `ResourceRequestHandler::GetResourceHandler`, with
    no global scheme handler factory and no `file://`.
  - The served document is the theme `<style>`, then the host script, then the
    agent's HTML. The HTML parser merges a full document's
    `<html>`/`<head>` into that prefix, so full documents and fragments both
    work.
- **Height** is reported with
  `console.debug("\u0001ferrite:height:N")`, which the browser process
  receives in `OnConsoleMessage` and swallows.
  - This avoids a render-process handler and V8 bindings in the sandboxed
    helper, which a process message or the message router would need. A page
    can spoof the message, but only for its own height.
  - Plain `scrollHeight` can only grow, because the view's height is set from
    it and `scrollHeight` is never less than the viewport. The script
    therefore reports the `<html>` box height, and `scrollHeight` only when
    content really overflows the view.
- **Safety**:
  - `on_before_browse` cancels any navigation that isn't the page URL
    (fragments ignored), in any frame.
  - `on_before_resource_load` cancels every request except the page,
    `data:` and `blob:`.
  - The page is served with a strict CSP: inline script and style only,
    `connect-src`/`frame-src` `'none'`.
  - Popups, `OnOpenURLFromTab`, the context menu (its model is cleared) and
    JS dialogs are all suppressed.
  - No `remote_debugging_port` is set, and nothing calls `ShowDevTools`.
    Windowless browsers are Alloy style, which has no DevTools accelerator.
  - Chromium's own background traffic is switched off with
    `disable-background-networking`, `disable-component-update`,
    `disable-sync` and `disable-extensions`.
  - The profile is a throwaway per-process directory,
    `$TMPDIR/inline-cef-profile-<pid>`. Directories left by dead processes are
    removed at boot.
  - The sandbox is on. `INLINE_CEF_NO_SANDBOX=1` turns it off as an escape
    hatch.
- **Threads.** The render, display, life-span and `on_before_browse` handlers
  run on the main (UI) thread. Resource handlers run on CEF's IO thread.
  Everything the handlers share goes through `Arc` and `Mutex`/atomics. The
  host's `Wake` (`!Send`) stays on the main thread in the engine.

## Gotchas hit

- **The build needs `ninja`.** cef-dll-sys builds `libcef_dll_wrapper` with
  CMake's Ninja generator. ninja wasn't installed, so for verification the
  release binary was downloaded to `/tmp/cef-tools`. Use
  `brew install ninja` for real; the bundle script checks for it.
- **CEF downloads into `OUT_DIR` unless `CEF_PATH` is set.** The script
  defaults `CEF_PATH=~/.local/share/cef`, so the 128 MB archive is fetched
  once; the crate creates `<CEF_PATH>/154.0.34/cef_macos_aarch64`.
- **cef-rs's `bundle-cef-app` doesn't fit here.** It runs
  `cargo build --bin <name>` with no feature flags and expects a separate
  helper bin. `bundle-macos.sh` builds the same layout itself.
- **`LibraryLoader::new` and `Sandbox::new` `unwrap()` a `canonicalize()`.**
  Outside the bundle they would panic, so `boot()` checks for the framework
  first and returns `Err`; `cargo run --features cef` prints a hint instead of
  crashing.
- **`command_line_create` is itself a CEF function.** The process type
  therefore has to come from `std::env::args()` before the framework is
  loaded, because the helper and the main app load it from different relative
  paths.
- **Two renderer processes appeared at first.** The second was Chromium's
  spare renderer; it is disabled now.
- **Not handled yet:**
  - `<select>` dropdowns paint to `PET_POPUP` and are ignored, so they don't
    appear. Fix: composite the popup layer into the frame.
  - Native `title` tooltips are swallowed (`on_tooltip`).
  - IME composition (`ImeSetComposition`).
  - Copy and select-all shortcuts, which on macOS OSR need routing to
    `CefFrame::Copy()`.
  - Drag and drop out of the page.
- **Not attempted:**
  - Windows: `boot()` returns `Err`.
  - Shared-texture (`OnAcceleratedPaint`) frames.
  - `cef::shutdown()` in a GPUI app. GPUI quits through `-terminate:` →
    `exit()`, so `EngineInner::drop` (which closes browsers, pumps until they
    are gone, then calls `cef_shutdown`) only runs when the engine is actually
    dropped, as it is in the smoke example. Helpers exit on their own when the
    browser process dies.

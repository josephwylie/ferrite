# Host notes: the GPUI side of the spike

This covers the element, the fake engine, the demo page and the app. The seam
in `src/web/mod.rs` has no new methods; only the `set_visible` contract was
made precise (release after a grace period, reload from scratch; see "Only
views near the screen stay live" below). For the Chromium adapter, see
[NOTES-cef.md](NOTES-cef.md).

## What was built

- **`src/web/fake.rs`, `FakeEngine`.** A second real adapter at the `Engine`
  seam. It has no dependencies and draws deterministic frames:
  - a background from the theme's `--ferrite-bg`, or a fixed grey if that
    doesn't parse;
  - five coloured blocks;
  - a 24 px white square under the last `Move`, which `Leave` clears.

  Content height is always 360. The cursor is `Pointer` over a block and
  `Arrow` elsewhere.

  **It renders synchronously:** `open`, `resize`, `input`, `set_theme` and
  `set_visible(true)` draw the frame and call `wake` before they return.
  `pump` does nothing and returns 250 ms. A hidden view produces nothing, and
  showing it again repaints.

  It also keeps a `Call` journal (open, resize, input, theme, visible, close).
  The element tests assert on what the host forwarded through that journal,
  without looking inside the host.
- **`src/element.rs`, `WebView`.** The deep module. Its interface is
  `WebView::new(engine, page, cx) -> Entity<WebView>`, `set_theme`, `stats`,
  plus the free function `pump(engine, cx)`. Internally it has a custom
  `Element`:
  - **Layout.** Width is 100 % of the parent. Height is the page's
    `content_height`, or 240 px until the page reports one.
  - **Open and resize.** These happen in `prepaint`, once the bounds are known:
    `Viewport { width: round(bounds.width), scale: window.scale_factor() }`.
    The view is opened on the first layout with a width of at least 1. After
    that, `resize` is called only when the viewport changes.
  - **Wake bridge.** `wake` sets a pending flag and sends `()` on an unbounded
    channel. A foreground task owned by the entity drains the channel and calls
    `cx.notify()`. This makes `wake` safe to call at any time, including inside
    our own `open`, `input` or `set_visible` calls (the fake does exactly that).
    The flag coalesces wake storms into a single notify.
  - **Frames.** `render` calls `take()`. A new `Frame` becomes
    `Arc<RenderImage>` with no conversion: `RenderImage` stores BGRA in an
    `image::RgbaImage` buffer, so the bytes are moved in. `paint` draws it with
    `window.paint_image`:
    - It is drawn at the frame's own logical size (device px / scale), anchored
      top-left and clipped to the element.
    - A frame that lags a resize is therefore cropped or short, never
      stretched.
    - The replaced image is passed to `window.drop_image` on the next paint, so
      atlas memory does not grow.
    - Until the first frame arrives, a faint placeholder quad is drawn.
  - **Input.**
    - One hitbox, and window mouse listeners registered in `paint`.
    - `Move` is forwarded while the hitbox is hovered, at the event position
      minus the element origin.
    - `Leave` is sent when a move lands outside the hitbox, the pointer leaves
      the window, or the view is hidden while hovered.
    - `Down` and `Up` (with click count and modifiers) are forwarded only over
      the element.
    - The wheel is not forwarded, so the list scrolls. Keys are not forwarded
      either.
    - The cursor comes from `Updates.cursor` via `set_cursor_style`.
  - **Visibility.** `prepaint` stores a `Presence` guard in the window's element
    state. GPUI drops element state that no element touched during a frame,
    once that frame is done. So the guard is dropped exactly when a frame was
    drawn without the web view, for example when the virtualized list scrolled
    it away. Its `Drop` marks the element unpainted and, unless the host says
    the view is near (`set_reach`), calls `set_visible(false)`. The next
    `prepaint` calls `set_visible(true)`. The last frame is kept the whole
    time.
  - **Instrumentation** (`stats()` → `Summary`, which has a `Display` impl):
    - *Move → frame latency* runs from the oldest `Move` not yet answered to
      the first paint of the next new frame. It covers the whole round trip:
      engine render, wake, notify, GPUI frame.
    - *Frames* counts frames taken.
    - *Idle frames* counts frames that arrive with no input, resize, theme or
      visibility change in the previous 1 s. The first frame is excluded.
- **`src/page.rs`.** `Demo` and `theme_css(dark)` (see "Demo pages" below
  for the other four). The first page is a squarified treemap of a fake repo,
  built with JS and divs:
  - two levels of nesting, and click-to-zoom with a breadcrumb;
  - a tooltip (path, lines, share, commits) that stays inside the page;
  - `Size` / `Churn` tabs (also keys `1` and `2`);
  - a legend, or a heat ramp in Churn.

  It is self-contained: an inline CSP, no network, no web fonts. **Every colour
  and font is a `--ferrite-*` variable.** A test checks for no hex, `rgb(` or
  `hsl(` in the stylesheet and no URLs. The dark palette is Ferrite's real
  opaque palette from `crates/ferrite/src/theme.rs`; light is a stand-in. The
  page is 376 px tall and `overflow: hidden`.
- **`src/main.rs` and `src/lib.rs`.** The app (the lib/bin split is there for
  tests):
  - a 200-row `list` with row 100 as the web view;
  - a header with live metrics;
  - `t` theme, `p` popover, `q` quit;
  - the popover is an absolutely positioned GPUI div over the list.

  Flags:
  - `--at <row>`: start row.
  - `--autopilot`: a ~60 Hz synthetic pointer sweep through
    `window.dispatch_event` (5 s), then park on a tile, idle 3 s, print.
  - `--quit`.
  - `--shots <dir>`, behind feature `shots`: saves `sweep`, `parked`, `light`
    and `clipped` PNGs rendered by the real Metal renderer via
    `window.render_to_image()`. No Screen Recording grant is needed.

  The engine boot block is exactly as specified. The CEF agent has since
  filled it in.

## Measured (fake engine, debug build, Apple Silicon, 2026-10-08)

```
autopilot [fake] move->frame p50 15.6ms p95 16.2ms max 32.0ms (n=301)  frames 302  idle frames 0  rss 115MB
```

- With the fake, latency is GPUI's own floor: the fake answers synchronously,
  so the cost is one notify plus waiting for the next display frame, about one
  vsync at 60 Hz. Earlier runs, made while the other agent's builds were loading
  the CPU, gave p50 18.7 / 27.2 ms.
- There were 0 idle frames and 302 frames for 301 moves plus the first frame.
- 115 MB RSS is mostly the GPUI app itself (debug build).
- The CEF agent's run of this app gave p50 about 27 ms and p95 about 35–40 ms.
  So Chromium adds roughly one frame on top of the floor, which is within the
  spike's "at most two frames" bar. See NOTES-cef.md.

Screenshots (outside the repo) are in `/tmp/inline-cef-shots/`:

- `parked.png`: dark theme, the pointer square under the parked pointer, and
  the popover drawn over the web view.
- `light.png`: after `set_theme`; both the host and the page are restyled.
- `clipped.png`: scrolled so the view's top is cut off by the list under the
  header. The popover is still on top and the hover square is gone (`Leave`).
- `sweep.png`: mid-sweep.
- `page-{dark,light}{,-hover}.png`: the treemap page rendered by headless
  Chrome with the theme block prepended (light-hover is on the Churn tab).

## Demo pages: five live views in one transcript

`page::Demo` is the page selector: `Demo::ALL`, `title()`, `html()`,
`page(dark)`, `from_title()`. The HTML lives in `src/pages/*.html` and is
`include_str!`'d. `main.rs` puts every demo in the same list, each as its own
`WebView`, after a user-prompt row: treemap 100, mockups 130, stats 160,
diagram 190, timeline 220. `--demo <name>` (or `--at <row-1>`) starts at one.

| Page | What it is | Interaction |
|---|---|---|
| treemap | squarified treemap of a fake repo (unchanged) | tooltip, click-to-zoom, Size/Churn |
| mockups | "show me options": Now / Option A / Option B of the composer's "thread settled" + "compact context" notices, each with a caption and the vertical space it costs | button hovers; Dismiss hides a notice (page shrinks); the model chip opens a menu; an `app dark / light` toggle flips the mocks' chrome independent of the host theme |
| stats | "token usage & cost, last 30 days": 4 KPI tiles (delta + sparkline), daily cost area chart with 7-day average, stacked bars by model, project table | crosshair + tooltip on the line; per-segment tooltip with the rest dimmed; 7d/30d/90d (90d switches bars to weekly); click a header to sort |
| diagram | 16-module, 5-layer dependency graph (SVG edges, HTML nodes), detail panel | hover traces depends-on (accent) and used-by (magenta) and dims the rest; click pins; edges that skip a layer run on a bus above/below the nodes so none crosses a node |
| timeline | CI run #4182: summary, job waterfall with queued (hatched) / passed / failed / skipped, failures list | job tooltip; click a job for its steps; click a failure for the assertion and backtrace (page grows 508 → 940 px) |

**Tokens.** `theme_css` gained `ok/warn/danger/info` (text-safe state colours),
`grid` (chart gridlines, quieter than `border`), and `chart-1..6`, a
categorical series palette in a fixed order. The chart palette is the dataviz
reference hues (blue, orange, aqua, yellow, magenta, violet), stepped per
theme and validated with the dataviz skill's `validate_palette.js`: both
modes pass lightness band, chroma floor, adjacent-pair CVD separation
(worst ΔE 8.4 dark / 9.1 light) and the normal-vision floor. In light mode
aqua, yellow and magenta are under 3:1 on `bg`, so charts using them carry a
legend and tooltips (and stats has the table). Ferrite's own pastel hues
failed the lightness band and CVD checks, so they stay text/accent colours.

`theme_css` also emits `[data-ferrite-scheme=dark]` and
`[data-ferrite-scheme=light]` blocks carrying each full palette whatever the
host theme. A page pins a subtree to one palette with the attribute; that is
how the mockups preview light app chrome inside a dark Ferrite without a
single literal colour. Worth keeping for Ferrite: "show it in the other
theme" is a common ask.

The self-containment test now runs over every page, and also checks inline
styles and script (not just `<style>`) for literal colours. It allows two
non-fetching strings: the SVG namespace URI and `url(#id)` marker references.

### Measured (Chromium, release, Apple Silicon, default 1100×820 window)

One demo per run (`--demo X --autopilot --quit`), so the first view also pays
for spawning the renderer and GPU processes. "First frame" runs from app
start; "hover→frame" is from one targeted `Move` (a button, a chart point, a
node) to the first new frame taken.

| Demo | first frame (cold) | first frame (`--tour`, scrolled in) | hover→frame | page height |
|---|---|---|---|---|
| treemap | 961 ms | 440 ms | 9–24 ms | 376 |
| mockups | 651–1660 ms | 180 ms | 7–40 ms | 462 |
| stats | 427–734 ms | 180 ms | 25–49 ms (one 108) | 647 |
| diagram | 592 ms | 244 ms | 41–48 ms | 418 |
| timeline | 507–551 ms | 146 ms | 8–26 ms | 508 → 940 expanded |

- In `--tour` the clock starts when the row is scrolled into view, which is
  when the element first lays out and calls `open`. 150–250 ms from scroll to
  first frame means a view shows a 240 px placeholder for about 10 frames.
  Pre-opening off-screen (or opening on stream end, before the user gets
  there) would hide that. The first frame already arrives at the page's own
  height ("sized" equals "first frame" in every run).
- Hover→frame is within 1–3 host frames, as with the treemap. The sweep's
  move→frame p50 stays 27–31 ms on every page. **The sweep's p95 is not
  meaningful on pages where most moves change nothing**: Chromium sends no
  frame, so the element's "oldest unanswered move" waits for the next visual
  change (p95 870 ms on mockups, 740 ms on diagram; 36 ms on the treemap,
  whose tooltip follows the pointer). That's correct behaviour by the engine
  and a measurement artefact; a fair metric would only time moves that
  produce a frame.
- **Height changes work.** Expanding a failure reports the new height, the
  element relayouts at 940 px and the list moves the rows below it; nothing
  is stretched or clipped (`timeline/expanded.png`). Shrinking (dismissing
  a mockup notice) goes through the same path but wasn't exercised in CEF.
- **Several live views coexist.** All five in one list, each its own
  `WebView`/`View`, with hover, clicks and theme switches routed to the right
  one. Each demo row gets an element id (`div().id(("demo", ix))`) so each
  view's `Presence` element state is keyed per row rather than relying on
  the list to disambiguate the element's fixed id `web-view` (not tested
  without it).

Memory with all five views open (after `--tour`, held idle; `footprint`):

| Process | Footprint | RSS |
|---|---|---|
| browser (GPUI app) | 155 MB | 157 MB |
| GPU helper | 118 MB | 72 MB |
| storage utility | 20 MB | 49 MB |
| renderer × 5 (one per view) | 35–41 MB each, 195 MB | 90–99 MB each |
| **total** | **~488 MB** | ~730 MB |

With one view open the total footprint was ~150–190 MB (NOTES-cef.md); each
further view adds a renderer process of ~40 MB footprint. (All pages are
in fact one site, `ferrite-view://page`; each got its own renderer because
each browser is its own BrowsingInstance, Chromium's default.) Both remedies
considered here (release views that have been off-screen for a while, and
share one renderer with `--process-per-site`) are now in; see "Only views
near the screen stay live" below and NOTES-cef.md "Memory".

### Problems found

- **The window only lays out when it draws.** If the window is occluded,
  `ListState::bounds_for_item` stays `None` and nothing opens. One fake-engine
  `--tour` out of four hit this; the autopilot now waits up to 5 s for the row.
- **Autopilot targets are fixed coordinates** for the default window (view
  width 1042 px) and a one-line agent introduction. A wrapped introduction
  pushes the page down 20 px and the clicks miss. Fine for a spike; Ferrite
  tests would ask the page where its targets are.
- **Placeholder flash on scroll-in** (above). Fixed by pre-opening; see
  "Only views near the screen stay live".
- **Fonts.** As before: the pages fall back to SF Mono (Geist Mono isn't
  reachable), the host uses Menlo. Close enough here; Ferrite should hand the
  engine its font.

Screenshots (outside the repo), each `--demo X --autopilot --quit --shots
/tmp/inline-cef-shots/X`, plus `/tmp/inline-cef-shots/tour/` from `--tour`
(all five with `<demo>-` prefixes):

- `treemap/`: `hover`, `light`, `clipped` (scrolled under the header, popover still on top).
- `mockups/`: `hover` (Compact button), `menu` (model menu open), `app-light` (mocks flipped to light chrome inside dark Ferrite, a status-row item hovered).
- `stats/`: `hover` (crosshair + tooltip), `sorted` (table sorted by Δ, a bar hovered, others dimmed), `light` (host light theme, crosshair).
- `diagram/`: `hover` (agent's edges traced), `pinned` (theme pinned, its bus edge to gpui).
- `timeline/`: `hover` (job tooltip), `expanded` (failure backtrace + job steps, page 940 px).
- every directory also has `loaded` and `sweep`.

## Only views near the screen stay live

Goal: memory that scales with the views you can see, no placeholder on
scroll-in, and scrolling that never hitches. Three pieces, each behind the
smallest interface that could carry it:

1. **Release, in the engine** (behind `View::set_visible`; NOTES-cef.md
   "Memory"). Hidden for 2 s → the browser is closed, the `View` stays.
   Shown again → reload; the engine sends nothing until the reload is laid
   out. The element already kept and drew its last frame and height, so
   there was nothing to change there: the frozen frame stays up until the
   reloaded one replaces it. `FakeEngine` mirrors it: `with_grace(d)`,
   releases from `pump()`, `Call::Released` / `Call::Reopened` in the
   journal, `live_views()`, and the hover square is its in-page state that
   a release loses.
2. **Near/far, from the host** (`WebView::set_reach(Reach::Near { width } |
   Reach::Far)`). A virtualized list knows where a row is relative to the
   screen; the element doesn't (an unpainted element is never laid out).
   The view is live while it is painted *or* near. A near view that was
   never opened opens at the width the host says it will have, so when it
   scrolls in nothing changes (no resize, no new frame). A near view that
   was released reloads. Far and unpainted → hidden → released after the
   grace period.
   - The element now takes updates in its wake task, painted or not, so an
     off-screen view's frame and height are current when it scrolls in.
   - It emits `HeightChanged` when the content height changes. The app
     subscribes and calls `ListState::remeasure_items(row..row+1)`, so the
     list re-measures that row while it is still off-screen (GPUI's list
     measures rows in its 200 px overdraw), never under the user. GPUI's
     list anchors on its top row, so a row above the screen changing height
     doesn't move visible content anyway; the risk is a row changing height
     as it becomes the top row while scrolling up, which this prevents.
3. **Where rows are** (`Proximity` in `main.rs`). `ListState` gives
   positions only for measured rows at or below its top row. `Proximity`
   remembers every row height it has seen and sums them (or 20 px for rows
   never measured: underestimating only opens early) to get each demo
   row's distance from the screen. Near within 1.5 screen heights, far
   beyond 2.5 (the gap stops flapping at the edge). It runs in the app's
   `render` on the previous frame's layout and requests one more frame
   when the list has moved since. Ferrite's transcript would do the same
   from its own row model.

The list grew to 400 rows so that its bottom is several screens from the
last demo; at 260 rows the diagram and timeline were always within the
keep-live margin.

### Measuring

- `--tour` now ends with memory checkpoints (footprint of the app and all
  helpers via `footprint(1)`) and times the scroll back to the treemap; with
  `--shots` it saves `back-0ms`, `back-100ms`, `back-250ms` and
  `back-new-frame`.
- `--scroll-bench`: opens all five (or none with `INLINE_CEF_BENCH_COLD=1`),
  parks at the bottom for 4 s so everything is released, then flings up to
  row 94 and down to the end three times at 4500 px/s
  (`INLINE_CEF_FLING=<px/s>`), one `scroll_by(v·dt)` per display frame from
  `on_next_frame`. It reports frame intervals, main-thread time in `pump()`
  (`element::take_pump_stats`), stalls seen by a 1 ms foreground heartbeat,
  and **height jumps**: every frame it checks that a visible row moved by
  exactly the scroll delta (more than 1 px off counts). With `--shots` it
  saves a frame as each demo enters the screen during an extra fling
  (`fling-up-*`, `fling-down-*`; in cold mode, during the first timed fling
  up, which meets never-opened views).
- **Run conditions matter.** The window must be frontmost and uncovered:
  macOS stops GPUI's display link for an occluded window and throttles
  background apps, and frames then come at a fixed 41.7 ms (24 Hz) whatever
  the app does. That happened in about half the runs here (the machine was
  in use). `INLINE_CEF_ON_TOP=1` opens the window as a pop-up above
  others, which helps but doesn't prevent it. The heartbeat measures
  main-thread stalls either way; frame-interval numbers below are only from
  runs at the display's 120 Hz.

### Results (release, Apple M4, 2 × 1920×1080 @ 120 Hz, scale 1)

Scroll benchmark, three round trips (frame intervals in ms; "before" is the
old adapter and element with the same harness and 260 rows, where nothing is
ever released, so nothing reloads during the flings):

| | fake | CEF before | CEF after, warm 4500 | CEF after, cold 4500 | CEF after, warm 6000 | CEF after, cold 6000 |
|---|---|---|---|---|---|---|
| frames | 720 | 823 | 1267 | 1266 | 953 | 952 |
| p50 / p95 / p99 | 8.3 / 8.8 / 9.2 | 8.3 / 8.7 / 9.2 | 8.3 / 8.8 / 9.7 | 8.3 / 9.0 / 10.1 | 8.3 / 9.1 / 10.4 | 8.3 / 9.2 / 9.4 |
| max | 9.4 | 10.7 | 15.2 | 16.9 | 15.9 | 13.3 |
| frames > 16.7 / > 33 ms | 0 / 0 | 0 / 0 | 0 / 0 | 1 / 0 | 0 / 0 | 0 / 0 |
| max pump | 0 | 9.7 | 10.1 | 10.1 | 9.9 | 9.5 |
| max heartbeat stall | 2.3–5.8 | 10.6 | 10.5 | 10.3 | 11.1 | 8.5 |
| height jumps | 0 | 0 | 0 | 0 | 0 | 0 |
| reloads during the flings | – | 0 | 6–9 | 9 + 2 first opens | ~2 | ~3 + 2 first opens |

- Pass bar (p99 ≤ 16.7 ms, nothing over 33 ms, no visible height jump) met in
  every 120 Hz run. The worst frames (15–17 ms, one per run at most) line up
  with a reload's first `do_message_loop_work`.
- At 6000 px/s a round trip is shorter than the 2 s grace period, so views
  mostly stay live between flings and fewer reloads happen.
- Memory during the same benchmark: see NOTES-cef.md (550 → 438 MB with all
  five live; 467 → 196–220 MB parked away from all of them).

Scroll-in, from the tour (each demo is scrolled to from the previous one, so
the next one is already near):

| | before | after |
|---|---|---|
| first frame after scroll-in, mockups / stats / diagram / timeline | 110–212 ms (240 px placeholder meanwhile) | **5–14 ms** (already there; the number is the first render's take) |
| treemap (on screen at launch, so not pre-openable) | 320 ms | 127–174 ms |

Screenshots (looked at, outside the repo, `/tmp/inline-cef-shots/`):

- `release/back-0ms.png`, `back-100ms.png`, `back-250ms.png`: right after
  jumping back to the released treemap, the frozen frame. `back-new-frame.png`:
  the reload, 141–148 ms later; no visible change except a 4 px height
  correction (the page had grown for a hover tooltip; see NOTES-cef.md).
  The first run showed the frozen frame in the *light* theme, which is how the
  restyle-before-hide race was found and fixed.
- `release/fling-{up,down}-*.png`: each demo entering mid-fling after
  everything was released; all show the real page, no placeholder.
- `release/cold/fling-up-*.png`: the same with the diagram and timeline never
  opened before the fling; both arrive laid out.
- `release-before/`: the same tour with the old code, for comparison.

Trade-offs and limits:

- **In-page state is lost on release** (zoom, tabs, sort, expanded rows,
  pins, dismissed notices, hover, focus). The 2 s grace period and the
  2.5-screen keep-live margin mean it only happens to views the user has
  scrolled well away from for a while. A jump scroll (search result,
  scrollbar drag, `scroll_to`) back to a released view shows its frozen
  frame for ~150 ms and then the freshly loaded page.
- **The margin costs memory**: 2–3 views are live around any demo in this
  transcript, not just the one on screen. Tunable (`NEAR`, `FAR` in
  `main.rs`, `RELEASE_AFTER` in the adapter).
- **Theme while released**: the frozen frame shows the theme it was taken
  in until the view is near again (then the reload uses the new theme). In
  normal scrolling the reload happens before the row is on screen.
- `Proximity` estimates rows it has never measured at 20 px. A transcript
  with tall unmeasured rows would open views earlier than needed, never
  later.

## What the CEF adapter must do for the element

Most of this is already true per NOTES-cef.md.

1. **`wake` is cheap and re-entrant-safe,** so it can be called at any time on
   the main thread. The host only queues a notify; it never calls back into
   the view from inside `wake`.
2. **Coalesce in `take()`.** Return only the latest frame. Frames are in
   device pixels, `round(width*scale) x round(content_height*scale)`. The
   element draws a frame at `frame / scale` from the top-left, so a frame
   sized for the previous viewport is harmless.
3. **Report `content_height` in logical px.** The element is exactly that
   tall, so the page must not scroll itself (`overflow: hidden`).
4. **Alpha.** GPUI's sprite pipeline blends *straight* alpha
   (`SourceAlpha, OneMinusSourceAlpha`), but CEF's OSR frames are
   *premultiplied*. That is fine while every pixel is opaque. Translucent
   pixels, for example over a transparent browser background, would come out
   darkened at their edges. Either paint an opaque background (the theme's
   `--ferrite-bg`; the demo page does this) or un-premultiply in the adapter.
   The element doesn't convert, to keep the per-frame cost to one move.
5. **Visibility arrives late.** `set_visible(false)` comes one host frame after
   the element stopped being painted (from a `Drop`, during the window's
   frame swap). `set_visible(true)` comes in `prepaint`, after any `resize`,
   before the paint that shows the old frame. It can also arrive when the
   window closes.
6. **Input contract.**
   - `Move` arrives only while hovered.
   - `Leave` is guaranteed when the pointer exits the element or the window,
     or when the view is hidden while hovered.
   - There is no pointer capture: a press that drags out of the element sends
     no `Up`, so the page can think a button is still down (open issue).
7. **Pump.** `element::pump` sleeps for the returned duration, at least 1 ms,
   on a background-executor timer, in a foreground task outside any
   `update`. Nothing can bring the next pump forward, so a CEF
   `OnScheduleMessagePumpWork(0)` that arrives while the host sleeps waits
   out the sleep. The adapter caps the sleep at 33 ms while pages are live.
   See the proposal below.

## Seam: one proposed change (not made)

The seam has no way for the engine to ask for an **earlier pump**. `wake` is
per view and means "take updates", and `pump()` returns a sleep that the host
honours blindly. CEF's external message pump says when work is due from any
thread (`OnScheduleMessagePumpWork(delay)`). With the current seam, the
adapter can only return short sleeps all the time, which costs idle CPU
wake-ups, or accept up to one sleep of extra latency.

Minimal change: `Engine::pump` stays as it is, and one method is added.

```rust
/// Called once by the host: `wake_pump` makes the host call `pump` soon. It
/// may be called from any thread.
fn set_pump_waker(&self, wake_pump: Arc<dyn Fn() + Send + Sync>) {}
```

The default implementation does nothing, so the fake is unaffected. The host
side is a channel that `element::pump` `select`s on next to its timer. Not
needed to pass this spike; worth it before shipping.

Nothing else in the seam got in the way. `Updates` with latest-only fields
mapped directly onto render-time `take()`.

## Open issues

- **No pointer capture or drag.** Pressing in the view and releasing outside it
  sends no `Up`. Fix: on `Down`, keep forwarding `Move`/`Up` until release
  (GPUI's `window.captured_hitbox`, or track the pressed state in `Inner`).
- **Hover under a still pointer.** When the list scrolls under a stationary
  pointer, nothing is sent until the next real move. GPUI doesn't synthesise a
  move on scroll, so the page's hover state goes stale for a moment.
- **Atlas on drop.** If a `WebView` entity is dropped, its last frame stays in
  the sprite atlas, because the element can't reach a `Window` from `Drop`.
  Fine for the spike. The fix is to hand the image to the window that last
  painted it, for example with an `on_release` that defers a `drop_image`.
- **Scale changes.** When the window moves between displays, the frame on
  screen is drawn at the scale it was taken at, until the engine sends one at
  the new scale. Correct, but one frame can be blurry.
- **Keys, focus and wheel are not forwarded.** The seam has them. The element
  would need a focus handle (and blur, which the seam lacks; see NOTES-cef.md
  "Focus") and an opt-in wheel mode.
- **RSS in the header is this process only.** With CEF, the helper processes
  are separate. NOTES-cef.md has per-process footprints.
- **Fonts.** The GPUI side uses Menlo. The page asks for Geist Mono, which is
  not installed system-wide and not fetchable (CSP), so it falls back to SF
  Mono. Ferrite proper would need to make Geist Mono available to the engine.

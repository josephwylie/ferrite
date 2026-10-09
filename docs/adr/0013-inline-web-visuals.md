---
status: proposed
date: 2026-10-09
---

# Inline web visuals: off-screen Chromium for agents' `show_visual`, nothing else

Narrowly supersedes [ADR 0001](0001-gpui-renderer.md)'s "no browser engine, no
webview, no JS layer anywhere" — for one surface only. Ferrite's own UI stays
GPUI, one process, one language; this ADR changes nothing about how Ferrite
draws itself.

## Context

Agents increasingly answer with things that are visual: UI options, a chart
of data with a shape, a dependency diagram, a CI timeline. A Markdown table
or an ASCII sketch carries that badly, and the operator then opens a browser
on a file the agent wrote. Both Providers can call a tool Ferrite offers
in-band (Claude's SDK MCP server, Codex's dynamic tools; see
`ferrite_core::visual`), so Ferrite can ask the agent for a self-contained
HTML page and show it where the answer is: in the transcript, live and
interactive, styled by Ferrite's own theme.

Drawing agent-authored HTML/CSS/JS needs a browser engine. The question the
`spikes/inline-cef` spike answered (2026-10-08, Apple M4, see its README,
NOTES-cef.md and NOTES-host.md) is whether one can sit *inline* in GPUI's
virtualized transcript without a native child view and without costing the
cockpit its feel. It can:

- The page is rendered off-screen by Chromium (CEF 154 via tauri-apps/cef-rs)
  and drawn as an ordinary GPUI image wherever layout puts the row, so
  scrolling, clipping and popovers need no coordination. Mouse input is
  forwarded in view coordinates; the cursor follows the page.
- Hover → new pixel: 13–23 ms in the engine, a move→frame p50 of ~27 ms in
  the GPUI app (one frame above GPUI's own floor). An idle page sends no
  frames.
- Scrolling: flinging at 4500–6000 px/s through five live pages from a fully
  released state held p99 ≤ 10.4 ms frames at 120 Hz, nothing over 33 ms, no
  visible height jump; the longest main-thread pump was ~10 ms.
- Memory (footprint, all processes): one page ~150–190 MB; five pages live
  ~440 MB (one shared renderer); scrolled away from all of them ~200–220 MB
  (no renderer at all). Each further live page costs ~10–15 MB.
- Size: the `.app` grows by the framework, **~323 MB** (128 MB download,
  fetched once at build time). The five helper apps are hard links to the
  main binary in an ad-hoc build.

## Decision

1. **One engine seam, app-side.** `crates/ferrite/src/visual/web` keeps the
   spike's `Engine`/`View` seam (GPUI-free), with two real adapters: Chromium
   (macOS) and a fake that draws synthetic frames (tests, and every build
   without Chromium). `View::set_html` was added so a page builds up while
   the agent's input streams: each new draft reloads the page, coalesced (one
   load in flight, ≥ 250 ms apart), the previous frame staying up until the
   new page is laid out. Core stays browser-free; it sees only the
   `visual::Renderer` seam that turns HTML into the PNG an agent checks.
2. **Only what is near the screen is live.** A visual within 1.5 screens
   opens (at the width it will have) or stays live; beyond 2.5 and undrawn it
   is hidden, and the engine releases it after 2 s. A released visual keeps
   its last frame and height, so coming back shows it at once and reloads
   underneath. All pages share one renderer process (`process-per-site`).
   Browser creation is staggered one per pump so a scroll that brings several
   back never costs one long frame.
3. **Chromium is behind the `cef` cargo feature, macOS only.** A plain
   `cargo build -p ferrite` (CI, Windows, everyday development) has no CEF:
   no cmake/ninja, no 128 MB download, no 323 MB framework. A visual there
   shows its title, caption, the reason (`Visuals aren't available on Windows
   yet`, `…in this build`) and two fallbacks: open in browser (the same page,
   theme and Content-Security-Policy, as a file) and copy HTML.
   `scripts/install-app.sh` (and so the release DMG) builds with the feature
   and bundles the framework and the five `ferrite Helper*.app`s.
4. **Chromium starts lazily.** `visual::boot()` is the first line of `main`:
   a CEF helper process (the same binary, started with `--type=`) runs and
   exits there, before any shell, store or window; the app only loads the
   framework and retrofits CEF's `NSApp` protocol onto GPUI's application
   class. `cef_initialize` — and with it the GPU and utility processes —
   waits for the first visual. A session that never shows one pays nothing
   beyond the mapped framework.
5. **Pages are sealed.** Served from memory under `ferrite-view://`, with a
   strict CSP (inline script and style only; no network, frames or workers
   from elsewhere); every other request, navigation, popup, dialog and context
   menu is refused; a throwaway profile per process; Chromium's sandbox on.
   The only thing a page can reach besides itself is Ferrite's bundled face
   (`/font/*.ttf`), so visuals are set in Geist Mono like the transcript.
6. **Pages are styled by Ferrite's theme.** The `--ferrite-*` token names
   (and what each is for) live once in core beside the tool description that
   lists them to the agent; the values come from `theme.rs` in the app, for
   dark and light, plus `[data-ferrite-scheme=…]` scopes. A reading-size
   change restyles live pages in place.

## Consequences

- The `.app` is ~325 MB bigger and the release build needs cmake and ninja.
  A Developer ID build signs each helper separately, which turns the hard
  links into copies of the main binary (codesign rewrites the file); a small
  dedicated helper binary would remove that cost and is the follow-up if the
  DMG size matters.
- Memory scales with the visuals near the screen, not with history; a
  visual scrolled far away for 2 s loses its in-page state (a zoomed treemap,
  a sort order) and reloads from scratch when it comes back.
- One shared renderer: a page that spins or crashes the renderer freezes or
  drops every live visual until each reloads (no watchdog yet).
- Windows has no engine yet; the fallback card is its whole story until a
  Windows adapter lands behind the same seam.
- Known gaps, inherited from the spike: `<select>` popups aren't composited,
  no IME, no copy shortcut inside a page, the wheel always scrolls the
  transcript (pages never scroll themselves).
- ADR 0001's argument still holds for the app itself: there is no IPC tier
  and no web UI; Chromium only ever draws pixels the agent asked for.

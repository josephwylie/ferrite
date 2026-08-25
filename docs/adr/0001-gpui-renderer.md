---
status: accepted
date: 2026-08-25
---

# GPUI as the renderer — native GPU UI, no webview

Ferrite renders with GPUI (Zed's Rust UI framework): Metal on macOS, DirectX 11
on Windows. No browser engine, no webview, no JS layer anywhere. We decided
this after a measured spike (`spikes/panes24`): 24 panes streaming ~2,880
deltas/s under worst-case whole-window re-render held 120fps (worst second
62fps) in 86MB total RSS — and it collapses the architecture to one process in
one language, deleting the IPC tier a webview would require.

## Considered options

- **Tauri 2 + React** (rejected): mature streaming-chat ecosystem and the only
  accessible option (~3–5 wks to MVP vs ~6–10), but adds a webview, an IPC
  tier, and a second language, and caps the feel below the native ceiling.
  Analysis showed the perf bottleneck is process/stream, not paint — so this
  was a product-feel choice made *with* evidence, not a necessity claim.
- **Electron / other Rust GUIs** (rejected earlier): Iced/Slint weak at rich
  documents; Dioxus Native unfinished by its authors' own account.

## Consequences

- **Pin policy**: gpui is pinned to the published crates.io release (0.2.2 at
  decision time); upgrades are deliberate events, never drive-by. Zed has
  reportedly paused packaging GPUI for external users (2026); if we outgrow
  the pin we choose between mainline-vendored, the gpui-ce community fork, or
  maintaining our own — a future ADR.
- **No screen-reader accessibility** until GPUI grows an AccessKit-style
  bridge upstream; acknowledged openly in the README.
- Everything below the app crate must stay gpui-free and headless-testable,
  so this decision remains swappable at exactly one seam.

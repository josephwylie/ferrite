# Ferrite

Multi-provider agentic development environment: a dense, Metal-native cockpit of
document panes, each driving a coding agent (Claude, Codex, Cursor, …) through its
strongest native transport behind one ACP-shaped internal seam. Spun off from
SwarmDeck learnings — no terminals, no PTYs: agents stream structured events,
Ferrite renders them as rich documents at terminal density.

## Settled decisions (2026-08-25)

- **Name**: Ferrite (Rust culture + metal + magnetic-core memory).
- **Agent transports**: native-first — Claude via CLI stream-json + stdio control
  protocol (pin CLI ≥ 2.1.224), Codex via `codex app-server` JSON-RPC, Cursor/Grok
  via ACP. One internal ACP-shaped adapter trait in front of all of them.
  Raw ACP as the uniform spine was researched and rejected (capability loss).
- **No per-delta durable writes** — event-sourced thread log persisted at
  block/turn boundaries (T3 Code's per-delta SQLite+WS path is the counterexample).
- **Session pool**: LRU of hot CLI processes, reattach-on-initialize, park the rest.
- **Renderer**: GPUI (Rust, Metal) — **spike passed 2026-08-25**: panes24 held
  120fps (worst second 62) with 24 panes streaming ~2,880 deltas/s under
  whole-window re-render, 86MB total RSS (~3.6MB/pane vs <10MB budget), on
  gpui 0.2.2 from crates.io. Tauri fallback retired.

## Research trail

Design canvas: claude.ai artifact "SwarmDeck Rich Pane" (concept + directions).
Research docs (currently in `../swarmdeck/docs/research/`):
- `perf-frontier-2026-08-24.md` — ACP maturity, WKWebView scheduler gap, wry zero-copy
- `acp-vs-native-adapters-2026-08-24.md` — why native-first transports
- `native-metal-ui-2026-08-24.md` — GPUI vs webview, spike gate definition
- `perf-rich-pane-recent-2026-08-24.md` — CLI version pins, park/resume support

## Layout

- `spikes/panes24/` — GPUI render spike: 24 synthetic streaming panes, FPS + RSS HUD.

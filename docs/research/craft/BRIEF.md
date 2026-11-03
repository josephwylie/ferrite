# Craft analysis brief

Goal: a measured spec for making Ferrite match Claude Desktop and Codex Desktop on craft (layout, type, colour, chrome, motion, copy, states). Measure, don't guess. Every claim cites a file and a pixel location.

## Inputs (all under `docs/research/craft/`)

- `claude/*.png`: Claude Desktop (Code tab), raw window captures at **1x** (1 px = 1 pt). Window 1920x1050 pt.
- `codex/*.png`: Codex Desktop (ChatGPT app), raw window captures at **1x**, window zoomed to fill a 1920x1080 display.
- `ferrite/*-app.png`: Ferrite branch build, visual-reference renders at **2x** (2 px = 1 pt) of a 1440x900 pt window. Halve pixel measurements.
- `mov/*.mp4`: screen recordings of the primary 1920x1080 display at 1x, 120 fps requested, variable frame rate (frames only on change; use ffprobe pts). `*.log` next to each.
  - claude-menus (model, permission, + menus open/close), claude-send (send → approval card), claude-stream (stream of reply + queued follow-up), claude-expand (tool group expand/collapse x3), claude-panel (right panel close/open/close)
  - codex-thread-switch, codex-expand (Worked for expand/collapse x3), codex-send (send → working), codex-stream (stream + queued follow-up), codex-menus (effort, permission, + open/close), codex-panel (panel close/open/close)
- `motion.md`: earlier findings (Codex measured, Claude from notes, Ferrite from code) and a ranked change list. Verify or correct it.
- Earlier prose docs: `docs/research/claude-quality-bar.md`, `docs/research/codex-quality-bar.md`.
- Ferrite code: `crates/ferrite-ui/src/theme.rs` (tokens), `crates/ferrite-ui/src/motion.rs`, and the views under `crates/`.

## Rules

- Read-only except your one output file. Do not edit code.
- No cargo, no builds, no tests, no launching apps, no screen capture.
- Disk: the machine crashed from low disk before. Never write decoded video frames to disk; pipe ffmpeg raw frames (cropped, scaled if needed) into numpy in-process. Temp files go in your scratchpad, total under 200 MB. Never `rm` through a shell variable; delete by literal path.
- Memory: process video in a streaming loop, never load a whole video into memory.
- Python: `python3 -I`; numpy and Pillow are available (install nothing).

## Output

One Markdown file at the path your task names. Lead with a table: element | Claude | Codex | Ferrite now | target for Ferrite | evidence (file + px). Then short notes on anything the table can't hold. Target values are concrete (pt, hex, ms, easing). Where Claude and Codex disagree, recommend one and say why in one line.

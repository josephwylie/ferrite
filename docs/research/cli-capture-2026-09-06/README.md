# CLI capture evidence

Primary pass: **2026-09-06 20:53–21:13 Australia/Adelaide**. Live installed
Claude Code 2.1.263 and Codex CLI 0.153.4; harmless synthetic tasks.

- [Interactive viewer](viewer.html): open locally in a modern browser; no server/network needed.
- [Measured visual specification](../cli-transcript-visual-spec.md): findings, exact spacing, provider differences, coverage boundaries.
- [State catalogue](STATE-CATALOGUE.md): every labelled state, images, whitespace views and row maps.
- [Pinned source investigation](../cli-rendering-source-reference.md): supporting implementation/documentation evidence.

The user requested automatic capture after Computer Use could not open Terminal.
The method here uses shell-managed PTYs and an isolated tmux parser; it does not
drive or screenshot the blocked Terminal app. All images are reconstructions.

## Files and evidence levels

| File | Meaning |
| --- | --- |
| `manifest.json` | Versions, exact launch arguments, viewport/environment, fixture/font hashes, exit status |
| `actions.jsonl` | Time-stamped text, key, resize and mark commands |
| `claude/output.bin`, `codex/output.bin` | Unmodified bytes emitted by the respective CLI's PTY |
| `*/output-index.jsonl` | Byte offsets, lengths and wall-clock arrival timestamps; reads are not token boundaries |
| `*/input.bin`, `*/input-index.jsonl` | Input bytes, including actual terminal query replies, with matching index |
| `*/frames.jsonl` | Changed screen snapshots: tmux ANSI export, timestamp, source index, cursor/dimensions/screen mode |
| `*/NN-label.json` | Labelled snapshot; many include full normal-screen scrollback in `history_ansi` |
| `*/final-history.json` | Final normal-screen scrollback; not a substitute for earlier transient screens |
| `*/rendered/*.png` | Reconstructed labelled frames |
| `*/rendered/*-spaces.png` | Same frames with ordinary spaces shown as `·`, NBSP as `⍽` |
| `*/rendered/*.txt` | Fixed-width visible text grid, including padded trailing cells |
| `*/rendered/*-cells.json` | Exact derived cell matrix for each labelled frame |
| `styles.json` | Style table indexed by those cell records |
| `grids.json.gz` | All valid sampled/labelled frames, with deduplicated rows and shared style table |
| `viewer.html` | Self-contained replay UI with the same compressed grids and bundled symbol font |
| `spacing-audit.json` | Cell-based offsets for all ordinary-space runs on each nonempty labelled row |
| `capture-integrity.json` | Continuous byte-index validation and SHA-256 for both input/output recordings |
| `render-quality.json` | Parser coverage, excluded resize race, image reconstruction parameters |
| `fixture/` | Exact synthetic input files |
| `model-source/*formatting.md`, `*.diff` | Actual model-returned Markdown and its difference from the fixture |
| `model-source/*messages-and-tools.jsonl` | Selected assistant text and tool records from these two Sessions only |

Model-source extraction excludes system/developer instructions, authentication,
opaque/encrypted reasoning and private thinking blocks. These files are useful
for comparing actual text/tool input with its presentation, not as additional
claims about what appeared on screen.

## Cell schema

In a cell JSON, `cells[row][column]` is `[text, style_id, width]`:

- `text`: one displayed character plus any combining marks; exact Unicode retained.
- `style_id`: index into `styles.json`.
- `width`: 1 for an ordinary cell, 2 for a wide glyph's first cell, 0 for its continuation cell.

Wide continuation cells use empty text. Empty visible cells use ordinary space.
Colour tokens are `default`, `index:N`, or an emitted RGB `#rrggbb`. Style flags
are explicit; hyperlink destinations are metadata, not extra visible characters.
Grid coordinates and `cursor` are zero-based. A terminal cursor can sit at the
right-edge wrap position; the reconstruction does not establish native cursor
shape/blink. `alternate` records the active screen buffer.

In `grids.json.gz`, each frame has `row_ids` referencing `row_pool`; each pooled
row contains the same cell triples. Row deduplication keeps the viewer small and
avoids allocating repeated cells for every spinner update. Labelled frames are
inserted into the replay alongside sampled frames; do not call their total a
count of distinct semantic states.

The original `frames.jsonl` metadata is comma-separated:
`columns,rows,cursor_x,cursor_y,cursor_visible,alternate_screen,history_rows,pane_dead`.
The initial collector captured grid and metadata separately. One 120→80 resize
raced those reads; its source index is retained and excluded from derived replay.
The reusable recorder now checks dimensions before and after grid capture.

## Reproduction and tools

Run from the Ferrite repository. Requires Python, tmux, installed/authenticated
CLIs; rendering requires Pillow and wcwidth. Exact versions from this run are
recorded in the manifest.

```sh
python3 scripts/capture-cli.py start /absolute/path/to/new-evidence
python3 scripts/capture-cli.py status /absolute/path/to/new-evidence
python3 scripts/capture-cli.py send /absolute/path/to/new-evidence claude 'Read only sample.txt.' --enter
python3 scripts/capture-cli.py mark /absolute/path/to/new-evidence claude read-result
python3 scripts/capture-cli.py resize /absolute/path/to/new-evidence claude 80 44
python3 scripts/capture-cli.py key /absolute/path/to/new-evidence claude C-o
python3 scripts/capture-cli.py stop /absolute/path/to/new-evidence
python3 scripts/render-cli-capture.py /absolute/path/to/new-evidence
python3 scripts/test_cli_capture.py
```

`start` creates its own synthetic temporary workspace and launches the two CLIs
behind a startup gate so the recorder is ready before their first output. It
does not automate answering trust/approval prompts: inspect each requested
action before responding. The recorded `actions.jsonl` documents this run's
actual decisions. Only one-time approvals were accepted. Model selectors were
canceled; no global model setting was intentionally changed. The providers may
persist ordinary Session history/trust records as part of normal operation.

For a new evidence directory, supply its `fonts/NotoSansSymbols2-Regular.ttf`
and `fonts/OFL.txt` before rendering. These are captured assets, not installed
system fonts. The included files came from the official
[Google Fonts repository](https://github.com/google/fonts/tree/main/ofl/notosanssymbols2),
downloaded 2026-09-06; hashes are in the manifest and the OFL licence is included.
The renderer uses system Menlo, Apple Symbols, Arial Unicode and Heiti where
available, with Noto Sans Symbols 2 as a fallback for symbols such as U+23FA.

## Reconstruction limits

- Images use declared 10×22 px cells, Menlo 16 px and 20 px outside padding.
- Indexed ANSI 0–15 colours use a declared xterm reference palette. They are not
  asserted as this user's Apple Terminal palette. Indexed 16–255 and emitted
  RGB values are preserved as separate tokens regardless of image appearance.
- Dim uses a chosen 50% blend; cursor is a chosen outline; blink is not animated
  by the viewer. Pixel appearance depends on font fallback and browser/platform.
- The 100 ms grid sample interval can miss intermediate repaints. Continuous
  raw bytes remain available for a future higher-resolution replay.
- The parser handles the normalized sequences present in this capture; it is
  not a general replacement for a terminal emulator. Unsupported sequences are
  reported, not silently claimed as supported. Complex emoji clusters were not
  tested. The actual CLIs' output was parsed by tmux before this export parser.
- `Pane is dead …` belongs to tmux, not to Claude or Codex.
- Startup tips, model choices, status-line fields, plugin results and account
  links reflect the captured installation. They should not be mistaken for
  universal defaults or automatically copied into Ferrite.

The main application was not changed. This directory, the supporting reports,
and the capture/render/test scripts are the work product.

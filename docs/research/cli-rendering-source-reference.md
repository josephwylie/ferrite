# CLI rendering: pinned supporting reference

Researched 2026-09-06. Supporting source investigation for the automated PTY
capture study. **This document contains no live visual observations.** Read-only
version commands returned Claude Code **2.1.263** and Codex CLI **0.153.4**.
No interactive model session or user configuration changes were made by this
research task. Capture evidence must establish which source branches actually
ran; source inspection alone cannot establish complete visual parity.

Codex source below is pinned to release commit
[`3d2ee51ca2d5db578f328aa75e20aa22c0197c9a`](https://github.com/openai/codex/tree/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a),
resolved from annotated tag `rust-v0.153.4`. Installed Claude executable:
`~/.local/share/claude/versions/2.1.263`, SHA-256
`ef5d2909c8af49f31ab6d5487e90316777bc2fac170adfe8160716caa8aaf4f9`.
Claude documentation is current documentation, accessed on the research date;
it is not a pinned specification of every 2.1.263 branch.

## Capture controls and prerequisites

- Codex's detailed transcript overlay is `Ctrl+T`; it combines committed
  history and a live active-cell tail. The tail can mutate in place as tool
  output arrives. [ChatWidget contract](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/chatwidget.rs#L1).
- Claude `Ctrl+O` toggles its transcript viewer, exposing detailed tool use,
  assistant timestamps, and model information. `Ctrl+E` expands all content
  **only in classic rendering**. In fullscreen, `[` exports the conversation
  to terminal scrollback; `{` and `}` navigate prompts. `q` or Escape exits.
  `/tui` without arguments reports the renderer. These shortcuts can be
  remapped. [Official interactive-mode reference](https://code.claude.com/docs/en/interactive-mode#transcript-viewer).
- Record terminal replies, not only CLI output: Codex queries default
  foreground/background through OSC 10/11. Missing replies are cached as
  unavailable and select fallback colours. [Palette probe](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/terminal_palette.rs#L180).
- Preserve the chosen font, font metrics, palette, terminal capabilities,
  emulator/version, rows/columns, and every resize in the capture manifest.
  A PTY establishes terminal data; a reconstructed image additionally reflects
  the replay renderer. Pixel measurements belong to that renderer, unless
  independently verified against the original terminal.

## Codex: character-cell spacing and styles

Offsets below describe the individual history cell, before any outer viewport
positioning. A space in a quoted prefix is literal U+0020.

| Cell or state | Exact source rule |
| --- | --- |
| User message | One styled blank row before and after. First text row starts `› `, with the marker bold and dim. Continuation prefix is two spaces. Text wraps at `width − 3`, clamped to at least one column: two prefix columns and one right margin. Trailing CR/LF and final whitespace-only wrapped rows are removed. |
| Assistant message | Initial row of the first streamed chunk starts dim `• `; later rows/chunks start two spaces. Wrapped continuations additionally retain the source line's leading whitespace. |
| Reasoning summary body | Markdown laid out with two reserved columns, patched dim and italic. Initial prefix dim `• `; continuation prefix two spaces. A transcript-only cell returns no normal-history lines. |
| Background-terminal input | Header begins dim `↳ `, then bold interaction wording. Optional command follows dim ` · `. Input begins dim `  └ ` and continues with four spaces. |

Sources: [user cell](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/history_cell/messages.rs#L154),
[assistant cell](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/history_cell/messages.rs#L392),
[reasoning cell](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/history_cell/messages.rs#L316),
[background interaction](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/history_cell/exec.rs#L21).

Do not translate every character to one cell. Codex uses Unicode display
width, with explicit one-cell treatment for halfwidth sound marks U+FF9E and
U+FF9F. Its tests expect `ｶﾞﾊﾟ` to occupy four cells. Source byte counts,
grapheme counts, display columns, and pixel widths are separate quantities.
[Width helpers](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/width.rs#L16).

## Codex: tool cells

Normal shell execution uses a bullet, one space, bold running/completed verb,
one space, then syntax-highlighted command text. Completed success bullets
are green and bold; failures red and bold. Active calls use the activity
marker. The verbs distinguish running, agent completion, and user-issued
shell commands. Completion colouring requires duration and output, rather
than being inferred from an inactive process alone.
[Command header](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/exec_cell/render.rs#L352).

The ordinary execution layout specifies:

- Command continuations: `  │ `, maximum two continuation rows.
- Output first row: `  └ `; subsequent rows: four spaces.
- Agent command output: maximum five displayed rows after wrapping.
- User shell command output: a larger 50-row limit.
- Output text is dimmed, including spans decoded from ANSI colours.
- Empty output gets a dim no-output message, except terminal-interaction calls.
- Middle truncation uses `… +N lines`, aligned to the output continuation
  gutter. Its count incorporates upstream omitted lines; it is not simply the
  difference between raw newlines and the five displayed rows.

Sources: [fixed gutters](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/exec_cell/render.rs#L696),
[output styling](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/exec_cell/render.rs#L129),
[row-aware output limit](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/exec_cell/render.rs#L431),
[truncation](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/exec_cell/render.rs#L526).

Exploration groups have an activity/dim bullet, one space, and a bold
active/completed exploration heading. Consecutive read-only calls combine
unique filenames with dim comma-space separators. Detail labels are cyan;
search query/path joins use dim ` in `. The group starts its first detail
with `  └ `, then four spaces. Whether calls qualify for this path is a
separate classifier/model decision.
[Exploration rendering](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/exec_cell/render.rs#L255).

Detailed transcript rendering changes the presentation substantially:
commands begin magenta `$ `, wrapped continuations have four spaces,
retained output is rendered without the compact five-row preview, and
completion uses a green bold check or red bold cross with nonzero exit code,
followed by dim bullet-separated duration. This is additional display detail,
not proof of access to bytes already discarded upstream.
[Transcript path](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/exec_cell/render.rs#L188).

## Codex: Markdown and word spacing

Codex does not simply print the model's Markdown or use browser Markdown
defaults. Its own parser-to-line renderer specifies:

| Input structure | Rendering rule |
| --- | --- |
| H1 | Retains `# ` prefix; bold and underlined. |
| H2 | Retains `## ` prefix; bold. |
| H3 | Retains `### ` prefix; bold and italic. |
| H4–H6 | Retain their hash prefixes; italic. |
| Inline code | Cyan. |
| Emphasis / strong / strike | Italic / bold / crossed out. |
| Links | Cyan and underlined baseline; local/web destinations have additional display and hyperlink rules. |
| Blockquote | `> ` indentation context; green blockquote style. |
| Unordered item | Hyphen-space marker. At depth `d`, marker field width is `4d − 3`; marker includes `width − 1` leading spaces. |
| Ordered item | Right-aligned number, period, space; marker uses light blue. Continuation indentation tracks the numbered marker width. |
| Horizontal rule | Three em dashes. |
| Table | Two spaces between columns plus one cell of padding on each side; heavy header rules and light body rules. Width heuristics may transform it into key/value records. |

Sources: [styles](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/markdown_render.rs#L105),
[heading and quote prefixes](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/markdown_render.rs#L578),
[list layout](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/markdown_render.rs#L786),
[rule](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/markdown_render.rs#L460),
[table constants and algorithm](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/markdown_render.rs#L1).

Both hard and soft Markdown breaks generally create new display lines here;
inside table cells soft breaks become spaces. Local-link labels and breaks
immediately following local-link targets have special handling. Thus HTML's
ordinary whitespace-collapse model is not an adequate reference. Known-language
code is buffered verbatim for highlighting; this low-level renderer avoids
wrapping code to preserve whitespace, but enclosing history rendering can
still apply its own layout. Capture the final grid rather than assuming a
single internal stage determines wrapping.
[Break handling](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/markdown_render.rs#L753),
[code buffering](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/markdown_render.rs#L654),
[low-level wrapping](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/markdown_render.rs#L1938).

For exact interword-spacing tests, retain raw UTF-8, parsed cells, and an
explicit codepoint view. Include single/double/triple spaces; tabs; leading
and trailing spaces; blank and whitespace-only lines; NBSP; code indentation;
combining characters; CJK; emoji; long unbroken tokens; punctuation beside
styling boundaries. The source rules above do not prove the outcome of every
combination. Styled runs must not introduce or swallow spaces at their joins.

## Codex: colours, status, approvals

User-message backgrounds depend on the terminal's own background: white at
12% alpha over a dark background, black at 4% alpha over a light background.
Unknown background means no special background. Selected controls use bold
cyan on dark/unknown backgrounds, or RGB `(0,95,135)` mapped to the supported
palette on light backgrounds. Success/failure use terminal green/red; attention
uses yellow only when a dark background is known. Table separators blend
20% foreground over background when supported, otherwise dim.
[Colour policy](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/style.rs#L1).

The active status composes optional animated marker, one space, shimmering
heading, one space, then dim elapsed time and optional interrupt shortcut
inside parentheses. The elapsed/shortcut separator is space-bullet-space.
Additional inline context uses space-middle-dot-space. A hook status can move
to a separate detail line when it no longer fits; the header is truncated to
width. Details use `  └ ` and four-space continuation. Time formats include
seconds, `1m 00s`, and `1h 00m 00s`; these are elapsed runtime presentation,
not timestamps.
[Status composition](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/status_indicator_widget.rs#L211),
[details/time rules](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/status_indicator_widget.rs#L43).

Approval titles vary by command execution, terminal input, network host,
permission grant, file patch, and MCP elicitation. The title is bold,
followed by a blank line and request-specific header; options and footer are
then delegated to the selection view. Available decisions and remapped keys
determine which choices exist. A single command-approval capture cannot stand
in for all approval types.
[Approval construction](https://github.com/openai/codex/blob/3d2ee51ca2d5db578f328aa75e20aa22c0197c9a/codex-rs/tui/src/bottom_pane/approval_overlay.rs#L247).

## Reasoning and Claude evidence boundaries

The existing [reasoning lifecycle investigation](reasoning-display-lifecycle.md)
already pins the same versions and explains the critical distinctions:

- Codex live summary headings update the status; committed reasoning-body
  visibility follows different rules. Plain unheaded summaries can be
  transcript-only. A headed body can be visible, dim/italic, with its first
  heading removed. Many bundled model defaults request no summaries.
- Claude narration-tagged thinking has a distinct renderer from ordinary
  thinking. Progress narration must not be inferred from paragraph length,
  first-line wording, or the existence of a thinking block.

The Claude binary hash was independently rechecked for this task. Readable
embedded references occur at byte offset `180359106`
(`AssistantNarrationSummaryMessage` import) and `160866187`
(`transcript:toggleShowAll` default binding). These identify inspected code
evidence, not screen coordinates. A stray `paddingLeft` or `marginTop` match
elsewhere in the bundled executable is insufficient to attribute spacing to
a transcript state, so no global Claude spacing constants are asserted here.

The prior [Claude grouping investigation](claude-code-tool-grouping.md) documents
read/search grouping and consecutive shell summaries from first-party evidence.
Its blanket Ctrl+E description needs the classic/fullscreen distinction above.
Its proposed Ferrite mockup is a recommendation, **not a captured CLI frame**.

## What this reference does not establish

No exhaustive state inventory, default theme for every installation, timing
trace, exact Claude cell geometry, original-terminal font rasterisation,
approval screenshots, error auto-expansion, or disclosure persistence is
claimed. Source-backed expectations and actual captured frames should remain
separate in the state catalogue. Each measured claim should cite its capture
frame and manifest; each unexercised branch should remain an explicit gap.

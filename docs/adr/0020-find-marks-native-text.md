---
status: proposed
date: 2026-10-15
---

# Find marks its occurrences inside native text, named by ordinal

## Context

⌘F has to find text in the focused Pane's transcript (and in its reader):
mark every occurrence, step through them, and bring the one it is on into
view — including occurrences in rows the virtualised list
([ADR 0006](0006-retained-transcript-rendering.md)) has never laid out.

The transcript's text is native `TextView`s (Markdown answers and literal
tool rows). Ferrite never sees glyph positions, so it cannot paint a mark or
say where an occurrence is on screen. It does already hold every row's text
without mounting it: the selection document's members
(`TextRuns::capture_members`), one per native text, keyed by its id. That
plain text is also exactly what copy reads.

## Decision

1. **Search the members, not the screen.** The bar searches each member's
   text as rendered, in row order — the plain text copy reads, but with
   every source run (a paragraph, a code block, a table cell) on a line of
   its own (`RunText::find_text`, `TextView::markdown_find_text`), and a
   query is one line, so no hit spans two runs. A hit is a member and an
   ordinal among that member's occurrences. What is folded (a call's
   output) is not a member, so it is not found — find searches what is
   shown. Output past `OUTPUT_INLINE_BYTES` is shown in a bounded viewer
   that owns its selection, so it is no copy member, but it is a find
   unit all the same, its text as written.
2. **The view paints the marks (vendor patch, `TextView::find`).** Each
   `Inline` washes the needle's occurrences in its source run, counting them
   in paint order, which is document order — the order the member's plain
   text has them. The active occurrence is named by that ordinal, so the
   same `find_matches` over the same runs on both sides keeps the count and
   the paint agreeing. A run split by wraps or inline cards keeps one
   count; an occurrence in an inline file card's label washes the card.
   A fence the view draws as something else (a ` ```mermaid ` fence as
   its diagram, [ADR 0016](0016-mermaid-diagrams.md)) still counts its
   occurrences where it paints, unwashed, so the runs after it keep their
   ordinals; on one of them, the diagram is what the find reports. The
   output viewer is the kit's input, not a `TextView`; a second vendor
   hook (`InputBaseState::set_find`) marks the same needle there in the
   same washes and reports the same way.
3. **The active occurrence reports where it painted** (`FindReport`).
   Bringing a hit into view scrolls its row in (the list measures it), then,
   once its text has painted, places the occurrence itself a third of the
   way down, clear of the pinned band — so a hit deep inside a tall answer
   lands on screen too. The paint also reports the first occurrence at or
   below the top of the view (`ahead`). While the query is unchanged, the
   bar keeps its occurrence by its text's key, so rows trimmed or added
   ahead of it do not move it to another.
4. **Smart case, one bar.** A capital letter makes the query match case.
   The bar is the palette's input row as a small float; ⏎/⌘G step forward,
   ⇧⏎/⌘⇧G back, esc closes, all in the bar's `Find` context so they win
   their ties with send, Groups and interrupt — and so only while the bar
   holds the keyboard: with focus back in the Composer, esc interrupts
   as ever and the bar stays open until ⌘F returns to it. Fullscreen moves
   from ⌘F to ⌘⇧⏎ (iTerm's maximize-pane). The washes are orange
   (`FIND_WASH`, `FIND_ACTIVE_WASH`): a hue no state wears, so a mark
   never reads as yellow's "needs you" or the accent's selection.
5. **The reader.** ⌘F goes to the reader when it took the last press (a
   press hands the keyboard back to the Pane, so focus cannot say so) or
   holds the keyboard. A file's lines wash per line and scroll by line,
   and a line too long for the reader starts further in, after a `…`,
   while the find is on it past the edge; a Markdown document uses the
   same view marks. A new query starts on the
   first occurrence in view everywhere: the transcript by row, a file by
   line, and a Markdown document — one view, its layout unknown until it
   paints — by its paint's `ahead`, one frame after the query lands on the
   document's first. The bar closes with its document, however the reader
   closes. A compare is not searched.

## Consequences

- Vendor patches on `gpui-base` (documented in `vendor/README.md`): drop
  them when upstream `TextView` offers search highlighting and its input
  takes an outside find.
- Copy's plain text joins a table row's cells with a space; find's text
  puts each cell on its own line instead, so a query across two cells is no
  occurrence on either side. A query pasted with a line break searches its
  first line.
- A new query in a Markdown reader can read `1/N` for one frame before it
  moves to the first occurrence in view.
- Retained-history trimming bounds what can be found: rows the Pane no
  longer holds are not searched. When the trimmed rows held the bar's own
  occurrence, it falls back to the hit now at its index.
- An occurrence inside a diagram is counted but never washed; stepping
  onto it brings the diagram into view.

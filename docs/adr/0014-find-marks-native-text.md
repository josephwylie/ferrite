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
   plain text (`RunText::plain`, the text the view renders), in row order.
   A hit is a member and an ordinal among that member's occurrences. What is
   folded (a call's output) is not a member, so it is not found — find
   searches what is shown, as copy copies it.
2. **The view paints the marks (vendor patch, `TextView::find`).** Each
   `Inline` washes the needle's occurrences in its source run, counting them
   in paint order, which is document order — the order the member's plain
   text has them. The active occurrence is named by that ordinal, so the
   same `find_matches` on both sides keeps the count and the paint agreeing.
   A run split by wraps or inline cards keeps one count.
3. **The active occurrence reports where it painted.** Bringing a hit into
   view scrolls its row in (the list measures it), then, once its text has
   painted, places the occurrence itself a third of the way down, clear of
   the pinned band — so a hit deep inside a tall answer lands on screen too.
4. **Smart case, one bar.** A capital letter makes the query match case.
   The bar is the palette's input row as a small float; ⏎/⌘G step forward,
   ⇧⏎/⌘⇧G back, esc closes, all in the bar's `Find` context so they win
   their ties with send, Groups and interrupt. Fullscreen moves from ⌘F to
   ⌘⇧⏎ (iTerm's maximize-pane).
5. **The reader.** ⌘F goes to the reader when it took the last press (a
   press hands the keyboard back to the Pane, so focus cannot say so) or
   holds the keyboard. A file's lines wash per line and scroll by line; a
   Markdown document uses the same view marks. A compare is not searched.

## Consequences

- One more vendor patch on `gpui-base` (documented in `vendor/README.md`);
  drop it when upstream `TextView` offers search highlighting.
- The plain text joins a table row's cells with a space, so a query that
  spans two cells counts there but no run paints it, and a later occurrence
  in that answer reads as the active one. One inside an inline file card's
  label counts but is not washed. Both are rare enough to leave.
- Retained-history trimming bounds what can be found: rows the Pane no
  longer holds are not searched.

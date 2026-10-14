---
status: proposed
date: 2026-10-15
---

# The Review slot: a commentable diff beside the Thread

The Herdr benchmark (`docs/research/herdr-benchmark-2026-10-15.md`, "Features
taken from plugins") asks for herdr-reviewr's review: a diff beside the
Thread, scoped to the uncommitted work, the branch or the last turn, whose
line comments become prompt context. Ferrite already had a compare (⌘⇧D):
the branch against `main`, read-only, in the reader slot.

## Decision

1. **The compare becomes the Review; one slot, no new surface.** The
   Review is a view (`review.rs`, `ReviewView`) that takes the reader slot
   beside its Pane (`attachment_preview::Document::review`), laid out,
   dragged and closed like any reader (F-14). ⌘⇧D opens it at the branch
   scope; the palette's `review changes` and the files-changed card's
   `review` open it at the uncommitted scope. A Review already open beside
   the Pane switches scope rather than opening another.
2. **Three scopes, read by the cockpit.** *Uncommitted* is `git diff HEAD`
   plus every untracked, unignored file as all-new (`git diff --no-index
   /dev/null <file>`); *branch* is `git diff <base>...HEAD`, the base being
   the Project's default branch (else `main`). Both read off the UI thread,
   a read that lands after the operator moved on is dropped. *Last turn*
   reads the diffs Main's tool results already carry after the latest
   prompt (`ferrite_core::review::last_turn`), one entry per edit and never
   merged, because each edit numbers the file as it stood then. The Review
   only draws; the cockpit reads (`cockpit/beside.rs`).
3. **Unified rows, every line selectable.** The Review draws its own
   unified rows (the transcript's signs, washes and syntax colours) so each
   diff line is a pick target: click or ↑↓ picks, shift-click or ⇧↑↓
   stretches the pick within one file. The split layout the compare had on
   wide slots goes: a comment needs one line to hang under.
4. **Comments live on the Composer.** `c`, ⏎ or `+ comment` opens a field
   under the pick; ⏎ adds the comment to the Pane's Composer, where the
   comments wait as one attachment chip (`N review comments`, × drops them)
   and hang under their lines in the Review. They are not persisted apart
   from the prompt: like files, they become prompt text at the send seam
   (`Composer::prompt` → `review::attach`), after the operator's words —
   `Review comments on your changes:`, then per comment its location
   (`path:12-14`, old-side numbers marked `(removed)`), its lines in a diff
   fence and what was said. Every Provider reads the same text; queueing,
   history and handover need nothing new.
5. **Keys in their own context.** The Review's keys are bound in `Review
   && !Composer` (never while its field takes text), the field's ⏎/esc in
   `ReviewComment`, all after the bare rows they shadow. They are listed in
   the slot's footer, not on the shortcuts sheet. The cockpit's focus pump
   leaves the keyboard on the Review (or its field) from the frame it takes
   it.

## Consequences

- Comments survive closing the Review and switching scope, and go with the
  next send; a recalled prompt shows them as the text that was sent.
- A large branch diff renders every row (as the compare did); virtualising
  would need uniform rows, which the inline comments break.
- Last turn sees Main's edits only; a Subagent's edits show under the
  uncommitted scope.

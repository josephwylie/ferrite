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
   dragged and closed like any reader (F-14). ⌘⇧D (`review branch`, in the
   palette and the menu — no longer `compare with main`, since the base is
   the Project's default branch) opens it at the branch scope; the
   palette's `review changes` and the files-changed card's `review` open it
   at the uncommitted scope. A Review already open beside
   the Pane switches scope rather than opening another.
2. **Four scopes; git runs where the checkout is.** *Uncommitted* is `git
   diff HEAD` plus every untracked, unignored file as all-new (`git diff
   --no-index /dev/null <file>`); *branch* is `git diff <base>...HEAD`, the
   base being the repo's default branch (`workspace::default_branch`, else
   `main`), or `origin/<base>` when the checkout has no local one;
   *commits* (herdr-reviewr's fourth) lists the branch's commits since that
   base, newest first (at most 500): ↑↓ or a click picks one, ⏎ or the
   click reviews `git show --diff-merges=first-parent <id>`, and the scope
   again (or `r`) goes back to the list. These reads are
   `ferrite_core::review::git` and run on the computer that has the code:
   the Server that owns the Thread answers `review_diff {path, scope,
   commit?}` with git's diff text (`diff {text}`) and `list_commits {path}`
   with `commits {commits[{id, subject}]}`, gen-1 additions (ADR 0025), off
   its pump as `list_repo` is. So a Thread on a machine (ADR 0026) reviews
   the machine's checkout, and a window on this computer's Server asks it
   the same way; only a window that runs its agents itself reads git in
   its own process, on a thread of its own. The window only parses and
   draws. A diff past 8 MB is refused ("too large to review here") rather
   than carried or drawn; a path that is no repo, or a machine with no
   git, says so in git's words (or "git is not installed") in place of the
   diff, as any refused read does. A file with no hunks (a binary one) is
   left out, so the bar counts only what the rows show; each untracked
   file costs one `git diff` process. An older Server answers
   `unsupported`, and the slot shows that reason. *Last turn* reads the
   diffs Main's tool results already carry after the latest prompt
   (`ferrite_core::review::last_turn`), one entry per edit and never
   merged, because each edit numbers the file as it stood then. A read
   that lands after the operator moved on is dropped. Each read is a
   snapshot: it does not follow the agent's further edits. `r`, or picking
   the scope again, reads it afresh.
3. **Unified rows, every line selectable.** The Review draws its own
   unified rows (the transcript's signs, washes and syntax colours) so each
   diff line is a pick target: click or ↑↓ picks, shift-click, a drag
   across rows or ⇧↑↓ stretches the pick within one file (a drag past the
   file's last line holds there; the slot does not scroll under a drag
   held past its edge). A right-click opens the app's context menu on the
   pick (keeping a range that holds the line, else picking it): *Comment*,
   *Open file at this line* (the reader, at the line's new number or a
   removed line's old one; disabled on a machine, whose files the reader
   cannot read) and *Copy* (the lines' text, without signs or numbers).
   The split layout the compare had on
   wide slots goes: a comment needs one line to hang under. The rows read
   at the operator's reading size, as the compare did (and every diff in a
   transcript does); the bar and footer are chrome, at the UI size.
4. **Comments live on the Composer.** `c`, ⏎ or `+ comment` opens a field
   under the pick; ⏎ (or the field's `↵ add`; esc or `esc cancel` drops
   it) adds the comment to the Pane's Composer, where the comments wait as
   one attachment chip (`N review comments`, × drops them all) and hang
   under the selection's last line in the Review. One comment comes back
   off the shelf by its own × there, or ⌫ on its line (the latest first);
   both remove by place on the shelf, so a twin comment stays. They are
   not persisted apart from the prompt: like files, they become prompt
   text at the send seam
   (`Composer::prompt` → `review::attach`), after the operator's words —
   `Review comments on your changes:`, then per comment its location
   (`path:12-14`, old-side numbers marked `(removed)`), its lines in a diff
   fence and what was said. Every Provider reads the same text; queueing,
   history and handover need nothing new. Round trips that put text back
   on the line (history recall, a held prompt merged back) read only the
   line and its files (`Composer::editable`), so the comments stay on the
   shelf, once, for whichever prompt goes next.
5. **Keys in their own context.** The Review's keys are bound in `Review
   && !Composer` (never while its field takes text), the field's ⏎/esc in
   `ReviewComment`, all after the bare rows they shadow. They are listed in
   the slot's footer, not on the shortcuts sheet. The cockpit's focus pump
   leaves the keyboard on the Review (or its field) from the frame it takes
   it.

## Consequences

- Comments survive closing the Review and switching scope, and go with the
  next send; a recalled prompt shows them as the text that was sent.
- A large branch diff (up to 8 MB) renders every row (as the compare
  did); virtualising would need uniform rows, which the inline comments break. Each row's
  syntax colours are worked out once per read, not per frame.
- A comment holds a path, a numbered range, where its last line sits (new
  number, else old for a removed line) and the quoted lines, not the scope
  or edit it was made in. It hangs under any row of that path whose place
  and text match its last line — in another scope, or under another
  edit of the same file in the last turn, the same line can show it too.
- Last turn sees Main's edits only; a Subagent's edits show under the
  uncommitted scope.

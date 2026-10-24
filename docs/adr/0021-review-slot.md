---
status: proposed
date: 2026-10-15
---

# The Review slot: herdr-reviewr's review, vendored, beside the Thread

The Herdr benchmark (`docs/research/herdr-benchmark-2026-10-15.md`, "Features
taken from plugins") asks for herdr-reviewr's review: a diff beside the
Thread, scoped to the uncommitted work, the branch or the last turn, whose
line comments become prompt context. Ferrite already had a compare (⌘⇧D):
the branch against `main`, read-only, in the reader slot. A first Review
re-wrote reviewr's scopes, diff and comments by hand; the operator chose to
take reviewr itself instead, so its fixes and features keep arriving.

## Decision

1. **herdr-reviewr is vendored, not re-written.** Its review core lives in
   `crates/ferrite-reviewr`: upstream's source verbatim (MIT, its edition,
   MSRV and lints; the commit it was taken at and every change in its
   README), so an upstream release merges in with small diffs. Kept: the
   git scopes (`git`, `model`), the diff model (`diff`, `highlight`,
   `text`), the comments and the text they are sent as (`model`,
   `export`), `fff-search` (`search`), the read-only forge reads (`forge`,
   `gitlab`, `azure_devops`, `snippet`) and their helpers (`proc`, `log`,
   `keymap`). Dropped: its terminal UI (ratatui, crossterm, its theming and
   frame loop), its plugin config and herdr adapter. Three small modules
   stand in for dropped ones the kept ones call (`config`, `theme`,
   `wake`); `ferrite.rs` replaces the herdr adapter: what the Server's
   commands and the window call. Ferrite's own scopes, diff parser,
   selection and comment model (`ferrite_core::review`) are gone.
2. **The compare is the Review; one slot, no new surface.** The Review is
   a GPUI view (`review.rs`, `ReviewView`) that takes the reader slot
   beside its Pane (`attachment_preview::Body::Review`), laid out,
   dragged and closed like any reader (F-14). ⌘⇧D (`review branch`, in the
   palette and the menu) opens it at the branch scope; the palette's
   `review changes` and the files-changed card's `review` open it at the
   uncommitted scope. A Review already open beside the Pane switches scope
   rather than opening another.
3. **Four scopes, as reviewr reads them, where the checkout is.**
   *Uncommitted* is the worktree against `HEAD`, untracked files included;
   *branch* is the worktree against its merge base with the resolved base
   (reviewr's chain: its per-worktree base pick, else the default branch —
   `origin/HEAD`, `init.defaultBranch`, `main`, `master`); *last turn* is
   the worktree against reviewr's snapshot of it when the latest turn
   that changed it began; *commits* lists the branch's first-parent commits since that
   base, newest first: ↑↓ or a click picks one, ⇧↑↓ or a shift-click
   stretches the pick to a run of them (reviewr's commit picker run), ⏎ or
   the click reviews the pick (the oldest's first parent against the
   newest), and the scope again (or `r`) goes back to the list. Each file comes with both its sides (one full-context `git
   diff`), and the window builds its rows with reviewr's model. These reads
   run on the computer that has the code: the Server that owns the Thread
   answers `review_read {path, scope, commit?, since?}` (scopes
   `uncommitted`, `branch`, `commit` and `last_turn`; `since` the oldest
   commit of a run, absent for one commit) with `review {files[{path,
   change, previous_path?, additions, deletions, old?, new?, notice?}]}` and
   `list_commits {path}` with `commits {commits[{id, subject, author?,
   time?}]}`, gen-1 additions (ADR 0025), off its pump as `list_repo` is.
   So a Thread on a machine (ADR 0026) reviews the machine's checkout; only
   a window that runs its agents itself reads in its own process, on a
   thread of its own. A file over reviewr's budget (2 MB, 50 000 lines) or
   past 8 MB of carried text in all, a binary one, or one that fails to
   read comes as its notice instead of its sides. A path that is no repo
   says so. `review_diff {path, scope, commit?}` keeps its gen-1 meaning
   for a window from before `review_read`: the Server answers it `diff
   {text}`, the same read's sides as a git-style unified diff. Against a
   Server from before `review_read`, which answers it `unsupported`, the
   window asks `review_diff` instead and reads the diff's hunks as the
   sides, the lines far from a change left blank so every line keeps its
   number; a last turn or a run of commits that Server cannot read is
   refused (update it), never shown as one commit or as something else. A read
   that lands after the operator moved on is dropped; each read is a
   snapshot, read afresh on `r` or on picking the scope again.
4. **The last turn is reviewr's turn tracking, fed Ferrite's agents.**
   Upstream's `turn.rs` is vendored whole; where herdr fed it each agent's
   status, the Engine feeds it every live Session's checkout and status
   (its `ferrite::TurnSource`, on each pump that changed them: streaming
   is `working`, blocked `blocked`, idle `idle`, closed `done`). Each
   checkout an agent works in gets reviewr's `TurnHost` on a thread of its
   own, reviewed or not. A turn is reviewr's: the checkout going from
   every agent at rest to one working. Its start snapshots the worktree
   (`git add -A` into a private index copy, `write-tree`); when the
   checkout rests again, the turn is promoted to *last turn* only if the
   checkout changed meanwhile, under reviewr's worktree-private ref
   `refs/worktree/reviewr/turn-base`. So a turn that wrote nothing (a
   question) leaves the turn before it on show, and the last turn holds
   every change since that start — Main's, its Subagents', their shell's,
   and another Thread's in the same checkout. Ferrite runs no watcher, so
   each turn is judged at its end, as reviewr judges a turn its watcher
   was down for: while a turn runs, *last turn* still reads from the start
   of the one before. The ref persists, as reviewr's does, so after a
   restart the last turn is still the one Ferrite (or reviewr, sharing the
   ref) last saw; every turn of every Session is seen, from the moment the
   Engine starts it, so the first turn that writes replaces it. Before any
   turn, the scope says it waits for the first. These, the base pick (⇧B,
   point 7) and reviewr's index copies under the git dir (gone when the
   Engine halts, `git::end_sessions`) are the only writes; the real index
   and the worktree are never touched.
5. **Rows as reviewr builds them, drawn as Ferrite draws diffs.** Each
   file is a head (its path, a rename's `old → new`, its `+N −N`) over its
   rows: numbered on both sides, signed and washed like every diff in a
   transcript, in Ferrite's syntax colours, the words reviewr pairs as
   edited washed deeper (`DIFF_*_WORD`), and long unchanged runs folded
   (`⋯ N lines`; a click, or ⏎ on it, opens it in place). A click or ↑↓
   picks a row; shift-click, a drag across rows or ⇧↑↓ stretches the pick
   within one file (a drag past the file's last row holds there). A
   right-click opens the app's context menu on the pick (keeping a range
   that holds the row, else picking it): *Comment*, *Open file at this
   line* (the reader, at the row's new number or a removed row's old one;
   disabled on a machine) and *Copy* (the rows' text, without signs or
   numbers). The rows read at the operator's reading size; the bar and
   footer are chrome, at the UI size.
6. **Comments are reviewr's, and wait on the Composer.** `c`, ⏎ or `+
   comment` opens a field under the pick; ⏎ (or `↵ add`; esc or `esc
   cancel` drops it) makes reviewr's comment — the file, the side and
   range its rows anchor to (new side unless every row was removed), the
   rows as marked diff lines, the words, whether it was made on a diff or
   a whole file, and the commit it was read at — and puts it on the Pane's
   Composer in reviewr's comment store, where the comments wait as one
   attachment chip (`N review comments`, × drops them all) and hang under
   the range's last row wherever that diff (or file) shows again. One
   comment comes back off the shelf by its own × there, or ⌫ on its row
   (the latest first), by its id, which is never reused. They become
   prompt text at the send seam (`Composer::prompt` →
   `ferrite::attach`): the operator's words, a blank line, then reviewr's
   export (`export::format_all`): per comment, sorted by file and line,
   its location (`path:12-14`, `(removed)` for old-side lines), its marked
   rows and what was said, a blank line between. Every Provider reads the
   same text; queueing, history and handover need nothing new. Round
   trips that put text back on the line (history recall, a held prompt
   merged back) read only the line and its files (`Composer::editable`).
7. **What reviewr has besides, in the same slot.** `/` opens a search
   over the slot: reviewr's `fff-search` engine, one per checkout on the
   Server (started on the first search, rescanning when it sat idle, its
   frecency store in Ferrite's cache), asked by `search_repo {path,
   query}` and answered `found {files[{path, spans}], code[{path, line,
   text, spans}], file_total, code_more}`; ↑↓ walk the results, ⏎ or a
   click opens one. A result opens its file whole (`read_file {path,
   file}`, answered `text`, inside the checkout only, a symlink pointing out
   of it refused too): every line a
   context row that takes comments as a diff's do, a found line picked;
   esc goes back to the diff. ⌘F finds in the rows shown (the
   transcript's bar, `crate::find`). `m` renders each Markdown file's new
   side with Ferrite's Markdown, in place of its rows. `p`, or the bar's
   `PR`, shows the branch's pull request as reviewr reads it with the
   forge's own CLI (`gh`, `glab`, `az`; read-only):
   `review_pull_request {path}` answers `review_pull_request {noun, pr?,
   note?}` — its number, title, state, branches, mergeability, how the
   checkout stands against it, its checks and its conversation — or
   reviewr's note in its place (no remote, no CLI, not signed in, no PR
   yet). A self-hosted forge is named in the settings' `forge_hosts`
   (`github`, `gitlab`, `azure_devops`: reviewr's `*_host` keys, checked
   as reviewr checks them; one it refuses drops them all, and the log says
   why); the Server reads its own settings file, a window that runs its
   agents itself the window's. A Thread row's PR label reads `gh` on its
   own (`pull_request`, ADR 0026). `a` lists every file of the checkout as
   reviewr's tree (`file_list`, vendored: directories collapsed, first,
   single-child chains joined; the scope's changed files in the accent and
   a closed directory holding one marked `•`; an ignored directory one
   quiet row, not opened): `list_files {path}` answers `all_files
   {entries[{path, ignored?, dir?}]}`; ↑↓ walk it, ⏎ or a click opens a
   directory or a file whole. ⇧B is reviewr's base picker: `list_bases
   {path}` answers `bases {bases[{name, default?, current?, tip, oid?}],
   picked?}` — the branches, local and `origin` merged, the default first,
   then the newest, and a revision the pick names — under a field that
   filters them as reviewr does (`neo_frizbee`); ⏎ or a click picks one,
   or, when none is kept, what the field reads (any revision git
   resolves), with `pick_base {path, base}` (answered `changed`), which
   writes reviewr's per-worktree pick (`refs/worktree/reviewr/base-pick`;
   the default branch clears it), and the branch scope is read over it.
   Reviewr's promotion of an open PR's target to the top is not taken: it
   would cost a forge read per open. `r` reads again what shows; esc goes
   back to the diff.
8. **Keys in their own context.** The Review's keys are bound in `Review
   && !Composer` (never while a field of it takes text), the comment
   field's ⏎/esc in `ReviewComment`, the search field's ↑↓/⏎/esc in
   `ReviewSearch`, all after the bare rows they shadow. They are listed in
   the slot's footer, not on the shortcuts sheet. The cockpit's focus pump
   leaves the keyboard on the Review (or a field of it) from the frame it
   takes it.

## Consequences

- Ferrite carries a crate it did not write: an upstream update is merged
  into `crates/ferrite-reviewr` by its README's steps, and its tests run in
  CI (`cargo test -p ferrite-reviewr`). The vendored engine brings
  `fff-search` (with libgit2 and LMDB, built from C), `syntect` with
  `two-face`, `similar` and `process-wrap`.
- Comments survive closing the Review and switching scope, and go with the
  next send; a recalled prompt shows them as the text that was sent. A
  comment made on one commit (or run) shows only on that commit (or run); one made on the
  worktree shows wherever the worktree's diff of that file has its row.
- Every file of a scope is read with its sides at once, and the slot draws
  every row (folds aside): virtualising would need uniform rows, which the
  inline comments break. Each row's colours are worked out once per read.
- Each turn costs a `git add -A` into a copy of the index at its start
  and its end, which hashes only what changed since `HEAD`, in every
  checkout a Session works in, off the pump. A write that lands before the
  start's snapshot (a fast first edit) folds into the baseline: with no
  watcher, reviewr cannot tell it raced.
- The search engine holds one index per searched checkout for as long as
  the Server runs.
- An ignored directory in the all-files list does not open: reviewr
  lists its entries on demand (`git::all_files_in`), which would need a
  read per directory.

## Amendment (2026-10-21, UI refresh phase 7a)

The status line's changed-files card and its `review` verb are gone. The
Solo panel's Changes tab (SPEC §5.6) lists the Thread's edited files and
ends in `Review changes`, which opens `Beside::Review(Uncommitted)` beside
the Pane as the card's verb did; the panel gives way while the Review is
open.

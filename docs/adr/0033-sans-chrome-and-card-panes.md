---
status: accepted
date: 2026-10-21
---

# Sans chrome, card Panes, and colour only for state

Follows the UI refresh design canvas (`docs/design/ui-refresh/SPEC.md`,
mockups beside it).

## Context

The terminal-native chrome (`theme.rs` rules 1–7, ADR 0014) made every
surface one monospace grid, square and flat, with steel blue as the
operator's colour. The transcript reads well under those rules.
Everything around it reads dated and dense, and in the operator's words
it "lacks polish":
- the sidebar's tree of one-line rows;
- the centred tab strip with `0/0`;
- the bar that carries mode, model, `ctx`, usage and clock;
- outlined keycaps and boxed bands.

Four reference apps the operator chose (Paseo, Termio, Orca, MonoCat)
share one pattern. A darker terminal card sits on a lighter frame. Chrome
is in a proportional face with mono kept for the terminal. List rows have
two lines with diffstats. A Composer box holds the mode, model and effort
controls. A thin bar along the bottom shows usage.

## Decision

- **Two faces.** Geist (already bundled) sets the chrome: nav, heads,
  tabs, buttons, menus, the palette, the Composer's input, the panel and
  the bar. Geist Mono sets the transcript and any code or numbers inside
  chrome (diffstats, spinners). This supersedes `theme.rs` rule 1's "one
  face, one grid" for chrome. The transcript keeps its grid, its reading
  size and its 1.5× line. Chrome metrics are pixels on the space scale,
  not cells.
- **Cards on a frame.** The Pane is a card: `CARD` #181818, radius 10, a
  1px white ring at 7%. It sits on the `FRAME` #222 that the nav, heads,
  panel and Group gutter share. Radii run 4–12 by role
  (`SPEC.md` §3.6). This supersedes rule 2 ("every in-app radius is 0",
  "no card, no pill"). Separation is still a 1px line or a fill step. There
  are still no drop shadows except on floats. A Group's Panes are separate
  cards with an 8px gutter. The seams still resize the Layout; a seam is
  the gutter's centre line.
- **No accent hue.** `ACCENT` steel blue is retired. The prompt `❯`,
  caret, focus and a selected choice use `TEXT_STRONG`, and selection is
  a lighter fill. Hue is for state only: green working, yellow needs you,
  red failed. Provider colours stay on logos and working glyphs. Done is
  the `circle-check` shape in a muted ink, not a teal hue. This amends
  ADR 0015's faces: `FINISHED` is no longer a row colour. Inline code and
  paths lose `CYAN`. Inline code is a `RAISED` chip in `TEXT_STRONG`, and
  tool arguments are `TEXT_DIM`.
- **Two-line nav rows.** A Thread row is 44px:
  - line 1 is the title, with the worktree's diffstat at the right (from
    the Digest's changed files);
  - line 2 is the branch and elapsed time, or `needs you · <reason>`, or
    `failed · <age>`.

  A row may now spend two hues: its state and the diffstat's `+`/`−`. This
  bends rule 5's "one colour" for that number pair only.
- **The nav opens with its views.** `Search` (the palette, its chord read
  from the keymap), `Needs you` (blocked Threads oldest first, then the
  bell's Notices, under a count), `Parked`, and `Tasks` when Projects is
  live. This reverses ADR 0014's removal of the `⌘K` door, for the nav
  only. The Needs-you *view* is not the strip ADR 0015 removed: the tree
  stays free of duplicated rows.
- **The Composer holds the session controls.** The permission mode,
  model and effort are chips in the Composer's toolbar, with a Draft's
  setup chips beside them. This supersedes ADR 0014's "focused Pane's
  status line" in the bar. There is still one bar, and nothing per Pane
  stacks on it. A Group Pane's compact Composer shows the input only.
  Chip labels are sentence case, which amends rule 11's "values
  lowercase" for chips.
- **A right panel for Solo.** `Changes` (branch, files, hunks), `PR` and
  `Context` (the `ctx` breakdown). `Changes` and `Context` replace the
  bar's changed-files and `ctx` popovers. `PR` is a read-only view of the
  branch's pull request, fed by the Review's existing forge snapshot:
  - state, title and branches;
  - merge readiness (checks, conflicts, unpushed commits);
  - the checks;
  - review comments and findings, unresolved first;
  - a door to the Review's PR scope. The panel is a fixed column outside the board, toggled,
  and its state is persisted. It is never shown on a Group or beside an
  open Beside slot. The Review (ADR 0021) still takes the reader slot,
  and the panel's `Review changes` opens it there.
- **The subagent strip shows live work.** The strip shows `Main`, one tab
  per running Subagent, and an `N done` menu for the rest. Finished
  Subagents leave the strip. This amends ADR 0002's overflow rule.
- **The bar is views and vitals.** Left: the view tabs with a roll-up dot.
  Right: `N need you`, weekly usage per provider, free disk when low, and
  the clock. The other items move:
  - the sidebar toggle goes to the nav head;
  - the bell goes to `Needs you`;
  - the board door goes to `Tasks`;
  - the gear and `ferrite` go to the nav foot;
  - `?` goes to the palette and the app menu.

  This supersedes `theme.rs` rule 7's "the bottom bar shows no counts".
  ⌘1–9 still focus the shown board's Panes. Views switch from the bar,
  ⌘G or the nav.
- **Solo gets a head.** The Thread title, its Project, and a branch chip
  with the agent-edit diffstat. There is no PR button: the pull request
  lives in the panel.
- **Group Pane heads speak.** A Group Pane's head shows the state glyph,
  the title, a state word (elapsed time, `needs you`, `failed`, or
  `done` while unseen), the provider logo, and a button that fills the
  board with it (`ToggleFullscreen`). A blocked Pane's ring is yellow at 35%. The focused Pane's ring
  is white at 22%. This supersedes rule 7's "a Pane head carries no state
  word" for Group Panes.

## Considered options

- **Keep mono everywhere and only round the corners.** This was tried on
  the canvas, and the operator rejected it as "tiny UI edits". The dated
  feel comes mostly from mono chrome.
- **A chat-style app (bubbles, sans transcript).** This was tried first,
  and the operator rejected it: it lost the terminal vibe that is
  Ferrite's identity.
- **An Agents tab in the right panel.** It duplicated the subagent strip.
  The strip keeps live Subagents, and its `N done` menu keeps the rest.
- **A `Create PR` button in the Solo head.** The operator declined it:
  pull requests are opened on GitHub. Ferrite shows them, in the PR tab.

## Consequences

- `theme.rs`'s module doc is rewritten to these rules. It stays the one
  place the rules live, and `SPEC.md` is the build reference. The approved
  picture moves from `spikes/terminal-native/index.html` to
  `docs/design/ui-refresh/mockups/`. Update the parity scenes, or retire
  them for the chrome.
- `ACCENT`, `ON_ACCENT`, `CYAN` (as `INLINE_CODE`/`PATH_INK`), `MODE_INK`
  and `FINISHED` (as a row colour) are deleted or remapped. The `R_*`
  radii get values.
- The glyph-coverage test (rule 10) gains a Geist case for the chrome
  glyphs (`⌘ ⇧ ↑ ↓ ·`).
- The frame wakes for the bar's minute (clock, free disk) and the
  spinners, as before. Free disk is read off the UI thread on that minute.
- The Composer's block caret becomes a 2px bar, since a mono cell no
  longer sizes it.
- `GRID_GAP` and `BOARD_SEAM` become 8px. The seam's grab band sits in
  the gutter, and the Layout's split tree is unchanged.
- Supersedes `theme.rs` rules 1, 2, 6 and 7 as stated above. Amends ADR
  0014 (bar contents, `⌘K`, popovers), ADR 0015 (done face, the folded
  rail's width becomes 52px), ADR 0002 (strip overflow) and ADR 0021 (the
  panel opens the Review). It overrides ADR 0003 and 0007's "proportional
  prose stays": the transcript stays mono.

## Amendment (2026-10-22): the panel follows the focused Thread

The operator's decisions after the refresh merged:
- **The panel follows the focused Thread in every view.** It shows in
  Solo, in a fullscreen Pane, in a Group (the focused Pane's Thread;
  focusing another Pane switches it, and each Thread keeps its last tab)
  and with the Beside slot open. It does not show on a draft, the empty
  board or the Projects coordinator: there is no Thread. This reverses
  "never shown on a Group or beside an open Beside slot" above.
- **No size rules.** The width rules (show from about 1280px, hide under
  about 1180px, the session-only override) are gone. The panel shows
  exactly when the operator has it on (`panel_shown`, persisted, default
  on) and there is a Thread. The operator decides when they have lost
  too much room; the board narrows, the nav folds to its rail, and a
  Group may fall to wall tiles. `WINDOW_MIN` is unchanged.
- **One toggle.** A single `panel-right` icon button, the sidebar
  toggle's twin, sits at the head row's right end whether the panel is
  shown (the panel head's) or hidden (the view head's), in Solo and
  Group alike. `Toggle Panel` joins the View menu beside `Toggle
  Sidebar`, and `Toggle panel` the palette.
- **The branch chip is removed.** The Solo head is the title and the
  Project; the branch and its diffstat live in the panel's Changes tab.
  This amends "Solo gets a head" above.
- **Queued prompts in Groups.** The focused Group Pane's compact
  Composer gains Solo's queued row above its input, amending "A Group
  Pane's compact Composer shows the input only".

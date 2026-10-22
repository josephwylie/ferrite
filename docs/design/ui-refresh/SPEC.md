# UI refresh: sans chrome, card Panes, colour only for state

Build spec for the redesign drawn on the design canvas (October 2026),
written for the agent that implements it. Its decisions are recorded in
[ADR 0033](../../adr/0033-sans-chrome-and-card-panes.md) (accepted).

How to use this document:
- Read it once, top to bottom.
- Then work through [§11 Build order](#11-build-order), one phase per
  pull request.
- The mockups are the visual truth. This text is the behavioural truth.
  Where they disagree, this text wins; say so in the pull request.

| Mockup | Files | Size |
| --- | --- | --- |
| Solo view (one Thread), panel on its PR tab | [`mockups/solo.png`](mockups/solo.png) · [`.html`](mockups/solo.html) | 1600×960 |
| Solo view, panel on its Changes tab | [`mockups/solo-changes.png`](mockups/solo-changes.png) · [`.html`](mockups/solo-changes.html) | 1600×960 |
| PR tab states (failing, merged, no PR) | [`mockups/pr-states.png`](mockups/pr-states.png) · [`.html`](mockups/pr-states.html) | 1040×980 |
| Group view (four Panes) | [`mockups/group.png`](mockups/group.png) · [`.html`](mockups/group.html) | 1600×960 |
| Command search (palette) | [`mockups/command-search.png`](mockups/command-search.png) · [`.html`](mockups/command-search.html) | 800×560 |
| Before: today's UI, for comparison | [`mockups/before.png`](mockups/before.png) · [`.html`](mockups/before.html) | 1600×960 |

The `.html` files are static exports. Open one in a browser at its size
and use the inspector to read any value this spec leaves out. Their
content (titles, diffs, numbers, model names) is illustrative. The app's
own data fills every slot, and §8 lists the slots that have no data
source yet.

Words follow `CONTEXT.md`: Thread, Pane, Group, Solo, Composer, Decision,
Subagent, Main, Notice, Review, Digest. A Pane's top row is its **head**
(never "header").

---

## 1. What changes

Ferrite keeps its terminal vibe:
- dark greys;
- a monospace transcript;
- the `❯ ● └ ✻` grammar;
- braille spinners;
- state inks.

Everything *around* the transcript becomes calmer and more modern:
- **Sans chrome.** Chrome switches to Geist, which is already bundled.
  The transcript stays in Geist Mono.
- **Cards, not bands.** Square full-bleed bands become rounded cards on a
  lighter frame. Separation is a 1px hairline.
- **No accent.** Steel blue is retired, and hue is kept for state only.
- **Two-line sidebar.** The nav becomes a two-line Thread list with
  branch, diffstat and state reason.
- **A real Pane head.** The Pane gains a head: title and Project in
  Solo; a tab strip of Main plus live Subagents.
- **Composer box.** The Composer becomes a rounded box that holds the
  permission mode, model and effort, so the bar no longer carries a
  status line.
- **Right panel.** A right panel shows the focused Thread's changes, its
  pull request (checks, merge state, review comments) and its context,
  in every view.
- **A calmer bar.** The bar keeps only views and vitals.
- **Group cards.** A Group is a grid of the same cards, each with a
  speaking head.

## 2. Principles

1. **Two faces, one rule.** Transcript content is Geist Mono. Everything
   else is Geist: nav, heads, tabs, buttons, menus, the palette, the
   Composer's input, the panel and the bar. Numbers and glyphs that come
   from the terminal world (diffstats, spinners, `❯`) stay in Geist Mono
   even inside chrome.
2. **No accent hue.** Focus, selection and `❯` use `TEXT_STRONG`
   (`#f2f2f2`). Selection is a lighter fill, never a coloured outline. Hue
   appears only for state:
   - green: working, `ok`, diff `+`;
   - yellow: needs you, risky mode;
   - red: failed, diff `−`.

   Provider colours stay on their logos and working glyphs.
3. **Cards on a frame.** The window frame is `FRAME` `#222`. The
   transcript lives in a darker card (`CARD` `#181818`, radius 10, a 1px
   ring at 7%). Only the bottom bar is full-bleed.
4. **Hairlines, not borders.** Separation is a 1px white line at 5–8%
   opacity, or a fill step. There are no visible grey borders, no keycap
   boxes and no boxed section labels.
5. **Two-line rows.** A row that stands for a Thread says what it is on
   line 1 (title, numbers right) and where it is on line 2 (branch, or the
   state's reason).
6. **Keys stay quiet.** A shortcut is plain `TEXT_DIM` text, and only
   where the operator is already looking: a menu, the palette, the
   `Search` row. Keep reading chords from the keymap, never typing them
   (theme rule 11, ADR 0024).
7. **Copy is short.** Follow `theme::words` and theme rule 11:
   - sentence case for labels;
   - lowercase state words;
   - `·` as the only separator.

   The Composer's placeholders are the existing short ones.

## 3. Tokens

Implement these in `theme.rs` (its shared head, through the integrator
process its layout section describes). The names are proposals: where an
existing token holds nearly the same value, re-value the existing token
rather than adding a new one. §10.1 maps old to new.

Values are the **opaque** greys. On macOS, rule 4 still applies: chrome
fills (`FRAME`, `BAR`) paint through `paint::*` at the glass alpha, and
`CARD` at the plane's 94%. Keep "never stack two region fills": the frame
and the card are separate regions. The main column paints no ground
under the card. The 8px gutter around a card is `FRAME`, painted by the
column as a region and clipped around the card, or painted as the card's
outer ring. Use whichever GPUI makes cheaper, but do not paint `CARD`
over `FRAME`.

### 3.1 Surfaces

| Token | Value | Today | Used for |
| --- | --- | --- | --- |
| `FRAME` | `#222222` | `CHROME` #242425 | Behind cards: nav, heads, right panel, Group gutter |
| `CARD` | `#181818` | `PLANE` #1b1b1c | Pane body (transcript surface) |
| `BAR` | `#1e1e1e` | `CHROME` | Bottom bar; diff and code-fence blocks |
| `FIELD` | `#222222` | `INBAND`, `BAND` | Composer box (Solo); prompt band |
| `FIELD_COMPACT` | `#202020` | `INBAND` | Composer in a Group Pane |
| `RAISED` | `#2a2a2a` | `BAND2` #363638 | Chips, inline code, secondary buttons, panel file block, hover fill on `FRAME` |
| `TAB_ACTIVE` | `#262626` | `SELECTION` | Active tab inside a card |
| `SELECTED` | `#2e2e2e` | `SELECTION` #343437 | Selected nav row, active view tab, active panel tab, cursor row in menus, Decision option and palette |
| `DECISION` | `#1f1f1f` | none | Decision box |
| `SUNKEN` | `#161616` | `BAND` | Command well in a Decision; palette scrim base |
| `SEND` | `#3a3a3a` | none | Send button |
| `PRIMARY` / `ON_PRIMARY` | `#f2f2f2` / `#161616` | `ACCENT` / `ON_ACCENT` | Primary buttons in dialogs and settings (`primary_button`) |
| `FLOAT` | keep `#2a2a2c` | `FLOAT` | Menus, popovers: now radius 8 with a `HAIRLINE_STRONG` ring |

### 3.2 Hairlines (white at an opacity, over whatever is below)

| Token | Value | Used for |
| --- | --- | --- |
| `HAIRLINE` | `rgba(255,255,255,.06)` | Dividers in cards and panels: tab strip foot, queued-row foot, panel sections |
| `CARD_RING` | `.07` | 1px ring around a card |
| `FIELD_RING` | `.08` | 1px ring around the Composer |
| `HAIRLINE_STRONG` | `.10` | Float edge |
| `ROW_RING` | `.06` | Inset ring on the selected nav row |
| `BAR_TOP` | `.05` | Top edge of the bottom bar |
| `FOCUS_RING` | `.22` | Focused Pane on a Group (was `ACCENT`) |
| `WAIT_RING` | `rgba(230,196,124,.35)` | Pane whose Thread is blocked (was `ATTENTION_EDGE` at 75%) |

### 3.3 Inks

Keep the ladder and its contrast floors (`theme::tests`). New values:

| Token | Value | Today | Used for |
| --- | --- | --- | --- |
| `TEXT_STRONG` | `#f2f2f2` | #f4f4f5 | Titles, selected rows, prompt text, `**strong**`, `❯`, caret, focus |
| `TEXT_PROSE` | `#e6e6e6` | `TEXT` | Assistant prose |
| `TEXT` | `#d0d0d0` | #d7d7d9 | Default chrome text, nav titles, tool output |
| `TEXT_BAR` | `#c0c0c0` | `CHROME_MUTED` | Emphasised bar items |
| `TEXT_MUTED` | `#a0a0a0` | #98989d | Unselected tabs and nav views, chips |
| `TEXT_DIM` | `#8c8c8c` | `TEXT_MUTED` | Metadata, times, icons, section headings, tool arguments. **This is the floor for readable text:** check 4.5:1 on `FRAME` and `CARD`, and 4:1 on `SELECTED`. |
| `TEXT_FAINT` | `#5e5e5e` | #5e5e63 | Structure only: `└`, diff line numbers. Never text. |

### 3.4 State and brand

| Token | Value | Tints |
| --- | --- | --- |
| `RUNNING` (green) | keep `#93cf8c` (mockup `#8cd398`; either) | Diff add row 9%, add line number `#5f8a66`, badge fill 16% |
| `ATTENTION` (yellow) | `#e6c47c` | Badge fill 16%, dot halo 15% (3px spread), Decision ring 22%, Pane ring 35% |
| `BLOCKED` (red) | `#ef8a80` | Diff delete row 9%, delete line number `#8a6460`, failure strip 7% |
| `PROVIDER_CLAUDE` | `#d97757` | `✻ ✶`, shimmer `#d97757 → #f3c2ac → #d97757` |
| `PROVIDER_CODEX` | `#10a37f` | `◉` |
| `CTX_*` | keep | Context meter and legend |
| Syntax `SYN_*` | keep | Code in diffs and fences only |

**Retired:**
- `ACCENT`, `ACCENT_HI`, `PRIMARY_HOVER/ACTIVE`, `ON_ACCENT`: every use
  becomes `TEXT_STRONG` (focus, `❯`, caret, current-row marks, links),
  `PRIMARY`, or is deleted.
- `ACCENT_EDGE` and `ACCENT_WASH`: become white at 22% and 6%.
- `INLINE_CODE` and `PATH_INK` (cyan): inline code becomes a `RAISED`
  chip in `TEXT_STRONG`; paths in prose become `TEXT_STRONG` underlined on
  hover; paths in tool arguments become `TEXT_DIM`.
- `MODE_INK` (magenta): becomes `ATTENTION` for bypass or full-access
  modes and `TEXT_MUTED` for the others.
- `FINISHED` (teal) as a row colour: done is a shape (`circle-check`),
  not a hue (ADR 0033).
- `FIND_WASH`: stays orange (ADR 0020). Re-check that it still reads apart
  from `SELECTED`.

### 3.5 Type

| Role | Face | Size / line | Weight |
| --- | --- | --- | --- |
| UI default | Geist | 13 / 20 | 400 |
| View title (Solo and Group heads) | Geist | 13.5 / 20 | 600 |
| Tabs, buttons, Pane titles, panel tabs, Project headings | Geist | 12.5–13 / 20 | 500 |
| Metadata, section headings, bar, row line 2 | Geist | 12 / 16–20 | 400 (headings 500) |
| Badge count | Geist | 11 / 18 | 600 |
| Composer input | Geist | 13.5 / 20 (Solo), 13 (Group) | 400 |
| Palette input | Geist | 14.5 | 400 |
| Transcript | Geist Mono | `answer_text_size` / 1.5× (13 / 21 in the mockup) | 400; strong 600 |
| Transcript in a Group Pane | Geist Mono | one step smaller (12.5 / 20 at default size) | 400 / 600 |
| Diff and fence blocks | Geist Mono | transcript size − 0.5 | 400 |
| Panel diff | Geist Mono | 11 / 19 | 400 |
| Diffstat, spinners, `❯` in chrome | Geist Mono | 11.5–12 | 400 / 600 |

- **Fonts.** Register Geist in `main.rs` beside Geist Mono (the TTFs are
  in `crates/ferrite/assets/fonts/`). `FONT_UI = "Geist"`;
  `FONT_CODE = "Geist Mono"`.
- **The kit.** Map the kit theme's UI family to Geist
  (`theme.rs` ~969).
- **Figures and ligatures.** Turn on tabular figures in chrome. Keep
  Geist Mono's ligatures off.
- **The reading-size keys.** Cmd-= / cmd-- / cmd-0 still scale the
  transcript only.
- **Chrome metrics.** Chrome metrics stop being cells (`CH`). Use pixels
  on the space scale, and give any off-scale metric a doc line (rule 9).

### 3.6 Radii (set the `R_*` names; add any that are missing)

| Value | Used for |
| --- | --- |
| 4 | Inline code chip |
| 5 | View tab in the bar, 22px icon buttons |
| 6 | 28px icon buttons, tabs in a card, Decision options, command well |
| 7 | Nav rows, chips, buttons, prompt band, diff and fence blocks |
| 8 | Floats, Group Composer, Decision box, failure strip, panel file block |
| 10 | Pane card, Solo Composer |
| 12 | Palette |
| round | Send button, state dots, count pills |

### 3.7 Icons

Ferrite draws its own SVG line icons (`assets/icons/`, `icons.rs`, 1.5px
stroke on a 16 grid; there is a test for the family). Keep that family
and add the shapes the mockups use, drawn the same way. The mockups use
Lucide shapes as the reference.

| Needed | Exists today as | Add |
| --- | --- | --- |
| sidebar toggle | `sidebar` | `panel-right` (panel toggle) |
| search | none | `search` |
| needs-you view | `bell` (reuse) or new `inbox` | `inbox` |
| parked view | none | `archive` |
| add | none | `plus` |
| project | `folder` | none |
| idle Thread | none | `terminal` (square with `>_`) |
| folds | `chevron-down`, `chevron-right` | none |
| settings | `gear` | none |
| branch | `branch` | none |
| close | `close` | none |
| splits | `board` | `split-right`, `split-down` |
| done | `check` | `check-circle` |
| failed | none | `x-circle` |
| permission mode | `warning` | `shield` |
| send | none | `arrow-up` |
| stop | `stop` | none |
| file | `file` | none |
| open in Solo | `window-maximize` | none (or `maximize`) |
| enter hint | none | `corner-down-left` |

State marks that are text glyphs today stay text glyphs: braille
spinners, `●`, `❯`.

## 4. Window anatomy

```
┌──────────────┬──────────────────────────────────────────┬─────────────┐
│ Nav 264      │ View head 46                             │ Panel 304   │
│  head 46     ├──────────────────────────────────────────┤ (focused    │
│  views       │ Pane card (radius 10)                    │  Thread,    │
│              │                                          │  toggled)   │
│  projects    │  tab strip 38                            │             │
│  …           │  transcript (mono)                       │             │
│  foot        │  Composer box                            │             │
│              │                                  gutter 8│             │
├──────────────┴──────────────────────────────────────────┴─────────────┤
│ Bottom bar 30 (full width)                                             │
└────────────────────────────────────────────────────────────────────────┘
```

- **Titlebar.** ADR 0014's empty strip becomes the **46px head row**.
  `WIN_CHROME_H` 32 → 46.
  - Over the nav it holds the traffic lights and the sidebar toggle.
  - Over the board it holds the view head (§5.2).
  - Over the panel it holds the panel's tabs.
  - The whole row stays a window-drag region except under its controls.
    Keep `app_owns_titlebar_drag` on macOS. Windows keeps its caption
    buttons at the right end of the row.
- **Board.** The board is the column between nav and panel. It has 8px
  padding right and bottom: right only when the panel is hidden, and none
  at the left because the nav's own padding provides it.
- **Fixed heights.** Head row 46, tab strip 38, bar 30
  (`STATUS_BAR_H` 24 → 30). Nav rows are 30 (one line) or 44 (two lines),
  and panel rows are 48.
- **Narrow windows.** There are no size rules for the panel (amended
  2026-10-22, ADR 0033): it shows whenever the operator has it on and
  there is a Thread to show, at any width. A narrow window just narrows
  the board; the nav folds to its rail by the existing auto-rail rule
  (`update_auto_rail`, its threshold tuned for 264px), and a Group's
  cells may fall to wall tiles. The operator hides the panel when they
  want the room back.
- **Window minimum.** `WINDOW_MIN` stays 640×420.

## 5. Components

Each component gives its anatomy, sizes, states and behaviour. §10 maps
each one to the code that draws it today.

### 5.1 Nav (sidebar)

Width 264, replacing `NAV_WIDTH` 36ch. Padding 0 8 8. The ground is
`FRAME`, with no seam line: the card's edge does the separating.

**5.1.1 Head (46, in the head row).** The traffic lights, a spacer, and
the sidebar-toggle icon button (28, radius 6), which moves here from the
bar. Its tooltip reads `Toggle sidebar ⌘B`, from the keymap.

**5.1.2 Views (30px rows, then a 14px gap).** Each row is a 15px icon,
a 10px gap, a label in `TEXT_MUTED`, and a trailing item:
- **`Search`**: trailing chord (`palette::Toggle`, ⌘K) in 12px
  `TEXT_DIM`. Opens the palette (§5.8).
- **`Needs you`**: a trailing count pill (min-width 18, height 18, radius
  9, `ATTENTION` 16% fill, `ATTENTION` 11/600), hidden at 0. It opens the
  **Needs-you list** in place of the tree:
  - first, blocked Threads oldest-first (`Cockpit::needs_you()`), as
    two-line rows;
  - then a `Recent` section holding the bell's Notices
    (`notices_float`'s rows).

  This absorbs the bell, which leaves the bar. ⌘D still walks the queue.
  It is a view, not the strip ADR 0015 removed. The strip's absence test
  (cockpit.rs ~15151) still holds for the tree, and a new test covers the
  view.
- **`Parked`**: trailing count in `TEXT_DIM`. It shows the parked list in
  place of the tree, the same rows as today's bottom `parked N` section,
  which goes. ⌘⇧P (palette parked scope) is unchanged.
- **`Tasks`**: only when Projects is live (ADR 0031). It replaces the
  bar's `board_door`, and its trailing text is the `projects_bar_word`
  tally.
- With `smart_filtering` on, its `working N` and `done N` rows join this
  group in the same style.

Row states:
- rest: `TEXT_MUTED`;
- hover: `RAISED` fill, `TEXT`;
- active view: `SELECTED` fill, `TEXT_STRONG`.

Esc or clicking the active view returns to the tree.

**5.1.3 Section heading (26).** `Projects` in 12/500 `TEXT_DIM`, with a
22px `+` icon button (new Project, the same act as today's) at the right.
The existing `groups` section (Groups spanning Projects) keeps its own
heading in this style, above `Projects`. The machine heading (ADR 0026)
and the group-error notice keep their places, restyled in this type.

**5.1.4 Project heading (30).** A `folder` icon in `TEXT_DIM`, the
lowercased Project label in 13/500 `TEXT`, and a right-aligned summary in
12 `TEXT_DIM`: the section branch's ahead/behind (`↑715`, `↑1 ↓5`).
Clicking it folds the Project. A folded Project with nothing working or
blocked shows as one row (`workmate 1`). The branch name moves from this
heading to each row's line 2. The Projects (ADR 0031) `space_numbers`
line stays under its heading, in 12 `TEXT_DIM`.

**5.1.5 Thread row (44, two lines).** Padding 0 10 0 35, radius 7. It
replaces today's one-line row with `├ └` leads.
- **Mark** at x 10–24, centred on line 1. The face (§6) comes from
  `RowStatus`:

  | State | Mark |
  | --- | --- |
  | working | braille spinner, Geist Mono 12, `RUNNING` |
  | blocked | 7px `ATTENTION` dot with a 3px 15% halo |
  | done (unseen) | `check-circle` 14 `TEXT_DIM` |
  | failed | 7px `BLOCKED` dot |
  | failing | spinner in `BLOCKED` |
  | background | 7px `RUNNING` ring |
  | idle | `terminal` 14 `TEXT_DIM` |
  | parked | `terminal` at 50% |

- **Line 1.** The title in 13 `TEXT`, ellipsised (`TEXT_STRONG` while
  unseen-done, the existing unread rule). At the right, when
  `facts.changed_files` is non-empty, its summed diffstat: `+N` in
  `RUNNING` and `−N` in `BLOCKED`, Geist Mono 11.5.
- **Line 2.** 12 `TEXT_DIM`. Its content depends on state:
  - blocked: `needs you · <reason>` in `ATTENTION`, using the existing
    words (`approval`, `plan`, `question`; the mockup's `allow Bash?` is
    illustrative);
  - failed: `failed · <age>` in `BLOCKED`;
  - failing: `failing N`;
  - working: `<branch> · <elapsed>`;
  - otherwise: `<branch>`, plus the row details `↑2↓1*`.

  At the right of line 2 goes the PR label (`#123 ✓`, from `facts.pr`;
  ADR 0026 keeps it on the row). The branch comes from `facts.branch`.
  Truncation order (narrowest first): drop the details, then the age,
  then ellipsise the branch, keeping the PR label. Port `row_words`'
  priority.
- **Selected** (the focused Pane's Thread): `SELECTED` fill, inset 1px
  `ROW_RING`, title `TEXT_STRONG` 500, line 2 `#9a9a9a`.
- **Hover:** `RAISED` fill, at today's 80ms blend.
- Board-run task rows (ADR 0031, `task_lines`) keep their extra lines
  under line 2, in 12 `TEXT_DIM`.

**5.1.6 Group row (30) and members.**
- **The row.** A chevron at x 11 (`chevron-right` folded,
  `chevron-down` open), then the Group title. At the right, the roll-up:
  the worst state's word in its ink (`1 failed`, `2 need you`), then
  `· N` members in `TEXT_DIM`.
- **When the Group is the shown view.** The row takes the selected style
  and lists its members as 30px one-line rows:
  - padding-left 54, with the mark (5.1.5) at x 35;
  - the title in `TEXT` (the focused Pane's in `TEXT_STRONG`);
  - the state word right-aligned (`1m`, `needs you`, `failed`, `done`) in
    its ink, or `TEXT_DIM`.
- **When the Group is not shown.** Members are two-line Thread rows,
  indented 16, as today's tree nests them.

**5.1.7 Rail (folded).** ⌘B or the toggle folds the nav to a 52px rail
(ADR 0015's rail, re-measured):
- one 32px cell per open Thread, holding its mark only;
- the ⌘-digit under the mark (`TEXT_DIM` 11) where ⌘ and that digit focus
  it, as today.

The head row still shows the traffic lights, and the toggle sits at the
rail's top. The canvas doesn't draw the rail; build it from these rules.

**5.1.8 Foot (36).** `ferrite` and the version in 12 `TEXT_DIM` (the
wordmark moves here from the bar, without its accent), and the `gear`
icon button. On macOS the gear stays in the app menu too. Elsewhere it
moves here from the bar.

Ordering, the attention sort, hold-under-pointer, drag-to-Group, the
`loose_ground` drop zone and the auto-rail are unchanged.

### 5.2 View head (46, in the head row)

**Solo** (new; today Solo has no head and the nav row names the
Thread). Left to right:
- **Title.** The Thread title in 13.5/600 `TEXT_STRONG`. Double-click
  renames, as `pane_title` does.
- **Project.** The Project label in `TEXT_DIM`.
- A spacer.
- **Panel toggle**, while the panel is hidden (§5.6).
There is no branch chip (removed 2026-10-22): the branch and its
diffstat live in the panel's Changes tab. There is no PR button in the
head. The pull request lives in the
panel's PR tab (§5.6.2), and Ferrite does not create pull requests.

**Group:**
- the Group title in 13.5/600 `TEXT_STRONG`;
- `<project> · N threads` in `TEXT_DIM`;
- a spacer;
- `+ New thread`, a secondary button (28, radius 7, `RAISED`, 12.5/500
  `TEXT_STRONG`). It starts a draft in this Group (the existing act).
- **Panel toggle**, while the panel is hidden (§5.6).

There is no layout switch: the Layout is the split tree, resized at its
seams and rearranged by dragging Pane heads (CONTEXT.md).

The head is window-drag outside its controls.

### 5.3 Pane card and tab strip (Solo)

**Card.** `CARD` ground, radius 10, a 1px `CARD_RING`, children clipped.
It replaces `pane_shell` and `pane_frame`'s square overlay.

**Tab strip (38, replacing `tab_strip`'s 24).** Padding 0 6, gap 2, with
a `HAIRLINE` under it.
- **`Main`**, always first: a `terminal` icon 13, then `Main`. Selected:
  28 high, radius 6, `TAB_ACTIVE` fill, `TEXT_STRONG` 12.5. Unselected:
  `TEXT_MUTED`. The mockup's `×` on Main is **not** built: ADR 0002 has no
  close buttons on tabs, and closing a Pane stays its existing act.
- **One tab per live Subagent.** A live Subagent is working, waiting or
  failing. Its tab shows:
  - its mark (spinner in `RUNNING`, or a dot in `ATTENTION`/`BLOCKED`,
    per `subject_mark`'s precedence);
  - its description in `TEXT_MUTED` 12.5, ellipsised at 200px.

  Order: waiting, then working, then failing, each by start time. A tab
  appears when its Subagent starts. It leaves when the Subagent stops
  being live, unless it is the selected tab: then it stays until the
  operator selects another. Tabs keep a fixed width while shown, as
  `tab_width` does, so a state change never resizes them.
- **Separator.** A 1×16 `HAIRLINE`, drawn only when the done menu
  follows.
- **`N done ▾` menu button.** A `check-circle` 13, then `N done`, then a
  `chevron-down`, all `TEXT_DIM` 12.5. It opens a `ChoiceMenu` of the
  finished, stopped and idle Subagents, newest first. Each row has the
  description, a result word in `TEXT_DIM`, and the time. Picking a row
  selects that Subagent's tab, which then shows until the operator leaves
  it. The button is hidden when N is 0. If a hidden Subagent is waiting,
  it is live by definition, so this menu never needs an attention mark.
- **Overflow.** If live tabs alone overflow the strip, the tail of them
  folds into the same menu under a `Live` heading, and the button reads
  `N more ▾`. Main and the selected tab are always kept, as today.
- **Right side**, after a spacer:
  - the plan meter (`tasks_strip`), restyled as 12 `TEXT_DIM`;
  - the split icon button(s), 28 each: **split right** opens the Beside
    slot (`open_beside`, Thread). Draw split down only if the Beside
    slot gains a vertical axis; otherwise omit it.

A Subagent's tab still swaps the Composer for `child_footer`, restyled:
a 38px row in the Composer's position, coverage text in `TEXT_DIM`, and
`Back to Main` as a secondary button. Esc on a Subagent's tab still stops
that Subagent alone.

### 5.4 Transcript

Mono (§3.5). Column insets are 28px left and right in Solo and 18px in a
Group Pane (replacing `TX_PAD_L` 2ch / `TX_PAD_R` 3ch). Keep the 2-cell
glyph gutter, the virtual list, `gap_before`'s spacing (one line between
blocks, flush within a run), the 108ch prose cap and the bottom
alignment.

| Block | Today | New |
| --- | --- | --- |
| **Prompt band** (`prompt_row`, `pinned_band`) | Full-width `BAND`, `❯` in `ACCENT` | `FIELD` `#222`, radius 7, padding 8 12, pulled out −12px horizontally so the text keeps its column. `❯` 600 `TEXT_STRONG`, text `TEXT_STRONG`, send time `TEXT_DIM` right. The sticky copy uses the same shape. File chips (`prompt_chip`) become `RAISED` radius 4 chips in `TEXT_STRONG`. |
| **Tool row** (`render_tool`) | Bullet carries the state; path arguments `PATH_INK` | Unchanged grammar: `●` (`TEXT_DIM` done, `RUNNING` live, `BLOCKED` failed, `○` faint when there is no result), name 600 `TEXT_STRONG`, `(args)` all `TEXT_DIM` (no cyan), trail at the right. |
| **Tool group** | `● Ran 4 commands, read 1 file ›` | Unchanged; the failure count is in `BLOCKED`. |
| **Result line** (`elbow_line`) | `└` faint, text muted | Unchanged. A lead status word takes its ink: `ok` `RUNNING`, `failed` `BLOCKED`. |
| **Prose** (`rich::style_at`) | Inline code cyan, links cyan | Prose `TEXT_PROSE`. `**strong**` 600 `TEXT_STRONG`. Inline code is a chip: `RAISED`, radius 4, padding 1 5, `TEXT_STRONG`. Links and paths are `TEXT_STRONG`, underlined on hover. Bullets keep `•` in `TEXT_DIM` (the mockup's `-` is illustrative). Headings are unchanged (body size, 600). |
| **Code fence** | No ground, `LINE2` rule | Same box as the diff block (below), keeping the syntax colours. |
| **Diff block** (`render_diff`) | No box; hunk header row | A box: margin-top 6, `BAR` ground, radius 7, an inset 1px `HAIRLINE`, padding 6 0. Each line: a 5ch right-aligned line number (`TEXT_FAINT`, or the add/delete number tint), a 2ch sign, then the code. Row washes as today, changed-word washes as today. Keep the split view (≥ `SPLIT_DIFF_MIN_W`), `HUNK_MAX_ROWS` and `… +N lines`. |
| **Reasoning** | `∴` folded | Unchanged. |
| **Subagent row** | Plain tool row | Unchanged. Clicking it selects that Subagent's tab. |
| **Working line** (`working_line_for`; the Composer stack's top row) | `Pondering… (5m22s · ↑ 4.1k tokens · esc to interrupt)` | It stays the Composer stack's top row, inside the card. The parentheses go: `✶ Pondering… 5m22s · ↑ 4.1k tokens` on the left, `esc to interrupt` right-aligned, `TEXT_DIM`. Keep the provider-ink spinner and the shimmer (focused Pane only, static under reduced motion). |
| **Turn end** | `✻ Worked for 41s · 7:32 pm · ↑3.2k ↓1.1k` | Unchanged; the inks follow the new ladder. |
| **Failure line** | `└ failed · 0.1s · msg` | Unchanged. |
| **Banner** | `~/repo on branch · started 7:18 pm` | Unchanged; it scrolls away. |

Keep the existing duration formatter (`5m22s`) everywhere. The mockups'
`5m 22s` spacing is illustrative.

### 5.5 Composer (Solo)

The Composer sits inside the card, 14px from its sides and bottom. Its
box has a `FIELD` ground, radius 10, and a 1px `FIELD_RING` (§3.2).

Above the box, inside the card, is the **shelf**: attachment chips
(`Composer::attachments`) on the left and background chips
(`background_chips`) on the right. Chips are 24 high, radius 7,
`RAISED`, 12px. Then the working line (§5.4).

Inside the box, top to bottom:
1. **Queued row (34, when anything is queued).**
   - Left: `Queued` in 12/500 `TEXT_DIM`.
   - Middle: the oldest queued prompt in `TEXT_MUTED`, one ellipsised
     line, then `+N` in `TEXT_DIM` if more are queued.
   - Right: a 24px `close` icon button that drops it (⌫'s act).

   ↑ still edits the queue and ⌫ still drops: the keys work, and they are
   no longer printed. A `HAIRLINE` follows the row.
2. **Input.**
   - Padding 14 14 8. Geist 13.5/20, growing to `MAX_ROWS` 8.
   - No `❯` in the input row: the box itself says "type here".
   - Placeholder in `TEXT_DIM`, unchanged (`placeholder`): `Steer this
     thread` / `Queue a follow-up` / `Answer above` / `Revive and
     continue` / `Start a thread`, or the predicted follow-up with
     `⇥ accept`. The mockup's long hint is not built.
   - **Caret.** The block caret assumes a mono cell (`CODE_ADVANCE ×
     CARET_CELLS`, composer.rs ~1357). In Geist it becomes a 2px bar in
     `TEXT_STRONG` at the cursor's x, blinking as today. Check that cursor
     x comes from shaped text (`shape_text`), not cell arithmetic. Fix any
     cell math in `LineElement`, and keep the @-mention pills.
3. **Toolbar (42).** Padding 0 8, gap 2:
   - `+`: a 30px icon button that attaches (today's attach act and file
     picker).
   - **Draft chips** (a Draft only): the Project and Workspace chips that
     today ride the bar's status slot, in the same chip style.
   - **Permission-mode chip**: a `shield` icon 14, the mode's label from
     `permission_modes()`, and a `chevron-down`. Bypass or full-access
     modes use `ATTENTION` text and icon; others use `TEXT_MUTED`. It is
     always shown, so the mode can be changed. It opens the mode picker,
     and ⇧⇥ still cycles.
   - A spacer.
   - **Model chip**: the provider logo 13 in its ink, the model's
     display name, and a chevron. It opens `model_picker`'s menu.
   - **Effort chip**: the effort's name and a chevron. It opens the
     effort part of the same picker. Hide it when the model has no effort
     levels.
   - **Send**: a 28px circle in `SEND` with `arrow-up` 14 in
     `TEXT_STRONG`. Enter still sends. While a turn runs with an empty
     input, it shows `stop` and interrupts (Esc's act).

   Chips are 28 high, radius 7, padding 0 9, 12.5 Geist, with `RAISED`
   on hover. Their menus anchor above the chip. Labels are sentence case,
   as the mockup shows. This amends rule 11's "values lowercase" for
   chip labels only (ADR 0033).
   - The chips work without a Session, from cached answers (ADR 0032).
   - Busy choices are disabled with a tooltip saying why (audit UI-05).

The mode, model and effort leave the bar. `ctx` and the files chip move
to the panel (§5.6).

### 5.6 Right panel (new)

The panel is a fixed column, 304 wide, padding 0 10 8 8, on `FRAME`,
with no border. It is not the Beside slot: the Reader, Review and Thread
still open beside the Pane inside the board (ADR 0021).

- **Head (46, in the head row).** Three tabs: `Changes N`, `PR #N` and
  `Context`.
  - Tabs are 28 high, radius 7, 12.5/500. The active tab is `SELECTED`
    with `TEXT_STRONG`; inactive tabs are `TEXT_MUTED`.
  - `Changes` carries the changed-file count in 12/400 `TEXT_DIM`.
  - `PR` carries the PR number in Geist Mono 11.5 `TEXT_DIM` when the
    branch has one, and nothing otherwise.
  - The panel remembers the last tab per Thread. A new Thread opens on
    `Changes`.
  - At the right, the **panel toggle**: one `panel-right` icon button,
    the nav head's sidebar toggle's twin (28, radius 6, the glyph 16 in
    `TEXT_DIM`, `RAISED` under the pointer), tooltip `Toggle panel ⌥⌘B`
    with the chord read from the keymap. It sits in the same place
    whether the panel shows or not: shown, it ends the panel head;
    hidden, it ends the view head, at the head row's far right over the
    board. It shows in Solo and Group heads alike whenever there is a
    focused Thread. On Windows the caption buttons keep the row's right
    end, the toggle just left of them.
  - The act `TogglePanel` (⌥⌘B) shows and hides it, from the toggle, the
    View menu's `Toggle Panel` (beside `Toggle Sidebar`) and the
    palette's `Toggle panel`.
  - The panel's visibility is a persisted setting, beside
    `nav_collapsed`.
- **Changes tab.**
  - **Branch row (48):** a `branch` icon, the branch in `TEXT_STRONG`
    500, and on line 2 `from <default_branch>` in `TEXT_DIM` 12. When
    `facts.status` has them, the ahead/behind and dirty counts sit on the
    right (`↑2 ↓1 · 3 dirty`).
  - **Section line (30):** `N files changed` in 12/500 `TEXT_DIM`, and
    the total diffstat in Geist Mono 11.5 at the right.
  - **One file block per `facts.changed_files` entry:** radius 8,
    `RAISED`, 2px gap between blocks.
    - Head (48): a `file` icon, the file name in `TEXT_STRONG` 500 with
      its diffstat at the right, and the directory on line 2 in
      `TEXT_DIM` 12. Clicking the name opens the file the way file links
      do.
    - Hunks (phase 7b): folded by default, and the head's chevron unfolds
      them. They draw on `BAR` in Geist Mono 11/19, with a 2.5ch sign
      column, the add/delete washes, and no line numbers. Fetch the hunks
      through the Review core (Uncommitted scope) off the UI thread, only
      for an unfolded file.
  - **Foot button:** `Review changes` (secondary, full width). It opens
    `Beside::Review(Uncommitted)`, the changed-files card's `review` verb
    today.
  - **Empty state:** `No changes yet` in `TEXT_DIM`.
- **PR tab** (§5.6.2).
- **Context tab.** The `ctx` card's content (`pane::context_usage`), in
  the new inks: the category legend rows (swatch, name, tokens right).
  Opening the tab fires `RefreshContext`, as the meter does today.
- **Foot (both tabs).** A hairline, then `Context` in 12/500 `TEXT_DIM`
  with `84k / 200k` at the right. Under it, a 6px segmented meter (2px
  gaps, radius 2) in the `CTX_*` inks. It turns `ATTENTION` past the
  existing warn fraction (.60) and `BLOCKED` past full (.85).
- **When it shows** (amended 2026-10-22). It follows the focused Thread
  in every view: Solo, a fullscreen Pane (⌘⇧⏎ or the fill button), a
  Group (the focused Pane's Thread; focusing another Pane switches it,
  and each Thread keeps its last tab), and with the Beside slot open.
  It shows exactly when the operator has it on (`panel_shown`, default
  on) and there is a Thread: never on a draft, the empty board or the
  Projects coordinator. There are no size rules (§4). When the panel
  hides, the board takes its width.
- **What it replaces.** The bar's `ctx` meter and its card
  (`context_usage_element`), and the files chip and its card
  (`changed_files_element`), are removed.

#### 5.6.2 PR tab

A read-only view of the branch's pull request, designed to answer at a
glance "can this merge, and what's in the way". Mockups: `solo.png` (open,
checks running) and `pr-states.png` (failing with conflicts, merged, no
PR). It is fed by the Review's forge read
(`Cockpit::review_pull_request` → `forge::PrSnapshot` → `WirePr`), the
same snapshot the Review's PR scope shows. Nothing new is fetched from
GitHub.

Top to bottom, with 14px between sections:

1. **PR head** (padding 2 2 14).
   - **Line 1.** A state pill (22 high, radius 11, padding 0 9 0 7,
     12/500, with a 13px icon), then `#N` in Geist Mono 12 `TEXT_DIM`, a
     spacer, and two 26px icon buttons:
     - `refresh` re-reads now (the `refresh PR status` act, plus a new
       snapshot read);
     - `external` opens the PR on GitHub (`OpenPrInBrowser`).

     The pill comes in four kinds:

     | State | Icon | Fill | Text |
     | --- | --- | --- | --- |
     | Open | `git-pull-request` | `RUNNING` 14% | `RUNNING` |
     | Draft | `git-pull-request` | `SELECTED` | `TEXT_MUTED` |
     | Merged | `git-merge` | `SELECTED` | `TEXT` |
     | Closed | `git-pull-request` | `SELECTED` | `TEXT_DIM` |

     Merged deliberately takes no purple: colour stays for state that
     needs the operator.
   - **Title.** The PR title in 14/20 600 `TEXT_STRONG`, wrapping, at most
     3 lines.
   - **Branches.** A `branch` icon, `head_ref` in `TEXT_BAR`, an arrow in
    `TEXT_FAINT`, then `base_ref` in `TEXT_DIM`, all 12px. A fork head
     shows `owner:branch`.
2. **Readiness** (one `RAISED` group, radius 8, 40px rows, `HAIRLINE`
   between rows). Each row has an icon 15, text 13 `TEXT`, and meta 12
   `TEXT_DIM` at the right. Rows appear in this order, and each only when
   it has something to say:

   | Source | Row |
   | --- | --- |
   | Checks: any failed | `x-circle` `BLOCKED`, `N check(s) failed`, meta `M of T passed` |
   | Checks: any running or pending | spinner `RUNNING`, `Checks running`, meta `M of T done` |
   | Checks: all passed | `check-circle` `RUNNING`, `All checks passed`, meta `T of T` |
   | `merge` Conflicting | `warning` `BLOCKED`, `Conflicts with <base>` |
   | `merge` Blocked | `warning` `ATTENTION`, `Merging is blocked` |
   | `merge` Clean (open PR) | `check-circle` `RUNNING`, `No conflicts with <base>` |
   | `sync` Unpushed | `arrow-up` `ATTENTION`, `N commit(s) not pushed` |
   | `sync` Behind | `arrow-down` `ATTENTION`, `N commit(s) to pull` |
   | `sync` InSync | `check-circle` `TEXT_DIM`, `In sync with your branch` |
   | Merged | `git-merge` `TEXT`, `Merged into <base>` (it replaces the merge and sync rows) |
3. **Checks.**
   - Heading (30): `Checks` 12/500 `TEXT_DIM`, with the count at the
     right.
   - One 30px row per check:
     - an icon 15: spinner `RUNNING` running, `check-circle` `RUNNING`
       success, `x-circle` `BLOCKED` failure, a hollow ring in
       `TEXT_FAINT` pending or skipped;
     - the name, 13 `TEXT`, ellipsised;
     - a status word at the right: `running`, `passed`, `failed` (in
       `BLOCKED`), `pending`, `skipped`.
   - Order: failed, running, pending, passed, skipped.
   - Clicking a check opens its page when the snapshot has a link.
     Otherwise it opens the PR.
   - After 8 rows, a `Show N more` row. `checks_truncated` adds `· more on
     GitHub`.
4. **Comments.**
   - Heading (30): `Comments` with `N · M unresolved` at the right, or
     `N · all resolved`.
   - Unresolved items come first, newest first. Each is a card: radius 8,
     `RAISED`, padding 10 12, gap 6.
     - **Line 1:** an 18px avatar circle (`#3a3a3a`, the author's initial
       in 10/600 `TEXT_STRONG`), the author in 12.5/500 `TEXT_STRONG`, the
       kind word in 12 `TEXT_DIM` (`review`, `comment`, `finding`),
       `unresolved` in 11.5 `ATTENTION` for an open finding, a spacer, and
       the age in 12 `TEXT_DIM`.
     - **Finding anchor:** for a finding, the `path:start–end` anchor in
       Geist Mono 11.5 `TEXT_MUTED`. Clicking it opens the file at those
       lines (file links).
     - **Body:** markdown rendered small: 12.5/18 `TEXT`, inline code as
       chips, clamped to 3 lines. Clicking the card expands it in place
       to the full body, the snippet and the replies (each reply has the
       author, age and body, indented 26).
     - **Outdated** findings show `outdated` in `TEXT_DIM` beside the
       kind.
   - Resolved items fold into one 30px row: a chevron and `N resolved`
     in `TEXT_DIM`. It expands to the same cards at 60% ink.
   - `comments_truncated` adds a last row, `More on GitHub`.
5. **`Review PR`.** A secondary full-width button (32, radius 7,
   `RAISED`, 12.5/500 `TEXT_STRONG`). It opens the Review beside the Pane
   in its PR scope (ADR 0021), where findings can be read against the
   diff.

**Empty state** (no PR for the branch, or `gh` unavailable):
- a 36px rounded tile with the `git-pull-request` icon;
- `No pull request` in 13.5/600;
- `When a pull request is open for <branch>, its checks, merge state and
  comments show here.` in 12.5 `TEXT_DIM`.

There is no create button. With no `gh` or no login, the line reads `Ferrite reads pull
requests with the GitHub CLI (gh). Sign in with gh auth login to see
them here.`

**Loading:** the last snapshot stays on screen while a new one is
fetched. The `refresh` icon spins (reduced motion: it dims). The first
fetch shows the PR head from `PrWatch`'s reading (number, state), then
fills in the rest.

**Freshness:** read the snapshot when the tab is opened, then on
`PrWatch`'s cadence (`AUTO_EVERY`, 30s) while the tab is visible and the
window is focused. Never poll a hidden tab. A change to the checks' or
merge state also updates the nav row's PR label.

### 5.7 Bottom bar (30)

Full width. `BAR` ground, a `BAR_TOP` line, padding 0 14, gap 18, Geist
12 `TEXT_DIM`.

- **Left: the view tabs** (`bar_tab`). There is one per view: `1 Solo`,
  then one per Group (`2 Work`, `3 InvoiceMate business`, …). The digit
  is the tab's index.
  - Each tab is 22 high, padding 0 8, radius 5. The digit is `TEXT_DIM`
    and the name `TEXT_BAR`.
  - A 5px roll-up dot follows: `RUNNING` if anything in the view is
    working, `ATTENTION` if anything is blocked (blocked wins), and none
    otherwise.
  - The current tab has a `SELECTED` fill and `TEXT_STRONG` 500 text.
  - Views still switch by clicking, by ⌘G, and by `enter_group`. ⌘1–9
    stay the shown board's Panes (ADR 0015).
  - Name the Solo tab `Solo` (sentence case).
- **Right**, in this order:
  1. a 6px `ATTENTION` dot and `N need you` in `TEXT_BAR`. Hidden at 0.
     A click opens the Needs-you view (§5.1.2).
  2. per provider with limits: its glyph (`✻` Claude, `◉` Codex) in its
     ink, then `N% this week`, or `N% of 5h` when the five-hour window is
     the more used (`usage_window`'s rule). Data comes from
     `account_limits`.
  3. `N GB free`: free space on the volume holding the Threads'
     checkouts (§8). Shown only under 20 GB, and in `ATTENTION` under
     5 GB.
  4. the clock (`clock_label`) in `TEXT_BAR`, with tabular figures.
- **What leaves the bar:**
  - the sidebar toggle goes to the nav head;
  - the bell goes to the Needs-you view;
  - the board door goes to the nav's `Tasks` view;
  - the gear goes to the nav foot;
  - `ferrite` goes to the nav foot;
  - the focused Pane's status line goes to the Composer and panel;
  - `?` goes to the palette's `Keyboard shortcuts` command and the app's
    Help menu (the act, `toggle_shortcuts`, stays).

  This amends ADR 0014's bar list and theme rule 7's "no counts".

### 5.8 Palette (command search)

- **Dialog.** 640 wide (replacing `PALETTE_W` 84ch), `PALETTE_TOP` 56,
  over a `VEIL` scrim. `FLOAT` ground, radius 12, a 1px `HAIRLINE_STRONG`
  ring, and today's one float shadow, deepened to `0 24 64` at 55%.
- **Input row (50).** A `search` icon 16, then the query in Geist 14.5
  `TEXT_STRONG`. At the right: `esc` in 12 `TEXT_DIM`, or, under a
  one-letter filter (`b w i d a`), the state's name in its ink. A hairline
  sits under the row.
- **Sections.** Today's order and content: `Threads`, `Projects`,
  `Groups`, `Commands`, or `Sessions` for the import picker. Headings are
  12/500 `TEXT_DIM`, 28–32 high. The mockup's `Actions` is `Commands`.
  Its `In transcripts` section is **not** built: there is no transcript
  search today.
- **Rows.** 36 high, radius 7, padding 0 10, gap 10.
  - A mark column: the Thread's mark (§5.1.5), or the command's icon.
  - The name in `TEXT`, with fuzzy-matched characters in 600
    `TEXT_STRONG` (was accent).
  - The context at the right in `TEXT_DIM`: Threads `scope · state`,
    Projects `N threads`, Groups `scope · N threads`, Commands the
    description.
  - Then the chord as plain `TEXT_DIM` text.
  - The cursor row is `SELECTED` with `TEXT_STRONG`, plus a
    `corner-down-left` icon at the far right.
- **Footer (36).** A hairline. On the left, the cursor row's cwd when it
  has one; otherwise today's keys: `↑↓ select · ⏎ open · ⇥ preview in a
  pane · esc`. On the right, `N results`. All 12 `TEXT_DIM`.
- **Other floats.** Every other float takes the same shell at a smaller
  scale (radius 8, rows 30): menus, pickers, the checks card, the plan
  card, session controls, tooltips and the Notices list.

### 5.9 Group view

The board column holds the Group head (§5.2), then the Layout. Each Pane
is a card with an **8px gutter** between cards (`GRID_GAP` and
`BOARD_SEAM` 1 → 8 in `ferrite_core::layout`, ~:801).
- **Seams.** A seam is the gutter's centre line, and keeps its 9px grab
  band. On hover or drag, a 2px `TEXT_STRONG` line at 40% draws in the
  gutter (was `ACCENT`).
- **Drop previews.** The drop preview's wash becomes white at 6%, with a
  `FOCUS_RING` edge, at radius 10.
- **Wall tiles.** The wall altitude (cells under 300×360) still swaps
  every Pane for its wall tile, now drawn in the card shell. Re-check
  `grid_shape`'s level choice with the gutter.

**Group Pane card.** `CARD` ground, radius 10, and a 1px ring by
`PaneEdge`:
- `Attention` → `WAIT_RING`, focused or not;
- `Focused` → `FOCUS_RING`, when more than one Pane shows;
- `Rest` → `CARD_RING`.

Top to bottom:

1. **Head (38, replacing `PANE_HEAD_H` 24).** Padding 0 6 0 14, gap 8,
   with a `HAIRLINE` under it. It is drawn on every board Pane, focused or
   not, and the focused head gets no band (the ring says focus).
   Contents:
   - the mark (§5.1.5) at 12–13px;
   - the title in 12.5/500 (`TEXT_STRONG` when focused, `TEXT`
     otherwise), with today's `title_floor`;
   - the state word in 12px from `HeadSlot`'s `state_word`:
     - live elapsed `1m04s` in `TEXT_DIM` while working;
     - `needs you` in `ATTENTION`;
     - `failed` in `BLOCKED`;
     - `done` in `TEXT_DIM`, only while unseen (ADR 0015's seen rule);
     - nothing when idle;
   - a spacer;
   - the provider logo, 13 (was `PROVIDER_MARK_SM` 11);
   - a `window-maximize` icon button (28) running the existing
     `ToggleFullscreen` (⌘⇧⏎). Its tooltip reads `Fill the board ⌘⇧⏎`.

   The head is the drag handle and the rename target, as today. The tab
   strip of §5.3 shows only on the focused Pane, and only when it has
   live Subagents or done ones. Otherwise the head stands alone.
2. **Transcript.** As §5.4, at the Group size.
3. **Decision**, when blocked. It is the transcript's tail row, as
   today (`decision_tail` → `approval_card` / `question_card` /
   `form_card`).
   - **Box.** `DECISION` ground, radius 8, an inset 1px `ATTENTION` 22%
     ring, padding 12, gap 10, in Geist. Solo uses the same box at the
     transcript's width.
   - **Head row.** A `shield` icon 14 in `ATTENTION`, then the existing
     lead (`Bash needs approval`) in 13/500 `TEXT_STRONG`. At the right,
     the context word (`claude · bypass permissions`) in `TEXT_DIM`.
     Question cards use `?`.
   - **Command well** (`well`). `SUNKEN` ground, radius 6, padding 6 10,
     Geist Mono 12.5, with `$ ` in `TEXT_DIM` and the command in
     `TEXT_STRONG`.
   - **Options** (`option_row`). Rows 30 high, radius 6, padding 0 10.
     Each row has the digit in Geist Mono 12 `TEXT_DIM`, then the label
     in Geist 13 `TEXT`. Keep the existing labels: `Allow`,
     `Allow <pattern> for this thread`, `Deny, and tell Claude what to do
     instead`. The pattern stays in Geist Mono. The cursor row has
     `SELECTED` and `TEXT_STRONG`, and no `❯`.
   - **Hint line** (`hint_line`). Below the box, in 12 `TEXT_DIM`
     (`↑↓ select · 1–3 pick · esc deny · tab amend`): the operator is
     looking here. Errors keep `✗` in `BLOCKED`.
4. **Failure strip** (new), when the Thread's last turn failed. It is 36
   high, radius 8, `BLOCKED` 7% fill, under the failure line. It holds:
   - a `x-circle` 14 in `BLOCKED`;
   - `Stopped <age> ago` in `TEXT`;
   - a spacer;
   - **`Retry`**, a 26-high button (radius 6, `SELECTED`, 500). It
     re-sends the failed turn's prompt. If no act does that yet, put the
     prompt back in the Composer (the history-retry path,
     `retry_subject_history`) and focus it. The button is hidden for a
     closed Session.

   Solo shows the same strip.
5. **Compact Composer (38).** Margin 0 10 10, radius 8, `FIELD_COMPACT`
   ground, ring 6% (12% when focused). It is the input, and on the
   focused Pane the queue.
   - **Queued row** (amended 2026-10-22), on the focused Pane while
     anything is queued: Solo's queued row (§5.5: `Queued` · the prompt
     · `+N` · a close button), above the input, with its hairline. The
     box grows by it only while something is queued. ↑ and ⌫ act on it
     as in Solo.
   - The placeholder is chosen by state (§5.5).
   - An unfocused Pane shows its flat facts (`flat_facts`,
     `1 attachment · 2 queued`) as the placeholder text when it has any.
   - The 24px send circle shows on the focused Pane only.
   - No chips: the mode, model and effort show when the Thread is in
     Solo, or in its Decision context word.

## 6. States

ADR 0015's four states and their faces, after this change:

| Attention (face) | Mark (nav, palette, Pane head) | Pane ring | Head word | Bar dot |
| --- | --- | --- | --- | --- |
| blocked | `ATTENTION` dot with halo | `WAIT_RING` | `needs you` | yellow |
| done (unseen) | `check-circle`, `TEXT_DIM` | by focus | `done` | none |
| working | braille spinner `RUNNING` | by focus | elapsed | green |
| working (failing) | spinner `BLOCKED` | by focus | `failing N` | green |
| working (background) | `RUNNING` ring | by focus | `background` | green |
| idle | `terminal` icon `TEXT_DIM` | by focus | none | none |
| idle (failed) | `BLOCKED` dot | by focus | `failed` | none |
| idle (parked) | `terminal` icon at 50% | not on a board | none | none |

Done keeps its own *shape*, but loses its teal hue (ADR 0033 amends ADR
0015). Unread stays ink: the title is `TEXT_STRONG`.

Motion is theme rule 8's catalog. What changes:
- the nav ride and the panel ride share `MOTION_RESIZE_MS`;
- hover fills keep their 80ms blend.

Nothing new moves.

## 7. Copy

- State words come from `theme::words`. Labels are sentence case
  (`New thread`, `Review PR`, `Review changes`, `Solo`).
- Chip labels show the mode, model and effort as their pickers name them,
  in sentence case.
- `·` is the only separator. There are no colon labels, and no full stop
  on one-line copy.
- The nav's `Search` row is the one always-on chord. Every other chord
  sits in a menu, the palette or a tooltip.
- Never invent data. When a value has no source (§8), hide its element.

## 8. Data the design needs

| Need | Status | Work |
| --- | --- | --- |
| Per-Thread branch for nav line 2 | Exists: `ThreadFacts.branch`, `.status` | Draw it |
| Per-Thread diffstat (nav, panel) | Exists as agent-edit totals: `ThreadFacts.changed_files` (`FileChange{path, added, removed}`) | Sum it. It counts the agent's edits, not a git diff against the base. That is acceptable for v1; name it as such in a doc line. |
| PR label | Exists: `PrWatch` (`#123 ✓`) | Draw it on nav line 2 and as the PR tab's number |
| PR tab | Exists: the Review's forge read, `Cockpit::review_pull_request` → `forge::PrSnapshot` (title, body, state, draft, head and base, merge, sync, checks, comments with findings, resolved, outdated, replies) | Fetch it for the panel, off the UI thread, on `PrWatch`'s cadence while the tab is visible. Per-check durations and links, the review decision (approved or changes requested) and the PR's author are **not** in the snapshot. v1 leaves them out; adding them is an extension of `forge.rs`'s `gh` query. |
| Needs-you queue and Notices list | Exists: `Cockpit::needs_you()`, `notices_float` rows | A view in the nav |
| Account usage | Exists: `account_limits` | Re-label |
| Free disk | **Missing** | Read free bytes on the volume holding the Threads' checkouts (`statvfs`, or `GetDiskFreeSpaceExW` on Windows; `fs4`/`sysinfo` are acceptable). Do it off the UI thread on the bar's existing minute wake (ADR 0012/0014: no new wake). Hide the item when it is unknown. |
| Panel hunks | Exists on demand: the Review core | Fetch per unfolded file, off the UI thread (phase 7b) |
| Panel visibility | **Missing** | A new setting beside `nav_collapsed` |
| Sans font | Bundled but not registered | Register Geist (§3.5) |
| Subagent results for the done menu | Exists: `AgentView` `status()`, `info()` | Map them to a result word through `status_label` |

## 9. Docs to update alongside the code

- **ADR 0033.** Land it as `accepted` with the first phase. Mark
  `theme.rs` rules 1, 2, 6 and 7 as superseded in the module doc, which is
  rewritten to describe this system (it stays "where the design rules
  live").
- **Amendment notes** (dated) in the amended ADRs:
  - ADR 0014: bar contents, `⌘K` door, popovers;
  - ADR 0015: the done face, the rail width, the Needs-you *view*;
  - ADR 0002: the strip's overflow and live-only tabs;
  - ADR 0021: the panel's `Review changes`.
- **The picture.** `spikes/terminal-native/index.html` stops being "the
  approved picture". Point the theme doc at `docs/design/ui-refresh/`.
  - The parity world (`cockpit/visual_reference/parity.rs`) and its
    scenes encode the prototype's pixels and metrics.
  - Update the parity tests that pin chrome metrics
    (`cockpit/tests/frame_parity.rs`, `nav_parity.rs`,
    `decisions_parity.rs`) to the new values in the same phase that
    changes them. Do not delete their coverage.
  - Transcript parity (`core_transcript_parity.rs`) should mostly hold;
    the inks and the boxes change.
- **`CONTEXT.md`.** Add **Panel** (the Solo view's right column: Changes
  and Context; not the Beside slot) and **Needs-you view**. Note that
  "the bar" no longer carries the status line.
- **`README.md`.** Update the feature list where it describes the strip,
  the status line and the bell.

## 10. Code map

`crates/ferrite/src/` unless noted. Line numbers are approximate at the
time of writing; search for the name.

### 10.1 Tokens and fonts

| Change | Where |
| --- | --- |
| Design rules (module doc) | `theme.rs` :1–156 |
| Grounds, lines, inks, state, accent | `theme.rs` :232–453 (`PLANE`, `CHROME`, `HEAD`, `INBAND`, `BAND`, `BAND2`, `HOVER`, `SELECTION`, `LINE`, `LINE2`, `TEXT_*`, `ACCENT*`, `RUNNING`, `ATTENTION`, `ATTENTION_EDGE`, `BLOCKED`, `FINISHED`, `MODE_INK`), `paint::*` :319–342 |
| Inline code and path inks | `theme.rs` :581–587 (`INLINE_CODE`, `PATH_INK`, `LINK_INK`) |
| Faces | `FONT_UI` :923, `FONT_CODE` :927, kit mapping :969–972; registration `main.rs` :96–118 |
| Radii | `R_*` :758–767 |
| Chrome sizes | `NAV_WIDTH` :774, `WIN_CHROME_H`/`TITLEBAR_H` :779–782, `STATUS_BAR_H` :784, `PANE_HEAD_H` :1493, `CHIP_H` :855, `PALETTE_*` :1861–1869, nav WP-G :2140–2187 |
| Tests to update | `theme::tests` (one-face asserts ~:2745, ~:2870–2900; glyph coverage: add a Geist case) |

### 10.2 Surfaces

| Component | Draws today | Wiring |
| --- | --- | --- |
| Head row and drag | `titlebar::strip` :85, `mac_drag_region` :129, Windows captions :148/:374 | Cockpit render |
| Nav shell, rail | `nav::shell` :659, `body` :678, `content` :692, rail :1390–1436 | `CockpitView::nav` (cockpit.rs ~13320), `nav_content` ~13295, `nav_tree` ~13456, width `nav_width` ~2677, toggle `toggle_nav_now`/`tween_nav` ~6401–6434, auto-rail ~2730 |
| Nav headings, rows, groups, parked | `project_section` :1106, `ThreadRow` :228, `RowDetails` :269, `NavTail` :314, `RowStatus` :385, `row_words` :981, `group_row_with_title` :846, member leads :540–558, smart sections :1011–1048, parked :1317–1372 | `nav_state` (cockpit.rs ~8558–8850), `thread_row` ~8894–8962, `section_branch` ~8966 |
| Bottom bar | `titlebar::bottom_bar` :188–241, `bar_tab` :275–315, `bar_usage` :339, `usage_window` :320, `help_door` :246 | `CockpitView::bottom_bar` (cockpit.rs ~10800–10911), bell `bell_element` ~13267 → `notifications::door` :606, board door `cockpit/projects.rs` :239 |
| Status line (to dissolve) | `pane::status_line` :3146, `mode_segment`/`mode_chip` :3171/:3215, `status_seg` :3193, `composer_control` :3227, `files_chip` :3253, `usage_meter_body` :4506 | `pane_status` (cockpit.rs ~10917–10966), `status_slot` (`cockpit/parts.rs` :160), model picker wiring ~12516, ctx meter ~11592 |
| ctx and files cards (to dissolve into the panel) | `pane::context_usage` :3977, `changed_file_row` :3266 | `context_usage_element` (cockpit.rs ~12406–12510), `changed_files_element` ~11106–11200, `changed_files_chip` ~11065, anchors `card_corner`/`bar_place` ~11687/~11735 |
| Beside slot (unchanged; the panel sits outside it) | `cockpit/beside.rs` (`Beside`, `open_beside`) | `READER_SHARE` |
| Pane shell and frame | `pane::pane_shell` :1283, `pane_frame` :1331, `PaneEdge` :1301–1308 | `pane_content`/`pane_cell` (cockpit.rs ~10590/~10516), `parts::pane_slot` |
| Pane head | `pane::group_head` :2085, `head_dot` :1830, `HeadSlot`/`state_word` :1872/:1917, `title_floor` :2240 | `CockpitView::pane_title` (cockpit.rs ~5675), `grid_board` ~3265 |
| Tab strip | `pane::tab_strip` :2198, `tasks_strip` :2547 | `CockpitView::subject_strip` (`cockpit/subagents.rs` :511), `subject_tab_face` :372, `subject_mark` :343, `tab_width` :418, overflow :552–586 and :722–766, `child_footer` :872 |
| Transcript rows | `transcript.rs` `render` :1847, `render_row` :920 (answers :950–985), `pinned_band` :1553, `render_banner` :1196; `pane::prompt_row` :5125, `prompt_chip` :5299, `render_tool` :6330, `tool_dot_ink` :5653, `call_highlights` :5702, `render_tool_group` :6745, `elbow_line` :4771, `failure_line` :5781, `turn_end` :5579, `render_block` :4856, `span_style` :8373 | `transcript/rows.rs` (`RowKind`, `gap_before`) |
| Prose | `rich.rs` `Markdown::render` :386, `style_at` :742, `syntax_style` :899 | |
| Diff | `pane::render_diff` :7850, `diff_rows` :7633, `DiffKind::paint` :8330 | |
| Working line | `pane::working_line` :1095, `working_line_for` :2699, `working_mark` :2651 | |
| Composer | `pane::composer_region` :2907, `queued_line` :3616, `placeholder` :3354, `ghost_row` :3421, `flat_facts` :3383; `composer.rs` `Composer` :137, render :885, `LineElement` :1251 (caret ~1357); `background_chips.rs` :83 | `l1_composer` :1212 |
| Decision | `decision.rs` `card` :700, `head`/`question_head` :743/:754, `well` :883, `option_row` :956, `error_line` :1102, `hint_line` :1133, `ApprovalChoice` :62, `approval_rows` :634 | `decision_tail` (`cockpit/decisions.rs` :45), `Cards::approval_card`/`form_card`/`question_card` (`cockpit/subagents.rs` :1740/:1919/:2248) |
| Board, seams, Layout | `CockpitView::tree_board` (cockpit.rs ~3959), `board_seam` ~915, seams ~4017–4139, drop preview ~4140 | `ferrite-core/src/layout.rs` `Tree::grid` :160, `grid_shape` :814, `GRID_GAP` :801 |
| Palette | `palette.rs` `row_element` :525–581, input :491, footer :408, filters :376 | `cockpit/palette.rs` `toggle_palette`/`open_palette` :208–244, floats :1377–1428 |
| Shared widgets | `components.rs` buttons :29–:1142, `floating_surface` :316, `float_shadow` :302, marks :371–392, `key_combo` :846; `menu.rs` float grammar :40–:457 | |
| Icons | `icons.rs` `icons!` :35–96; `assets/icons/` | |
| PR label | `pr.rs` `PrWatch` :21, `label` / `symbol`; `ferrite-core/src/workspace/mod.rs` `pull_request` / `pr_view` / `parse_pr_json` (~:625–700) | `OpenPrInBrowser` (cockpit.rs ~248, ~10342) |
| PR tab data | `crates/ferrite-reviewr/src/forge.rs` `PrSnapshot` :104, `read_pr` :553; `ferrite-core/src/protocol/review.rs` `WirePr` :478, `WirePrComment` :558 | `cockpit/beside.rs` `read_pull_request` :329 (the Review's fetch, to reuse); `review.rs` `pr_facts` :2256; checks card pieces `pane::checks_card` / `check_row` / `check_ink` (pane.rs ~2250–2452) |
| Usage, clock | `ferrite-core` `Cockpit::account_limits` (cockpit.rs ~1921), `progress::clock_label` :393 | |
| Facts | `facts.rs` `ThreadFacts` :26; `ferrite-core/src/cockpit/mirror.rs` `ThreadView` :520; `workspace/mod.rs` `BranchStatus` :482 | |

## 11. Build order

One phase per pull request, each shippable on its own. In every phase:
- **Tests and lint.** `cargo test -p ferrite` and
  `cargo test -p ferrite-core` pass. Clippy and fmt are clean (the repo's
  usual checks).
- **Captures.** Run the visual reference
  (`cargo run -p ferrite --features visual-reference -- --visual-reference <dir>`)
  and the live demo (`--demo parity`). Compare the relevant scenes with
  the mockups side by side, and attach the captures to the pull request.
- **Parity tests.** Update the parity tests that pin the metrics you
  changed, in the same PR (§9).
- **Legacy aliases.** When renaming tokens, map the old names onto the
  new ones (`theme.rs`'s legacy-alias block) so later phases compile, and
  remove the aliases in the final phase.

| Phase | Scope | Done when |
| --- | --- | --- |
| **0. Tokens and faces** | §3: the new values, Geist registered, `FONT_UI` Geist, radii set, `ACCENT` remapped to `TEXT_STRONG`/`PRIMARY`, cyan and magenta retired. ADR 0033 → accepted; theme doc rewritten. | The app runs with sans chrome and no blue anywhere (grep for `ACCENT` finds only aliases). The theme tests assert the new ladder, floors and radii, and the glyph test covers Geist. |
| **1. Frame and cards** | §4: the head row at 46, the bar at 30, the Pane as a card on `FRAME` with an 8px gutter, `GRID_GAP` 8, the seam grab band in the gutter, the rings by `PaneEdge`. | Solo and a 2×2 Group match the mockups' frame (positions within 2px at 1600×960). Seams still resize, and drag-to-swap still works. |
| **2. Nav** | §5.1: head, views (Search, Needs you, Parked, Tasks), headings, two-line rows, Group rows and members, rail, foot. The Needs-you view replaces the bell's list. | The nav matches `solo.png` and `group.png`. The attention sort, hold-under-pointer, drag-to-Group and ⌘B still work. ⌘D walks the needs-you queue. |
| **3. Bar** | §5.7: view tabs with roll-up dots, need-you count, usage labels, clock; items moved out. Free disk lands here (§8) or in phase 9. | The bar matches the mockups. Nothing that left the bar is unreachable (toggle, Notices, gear, `?`, Tasks). |
| **4. Pane head and tab strip** | §5.2 Solo head (title, branch chip), §5.3 card strip with live tabs and the `N done` menu, split right. | A Thread with 3 running and 20 finished Subagents shows `Main`, 3 tabs and `20 done ▾`. Picking a done Subagent opens it. Esc stops a Subagent. |
| **5. Transcript** | §5.4: prompt band, inks, inline code chips, fence and diff boxes, working line without parentheses. | `core_transcript_parity` passes with updated inks. A long Edit diff in split view still renders and folds. |
| **6. Composer** | §5.5: the box, queued row, input in Geist with the bar caret, toolbar chips (draft chips, mode, model, effort), send/stop; the bar's status line deleted. | Every picker that was in the bar works from its chip, with and without a live Session. ⇧⇥ cycles the mode. @-mentions, attachments and predictions still work. |
| **7a. Panel** | §5.6 without hunks: Changes (branch, files, Review button), Context, the meter foot, toggle and setting; the ctx and files cards deleted. | The panel matches `solo.png`. It hides on a Group, beside a Reader, and under ~1280px. `Review changes` opens the Review beside the Pane. |
| **7b. Panel hunks** | §5.6 hunks per file, fetched lazily. | Unfolding a file shows its hunks within a frame or two, with no UI-thread git. |
| **7c. PR tab** | §5.6.2: PR head, readiness, checks, comments, `Review PR`, the empty and no-`gh` states, refresh and cadence. | It matches `solo.png` and each column of `pr-states.png` on a real PR. A pushed commit flips the sync row within one cadence. A hidden tab makes no `gh` calls. |
| **8. Group Panes** | §5.9: the Group head, Pane heads with state words and the fullscreen button, the Decision box, the failure strip and Retry, the compact Composer, wall tiles in the card shell. | `group.png` is matched. The quick answers (1/2/3) work on tiles and boxes, the seen rule clears `done`, and Retry re-sends. |
| **9. Palette, floats, cleanup** | §5.8, the shared float shell, free disk if not done, legacy aliases removed, amendment notes, `CONTEXT.md`, `README.md`. | `command-search.png` is matched (with the real sections). Every float uses the new shell. |

## 12. Out of scope

- Light theme (the app is dark only).
- Creating, merging or commenting on pull requests from Ferrite. The PR
  tab reads; GitHub writes.
- Search inside transcripts. The mockup's `In transcripts` palette
  section is an idea for later.
- A layout switch for Groups.
- Per-Pane mode, model or effort controls on Group Panes.
- Resizable nav or panel widths (both stay fixed, as the nav is today).
- System stats beyond free disk.

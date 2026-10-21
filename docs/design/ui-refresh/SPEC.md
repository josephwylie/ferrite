# UI refresh: sans chrome, card panes, one calm palette

Build spec for the redesign drawn on the design canvas (October 2026).
It is written for the agent that implements it. Read this file
top to bottom once, then work through the phases in
[Build order](#11-build-order). The mockups are the visual truth.
Where a mockup and this text disagree, this text wins, and the
disagreement should be noted in the pull request.

| Mockup | File | Size |
| --- | --- | --- |
| Solo view (one Thread) | [`mockups/solo.png`](mockups/solo.png) · [`.html`](mockups/solo.html) | 1600×960 |
| Group view (four Panes) | [`mockups/group.png`](mockups/group.png) · [`.html`](mockups/group.html) | 1600×960 |
| Command search | [`mockups/command-search.png`](mockups/command-search.png) · [`.html`](mockups/command-search.html) | 800×560 |
| Before (today's UI, for comparison) | [`mockups/before.png`](mockups/before.png) · [`.html`](mockups/before.html) | 1600×960 |

The `.html` files are static exports. Open one in a browser at its size
and use the inspector to read any value this spec does not state. Their
content (thread titles, diffs, numbers) is illustrative; the data comes
from the app.

---

## 1. What changes, in one paragraph

Ferrite keeps its terminal vibe: dark greys, a monospace transcript,
`❯ ● └ ✻` grammar, spinners, state colours. Everything *around* the
transcript becomes calmer and more modern:
- the chrome switches to a sans-serif UI font (Geist);
- square full-bleed bands become rounded cards on a lighter frame,
  separated by 1px hairlines instead of borders;
- the steel-blue accent goes, and colour is kept for state only;
- the sidebar becomes an information-rich two-line thread list;
- the Pane gets a proper head with tabs, and the Composer becomes a
  rounded box that holds the permission mode, model and effort;
- a right-side panel shows the Thread's changes and context;
- a Group is a grid of the same cards.

## 2. Principles

1. **Two fonts, one rule.** Transcript content is Geist Mono. Everything
   else (sidebar, heads, tabs, buttons, menus, Composer input, bar, panel,
   palette) is Geist. Inline code inside chrome text uses Geist Mono, for
   example the diffstat numbers and spinner glyphs.
2. **No accent hue.** Focus, selection and the prompt glyph `❯` use
   off-white `#f2f2f2`. Selection is a lighter fill, never a coloured
   outline. Hue appears only for state: green working, yellow needs you,
   red failed. The provider colours (Claude orange, Codex green) appear only
   on their logos and their working shimmer.
3. **Cards on a frame.** The window frame is `#222`. The transcript lives
   in a darker card (`#181818`, radius 10, 1px hairline ring). Nothing is
   full-bleed except the bottom bar.
4. **Hairlines, not borders.** Separation is a 1px white line at 5–8%
   opacity, or a fill step. There are no visible grey borders, no outlined
   keycaps and no boxed section labels.
5. **Two-line rows.** A list row that stands for a Thread says what it is
   (title) and where it is (branch, or state reason) on two lines, with
   numbers right-aligned.
6. **Keys stay quiet.** Shortcuts appear as plain dim text, only where the
   operator is already looking (a menu, the palette, the Search row). There
   are no keycap boxes anywhere.
7. **Copy is short.** Placeholders are a couple of words (`Steer this
   thread`, `Queue a follow-up`, `Answer above`, `Follow up`). State words
   are lowercase (`needs you`, `failed`, `done`).

## 3. Tokens

Implement these as named constants in `theme.rs`. Section 10 lists which
existing constants they replace. The names below are suggestions, so
match the file's naming style.

### 3.1 Colour: surfaces

| Token | Value | Used for |
| --- | --- | --- |
| `FRAME` | `#222222` | Window background behind cards: sidebar, headers, right panel, Group gutter |
| `CARD` | `#181818` | Pane body / transcript surface |
| `BAR` | `#1e1e1e` | Bottom bar; diff block background |
| `FIELD` | `#222222` | Composer box (Solo), prompt band in the transcript |
| `FIELD_COMPACT` | `#202020` | Composer box inside a Group Pane |
| `RAISED` | `#2a2a2a` | Chips (branch chip, inline code), secondary buttons, panel file block |
| `TAB_ACTIVE` | `#262626` | Active tab inside a card |
| `SELECTED` | `#2e2e2e` | Selected sidebar row, active view tab, active panel tab, chosen Decision option, secondary button in a Pane |
| `DECISION` | `#1f1f1f` | Decision box |
| `SUNKEN` | `#161616` | Command line inside a Decision; palette scrim |
| `SEND` | `#3a3a3a` | Round send button |
| `PRIMARY` / `ON_PRIMARY` | `#f2f2f2` / `#161616` | The one primary button per view (Create PR) |

### 3.2 Colour: hairlines (white at an opacity)

| Token | Value | Used for |
| --- | --- | --- |
| `HAIRLINE` | `rgba(255,255,255,.06)` | Dividers inside cards and panels (tab strip foot, queued-row foot, panel sections) |
| `CARD_RING` | `rgba(255,255,255,.07)` | 1px ring around a card |
| `FIELD_RING` | `rgba(255,255,255,.08)` | 1px ring around the Composer |
| `ROW_RING` | `rgba(255,255,255,.06)` | Inset ring on the selected sidebar row |
| `BAR_TOP` | `rgba(255,255,255,.05)` | Top edge of the bottom bar |
| `FOCUS_RING` | `rgba(255,255,255,.22)` | Ring of the focused Pane in a Group |
| `WAIT_RING` | `rgba(230,196,124,.35)` | Ring of a Pane that needs you |

### 3.3 Colour: text

| Token | Value | Used for |
| --- | --- | --- |
| `TEXT_STRONG` | `#f2f2f2` | Titles, selected rows, prompt text, bold lead-ins, `❯` |
| `TEXT_PROSE` | `#e6e6e6` | Assistant prose in the transcript |
| `TEXT` | `#d0d0d0` | Default chrome text, tool output |
| `TEXT_BAR` | `#c0c0c0` | Emphasised items in the bottom bar |
| `TEXT_MUTED` | `#a0a0a0` | Unselected tabs and nav items, model and effort chips |
| `TEXT_DIM` | `#8c8c8c` | Metadata, timestamps, icons, section headings, tool arguments |
| `TEXT_FAINT` | `#5e5e5e` | Tree glyph `└`, diff line numbers |

### 3.4 Colour: state and brand

| Token | Value | Tints |
| --- | --- | --- |
| `WORKING` (green) | `#8cd398` | Diff add row `rgba(140,211,152,.09)`, add line number `#5f8a66`, badge fill `rgba(140,211,152,.16)` |
| `WAITING` (yellow) | `#e6c47c` | Badge fill `rgba(230,196,124,.16)`, dot halo `rgba(230,196,124,.15)` (3px spread), Decision ring `rgba(230,196,124,.22)` |
| `FAILED` (red) | `#ef8a80` | Diff delete row `rgba(239,138,128,.09)`, delete line number `#8a6460`, failure strip `rgba(239,138,128,.07)` |
| `CLAUDE` | `#d97757` | Logo, `✻ ✶`, shimmer `#d97757 → #f3c2ac → #d97757` |
| `CODEX` | `#10a37f` | Logo, `◉` |
| Context segments | `#6f9be0` `#d98a5f` `#5fae84` `#c9a55a` on `#333333` | Context meter. Keep the existing `CTX_*` mapping if it differs, and only adjust lightness to sit on `#222`. |

`ACCENT` (steel blue `#8eb1f0`) is retired. Every use either becomes
`TEXT_STRONG` (focus, prompt glyph, caret, selection marks) or is
deleted. The design canvas exposes the prompt-glyph colour as a tweak.
Ship it as `TEXT_STRONG` and do not add a preference.

### 3.5 Type

| Role | Font | Size / line | Weight |
| --- | --- | --- | --- |
| UI default | Geist | 13 / 20 | 400 |
| View title (Solo head, Group head) | Geist | 13.5 / 20 | 600 |
| Tabs, buttons, card titles, panel tabs | Geist | 12.5 / 20 | 500 |
| Metadata, section headings, bar, row subtitles | Geist | 12 / 16–20 | 400 (headings 500) |
| Badge count | Geist | 11 / 18 | 600 |
| Composer input | Geist | 13.5 / 20 (Solo), 13 (Group) | 400 |
| Palette input | Geist | 14.5 | 400 |
| Transcript (Solo) | Geist Mono | 13 / 21 | 400, bold lead-ins 600 |
| Transcript (Group Pane) | Geist Mono | 12.5 / 20 | 400 / 600 |
| Diff block | Geist Mono | 12.5 / 20 | 400 |
| Panel diff | Geist Mono | 11 / 19 | 400 |
| Diffstat, spinner glyphs in chrome | Geist Mono | 11.5–12 | 400 |

Turn on tabular figures for both fonts, and turn off ligatures for
Geist Mono. Geist and Geist Mono are both SIL OFL. Bundle the regular,
medium and semibold weights the same way the current mono font is bundled.

### 3.6 Radii

| Value | Used for |
| --- | --- |
| 4 | Inline code chip |
| 5 | View tab in the bottom bar, 22px icon buttons |
| 6 | 28px icon buttons, tabs inside a card, Decision options, command line in a Decision |
| 7 | Sidebar rows, chips, buttons, prompt band, diff block |
| 8 | Group Composer, Decision box, failure strip, panel file block |
| 10 | Cards (Pane), Composer (Solo) |
| 12 | Palette dialog |
| full | Send button, state dots, badge pills (9) |

### 3.7 Icons

Line icons, 24-unit grid, stroke 1.75 (2 for chevrons, ×, +), round caps
and joins, drawn at 13–16px in `TEXT_DIM` unless stated. The mockups use
Lucide shapes (ISC licence): `panel-left`, `panel-right`, `search`,
`inbox`, `archive`, `plus`, `folder`, `square-terminal`, `chevron-right`,
`chevron-down`, `settings`, `git-branch`, `x`, `columns-2`, `rows-2`,
`circle-check`, `circle-x`, `shield-alert`, `arrow-up`, `file`,
`maximize-2`, `corner-down-left`. Keep the existing Claude and Codex
logo paths. Spinners stay braille glyphs (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`) in Geist Mono.

## 4. Window anatomy

```
┌──────────────┬──────────────────────────────────────────┬─────────────┐
│ Sidebar 264  │ Head 46                                  │ Panel 272   │
│              ├──────────────────────────────────────────┤ (Solo only, │
│              │ Card (radius 10)                         │  toggle)    │
│              │  tab strip 38                            │             │
│              │  transcript (mono)                       │             │
│              │  Composer                                │             │
│              ├──────────────────────────────────────────┤             │
│              │ 8px gutter                               │             │
├──────────────┴──────────────────────────────────────────┴─────────────┤
│ Bottom bar 30 (full width)                                            │
└────────────────────────────────────────────────────────────────────────┘
```

- **Frame.** Everything sits on `FRAME`. The main column has 8px padding
  at the bottom, and 8px at the right when the panel is hidden. The card
  fills the rest.
- **Titlebar.** The ADR 0014 empty drag strip survives *as the 46px head
  row*. Over the sidebar it holds the traffic lights and the
  `panel-left` toggle. Over the main column it holds the view head. The
  whole row stays a window-drag region except where a control sits.
- **Heights.** Head row 46, card tab strip 38, bottom bar 30. Sidebar
  rows are 30 (one line) or 44 (two lines). Panel rows are 48.
- **Minimum widths.** At window widths under ~1180px, hide the panel
  first, then fold the sidebar to its rail (section 5.1.7).

## 5. Components

Each component lists its anatomy, sizes, states and behaviour. "Today"
names the current counterpart. Section 10 maps each one to code.

### 5.1 Sidebar

Width 264. Padding 0 8 8. Background `FRAME` (no separate fill or
border: the card's edge is the separation).

**5.1.1 Header row (46).** Traffic lights (system), then a spacer, then
the `panel-left` icon button (28×28, radius 6), which folds the sidebar.
This row is window-drag.

**5.1.2 Views (three 30px rows, then a 14px gap).** Each row is an icon
(15px) with a 10px gap, a label, and a trailing item.
- `Search`. Trailing `⌘K` in `TEXT_DIM` 12px. Opens the command search
  (5.8).
- `Needs you`. Trailing count pill: min-width 18, height 18, radius 9,
  fill `rgba(230,196,124,.16)`, text `WAITING` 11/600. It is hidden when
  the count is 0. Selecting it filters the list to blocked rows (the
  palette's `b` filter as a sidebar view). ⌘D still walks the queue.
- `Parked`. Trailing count in `TEXT_DIM`. Shows parked Threads.

Rows are `TEXT_MUTED` at rest, `TEXT` on hover with a `RAISED` fill, and
`TEXT_STRONG` with a `SELECTED` fill when that view is active.

**5.1.3 Section heading (26).** `Projects`, Geist 12/500 `TEXT_DIM`, with
a 22px `+` icon button at the right (add Project).

**5.1.4 Project heading (30).** A `folder` icon in `TEXT_DIM`, the
Project name in `TEXT` 13/500, and a right-aligned git summary in
`TEXT_DIM` 12/400 (`↑715`, `↑1 ↓5`, or nothing). Click to collapse or
expand. A collapsed Project with nothing live is one row (`workmate`,
`tempo` in the mockup) with its count or a short note at the right.

**5.1.5 Thread row (44, two lines).** Padding 0 10 0 35, radius 7.
- *Glyph* at x 10–24, top-aligned to line 1:
  - working: braille spinner, Geist Mono 12, `WORKING`;
  - blocked: 7px dot `WAITING` with a 3px halo;
  - failed: 7px dot `FAILED`;
  - done (unseen): `circle-check` 14px;
  - idle: `square-terminal` 14px `TEXT_DIM`;
  - parked: the same icon at 50% opacity.
- *Line 1:* the title, 13px `TEXT`, ellipsised. Diffstat right-aligned
  when the Thread's worktree has changes: `+N` `WORKING`, `−N` `FAILED`,
  Geist Mono 11.5.
- *Line 2:* 12px `TEXT_DIM`. It depends on state:
  - blocked: `needs you · <reason>` in `WAITING`, where the reason is
    `allow Bash?`, `allow Edit?`, `asked a question` or `plan`;
  - failed: `failed · <age>` in `FAILED`;
  - working: `<branch> · <elapsed>`;
  - otherwise: `<branch>`, or `main`.
- *Selected:* fill `SELECTED`, inset 1px `ROW_RING`, title `TEXT_STRONG`
  500, line 2 `#9a9a9a`.
- *Hover:* fill `RAISED`.

**5.1.6 Group row (30) and its members.** A chevron (`chevron-right`
folded, `chevron-down` open) at x 11, then the Group name. On the right:
a roll-up such as `1 failed` in `FAILED` followed by `· 4` in `TEXT_DIM`,
or just the member count. When the Group is the shown view, the row takes
the selected style and lists its members as 30px one-line rows: padding
left 54, a glyph at x 35, the title in `TEXT` (the focused Pane's title in
`TEXT_STRONG`), and the state word right-aligned (`1m`, `needs you`,
`failed`, `done`) in the state's colour or `TEXT_DIM`.

**5.1.7 Folded.** `panel-left` or ⌘B folds the sidebar to the rail
required by ADR 0015. The rail is 52 wide on `FRAME` and shows one 32px
cell per row with its glyph from 5.1.5 only, plus the ⌘-digit where that
digit focuses it (ADR 0015). The traffic lights stay in the head row,
which then also shows `panel-left` to unfold. The rail is not drawn on
the canvas; build it from these rules.

**5.1.8 Footer.** `Ferrite <version>` in 12px `TEXT_DIM`, and a `settings`
icon button (28). On macOS the gear may stay in the app menu only. If so,
drop it here and keep just the version.

**Order and sorting** are unchanged: the four-state attention sort from
ADR 0015, recent-sort and the hold-under-pointer behaviour.

### 5.2 View head (46)

Solo, left to right:
- the Thread title, 13.5/600 `TEXT_STRONG`;
- the Project name in `TEXT_DIM`;
- a spacer;
- the **branch chip**: height 28, radius 7, fill `RAISED`, padding 0 10,
  holding a `git-branch` 13px icon, the branch name in Geist 12.5 `TEXT`,
  and the diffstat in Geist Mono 12. Click opens the panel's Changes tab;
- the **primary action**: height 28, radius 7, fill `PRIMARY`, text
  `ON_PRIMARY` 12.5/500. It reads `Create PR` when the branch has commits
  and no PR, and `View PR #N` when a PR exists. It is hidden on the
  default branch.

Group: the Group name (13.5/600), then `<project> · N threads` in
`TEXT_DIM`, a spacer, and a secondary button `+ New thread` (fill
`RAISED`, 28 high, radius 7). There is no layout switch: the split tree
decides layout.

The head is window-drag outside its controls.

### 5.3 Pane card and tab strip (Solo)

The card: fill `CARD`, radius 10, a 1px `CARD_RING` (draw it as an outer
ring or an inset border, whichever GPUI clips cleanly), and clip
children.

**Tab strip (38).** Padding 0 6, gap 2. Bottom hairline `HAIRLINE`.
- **Main** tab, always first: a `square-terminal` 13px icon, `Main`, and
  an `x` 12px. Active style: height 28, radius 6, fill `TAB_ACTIVE`, text
  `TEXT_STRONG` 12.5. The `x` closes the Thread's Pane (today's close).
- **One tab per running Subagent:** a spinner (`WORKING`) and the
  Subagent's description, `TEXT_MUTED` 12.5, ellipsised at about 28ch.
  It is added when the Subagent starts and removed when it finishes. If
  the operator has that tab open when it finishes, it stays until they
  leave it.
- A 1×16 `HAIRLINE` separator, then the **`N done` menu button**:
  `circle-check` 13px, `N done`, `chevron-down`, in `TEXT_DIM`. It opens a
  menu of finished Subagents, newest first. Each row has the title, a
  result line (for example `No blocking findings`) and the time. Picking
  one opens it as a tab. The button is hidden when N is 0.
- A spacer, then `columns-2` (split right) and `rows-2` (split down) icon
  buttons (28). They split the board with this Pane's Thread or a new
  draft, as today's split acts do.

Today's strip (`Main · Spec review o… · Window needs… · +24`) is replaced.
Finished Subagents no longer take tab space.

### 5.4 Transcript

Mono (section 3.5). The transcript column has 28px padding at the sides
in Solo and 18px in a Group Pane. Blocks are separated by 14px gaps (12
in a Group Pane) and bottom-aligned when short. Grammar is a two-column
grid: a 2ch glyph column, then content, plus an optional right-aligned
meta column with `padding-left: 2ch`.

| Block | Rendering |
| --- | --- |
| **Prompt band** (operator message) | Fill `FIELD` `#222`, radius 7, padding 8 12, pulled out by −12px horizontally so its text aligns with the other blocks. It holds `❯` (600, `TEXT_STRONG`), the message in `TEXT_STRONG`, and the time in `TEXT_DIM` at the right. |
| **Tool row** | A `●` in `TEXT_DIM` (or `WORKING` while running, `FAILED` if it failed). The tool name in 600 `TEXT_STRONG`, immediately followed by `(args)` in `TEXT_DIM`. The duration sits at the right, or the diffstat for an edit. |
| **Result line** | Under a tool row, in the content column: `└ ` in `TEXT_FAINT`, then the summary in `TEXT_DIM`. A status word leads in its colour: `ok` `WORKING`, `failed` `FAILED`. |
| **Collapsed run** | `● Ran 7 commands · 1 failed ›` in `TEXT`, with the failed count in `FAILED`. Click to expand. |
| **Prose** | A `●` in `TEXT_STRONG`, then paragraphs in `TEXT_PROSE`, max width 100ch. Markdown `**bold**` renders 600 `TEXT_STRONG`. Inline `code` renders as a chip: fill `RAISED`, radius 4, padding 1 5, `TEXT_STRONG`. Lists use a `-` in `TEXT_DIM` in a 2ch column with 6px between items. The first sentence is *not* auto-bolded; bold comes from the markdown. |
| **Diff block** | Under an Update/Edit row: margin-top 6, fill `BAR`, radius 7, inset 1px `HAIRLINE`, padding 6 0, Geist Mono 12.5/20. Each line has a 5ch right-aligned line number (`TEXT_FAINT`, or the add/delete tint), a 2ch sign (`+` `WORKING`, `−` `FAILED`) and the code. Added and deleted lines get the row tint from 3.4. Whitespace is preserved per line. |
| **Subagent row** | `● Agent(<description>)`, with the dot `WORKING` while running. The duration sits at the right. Clicking it opens the Subagent's tab. |
| **Working line** | A provider glyph (`✶` for Claude in `CLAUDE`, `◉` for Codex in `CODEX`), then the verb (`Pondering…`) with a moving gradient shimmer in the provider colour (3.4), then ` 5m 22s · ↑ 130 tokens` in `TEXT_DIM`, and `esc to interrupt` at the right in `TEXT_DIM`. Keep the shimmer subtle and honour reduced motion with a static `CLAUDE` colour. |
| **Done line** | `✻ Worked for 8m 12s · 12:09 pm` in `TEXT_DIM`, with the glyph in its provider colour. |

### 5.5 Composer (Solo)

The Composer sits inside the card, 14px from its sides and bottom. Box:
fill `FIELD`, radius 10, a 1px `FIELD_RING`.

1. **Queued row (34, only when something is queued).** `Queued` in 12/500
   `TEXT_DIM`, then the queued text in `TEXT_MUTED` on one ellipsised
   line, then an `x` icon button (24) to remove it. A bottom hairline
   follows. With several queued items, show the oldest and a `+N` after
   it.
2. **Input.** Padding 14 14 8, Geist 13.5/20, multi-line, growing to about
   8 lines before it scrolls. Placeholder in `TEXT_DIM`: `Steer this
   thread` while working, `Answer above` while a Decision waits, `Queue a
   follow-up` while a turn runs and input would queue, and `Message`
   when idle. Ship these words, not the mockup's longer hint.
3. **Toolbar (42).** Padding 0 8, gap 2.
   - `+` (attach, 30×30 icon button);
   - **permission mode** chip: shield icon and mode name, with a
     `chevron-down`. It uses `WAITING` text for bypass/full-access modes
     and `TEXT_MUTED` for the others;
   - a spacer;
   - **model** chip: the provider logo in its colour, the model name and
     a chevron;
   - **effort** chip: `High` and a chevron;
   - **send**: a 28px circle filled `SEND` with an `arrow-up` in
     `TEXT_STRONG`. It becomes a square `stop` while a turn runs and the
     input is empty.

   Chips are 28 high, radius 7, padding 0 9, 12.5px, and open the
   existing pickers as menus anchored above the chip.

This moves mode, model and effort out of the bottom bar (ADR 0014's
focused-Pane status line) and into the Composer.

### 5.6 Right panel (Solo)

Width 272, padding 0 10 8 8, on `FRAME`, with no border.

- **Head (46).** Tabs `Changes N` and `Context` (28 high, radius 7,
  12.5/500; active tab filled `SELECTED` in `TEXT_STRONG`, inactive tab
  `TEXT_MUTED`, with the count in `TEXT_DIM` 12/400). A `panel-right`
  icon button at the right hides the panel. ⌥⌘B (or the existing key for
  the ctx card) toggles it.
- **Changes tab:**
  - **Branch row (48):** a `git-branch` icon, the branch name in
    `TEXT_STRONG` 500, and `from <base>` in `TEXT_DIM` 12;
  - **Section line (30):** `N files changed` in 12/500 `TEXT_DIM`, with the
    total diffstat in Geist Mono 11.5 at the right;
  - **File block** per file, a radius-8 `RAISED` group:
    - a 48 row with a `file` icon, the filename in `TEXT_STRONG` 500, the
      file's diffstat at the right, and the directory on line 2 in
      `TEXT_DIM` 12;
    - below it, collapsible, the file's hunks: fill `BAR`, Geist Mono
      11/19, a 2.5ch sign column and the add/delete tints, with no line
      numbers.
  - Clicking a filename opens it the way file links do today.
- **Context tab.** Today's `ctx` card content, restyled: segment legend
  rows (a colour square, the category, and tokens right-aligned).
- **Footer (both tabs).** A hairline, then `Context` 12/500 `TEXT_DIM` with
  `84k / 200k` at the right, then a 6px segmented meter (2px gaps, radius
  2) using the context segment colours.

The panel replaces the bar's changed-files and `ctx` popovers. It is shown
by default in Solo when the window is at least about 1280 wide, and never
in a Group. There is no Agents tab: Subagents live in the tab strip and
its `N done` menu.

### 5.7 Bottom bar (30)

Full width, fill `BAR`, top edge `BAR_TOP`, padding 0 14, gap 18, Geist
12 `TEXT_DIM`.

- **Left: view tabs.** One per view: `1 Solo`, `2 Work`, `3 InvoiceMate
  business`, … Each tab is 22 high with padding 0 8 and radius 5. The
  digit is `TEXT_DIM` and the name `TEXT_BAR`. A 5px roll-up dot follows
  the name: green if anything in that view is working, yellow if anything
  is blocked, and none when idle. The active tab has a `SELECTED` fill,
  `TEXT_STRONG` 500 text, and a `TEXT_DIM` digit. The digits are whatever
  key reaches the view today (check `keymap.rs`; ADR 0015 keeps ⌘1–9 for
  Panes on the shown board).
- **Right**, in order:
  - a 6px `WAITING` dot and `N need you` in `TEXT_BAR` (hidden at 0;
    clicking it walks the queue like ⌘D);
  - `✻ 14% this week` (Claude) and `◉ 6% this week` (Codex), each glyph
    in its provider colour;
  - `7.5 GB free` (free space on the volume holding the worktrees),
    shown only under 20 GB, and in `WAITING` under 5 GB;
  - the clock in `TEXT_BAR`.

These move out of the bar into other places:
- the sidebar toggle goes to the sidebar head;
- the bell becomes the `Needs you` view and the need-you count;
- `ferrite` goes to the sidebar footer;
- the focused Pane's status line goes to the Composer and the panel;
- `?` stays reachable from the palette and the app menu, with no glyph
  in the bar.

Note that the bell's own list (notices that are not Decisions) needs a
home: put it as a third sidebar view, `Activity`, if it carries more
than Decisions. Decide this in phase 3.

### 5.8 Command search

A dialog centred horizontally, 56px from the top of the window, 640
wide. Fill `FIELD`, radius 12, with a ring `FIELD_RING` and the shadow
`0 24 64 rgba(0,0,0,.55)`. The scrim is `SUNKEN` at about 60%.

- **Input row (50):** a `search` icon 16, the input in Geist 14.5
  `TEXT_STRONG`, and `esc` in 12 `TEXT_DIM` at the right. A hairline sits
  under it.
- **Sections:** `Threads`, `Actions`, `In transcripts`, as 12/500
  `TEXT_DIM` headings 28–32 high. Rows are 36 high, radius 7, padding 0
  10, gap 10.
  - Thread rows: the state glyph (5.1.5) and the title with matched
    characters in 600 `TEXT_STRONG` (the rest `TEXT`). On the right go
    `project · group` in `TEXT_DIM`, and `corner-down-left` on the
    highlighted row.
  - Action rows: an icon, the label, and the shortcut as plain `TEXT_DIM`
    text.
  - Transcript hits: `❯` and a snippet in Geist Mono 12.5 with the match
    emphasised, then `thread · time` at the right.
- **Highlighted row:** fill `SELECTED`, text `TEXT_STRONG`.
- **Footer (36):** a hairline, then `↑↓ to move  ⏎ to open  ⌘⏎ to open
  beside` and `N results` at the right, all 12 `TEXT_DIM`.

The ADR 0015 one-letter state filters (`b w i d a`) keep working. A
filter shows the state's name at the input's right, in place of `esc`.

### 5.9 Group view

The main column (no panel): the head (5.2), then a grid of Pane cards
with an 8px gap and 8px padding at the right and bottom. The split tree
and seams work as today; only the Pane's look changes. Drag-to-swap and
drag-to-split targets keep working on the new cards.

**Group Pane card.** Fill `CARD`, radius 10, a 1px ring:
- `FOCUS_RING` on the focused Pane;
- `WAIT_RING` on a Pane whose Thread is blocked;
- `CARD_RING` otherwise.

1. **Head (38).** Padding 0 6 0 14, gap 8, bottom `HAIRLINE`. It holds:
   - the state glyph (as 5.1.5, at 12–13px);
   - the title, 12.5/500, `TEXT_STRONG` when focused and `TEXT` otherwise;
   - the state word in 12px: elapsed time (`1m 04s`) in `TEXT_DIM` while
     working, `needs you` in `WAITING`, `failed` in `FAILED`, or `done`
     in `TEXT_DIM` only while unseen (ADR 0015);
   - a spacer;
   - the provider logo (13);
   - a `maximize-2` icon button that opens this Thread in Solo.

   The head is the Pane's drag handle.
2. **Transcript.** As 5.4, at the Group size.
3. **Decision (when blocked).** It replaces the bottom of the transcript.
   The box is fill `DECISION`, radius 8, an inset 1px `rgba(230,196,124,.22)`,
   padding 12, gap 10, in Geist. It holds:
   - a header row with `shield-alert` 14 `WAITING`, the question in
     13/500 `TEXT_STRONG` (`Run this command?`), and the tool name in
     `TEXT_DIM` at the right;
   - the command in a `SUNKEN` band (radius 6, padding 6 10, Geist Mono
     12.5, `$ ` in `TEXT_DIM`);
   - the options as 30px rows (radius 6, padding 0 10): the digit in Geist
     Mono 12 `TEXT_DIM`, then the label. The highlighted option is filled
     `SELECTED` in `TEXT_STRONG`. Digits pick an option and ⏎ confirms, as
     today.

   Solo uses the same Decision block, at full transcript width.
4. **Failure strip (when failed).** Height 36, radius 8, fill
   `rgba(239,138,128,.07)`. It holds `circle-x` 14 `FAILED`, `Stopped 3m
   ago` in `TEXT`, a spacer, and a `Retry` button (26 high, radius 6, fill
   `SELECTED`, 500). `Retry` re-sends the last prompt, as today's retry
   does.
5. **Compact Composer (38).** Margin 0 10 10, radius 8, fill
   `FIELD_COMPACT`, and a ring at 6% (12% when focused). It holds the input
   only, with the placeholder chosen by state (5.5). The send circle (24)
   appears on the focused Pane only. Mode, model and effort are not shown
   here; they appear in the focused Pane's Solo view.

## 6. States and motion

| State (ADR 0015) | Sidebar glyph | Pane ring | Pane head word | Bar dot |
| --- | --- | --- | --- | --- |
| working | spinner `WORKING` | card / focus | elapsed, `TEXT_DIM` | green |
| blocked | dot `WAITING` + halo | `WAIT_RING` | `needs you` | yellow |
| done (unseen) | `circle-check` `TEXT_DIM` | card / focus | `done` | none |
| idle | `square-terminal` `TEXT_DIM` | card / focus | nothing | none |
| failed (face of idle) | dot `FAILED` | card / focus | `failed` | none |
| parked (face of idle) | terminal icon, 50% | n/a | n/a | none |

Done keeps its distinct *shape* (the check) rather than its own hue. That
retires `theme::FINISHED` teal as a row colour, which the ADR in
section 9 records.

Motion:
- spinners run at today's frame rate;
- the shimmer moves once every 2s;
- hover fills appear instantly;
- the panel and the sidebar fold slide over 120ms ease-out, or snap under
  reduced motion.

There are no other animations.

## 7. Content and copy rules

- Titles are sentence case as the operator or agent wrote them. Project
  names are shown as stored (lowercase in the mockups).
- State words are lowercase: `needs you`, `failed`, `done`, `working`.
- Durations are compact: `34s`, `2m 00s`, `5m 22s`, `1h 04m`.
- Placeholders are the short ones in 5.5. There is no key hinting on
  always-visible chrome, except `⌘K` on the Search row, which is the one
  door to commands now that the titlebar's `⌘K` is gone.
- Do not invent data. When a value has no source yet (section 10.3),
  hide that element rather than showing a placeholder.

<!-- Sections 8–11 follow. -->

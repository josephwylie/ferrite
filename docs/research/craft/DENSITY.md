# Float density: Claude, Codex, Ferrite

Lane D, 2026-11-03. Claude and Codex were measured at 1x from `claude/` and `codex/`.
Ferrite was measured from `ferrite-cp3/` at 2x and halved. The measuring scripts scan
pixels: they find the box edge from a seed point inside the float, the ink rows of each
line, and the ink width of the same words in each app.

## Headline

The Composer's menus already sit on Claude's grid. These four match within 2px:

- the model picker;
- the `+` menu;
- the permission-mode menu;
- the context menu.

The width, the 28 row pitch, the 46 two-line row, the 6px fill inset, the 16px text
inset, the 12 radius and the text size all match. The same words measure the same ink
width in both apps: "Add folder" is 68 in Claude and 66.5 in Ferrite, and "Always ask
before making changes" is 212 and 210.5. So the menus are not fat because of their rows.

The "fat and boxy" read comes from five places.

1. **The tooltips were 14px text in a 16px side pad at radius 12.** That is a menu row,
   not a tooltip. Claude's tooltip text is 8 in at radius ~6. Codex's tooltip is 26 high
   with text 11 in.
2. **A heading that opens a float sat 16 under its top edge.** Claude's sits 11 under
   (`Mode`).
3. **A divider between menu groups took 13px.** Claude's takes 9: 4px of air, the 1px
   rule, then 4px of air. The rows before and after a rule were 41 apart; Claude's are
   37 apart.
4. **The `/` and `@` menus were the palette's 640 wide.** The descriptions end around
   x≈450, so the rest of each row was empty. Claude's slash picker is 238 wide and shows
   names only. Codex's spans the width of the composer.
5. **Ferrite's captures were taken in a 1440×900 window, and Claude's in 1920×1050.** A
   float of the same size covers more of the smaller window. This is about how the
   captures were taken, not a token, but it adds to the impression.

## Per surface

All values are in logical px. The before and after values are Ferrite's.

| Surface | Claude | Codex | Ferrite before | Ferrite after |
|---|---|---|---|---|
| Model menu: width | 259 | 252 (model list) | 259 (`MODEL_PICKER_W` 260) | unchanged |
| Model menu: row pitch / fill | 28 / 24, fill 6 in | 28.5 / 26 | 28 / 24 | unchanged |
| Model menu: rows either side of a divider | 37 apart (rule with 4+1+4) | n/a | 41 apart (13px band) | **37** (4 + 1 + 4) |
| Permission menu: width | 318 | 438 (with icons) | 313 (`MODE_MENU_W` 316) | unchanged |
| Permission menu: two-line row | 46 | 42 | 46 | unchanged |
| Permission menu: box top to heading cap | 11 | n/a (`How should…` head ≈ 9) | 16.5 | **12.5** |
| Permission menu: text inset | 15 | 12 | 14.5 | unchanged |
| `+` menu: width × pitch | 226 × 28 | composer-wide × 28.5 | 223 × 28 | unchanged |
| Effort submenu: width | 316 | n/a (slider card 252) | 318 | unchanged |
| Slash picker: width | 238 (names only) | composer-wide, about 720 | 640 (`PALETTE_W`) | **480** |
| Slash picker: pitch / fill | 28 / 24 | 28.6 / 26 | 28 / 24 | unchanged |
| Usage popover | 358 × 81, 13px text | n/a | not captured | unchanged |
| Tooltip: text | 12–13px, two lines on 18 | 13px | 14px on 20 | **13px on 16** |
| Tooltip: side pad / height (one line) | 8 / about 22 | 11 / 26 | 16 / 30 | **8 / 26** |
| Tooltip: radius | about 6 | about 8 | 12 | **6** (`R_CONTROL`) |
| Float radius (menus) | about 12 | about 10 | 12 | unchanged |
| Float edge / ground | #303030 ground, ~#363636 1px ring | #2d2c2d, ~#383838 ring | #303030, #383838 | unchanged |
| Shadow | soft, dims the ground beside the float by ~4–5 levels | similar | `0 6 14` at 50%, comparable | unchanged |
| Palette | no capture | no capture | 640 wide, input 50, rows 36 (SPEC §5.8) | unchanged |
| Dialog (New project) | no capture | no capture | 600 wide, 40 head, 16 pad | unchanged |
| Toast | no capture | no capture | about 285 × 87 in the panel column | unchanged |

The palette, the confirm dialog and the row ⋯ menu were captured later and are covered in the
next section. The toast has no Claude equivalent (Claude shows none for pin and unpin), so it
stays as it is.

## Palette, dialog and row menu (Claude captures of 2026-11-03, 1x)

These come from Claude's ⌘K search (a private capture, measured but not copied into the
repo), `claude/confirm-dialog.png` and `claude/row-menu.png`. Ferrite's values come from
`ferrite-cp3/refresh-palette-app.png` and `projectcreator-app.png`.

Unlike the menus, Claude's palette is *roomier* than Ferrite's on most measures. Matching it
widens Ferrite's dialog slightly and moves its insets onto one 24px edge.

| Palette part | Claude | Ferrite before | Ferrite after |
|---|---|---|---|
| Width | 670 | 640 | **670** |
| Input row | 54 (query centred 27 under the top), 14px text, no icon, × at the right | 50, a 16 icon then 16px text | **54**, the icon kept, then **14px** text (`FS_PALETTE` deleted; it uses `FS_UI`) |
| Left edge of input, tabs, row icons and footer | 24 (footer 21) | 16 | **24** |
| Tabs row | 40 below the input: pills 26 high at the row's top with 15 of air under them, word 10 in, 4 apart | 36, pills 24, 8 in, 4 apart, centred | **40, pills 26, 10 in, 4 apart, at the row's top** |
| From the hairline to the first row's fill | 48 (`Recents` on 12px) | 38 | **48**: list pad 10, then a 38 heading with its word's line 10 over its foot |
| From the row above to the row below a later heading | 41 | 42 | **40**, via a 2px section gap |
| Row height / fill inset / icon / icon to name | 36 / 10 / 16 / 10 | 36 / 6 / 14 / 10 | 36 / **10** / **16** / 10 |
| Bottom padding of the list | 10 or more | 8 (`PALETTE_LIST_PAD_B`) | **10** (`PALETTE_LIST_PAD_B` deleted; the list uses `PALETTE_LIST_PAD` on all sides) |
| Footer | 40, words 12px, each key on an outlined cap (Esc 28×18, radius 4), hints about 20 apart | 36, the key as plain bright text, 16 apart | **40**, each key on a **`components::key_cap`** cap (17, radius 4, `FLOAT_EDGE` edge), **20** apart |
| Filter tabs: worth keeping? | yes: All, Artifacts, Sessions, Code, Projects, Scheduled | yes | kept, at Claude's size |

| Row ⋯ menu | Claude | Ferrite (context menu) before | after |
|---|---|---|---|
| Width | 165, hugs its verbs | at least 256 (`MENU_W`) | **at least 160**; it still grows to its longest verb and key |
| Row pitch / text inset / text | 28 / 16 / 14px | 28 / 16 / 14px | unchanged |
| Between groups | a hairline with 4 + 1 + 4 (37 from row to row) | 8 of space (36 from row to row) | unchanged: 1px apart, and "space, not a rule" is the float rule |

| Confirm dialog | Claude | Ferrite (sheet) before | after |
|---|---|---|---|
| Width | 520 (a confirm) | 600 (the Project editor, a form) | unchanged; Ferrite has no confirm dialog: a destructive verb arms on its first press instead |
| Side padding | 16 | `2 × CH` ≈ 15.6 | **16** (`MODAL_PAD = SPACE_4`) |
| Title / body | 14 semibold, 21 under the top; 13px body on a 20 line | 14 `W_LABEL` title in a 40 head over a rule | unchanged |
| Buttons | 28 high, 9–10 in, 12 apart, 14 from the bottom and 16 from the side | 30 high (`FORM_CONTROL_H`), `CH` in | unchanged |
| Cancel | plain text, no box | the sheet's × in the head (no Cancel button) | unchanged |
| Secondary | filled #4a4a4a, white word | none (the Project editor's completing action is the white primary) | unchanged |
| Destructive | outlined in red (#d9333b), red word, faint red wash | word in `INK_SUBTLE` in a `FLOAT_EDGE` box, no red | **a `BLOCKED` word in a 1px `BLOCKED` box, with `BLOCKED_WASH` under the pointer** (`project_editor::destructive_button`, `Remove project`, `Remove` directory) |

## Token changes (`crates/ferrite-ui/src/theme.rs`)

| Token | Before | After | Why |
|---|---|---|---|
| `FLOAT_SECTION_H` | 28 | **24** | The heading that opens a float now puts its cap 12 under the top edge (Claude's is 11). The word still sits on the heading's foot, so the gap from heading to row is unchanged. The checks card's workflow heading (`components::menu_section`) uses the same token, so it tightens too. |
| `SLASH_MENU_W` | `PALETTE_W` (640) | **480** | The width a name column plus its description needs. A longer description is cut and keeps its tooltip. This also caps the width of `ChoiceMenu` and of the draft's Project and Workspace menus. |
| `TOOLTIP_PAD_X` | `FLOAT_PAD_X` (16) | **`SPACE_2` (8)** | Claude puts its tooltip text 8 in. |
| `TOOLTIP_PAD_Y` | `(FLOAT_ROW_H − LH_UI)/2` (4) | **`SPACE_1` (4)** | Same value, now stated directly instead of derived from the menu row. |
| `MENU_W` | 256 | **160** | Claude's row menu is 165 and hugs its verbs. |
| `MODAL_PAD` | `2 × CH` | **`SPACE_4`** | Claude's dialog inset. |
| `PALETTE_W` | 640 | **670** | Claude's ⌘K. |
| `PALETTE_INPUT_H` | 50 | **54** | |
| `PALETTE_PAD_X` | 16 | **24** | |
| `PALETTE_LIST_PAD` | 6 | **10** | |
| `PALETTE_LIST_PAD_B` | 8 | **deleted** | The list uses `PALETTE_LIST_PAD` on all sides. |
| `PALETTE_SECTION_H` / `_PAD_B` / `_GAP` | 32 / 4 / 4 | **38 / 10 / 2** | |
| `PALETTE_ROW_PAD_X` | 10 | **`PALETTE_PAD_X − PALETTE_LIST_PAD` (14)** | Puts the row icons on the 24 edge. |
| `PALETTE_MARK_W` / `_MARK_ICON` | 14 | **16** | |
| `PALETTE_FOOT_H` | 36 | **40** | |
| `PALETTE_TABS_H` / `_TAB_H` / `_TAB_PAD_X` | 36 / 24 / 8 | **40 / 26 / 10** | |
| `PALETTE_HINT_GAP` | 16 | **20** | |
| `FS_PALETTE` | 16 | **deleted** | The query uses `FS_UI` (14), Claude's size. |

## Code changes

- **The palette** (`crates/ferrite-cockpit/src/palette.rs`):
  - The tabs row starts at `PALETTE_PAD_X`, with its pills at the top.
  - The list is padded `PALETTE_LIST_PAD` on all sides.
  - Each footer key sits on a `components::key_cap`.
  - The query is set in `FS_UI`.
  - `shortcuts.rs` takes the same list padding.
- **`components::key_cap`** (new, ferrite-ui) is the one key-cap renderer. The Decision
  buttons in `ferrite-pane/src/decision.rs` used their own copy, which now calls this one.
- **`project_editor::destructive_button`** is red-outlined (see the table above).
- **SPEC §5.8** has Claude's numbers in place of the mockup's.
  `cockpit/tests/floats_parity.rs` reads the tokens, and its messages now name the new
  numbers.

- `crates/ferrite-ui/src/menu.rs`:
  - **`tooltip` and `tooltip_with_key`** now set `FS_META` text on `LH_META` at
    `R_CONTROL`. Before, they set `FS_UI` on `LH_UI` at `R_FLOAT`. A one-line tooltip
    is now 26 high.
  - **A submenu row without an icon** (the model picker's `Effort` and `More models`)
    now pads only below its rule. Before, it padded above and below. The rows above it
    already end 4px short of the rule, so the rule's band is 4 + 1 + 4 = 9, the same as
    Claude's.
- **`DROP_LABEL_PAD_*`** still alias the tooltip's pads, so a dragged Pane's label
  narrows from 16 to 8 at the sides. It keeps its `FS_UI` text, which is drawn in
  `cockpit.rs`.
- **No tokens became duplicates.** `SLASH_MENU_W` is no longer an alias of `PALETTE_W`.

## Not changed, noted

- **Claude's palette input has no search icon**, only a × at the right. Ferrite keeps its
  icon on the 24 edge, so the query, at 50, lines up over the row names, as Claude's does.
- **Claude's confirm dialog has no head rule and no footer rule.** Ferrite's sheets keep
  both, because the Settings and Project sheets scroll. Ferrite has no confirm dialog and no
  secondary button to restyle.
- **The slash picker sets its command names in the code face.** Claude and Codex set
  them in the UI face. A monospace column reads wider and boxier than proportional
  text. This is a type decision for its owner, not a density token.
- **`pane.rs` and `panel/preview.rs` build three plain kit `Tooltip::new` tooltips**
  outside the float grammar, so they miss these metrics. They should move to
  `ferrite_ui::menu::tooltip`. Other lanes own those files and are editing them now.

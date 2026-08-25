# Ferrite rich-pane design study — "Aperture" system spec

Extracted verbatim from four artboards (design canvas `.dc.html` comps). Every value below is read from
inline CSS in the source files — nothing eyeballed. The `<x-dc>`/`<helmet>`/`support.js` wrappers are the
design-canvas harness, not part of the design.

Sources (authoritative):

| Board | File | Size | Role |
|---|---|---|---|
| Main — "Rich pane — focused" | `Main.dc.html` | 900×640 | Overall focused-Pane structure (header, todo strip, composer chrome) |
| DirectionDense — "Focused transcript — dense" | `DirectionDense.dc.html` | 900×760 | **Canonical L1 transcript rendering** — supersedes Main wherever they conflict on transcript content |
| PromptBox — "Prompt box — four states" | `PromptBox.dc.html` | 620×800 | **Canonical Composer spec** — four states |
| Cockpit — "Cockpit — glance view" | `Cockpit.dc.html` | 900×600 | L2 semantic-zoom cell grammar + wall header |

Precedence rule (designer's note): where Main and DirectionDense conflict on transcript rendering,
**DirectionDense wins**; Main still governs overall pane structure (header rows, todo strip, composer
placement). PromptBox is the Composer's own spec and matches Main's composer metrics.

---

## 1. Aperture tokens (shared across boards)

### 1.1 Color — solids (16)

| Token (suggested name) | Value | Where used |
|---|---|---|
| `bg.canvas` | `#050505` | Page/app background behind everything (all boards' body; Cockpit app surface; PromptBox artboard) |
| `bg.recessed` | `#0a0a0a` | Sunken strips inside a pane: Main todo strip, Main diff-card header, Dense code-block body |
| `bg.pane` | `#0e0e0e` | Pane/cell surface (Main pane, Dense pane, every Cockpit cell); also the *ink* of the inverse block cursor in the pty cell (`color: #0e0e0e` on accent bg) |
| `bg.composer` | `#161616` | Composer background (Main, Dense, all PromptBox states) |
| `bg.chip` | `#191919` | Chips & inline code: issue chip, inline-code spans, changed-file chips, diff-stat chips, y/n/a keycaps, popover surface (slash + @ menus), "pty"/"In Review" chips |
| `text.faint` | `#54575f` | Faintest ink: unchanged diff line numbers, `·` separators (Dense header), code comments, code-block header text ("XtermPane.tsx · ts", "copy") |
| `text.dim` | `#7f8187` | Meta/hints: ctx label, `$0.84`, "todo", tool-row meta ("1.4k lines", "8.2s"), key hints (⇧⇥ plan, ↵, ⌫ unqueue, esc dismiss), placeholder text, section labels, `⌘6`, idle text, ⏳ glyph, chevron icon stroke, `.cont` continuation lines, "a always" keycap ink, window-control icon color |
| `text.secondary-dim` | `#8b8f97` | ⏺ agent gutter glyph, Bash command text (Main), unchanged diff code, issue-chip text, slash-menu selected description, permission subtitle, queued text, Cockpit activity lines ("◐ vitest run — 8s"), footnote, idle LED dot |
| `text.secondary` | `#a7abb4` | Agent prose, tool labels (Main "Read"/"Bash"), todo-strip task text, unselected menu items, "n deny" ink, Cockpit dimmed titles (done/idle/pty), diff-stat chip ink, operator `=` in syntax, wall header title |
| `accent` | `#c7ccd6` | THE accent (desaturated blue-gray): ❯ prompt glyph, links, block cursor, focus ring, progress/ctx-meter fill, ▰▱ meter, mode-chip ink, provider-chip ink, @-mention pill ink, selected slash command, fuzzy-match highlight, markdown bullets `•` and list numbers, ◐ running glyph (composer) |
| `accent.hover` | `#d7dbe3` | Link hover (only hover state drawn anywhere) |
| `text.primary` | `#f3f4f7` | Titles, user prompt text, typed composer text, bold tool names, emphasized terms, diff changed-line code, permission command, "y allow" ink, active cell titles, syntax identifiers |
| `status.ok` | `#7fc99b` | Green: running/ok LED dots, `exit 0`, `+N` diff counts, ✓ badges, "done" label, pty prompt ❯ |
| `status.err` | `#e08c84` | Red: `−N` diff counts, removed-line numbers, ✗ badge ink, failing-thread LED |
| `status.warn` | `#d8c082` | Amber = **Decision/attention**: warning-triangle stroke, "esc interrupt", "needs you" ink + LED + ring, "· 1 needs you" wall count; also doubles as syntax color for function calls and pty git hashes |
| `syntax.keyword` | `#8fb3d9` | Code-block keywords (`const`, `return`, `=>`) — only non-shared solid; Dense board only |

Full syntax mini-palette (Dense code block): keyword `#8fb3d9`, function `#d8c082`, identifier `#f3f4f7`,
operator `#a7abb4`, comment `#54575f`.

### 1.2 Color — alphas (13)

| Token | Value | Where used |
|---|---|---|
| `line.hairline` | `rgba(255,255,255,0.045)` | Hairline dividers: pane-header bottom, todo-strip bottom, diff-card header bottom, code-block header bottom, Cockpit cell-header bottom, popover footer top, composer input-row top border (running state), wall header bottom |
| `line.ring-faint` | `rgba(255,255,255,0.05)` | Popover outer ring (`0 0 0 1px` in shadow stack) |
| `line.subtle` / `bg.track` | `rgba(255,255,255,0.07)` | Subtle borders (issue chip, diff card, code block, changed-file chips, "a always" keycap, Cockpit cell border) AND progress/ctx meter track AND popover selected-row bg |
| `line.strong` | `rgba(255,255,255,0.12)` | Strong borders: pane border, composer top border, popover border, y/n keycap borders, Dense markdown-heading underline |
| `accent.tint` | `rgba(199,204,214,0.14)` | Accent chip bg: provider chip, ⏵ auto-edit chip, @-mention pill |
| `accent.underline` | `rgba(199,204,214,0.40)` | Link `text-decoration-color` (Main + Dense) |
| `ok.tint` | `rgba(127,201,155,0.13)` | Success badge bg; diff added-row bg |
| `ok.tint-strong` | `rgba(127,201,155,0.30)` | Intraline added-char emphasis inside a `+` diff line (Main only; radius 2) |
| `err.tint` | `rgba(224,140,132,0.13)` | Diff removed-row bg; ✗ failing badge bg |
| `warn.tint` | `rgba(216,192,130,0.13)` | Permission-card bg; "needs you" badge bg |
| `warn.border` | `rgba(216,192,130,0.35)` | Permission-card border |
| `shadow.near` | `rgba(0,0,0,0.30)` | `0 2px 4px` layer of elevation shadow |
| `shadow.far` | `rgba(0,0,0,0.40)` | `0 6px 16px -4px` layer of elevation shadow |

### 1.3 Shadow / ring recipes

| Recipe | Value | Used on |
|---|---|---|
| Focus ring | `inset 0 0 0 1.5px #c7ccd6` | Focused Pane (Main), focused Cockpit cell |
| Decision ring | `inset 0 0 0 1.5px #d8c082` | Cockpit cell blocked on a Decision |
| Pane elevation | `0 2px 4px rgba(0,0,0,0.30), 0 6px 16px -4px rgba(0,0,0,0.40)` | Main focused pane (combined with focus ring in one `box-shadow`) |
| Popover elevation | `0 0 0 1px rgba(255,255,255,0.05), 0 2px 4px rgba(0,0,0,0.30), 0 6px 16px -4px rgba(0,0,0,0.40)` | Slash menu, @-file menu |

Dense pane and unfocused Cockpit cells: border only, no shadow, no ring.

### 1.4 Typography

Families (Google Fonts loaded):
- `.mono` → `"JetBrains Mono", "SF Mono", Menlo, monospace` — weights 400, 500, 700 + italic 400. All agent/terminal/data content.
- `.ui` → `"Geist", -apple-system, system-ui, sans-serif` — weights 400, 500, 600. Chrome only: pane titles, todo task text, menu descriptions, permission subtitle, footnotes. (DirectionDense loads **only** JetBrains Mono — the dense board is 100% mono.)
- Weight 500 is loaded for both families but never explicitly used in any board (400 default; 600 for ui titles; 700 for mono emphasis). Italic used once (queued prompt text).

Font-size scale (9 steps) and every use:

| px | Uses |
|---|---|
| 9.5 | Cockpit cell-header meta (`#214`, "needs you" badge, "done", "pty" chip, `⌘6`); pty terminal text |
| 10 | Hints/labels/badges: "todo", "CHANGED", mode chips (⏵ auto-edit), key hints (⇧⇥ plan, ↵, ⌫ unqueue, esc interrupt, ↑↓ select…), PromptBox section labels, exit/✓/✗ badges, Cockpit badges + activity lines + done-cell lines, y/n/a keycaps, Dense code-block header, "fable-5 · max", "running 2m14s · turn 6" |
| 10.5 | Chips (issue, provider), tool-row meta ("1.4k lines", "8.2s"), diff `+2 −1` header counts, changed-file chips, "open diff" link, @-menu paths, permission subtitle, Cockpit progress fraction + decision command + idle-cell text, Main header "ctx" |
| 11 | `$0.84` (Main), todo-strip task text, "6 panes"/"· 1 needs you", diff-card filename, slash-menu descriptions, Dense header row, PromptBox footnote |
| 11.5 | Queued row (PromptBox); Cockpit cell titles (weight 600) |
| 12 | Tool rows (Main: label/command/file link), diff code lines (both boards), slash commands, permission command, Dense code-block code, wall header title (600), @-menu filenames |
| 12.5 | **Dense base**: transcript + composer (the terminal metric) |
| 13 | Main transcript base; composer input (Main + PromptBox); Main header title (600) |
| 13.5 | Dense markdown section heading (700) |

Letter-spacing: `-0.012em` (ui titles: Main header, wall header), `0.06em` ("CHANGED"), `0.08em`
(▰▰▰▱ meter, Main todo strip only), `0.10em` (PromptBox section labels).

Line-height: `1.55` Main transcript · `1.45` Dense transcript (canonical) · `1.5` code block + footnote ·
`1.4` pty cell · fixed `20px` Main diff rows.

### 1.5 Radii, borders, geometry

| Token | Value | Uses |
|---|---|---|
| `r.inline` | 2px | Intraline diff emphasis span |
| `r.tight` | 3px | Dense: inline `.code` chips, code block, bare-diff none (Dense diff has no container) |
| `r.chip` | 4px | Chips, keycaps, badges, popovers, popover selected row, icon buttons, @-pill — the default radius |
| `r.card` | 5px | Diff card (Main), permission card, standalone composer box (PromptBox demos) |
| `r.pill` | 999px | LED dots, progress/ctx meters |
| Border width | 1px | All borders |
| Ring width | 1.5px | Focus/Decision inset rings |
| SVG strokes | 1.5 (chevron, window icons) / 1.4 (warning triangle); round caps + joins |
| LED dot | 7px (Main header) / 6px (Dense + Cockpit) |
| Block cursor | 7×16px (13px rows) / 7×15px (Dense 12.5px row); solid `#c7ccd6`; pty variant = inverse video (accent bg, `#0e0e0e` ink) |
| Meters | Main ctx: 64×4px; Cockpit progress: flex×6px; both radius 999, track `white/07` |
| Icon buttons | 23×23, radius 4, 12×12 SVG |
| Gutter column | 14px fixed width (❯ / ⏺ / list numbers) |
| Diff line-number column | 26px (Main) / 30px (Dense), right-aligned |

### 1.6 Spacing scale (every value that appears)

`2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 18, 22, 24` px.
Key assignments — gaps: 2 (perm-card text stack), 3 (pty lines), 4 (Dense transcript rows), 5 (markdown
block), 6 (badge rows, popover↔input stack, bullets), 8 (Dense rows, header items, Cockpit grid/cells),
10 (Main transcript gutter gap, composer, wall header, menu rows, perm card), 12 (Main diff cols), 18
(PromptBox sections). Paddings: chips `1px 6px` (small) / `2px 6px` / `2px 7px`; menu rows `0 8px`;
composer `0 14px` (Main/PromptBox) / `0 12px` (Dense); transcript `12px 14px` (Main) / `8px 12px`
(Dense); cell body `10px`; perm card `8px 10px` with `8px 8px 0` margin; popover `4px`. Indent under a
tool row: 24px (Main) / **22px (Dense, canonical)**.

### 1.7 Row-height scale

`20` (Main tool rows) · `22` (composer meta row, popover footer) · `24` (diff-card header, queued row,
Cockpit cell header) · `26` (menu rows) · `28` (Main todo strip, Dense header) · `32` (Dense composer
input) · `34` (wall header) · `38` (Main pane header) · `40` (composer input, Main/PromptBox; all
`min-height` on inputs — the line grows).

### 1.8 Opacity

Single non-color opacity: `0.75` on an entire done Cockpit cell.

### 1.9 Glyph inventory (exact characters)

| Glyph | Meaning / where |
|---|---|
| `❯` | Prompt marker: user turn in transcript (700, accent), composer prompt (700, accent), idle state ("❯ idle"), pty prompt (green) |
| `⏺` | Agent/tool turn gutter marker (`#8b8f97`) |
| `⎿` | Continuation/result line lead (Dense `.cont`, 22px indent, `#7f8187`) |
| `▰` `▱` | Todo progress meter blocks ("▰▰▰▱ 3/4") |
| `✓` / `✗` | Pass / fail badge glyphs |
| `◐` | Busy spinner glyph: composer while running (accent), Cockpit activity lines (`#8b8f97`) |
| `⏳` | Queued-prompt marker |
| `⏵` | Mode-chip prefix ("⏵ auto-edit") |
| `⇧⇥` | Shift-tab hint ("⇧⇥ plan") |
| `↵` `↑` `↓` `⌫` | Key hints; `esc` spelled out lowercase |
| `·` | Meta separator (middle dot, everywhere) |
| `•` | Markdown bullet (accent) |
| `⌘6` | Pane hotkey label |
| `—` | Em dash in prose/labels |
| `−` / `+` | Diff signs (U+2212 minus in `−1`, `−2` etc.) |

SVG icons (inline, stroke=currentColor unless noted): right chevron `M3.5 2 L7 5 L3.5 8` in 10×10 (Main
tool rows, `#7f8187`); minus `M2 6 H10` and X `M2.5 2.5 L9.5 9.5 M9.5 2.5 L2.5 9.5` in 12×12 (window
controls); warning triangle + exclamation in 14×14 stroke `#d8c082` 1.4 (permission card).

### 1.10 Link style

`color: #c7ccd6; text-decoration: underline; text-decoration-color: rgba(199,204,214,0.40); text-underline-offset: 2px;`
hover → `#d7dbe3`. (PromptBox/Cockpit define color+hover only, no underline decoration — they draw no
body links.)

---

## 2. Board: Main — "Rich pane — focused" (900×640)

Pane container: `flex column; bg #0e0e0e; border 1px white/12; box-shadow: inset 0 0 0 1.5px #c7ccd6,
0 2px 4px black/30, 0 6px 16px -4px black/40; overflow hidden; box-sizing border-box`. Default font `.ui`;
mono opt-in per element. Square corners (no radius on the pane).

### 2.1 Structure (top → bottom)

**Row 1 — Pane header, 38px** (`flex none`, gap 8, padding 0 10, border-bottom hairline), left → right:
1. Status LED — 7px dot, radius 999, `#7fc99b` (running-ok)
2. Thread name — "font-ligatures", ui 13px / 600 / −0.012em, `#f3f4f7`
3. Issue chip — "swarmdeck#214", mono 10.5, `#8b8f97` on `#191919`, border white/07, r4, padding 2 6
4. Provider chip — "claude · fable", mono 10.5, `#c7ccd6` on accent/14, r4, padding 2 6 (no border)
5. Spacer (`flex:1`)
6. "ctx" label — mono 10.5 `#7f8187`
7. Context meter — 64×4px pill, track white/07, fill 62% `#c7ccd6`
8. Cost — "$0.84", mono 11 `#7f8187`
9. Icon button (minimize) — 23×23 r4, `#7f8187`, 12×12 minus SVG
10. Icon button (close) — 23×23 r4, `#7f8187`, 12×12 X SVG

**Row 2 — Todo strip, 28px** (bg `#0a0a0a`, border-bottom hairline, gap 10, padding 0 14):
`▰▰▰▱ 3/4` (mono 10.5, ls 0.08em, accent) · current step "run checks & update ADR-0012" (ui 11,
`#a7abb4`) · spacer · "todo" (mono 10, `#7f8187`).

**Transcript** (`flex:1; min-height:0`, `.mono`, column gap 10, padding 12 14, 13px/1.55, overflow
hidden). Block types drawn:
- **User turn**: row gap 10; gutter `❯` 14px wide, accent, 700; text `#f3f4f7` with inline links.
- **Agent prose**: gutter `⏺` `#8b8f97`; prose `#a7abb4`; inline code = `#f3f4f7` on `#191919`, r4,
  padding 0 4; emphasized identifiers `#f3f4f7` 700.
- **Tool row** (Read/Bash): height 20, padding-left 24, gap 8, align center. Chevron SVG 10×10
  `#7f8187` → tool label 12px `#a7abb4` → argument (file = 12px link; command = 12px `#8b8f97`) →
  spacer → right-aligned meta ("1.4k lines" 10.5 `#7f8187`; or result badge "exit 0" 10px `#7fc99b`
  on ok-tint r4 padding 1 6; or badge "✓ 41 passed" + duration "8.2s" 10.5 `#7f8187`).
- **Diff card**: margin-left 24, border white/07, r5, overflow hidden. Header 24px, bg `#0a0a0a`,
  border-bottom hairline, padding 0 10: filename 11px `#a7abb4` · spacer · `+2` 10.5 green ·
  `−1` 10.5 red. Body rows 12px / 20px line-height, gap 12, padding 0 10, number column 26px
  right-aligned: context row (num `#54575f`, code `#8b8f97`, two-space lead); removed row (bg
  err-tint, num `#e08c84`, code `#f3f4f7`, `− ` lead); added rows (bg ok-tint, num `#7fc99b`, code
  `#f3f4f7`, `+ ` lead) with **intraline emphasis** spans (`bg rgba(127,201,155,0.30)`, r2).
- **Spacer** (`flex:1`) pushing footer row down.
- **Changed-files row**: gap 6. "CHANGED" 10px `#7f8187` ls 0.06em · file chips (mono 10.5 `#a7abb4`
  on `#191919`, border white/07, r4, padding 2 7) containing `+18` green / `−2` red · spacer ·
  "open diff" link 10.5.

**Composer** (`flex none`, bg `#161616`, border-top 1px white/12):
- Input row: `.mono`, min-height 40, gap 10, padding 0 14, 13px. `❯` accent 700 → typed text
  `#f3f4f7` → block cursor 7×16 accent → spacer.
- Meta row: height 22, gap 10, padding 0 14 4. Left: mode chip "⏵ auto-edit" (mono 10, accent on
  accent/14, r4, padding 1 6) · "⇧⇥ plan" (mono 10 `#7f8187`). Right: "@ files · / commands ·
  ↑ history" · "fable-5 · max" (both mono 10 `#7f8187`).

### 2.2 Main-only tokens/deltas
- 38px header + separate 28px todo strip (Dense merges them — see §5 precedence).
- Tool rows as icon rows (chevron + right meta) — superseded by Dense's `Tool(arg)` + `⎿` form.
- Diff in bordered card with filename header and intraline highlights — superseded by Dense's bare diff
  (intraline emphasis and the changed-files footer exist only here; keep as pane-structure features).
- Inline-code radius 4 (Dense uses 3).
- 24px transcript indent (Dense: 22px).
- Elevation shadows on the pane; LED 7px.

---

## 3. Board: DirectionDense — "Focused transcript — dense" (900×760) — canonical L1 transcript

Container: `.mono` throughout, bg `#0e0e0e`, border 1px white/12, **no ring, no shadow**, base
**12.5px / 1.45**. Utility classes: `.row {flex; gap 8}`, `.g {flex none; width 14px}` (gutter),
`.cont {padding-left 22px; color #7f8187}` (continuation), `.code {#f3f4f7 on #191919; r3; padding 0 4}`.

### 3.1 Header — single 28px row (gap 8, padding 0 10, 11px, border-bottom hairline)
LED 6px green → Thread name 700 `#f3f4f7` → `·` `#54575f` → "swarmdeck#214" `#8b8f97` → spacer →
`▰▰▰▱ 3/4` accent (no letter-spacing here) → `·` → "ctx 62% · $0.84" `#7f8187`. (Todo meter, ctx, and
cost fold into the one header line; ctx is text, not a bar.)

### 3.2 Transcript (gap 4, padding 8 12)
- **User turn**: `.row` + `.g` `❯` accent 700; text `#f3f4f7`, inline links.
- **Agent prose**: `.g` `⏺` `#8b8f97`; prose `#a7abb4`.
- **Tool call**: `⏺` gutter; content = **bold tool name** `#f3f4f7` 700 + `(` + arg + `)`; file args
  are links, command args inherit prose `#a7abb4`. Forms drawn: `Read(src/lib/components/XtermPane.tsx)`,
  `Bash(pnpm add @xterm/addon-ligatures)`, `Edit(src/lib/components/XtermPane.tsx)`,
  `Bash(pnpm check && vitest run tests/unit)`.
- **Continuation / result** (`.cont`, 22px indent, `#7f8187`, leads with `⎿ `): result fragments carry
  color inline — `⎿ 1,449 lines · CanvasAddon load at 841, disposal-ordering guard at 1325`;
  `⎿ exit 0 · 3.1s · + @xterm/addon-ligatures 0.10.0` (exit 0 green); `⎿ tsc clean · ✓ 41 passed
  (0 failed) · 8.2s` (✓ part green).
- **Inline diff** (under an Edit row): margin-left 22, 12px, **no border/card/filename**. Rows gap 10,
  padding 0 6, number col 30px right-aligned. Context: num `#54575f` / code `#8b8f97`. Removed: bg
  err-tint, num `#e08c84`, code `#f3f4f7`. Added: bg ok-tint, num `#7fc99b`, code `#f3f4f7`. No
  intraline emphasis.
- **Turn-final summary**: agent row with `margin-top 6`, inline `.code` chips (`=>` `!==` `->`).
- **Markdown block** (agent long-form; padding-left 22, column gap 5, margin-top 2):
  - Heading: 13.5px 700 `#f3f4f7`, border-bottom 1px white/12, padding-bottom 3,
    `align-self: flex-start` (rule spans only the text width).
  - Bullets: rows gap 6; `•` accent (flex none); text `#a7abb4` with lead terms `#f3f4f7` 700.
  - Code block: bg `#0a0a0a`, border white/07, r3, margin-top 3. Header 20px, padding 0 8,
    border-bottom hairline: "XtermPane.tsx · ts" 10px `#54575f` · spacer · "copy" 10px `#54575f`.
    Body padding 6 8, 12px/1.5, syntax per §1.1 mini-palette.
  - Numbered list: rows gap 6 (first gets margin-top 3); marker "1."/"2." accent, fixed width 14;
    text `#a7abb4`, inline links (`XtermPane.tsx:843`).

### 3.3 Composer (Dense variant)
Single row: bg `#161616`, border-top white/12, min-height **32**, padding 0 12, **12.5px**. `❯` accent
700 → text `#f3f4f7` → cursor **7×15** → spacer → "⏵ auto-edit" chip (10px) → "fable-5 · max" 10px
`#7f8187`. (No separate meta row; chips ride the input line's right side.)

---

## 4. Board: PromptBox — "Prompt box — four states" (620×800) — canonical Composer

Artboard: `.ui`, bg `#050505`, padding 18, column gap 18. Each state labeled by a section header
(mono 10px, ls 0.10em, `#7f8187`). Standalone composer boxes here have r5 + border white/12 (inside a
pane the composer is edge-to-edge, no radius — Main/Dense). Doctrine (designer's footnote, 11px
`#8b8f97`): *"One text line that grows. Everything else — menus, permission cards, queue — stacks
**above** it and is driven by keys. No send button, no attachment tray, no floating rounded box."*

### State 01 — IDLE ("a shell line, not a composer")
Box: `#161616`, border white/12, r5. Input row 40px, padding 0 14, mono 13: `❯` accent 700 → **cursor
7×16 immediately after ❯** → placeholder `#7f8187`: `message font-ligatures — / commands · @ files ·
↵ send` (pattern: "message ‹thread-name› — hints"). Meta row 22px (padding 0 14 4): "⏵ auto-edit" chip ·
"⇧⇥ plan" · spacer · "fable-5 · max".

### State 02 — SLASH COMMANDS (dense keyboard menu)
Stack (gap 6): **popover above, input below** (popover renders above the line, in-flow in the comp).
- Popover: bg `#191919`, border white/12, r4, padding 4, shadow = popover recipe (§1.3).
- Selected row: 26px, padding 0 8, bg white/07, r4 → `/code-review` mono 12 accent · desc "review
  branch vs main" ui 11 `#8b8f97` · spacer · `↵` mono 10 `#7f8187`.
- Unselected rows (26px): command mono 12 `#a7abb4` · desc ui 11 `#7f8187`. Drawn: `/commit` "stage +
  commit this pane's diff", `/compact` "summarize context", `/to-tickets` "plan → GitHub issues".
- Footer: 22px, border-top hairline, margin-top 2: `↑↓ select · ↵ run · esc dismiss` mono 10 `#7f8187`.
- Input: `❯` + typed `/co` `#f3f4f7` + cursor. (No meta row drawn in this state's box.)

### State 03 — @-FILE MENTION (fuzzy, inline)
Same popover chrome. Rows: filename mono 12 + path mono 10.5 `#7f8187` sitting directly after the name
(gap 10, no spacer). Fuzzy-match highlight: matched prefix in accent — selected row `Xterm` **700
accent** + rest `#f3f4f7`; unselected `xterm` accent (not bold) + rest `#a7abb4`; no-match row all
`#a7abb4`. Drawn: `XtermPane.tsx` (src/lib/components, selected), `xterm-options.ts` (src/lib),
`terminalFonts.ts` (src/lib). Input: `fix the joiner in ` + **mention pill** `@Xterm` (accent ink on
accent/14, r4, padding 0 4) + cursor.

### State 04 — AGENT RUNNING (permission lands inline, typing queues)
One composer box, stacked top → bottom:
1. **Permission card** (a Decision): margin 8 8 0 8, padding 8 10, gap 10, bg warn-tint, border 1px
   warn/35, r5. Warning-triangle SVG 14×14 stroke `#d8c082` 1.4 → text stack (gap 2, min-width 0):
   command mono 12 `#f3f4f7` (`rm -rf node_modules && pnpm install`); subtitle ui 10.5 `#8b8f97`
   ("Bash wants to run this in source/") → spacer → **keycaps** mono 10, r4, padding 2 6, bg
   `#191919`: `y allow` (`#f3f4f7`, border white/12) · `n deny` (`#a7abb4`, border white/12) ·
   `a always` (`#7f8187`, border white/07 — visually de-emphasized).
2. **Queued row**: mono, 24px, padding 0 14, 11.5px: `⏳` `#7f8187` → *italic* `#8b8f97`
   `queued — "also bump the addon version"` → spacer → `⌫ unqueue` 10px `#7f8187`.
3. **Input row** (still live while running): 40px, padding 0 14, 13px, **border-top hairline**:
   `◐` accent (regular weight — replaces ❯) → typed text `#f3f4f7` → cursor 7×16 → spacer →
   `esc interrupt` 10px **`#d8c082`**.
4. **Meta row**: 22px: "⏵ auto-edit" chip · spacer · `running 2m14s · turn 6` 10px `#7f8187`.
   (⇧⇥ plan and the @// hints disappear while running.)

---

## 5. Board: Cockpit — "Cockpit — glance view" (900×600) — L2 grammar

App surface `#050505`, `.ui`. *"Glanced = instruments. Graphics are what survive zoom-out: progress,
diff stats, red/green badges."*

**Wall header, 34px** (gap 10, padding 0 12, border-bottom hairline): title "codingOS — swarm" ui 12 /
600 / −0.012em `#a7abb4` (dimmer than pane titles) → spacer → "6 panes" mono 11 `#7f8187` →
"· 1 needs you" mono 11 `#d8c082` (wall-level Decision count, amber).

**Grid**: `3×2, minmax(0,1fr)`, gap 8, padding 10. Cell = bg `#0e0e0e`, border 1px white/07, square,
overflow hidden. Cell header 24px (gap 6, padding 0 8, border-bottom hairline): LED 6px → title 11.5 /
600 (`#f3f4f7` active / `#a7abb4` dimmed) → spacer → right meta mono 9.5. Cell body padding 10, gap 8.

Six cells drawn (the L2 state matrix):

| Cell | Ring | LED | Right meta | Body |
|---|---|---|---|---|
| 1. font-ligatures — **focused, running** | inset 1.5 accent | green | `#214` `#7f8187` | Progress row: bar flex×6px pill, track white/07, **fill 75% accent** + `3/4` mono 10.5 accent. Badge row gap 6: `✓ 41` (10px green on ok-tint, r4, pad 1 6) + diff chip `+18 −2` (10px `#a7abb4` on `#191919`, green/red counts). Spacer. Activity `◐ vitest run — 8s` mono 10 `#8b8f97`. |
| 2. issue-triage — **Decision** | inset 1.5 **amber** | amber | `needs you` badge (9.5 amber on warn-tint, r4, pad 1 5) | Command mono 10.5 `#f3f4f7` (`Bash: gh issue close 212`); "wants approval to run" ui 10 `#8b8f97`; spacer; keycaps `y allow` / `n deny` (mono 10, pad 2 7, r4, `#191919`, border white/12; y `#f3f4f7`, n `#a7abb4`) — **no `a always` at L2**. |
| 3. relay-fix — **failing** | none | red | `#198` `#7f8187` | Progress 40% **fill `#a7abb4`** + `2/5` `#a7abb4`. Badges: `✗ 2 failing` (red on err-tint) + diff chip `+64 −31`. Activity `◐ bisecting ws reconnect`. |
| 4. docs-pass — **done** | none; **whole cell opacity 0.75** | green | `done` 9.5 green, plain (no chip) | `turn complete · $0.31` mono 10 `#8b8f97`; chips: diff `+142 −8` + `In Review` (`#a7abb4` on `#191919`); spacer; `❯ idle` mono 10 `#7f8187`. Title dimmed `#a7abb4`. |
| 5. scratch — **pty** | none | gray `#8b8f97` | `pty` chip (9.5 `#7f8187` on `#191919`, pad 1 5) | Mini terminal: mono 9.5/1.4, gap 3: prompt lines `❯` green + cmd `#8b8f97`; git hashes `#d8c082` + messages `#a7abb4`; final line inverse block cursor (bg accent, ink `#0e0e0e`, one nbsp). Title dimmed. |
| 6. board-sync — **idle** | none | gray | `⌘6` 9.5 `#7f8187` | Body `grid; place-items center`: `❯ idle — waiting for work` mono 10.5 `#7f8187`. Title dimmed. |

**LED semantics**: green = Session running ok (and done), amber = Decision waiting, red = failing,
gray = idle / no live activity. **Title ink**: `#f3f4f7` when the Thread merits attention/is hot,
`#a7abb4` when done/idle/pty. **Focused cell** carries the same accent inset ring as the focused pane;
a Decision overrides the ring to amber.

---

## 6. States — everything that varies

| State | Treatment |
|---|---|
| Focused (Pane or cell) | `inset 0 0 0 1.5px #c7ccd6`; focused Main pane adds the two-layer drop shadow; unfocused = 1px border only |
| Decision waiting | Amber everywhere it appears: inline card (warn-tint bg + warn/35 border + triangle), cell ring `inset 1.5px #d8c082`, amber LED, "needs you" badge, wall count "· 1 needs you" |
| Running | LED green; composer prompt glyph `❯` → `◐`; right hint `esc interrupt` in amber; meta shows `running ‹t› · turn ‹n›`; ⇧⇥/@// hints hidden; L2 activity line `◐ ‹verb phrase› — ‹t›` |
| Queued prompt | 24px row above input: `⏳` + italic `#8b8f97` `queued — "…"` + `⌫ unqueue` |
| Blocked→answer keys | `y allow` (primary ink) / `n deny` (secondary) / `a always` (dim, fainter border); L2 shows only y/n |
| Done | L2: whole cell opacity 0.75, "done" label green, title dimmed, `❯ idle` footer; transcript ends with summary row + (Main) CHANGED chips |
| Failing | Red LED, `✗ N failing` badge, progress fill falls back to `#a7abb4` |
| Idle | Gray LED, dim title, centered `❯ idle — waiting for work`; Composer shows placeholder with cursor parked after `❯` |
| Selection (menus) | Row bg white/07 + r4; command/name ink promotes to accent (+700 on fuzzy match); desc promotes one step (`#7f8187`→`#8b8f97`) |
| Fuzzy match | Matched substring in accent (700 only on the selected row) |
| Mention pill | Accent ink on accent/14, r4, pad 0 4 |
| Mode chip | `⏵ auto-edit` accent on accent/14 — the Session's permission mode, always visible in the meta row |
| Hover | Links only: `#c7ccd6` → `#d7dbe3`; no other hover drawn |
| Typing while idle | Cursor follows typed text; placeholder replaced |

PromptBox's four named states: **01 Idle · 02 Slash-command menu · 03 @-file mention · 04 Agent
running** (permission inline + queue + live input).

---

## 7. Content semantics in Ferrite vocabulary (CONTEXT.md)

| Visual element | Ferrite meaning |
|---|---|
| Main / Dense board | A focused **Pane** at **semantic zoom L1** (transcript + prompt) rendering one **Thread** |
| Cockpit board | The **Cockpit** at **L2** (instruments); each cell is a **Pane**; L2's grammar = progress, diff stats, red/green badges — "what survives zoom-out" |
| Bottom prompt line + its stack | The **Composer** — one growing shell line, keyboard menus, queue-while-busy (matches the glossary definition verbatim) |
| Amber anything (card, ring, LED, badge, wall count) | A **Decision** waiting — the Thread is blocked on something only the operator can answer; answerable from Pane (y/n/a card) and wall badge (y/n), Remote planned |
| Header LED | Thread/Session status (green running-ok, amber Decision, red failing, gray parked/idle) |
| Thread name ("font-ligatures") | The **Thread**'s name; per CONTEXT, likely mirrors its **Workspace binding** (worktree name) — not stated in comps |
| "swarmdeck#214" chip | External issue reference attached to the Thread |
| "claude · fable" chip / "fable-5 · max" | The **Provider** (claude) and its selected model+plan for this Thread's **Session** — Provider ≠ model per glossary |
| ctx meter / "ctx 62%" + "$0.84" | Live **Session** instruments: context-window usage and cost |
| ▰▰▰▱ 3/4 + step text + "todo" | The agent's plan/todo progress for the current run (harness todo stream) |
| ⏵ auto-edit chip / ⇧⇥ plan | The Session's permission mode and the mode-switch affordance |
| ❯ vs ⏺ gutters | Operator turn vs agent turn in the Thread history |
| `Tool(arg)` + `⎿ result` | A tool call the Provider's harness streamed, with its result event |
| Queued row | Composer queue-while-busy: prompts typed during a run wait their turn |
| "6 panes · 1 needs you" | Cockpit population + count of Threads holding Decisions |
| "In Review" chip | Issue/board stage of the done Thread's work (see open questions — tracker integration is post-v1) |
| Done cell "turn complete · $0.31" | Session turn finished; cost of the run |
| − / × header icons | Pane window controls — presumably zoom-down / close-Pane-park-Thread (see open questions) |
| "scratch · pty" cell | A shell/terminal pane — **contradicts** CONTEXT.md's "No Terminal" decision (see open questions) |
| ⌘6 | Pane hotkey — keyboard navigation to Pane 6 |

---

## 8. Cross-board deltas (after factoring shared tokens)

| Aspect | Main | Dense (transcript-canonical) | PromptBox | Cockpit |
|---|---|---|---|---|
| Base font | ui; transcript mono 13/1.55 | mono everywhere 12.5/1.45 | ui chrome, mono content, input 13 | ui chrome, mono data |
| Header | 38px + separate 28px todo strip | single 28px merged row, 11px | — | wall 34px; cell 24px |
| Transcript row gap | 10 | 4 | — | — |
| Transcript padding | 12 14 | 8 12 | — | cell body 10 |
| Tool call | icon row (chevron, 20px, right meta) | `⏺ Bold(arg)` + `⎿` continuation | — | — |
| Tool indent | 24 | 22 | — | — |
| Diff | bordered card r5 + filename header + intraline emphasis | bare, indented, no card | — | chip `+N −M` only |
| Inline code radius | 4 | 3 | — | — |
| Diff num col | 26 | 30 | — | — |
| Composer | 40px input + 22px meta row | single 32px row, chips inline right | 40+22 (= Main) | — |
| Cursor | 7×16 | 7×15 | 7×16 | inverse block (pty) |
| LED | 7px | 6px | — | 6px |
| ctx display | 64×4 bar | text "ctx 62%" | — | — |
| Pane chrome | ring + shadows | border only | boxes r5 (demo) | ring on focus only |

Precedence resolution: transcript anatomy (§3.2) from Dense; pane structure (header/todo/composer
placement, changed-files footer, window controls) from Main; Composer states from PromptBox; L2 from
Cockpit.

---

## 9. Open questions (underspecified in the comps — do not invent)

1. **"scratch · pty" cell vs CONTEXT.md "No Terminal."** The Cockpit board draws a live shell pane
   (git log, inverse cursor, `pty` chip); CONTEXT.md explicitly forbids any terminal/PTY concept in v1.
   Is this cell aspirational/legacy set-dressing, or a deliberate future "shell pane" concept? As specced,
   it cannot ship in v1.
2. **"In Review" chip** on the done cell implies board/issue-stage display, but settled decisions say
   issue-tracker integration is post-v1. Drop, or source from elsewhere?
3. **Which header wins for a focused L1 Pane** — Main's 38px header + 28px todo strip, or Dense's single
   28px merged row? The designer's note gives Main "overall pane structure," but Dense demonstrates a
   denser header too. Plausibly Main = large/focused, Dense = smaller L1 panes; not stated. Same
   question for the composer (40+22 two-row vs 32 single-row) and ctx as bar vs text.
4. **Progress-fill color rule**: accent `#c7ccd6` on the focused/green cell but `#a7abb4` on the
   unfocused/failing cell. Tied to focus, or to health? One data point each.
5. **Window-control semantics**: − and × in the Main pane header — minimize = zoom to L2? close = park
   Thread (per "closed Pane = parked Thread")? Unlabeled.
6. **Motion**: nothing animated is specified — cursor blink, ◐ spin/step, popover entrance, ring
   transitions all undefined (static comps).
7. **Hover**: only link hover exists. Chips, menu rows, icon buttons, Cockpit cells have no drawn hover.
8. **Scroll**: every board is `overflow: hidden`; scrollbar treatment for a long transcript is unspecced.
9. **Two provider/model indicators** coexist (header "claude · fable" chip; composer "fable-5 · max").
   Both persistent, or header = Provider and composer = current model/plan detail? Redundancy unresolved.
10. **`a always` scope**: present in the L1 permission card, absent at L2 (y/n only). Deliberate
    reduction or omission?
11. **L3 (wall: status LED + one signal)** is not drawn on any board — no comp exists for the 24-pane wall
    zoom level.
12. **Multiple queued prompts**: one queued row drawn; stacking order/limit for several unspecified.
13. **Light theme**: none; the system is dark-only as drawn (`#050505` world).
14. **Weight 500** loaded for both families but never used — intended for something, or dead weight?
15. **Focused-pane drop shadow**: Main floats on `#050505` with elevation shadows — is that a real
    "focused pane lifts above the grid" treatment, or artboard presentation? (Cockpit's focused cell has
    ring only, no shadow.)
16. **Error/exceptional states** beyond "failing tests": Session crash, watchdog restart ("restarts
    leaking Sessions visibly" per CONTEXT), provider auth loss, ctx-full — none drawn.
17. **Placeholder pattern** "message ‹thread-name› — …" shown once; behavior when the Thread name is long
    or the pane narrow is unspecified.
18. **Popover placement**: comps stack the menu above the input inside the flow; whether it overlays the
    transcript (absolute) or pushes content is not drawn.

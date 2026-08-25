# Glance / semantic-zoom spec — extracted from ZoomLadder.dc.html + Wall.dc.html

Design study, read-only. Every value below is transcribed from the two comps; nothing invented.
Sources:

- `comps/ZoomLadder.dc.html` — "Semantic zoom — one pane, three renderings" (L3/L2/L1 side by side)
- `comps/Wall.dc.html` — "24-pane wall — L3 scale"

Reading guide: in ZoomLadder, the page title, the `L3 · UNDER 200PX` caption labels and the descriptive
paragraphs under each cell are **annotation chrome** (designer notes), not product UI. In Wall, the top bar
("codingOS — full swarm · 24 panes · 3 need you") and the bottom legend **are product UI** — they are part
of the wall design itself.

---

## 1. Shared token tables

### 1.1 Color

Identical palette across both boards. Dark-only; no light theme exists. **No blue anywhere** — the shipped
app's "blue number" has no counterpart; the neutral accent is a cool gray.

| Role | Value | Where used |
|---|---|---|
| Canvas / app bg | `#050505` | `body` and root frame of both boards |
| Pane surface | `#0e0e0e` | Every pane/tile background, all three levels |
| Prompt-line bg | `#161616` | L1 live prompt row only |
| Diff-badge bg | `#191919` | Diff-stat badge fill (L2, L1) |
| Pane border | `rgba(255,255,255,0.07)` | 1px solid border on every pane/tile; **also** the progress-bar track fill (same token, two uses) |
| Hairline separator | `rgba(255,255,255,0.045)` | L2/L1 header underline, L1 transcript top rule, L1 prompt top rule, wall top-bar bottom border, wall legend top border |
| Text primary | `#f3f4f7` | Board title (13px), L2/L1 pane name, **focused** wall-tile name |
| Accent (neutral bright) | `#c7ccd6` | Progress-bar fill, step fraction ("3/4"), prompt `❯`, **focus ring**, link color |
| Accent hover | `#d7dbe3` | `a:hover` only |
| Text secondary | `#a7abb4` | Default L3 tile name, diff-badge base text, wall top-bar title |
| Text tertiary | `#8b8f97` | Annotation paragraphs, L2 activity line, L1 transcript lead glyphs (`⏺` `▸`), **idle dot** |
| Text faint | `#7f8187` | L3 status lines, thread id `#214`, ladder caption labels, L1 header meta, transcript body text, "24 panes" count, legend text |
| Green — ok/running | `#7fc99b` | Running dot, `✓ 41` test badge text, `+18`/`+2` additions, done-state text (`✓ done · …`), legend "working"/"done" swatches |
| Green badge bg | `rgba(127,201,155,0.13)` | Test-pass badge fill |
| Amber — Decision | `#d8c082` | Decision dot, decision ring, `⚠ needs you` / `⚠ plan ready` text, top-bar "· 3 need you" |
| Red — blocker/fail | `#e08c84` | Blocker dot, blocker ring, `✗ wrangler 403` / `✗ 2 failing` text, `−2`/`−1` deletions |

### 1.2 Typography

Two families only.

| Family | Stack | Weights loaded | Weights used | Used for |
|---|---|---|---|---|
| Geist (`.ui`) | `"Geist", -apple-system, system-ui, sans-serif` | 400/500/600 | 400, 600 | Pane names, board/top-bar titles, annotation prose |
| JetBrains Mono (`.mono`) | `"JetBrains Mono", "SF Mono", Menlo, monospace` | ZoomLadder: 400/500/700 + italic 400; Wall: 400/500/700 | 400, 700 | Everything numeric/status: status lines, ids, fractions, badges, activity, transcript, prompt, legend, caption labels |

(500 and italic-400 are loaded but never used — vestigial. Wall tile name class `.tn` hard-codes Geist even though the mono class could cascade.)

Font metrics inventory (size / weight / letter-spacing / line-height / color):

| Text | px | wt | ls | lh | color |
|---|---|---|---|---|---|
| ZoomLadder board title | 13 | 600 | −0.012em | — | #f3f4f7 |
| ZoomLadder board subtitle | 11 | 400 | — | — | #8b8f97 |
| Wall top-bar title | 12 | 600 | −0.012em | — | #a7abb4 |
| Wall top-bar counts ("24 panes" / "· 3 need you") | 11 mono | 400 | — | — | #7f8187 / #d8c082 |
| L3 tile name (both boards) | 10 | 600 | — | — | #a7abb4 (focused: #f3f4f7), nowrap+ellipsis |
| L3 status line | 9 mono | 400 | — | — | #7f8187, nowrap+ellipsis |
| L3 alert line (decision/blocker first line) | **10** mono | 400 | — | — | #d8c082 or #e08c84 |
| L2/L1 pane name | 11.5 | 600 | — | — | #f3f4f7 |
| Thread id `#214` | 9.5 mono | 400 | — | — | #7f8187 |
| L1 header meta `$0.84 · 62%` | 9.5 mono | 400 | — | — | #7f8187 |
| Step fraction `3/4` (L2/L1) | 10.5 mono | 400 | — | — | #c7ccd6 |
| Test badge `✓ 41` | 10 mono | 400 | — | — | #7fc99b on rgba(127,201,155,0.13) |
| Diff badge `+18 −2` | 10 mono | 400 | — | — | #a7abb4 base; +#7fc99b, −#e08c84; on #191919 |
| L2 activity line `◐ vitest run — 8s` | 10 mono | 400 | — | — | #8b8f97 |
| L1 transcript lines | 10 mono | 400 | — | 1.5 | body #7f8187; lead glyph #8b8f97; inline ✓/+ green, − red |
| L1 prompt `❯` | 11 mono | **700** | — | — | #c7ccd6 |
| L1 prompt placeholder | 11 mono | 400 | — | — | #7f8187, "type to steer — focuses on ↵" |
| Legend items | 9.5 mono | 400 | — | — | #7f8187 text, colored ● swatches |
| Ladder caption label (`L3 · UNDER 200PX`) | 10 mono | 400 | +0.10em | — | #7f8187 (annotation) |
| Ladder caption prose | 10.5 | 400 | — | 1.45 | #8b8f97 (annotation) |

### 1.3 Borders, radii, shadows, opacity

| Property | Value |
|---|---|
| Pane/tile border | `1px solid rgba(255,255,255,0.07)`, all levels |
| Pane/tile corner radius | **none specified → 0 (square corners)** |
| Dot radius | `999px` (perfect circle) |
| Progress bar radius | `999px` (pill), `overflow: hidden` |
| Badge radius | `4px` |
| Attention/focus ring | `box-shadow: inset 0 0 0 1.5px <color>` — the ONLY shadow in either comp. Colors: `#c7ccd6` focused, `#d8c082` decision, `#e08c84` blocker. Drawn inside the 1px border (border + inner ring stack). No drop shadows, no glows. |
| Done tile | whole-tile `opacity: 0.6` |
| Legend "done (dimmed)" item | `opacity: 0.7` on the legend entry itself |
| All panes | `overflow: hidden` |

### 1.4 Glyph vocabulary (all literal text characters, mono)

| Glyph | Meaning |
|---|---|
| CSS dot (5–6px circle) | Thread state LED |
| `●` | Legend swatch (text char, 9.5px) |
| `◐` | Activity in progress (running tool/phase): `◐ vitest`, `◐ profiling panes`, `◐ bisecting`, `◐ check`, `◐ editing`, `◐ wiring store`, `◐ container qs` |
| `⚠` | Needs the operator (Decision waiting) |
| `✓` | Success — tests passing / done |
| `✗` | Failure — failing tests / hard blocker |
| `❯` | Shell prompt: idle pane status and the L1 live prompt line |
| `⏺` | Transcript: agent utterance (L1) |
| `▸` | Transcript: tool call (L1) |
| `↵` | Enter key (prompt placeholder) |
| `·` | Field separator everywhere |

---

## 2. ZoomLadder — the three renderings

Canvas: 900×430, bg #050505, padding 18px, column gap 16px. Ladder row: `align-items: flex-end`, gap 28px
between the three columns (annotation layout). Each column: cell + caption, gap 8px.

**Breakpoints (from caption labels + subtitle):** the pane picks its rendering from its **measured tile
size via container queries** — no mode switch, no user setting; resize the grid and every pane re-renders.

| Level | Threshold label | Example cell size | Aspect |
|---|---|---|---|
| L3 | UNDER 200PX | **160 × 100** | 1.60 |
| L2 | 200–380PX | **280 × 176** | 1.59 |
| L1 | OVER 380PX | **400 × 264** | 1.52 |

Thresholds are single numbers; example widths (160/280/400) fit them, but the query axis (width vs
height vs both) is not stated — see Open questions.

### 2.1 L3 cell (160×100) — "One signal only. Name, state dot, progress. A status LED."

Structure (flex column, bg #0e0e0e, border 1px rgba(255,255,255,0.07), overflow hidden, radius 0):

| Region | Metrics | Content |
|---|---|---|
| Header row | height 20px, flex-none, align-center, gap 5px, padding 0 7px | dot 5×5 #7fc99b; name 10px/600 #a7abb4, ellipsis — `font-ligatures` |
| Body | flex 1, column, **justify-content: center**, gap 6px, padding 0 7px 6px 7px | progress bar + status line |
| Progress bar | **height 6px**, pill, track rgba(255,255,255,0.07); fill span `width: 75%` #c7ccd6 | |
| Status line | 9px mono #7f8187 | `3/4 · ◐ vitest` |

### 2.2 L2 cell (280×176) — "Instruments … the cockpit default."

| Region | Metrics | Content |
|---|---|---|
| Header | height 24px, gap 6px, padding 0 8px, border-bottom 1px rgba(255,255,255,0.045) | dot 6×6 #7fc99b · name 11.5px/600 #f3f4f7 `font-ligatures` · spacer · id `#214` 9.5px mono #7f8187 **right-aligned** |
| Body | flex 1, column, gap 8px, padding 10px | three instrument rows + bottom-pinned activity |
| Row 1 — progress | flex, gap 8px | track flex-1 h6 pill rgba(255,255,255,0.07), fill 75% #c7ccd6; fraction `3/4` 10.5px mono #c7ccd6 |
| Row 2 — badges | flex, gap 6px | test badge `✓ 41` (10px mono #7fc99b, bg rgba(127,201,155,0.13), r4, pad 1px 6px); diff badge `+18 −2` (10px mono #a7abb4 on #191919, r4, pad 1px 6px; + green, − red) |
| Spacer | flex 1 | pushes activity to bottom |
| Activity line | 10px mono #8b8f97 | `◐ vitest run — 8s` |

Instruments present at L2: **progress bar + step fraction, test badge, diff stat, current-activity line.**
No sparkline exists in either comp. Text surviving at L2: pane name, thread id, fraction, badge numerals,
one activity sentence fragment. No transcript, no prompt, no cost.

### 2.3 L1 cell (400×264) — "Instruments + transcript tail + live prompt line."

| Region | Metrics | Content |
|---|---|---|
| Header | height 24px, gap 6px, pad 0 8px, border-bottom hairline | dot 6×6 green · name 11.5px/600 #f3f4f7 · id `#214` 9.5px mono (now **left**, right after name) · spacer · meta `$0.84 · 62%` 9.5px mono #7f8187 (cost + presumably context-window %) |
| Instruments row | flex-none, gap 8px, **padding 8px 10px** | same four instruments as L2 compressed to one row: track flex-1 h6 fill 75% · `3/4` 10.5px · `✓ 41` badge · `+18 −2` badge |
| Transcript tail | flex 1, column, gap 4px, padding 4px 10px, 10px mono, lh 1.5, border-top hairline | 4 lines, body #7f8187, lead glyph #8b8f97: `⏺ Ligatures render in the canvas path — => joins correctly.` / `▸ Bash pnpm check && vitest run ✓ 41` (✓ 41 green) / `▸ Edit XtermPane.tsx +2 −1` (+green −red) / `⏺ Wiring @xterm/addon-ligatures into the canvas path…` |
| Live prompt line | flex-none, **height 26px**, gap 8px, pad 0 10px, border-top hairline, **bg #161616** | `❯` 11px mono **700** #c7ccd6 · placeholder `type to steer — focuses on ↵` 11px mono #7f8187 |

Stack proportions at 400×264: header 24 + instruments ≈22 (8+6+8) + transcript ≈flex (~192) + prompt 26.
Transcript tail owns roughly 73% of the pane height. Designer note: "L1's prompt line accepts typing at
glance distance"; the placeholder specifies the interaction: typing steers, Enter focuses the pane.

---

## 3. Wall — 24 panes at L3

Canvas: 900×560, bg #050505, flex column: top bar / grid / legend.

### 3.1 Top bar (product chrome)

Height **34px**, flex-none, gap 10px, padding 0 12px, border-bottom 1px rgba(255,255,255,0.045).

- Left: `codingOS — full swarm` — 12px/600, ls −0.012em, **#a7abb4** (secondary, not primary).
- Right: `24 panes` 11px mono #7f8187, then `· 3 need you` 11px mono **#d8c082** (amber; the interpunct is
  inside the amber span). The rollup counts amber + red ringed tiles (2 Decisions + 1 blocker = 3);
  the tile with failing tests but no ring is NOT counted.

### 3.2 Grid geometry

`grid-template-columns: repeat(6, minmax(0,1fr))`, `grid-template-rows: repeat(4, minmax(0,1fr))`,
**gap 6px, padding 8px**, filling 900×(560−34−30).

Computed tile size: width (900−16−30)/6 = **142.33px**; height (496−16−18)/4 = **115.5px**.
Interior content width = 142.33 − 2(border) − 16(padding) ≈ **124px**. Both dimensions < 200 ⇒ L3.

### 3.3 Wall tile classes (the canonical dense-L3 recipe)

| Class | Rule |
|---|---|
| `.t` tile | flex column, **gap 6px**, bg #0e0e0e, border 1px rgba(255,255,255,0.07), **padding 8px**, overflow hidden. Content is **top-anchored** (no justify) |
| `.th` header | flex, align-center, gap 5px, min-width 0 |
| `.td` dot | 5×5, radius 999px, flex-none |
| `.tn` name | 10px/600 #a7abb4, nowrap + ellipsis, Geist |
| `.tb` bar | **height 5px**, pill, track rgba(255,255,255,0.07), overflow hidden; fill span `width:N%` #c7ccd6 |
| `.ts` status | 9px mono #7f8187, nowrap + ellipsis |

### 3.4 What a 24-pane L3 cell actually contains

Per tile, in stacking order (6px gaps):

1. **Header:** state dot (5px circle) + **the Thread's slug name** (10px Geist semibold, ellipsized).
   Identity is the **name, never a number** — no thread numbers exist anywhere on the wall; `#214`
   appears only at L2/L1. Position in the grid is the secondary identity cue.
2. **Progress bar** (running tiles only): 5px pill, #c7ccd6 fill at the plan fraction.
3. **Status line(s):** one 9px mono line normally; alert tiles (Decision/blocker) carry **two** lines —
   line 1 a 10px colored alert (`⚠ needs you` amber / `✗ wrangler 403` red), line 2 a 9px default-color
   context (`gh issue close 212`, `approve to build`, `blocked · open`). Alert tiles have **no bar**.
4. **Overlays:** inset 1.5px ring (focus #c7ccd6 / decision #d8c082 / blocker #e08c84); done tiles at
   opacity 0.6. No ring on running/done/idle tiles.

There is **no per-tile ring/gauge graphic and no sparkline** — "ring" in the legend means the tile's
inset border ring. Graphics inventory at L3 = dot, bar, ring, dimming, colored glyph text. Amber and red
read via a **triple redundancy**: dot color + 1.5px full-perimeter ring + colored 10px first status line —
this is what survives when 9px text dissolves at distance.

### 3.5 Full 24-tile inventory (row-major, 6×4)

| # | r,c | Name | Ring | Dot | Bar % | Status line(s) | State |
|---|---|---|---|---|---|---|---|
| 1 | 1,1 | ligatures | #c7ccd6 focus | green | 75 | `3/4 · ◐ vitest` (name #f3f4f7) | working + focused |
| 2 | 1,2 | issue-triage | #d8c082 | amber | — | `⚠ needs you` amber 10px / `gh issue close 212` | Decision |
| 3 | 1,3 | relay-fix | — | green | 40 | `✗ 2 failing` **red text** | working, tests failing |
| 4 | 1,4 | docs-pass | — | green | — | `✓ done · review` green; tile opacity .6 | done |
| 5 | 1,5 | theme-presets | — | green | 20 | `1/5 · reading` | working |
| 6 | 1,6 | scratch | — | gray | — | `pty · ❯ idle` | idle (raw pty) |
| 7 | 2,1 | perf-receipt | — | green | 55 | `◐ profiling panes` | working |
| 8 | 2,2 | mobile-cockpit | #d8c082 | amber | — | `⚠ plan ready` amber 10px / `approve to build` | Decision |
| 9 | 2,3 | e2e-flake | — | green | 62 | `◐ bisecting` | working |
| 10 | 2,4 | adr-0013 | — | green | — | `✓ done · $0.22` green; opacity .6 | done |
| 11 | 2,5 | board-poller | — | green | 35 | `2/6 · tests` | working |
| 12 | 2,6 | worktree-gc | — | gray | — | `❯ idle` | idle |
| 13 | 3,1 | prompt-queue | — | green | 85 | `4/5 · ◐ check` | working |
| 14 | 3,2 | diff-cards | — | green | 48 | `◐ editing` | working |
| 15 | 3,3 | relay-deploy | **#e08c84** | red | — | `✗ wrangler 403` red 10px / `blocked · open` | blocked |
| 16 | 3,4 | font-picker | — | green | — | `✓ done · merged` green; opacity .6 | done |
| 17 | 3,5 | osc133 | — | green | 15 | `1/7 · reading` | working |
| 18 | 3,6 | spare-01 | — | gray | — | `❯ idle` | idle |
| 19 | 4,1 | todo-strip | — | green | 66 | `◐ wiring store` | working |
| 20 | 4,2 | glance-lod | — | green | 30 | `◐ container qs` | working |
| 21 | 4,3 | decision-rail | — | green | 52 | `3/6 · tests` | working |
| 22 | 4,4 | stream-json | — | green | — | `✓ done · review` green; opacity .6 | done |
| 23 | 4,5 | spare-02 | — | gray | — | `❯ idle` | idle |
| 24 | 4,6 | spare-03 | — | gray | — | `❯ idle` | idle |

Census: 12 working (1 focused, 1 with failing tests), 2 Decision, 1 blocked, 4 done, 5 idle. Top-bar
"3 need you" = 2 amber + 1 red.

Bar semantics: when a `n/m` fraction is shown, bar ≈ n/m with slight over-fill for partial-step progress
(2/6→35%, 4/5→85%, 1/7→15%, 3/6→52%; 3/4→75%, 1/5→20% exact). Six bars have no fraction, only a `◐`
activity phrase — bar source in that case unstated.

### 3.6 Pinned legend (product chrome)

Height **30px**, flex-none, align-center, **gap 14px**, padding 0 12px, border-top 1px
rgba(255,255,255,0.045). All items 9.5px mono #7f8187 with `●` swatches:

- `● working` (● #7fc99b)
- `● needs you` (● #d8c082)
- `● blocked / failing` (● #e08c84)
- `● done (dimmed)` (● #7fc99b, whole item opacity 0.7)
- `● idle` (● #8b8f97)
- spacer, then right-aligned: `ring = focused · amber ring = decision · red ring = blocker`

---

## 4. State matrix

### L3 (Wall — authoritative)

| State | Dot | Ring (inset 1.5px) | Bar | Status text | Tile opacity | Name |
|---|---|---|---|---|---|---|
| Working | #7fc99b | none | yes, #c7ccd6 fill | 9px #7f8187: `n/m · ◐ activity` or `◐ activity` | 1 | #a7abb4 |
| Working + focused | #7fc99b | **#c7ccd6** | yes | same | 1 | **#f3f4f7** |
| Working, tests failing | #7fc99b | none | yes | `✗ n failing` in **#e08c84** | 1 | #a7abb4 |
| Decision waiting | **#d8c082** | **#d8c082** | **none** | line1 10px #d8c082 `⚠ …`; line2 9px #7f8187 context | 1 | #a7abb4 |
| Blocked / failed | **#e08c84** | **#e08c84** | **none** | line1 10px #e08c84 `✗ …`; line2 `blocked · open` | 1 | #a7abb4 |
| Done | #7fc99b | none | none | `✓ done · <review\|merged\|$cost>` in #7fc99b | **0.6** | #a7abb4 |
| Idle | **#8b8f97** | none | none | `❯ idle` (pty: `pty · ❯ idle`) | 1 | #a7abb4 |

No "parked" state is drawn anywhere; done+dimmed and idle are the only de-emphasized states.

### L2 / L1

Only the **working** state is drawn (green dot, 75% bar, `✓ 41`, `+18 −2`, activity / transcript /
prompt). Decision, blocked, done, idle at L2/L1 are **unspecified** — open question.

---

## 5. Semantics in Ferrite vocabulary

| Graphic | Meaning |
|---|---|
| Tile / pane cell | A **Pane** hosting one **Thread**; the full grid is the **Cockpit** zoomed out to a wall |
| Dot (LED) | Thread lifecycle state: green running, amber waiting on a **Decision**, red blocked, gray idle (no Thread activity / bare pty), green+dim done |
| Inset 1.5px ring | Attention channel, independent of the dot: neutral = the operator's focused Pane; amber = a Decision awaits the operator; red = hard blocker needing the operator |
| Progress bar | Thread plan progress (fraction of plan steps, `n/m`) |
| `n/m` fraction | Plan steps completed / total |
| `◐ phrase` | The Thread's current activity (tool or phase now running) |
| Test badge `✓ 41` | Latest test-run result for the Thread's worktree |
| Diff badge `+18 −2` | Thread's accumulated diff stat |
| `$0.84 · 62%` | Thread cost so far · (presumably) context-window fill — unlabeled |
| `#214` | Thread id — shown at L2/L1 only, never on the wall |
| Transcript tail | Last N Thread events: `⏺` agent message, `▸` tool call, with inline result colors |
| `❯` prompt line (L1) | Live steering input into the Thread; typing steers, `↵` focuses the Pane |
| `❯ idle` | Pane's shell waiting; no Thread running |
| Dimmed tile | Thread finished; awaiting review/merge, out of the attention economy |
| Top-bar `3 need you` | Count of Panes whose ring is amber or red — the Cockpit's interrupt count |

---

## 6. Delta vs the shipped L3 wall

Shipped (rejected): LED + **thread number** + one word → "24 panes with a blue number".

Comp L3 cell replaces it with:

1. **Slug name instead of number** — 10px Geist-600, ellipsized; numbers banished to L2+. No blue exists
   in the palette at all; the bright neutral is #c7ccd6.
2. **A real instrument**: 5px progress bar with measured fill — motion/state visible with zero text.
3. **A status line with glyph grammar** (`n/m`, `◐`, `✓`, `✗`, `⚠`, `❯`) — still legible up close,
   disposable at distance.
4. **A second channel for urgency**: full-perimeter inset amber/red ring + colored dot + colored alert
   line, so "needs you" reads peripherally when all text is gone.
5. **Attention accounting as chrome**: dimmed done tiles, top-bar `24 panes · 3 need you` rollup, and a
   pinned legend teaching the encoding.

---

## 7. Discrepancies between the two boards' L3 (both are "L3")

| Property | ZoomLadder L3 (160×100) | Wall tile (~142×116) |
|---|---|---|
| Padding scheme | header row h20 pad 0 7px; body pad 0 7px 6px | uniform 8px tile padding |
| Vertical anchor | body **centered** (justify-content: center) | **top-anchored** stack |
| Bar height | **6px** | **5px** |
| Internal gaps | body gap 6px; header gap 5px | tile gap 6px; header gap 5px |
| States shown | running only | full matrix |

Wall is the denser, state-complete recipe; ZoomLadder L3 matches L2/L1's 6px bar. Which is canonical is
unspecified.

---

## 8. Open questions (nothing invented — all left open)

1. **Container-query axis**: "UNDER 200PX / 200–380PX / OVER 380PX" — width, height, or min(w,h)?
   Example cells scale both axes; labels give one number.
2. **Canonical L3 metrics**: 6px centered bar + header-row layout (ZoomLadder) vs 5px top-anchored uniform-padding tile (Wall) — see §7.
3. **L2/L1 non-running states**: Decision/blocked/done/idle renderings at L2/L1 are never drawn. Do rings, two-line alerts, and dimming carry up the ladder? Does a Decision at L1 show the Decision content?
4. **Ring collisions**: focused + Decision (or focused + blocked) simultaneously — which ring wins, or do they combine?
5. **Bar source without `n/m`**: six working tiles show a bar but no fraction — what quantity drives fill?
6. **Bar over-fill**: fractions like 2/6→35% imply intra-step progress; the rule is unstated.
7. **"62%"** in `$0.84 · 62%` is unlabeled — context-window fill is a guess.
8. **`#214` at L2 right vs L1 left**: header slot swap is deliberate (L1 right slot = cost/context meta)? And should L3 ever show the id (Wall says no)?
9. **Failing-tests vs blocked**: legend groups them (`● blocked / failing` red) but relay-fix draws failing tests with a green dot, red text, no ring, excluded from "need you". Is red-dot/ring strictly "hard blocker"?
10. **Decision/blocked tiles drop the progress bar** — deliberate replacement (alert supersedes bar) or sample coincidence?
11. **Corner radius**: none specified anywhere — confirm square panes are intended.
12. **Animation**: is `◐` static or a spinner? Does the ring/dot pulse? Comps are static.
13. **Legend persistence**: does the pinned legend exist only at wall/L3 zoom, or always? ZoomLadder shows no product legend.
14. **Idle taxonomy**: `pty · ❯ idle` vs `❯ idle` — is "pty" a distinct pane kind marker?
15. **"done · review / merged / $0.22" suffixes**: is the suffix a lifecycle field (review→merged) plus optional cost, or freeform?
16. **Focused-tile name promotion** (#f3f4f7) is shown once — does focus also brighten anything else?
17. **Theme**: dark-only. No light-mode tokens exist.
18. **Overflow**: names/status ellipsize; behavior when a tile is even narrower than ~124px content (e.g. denser walls) unspecified.

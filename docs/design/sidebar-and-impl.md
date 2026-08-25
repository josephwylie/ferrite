# Aperture: left nav, fullscreen, and the implementation map

Design study for the two surfaces the Rich Pane canvas does not cover — the left nav bar
(josephwylie/ferrite#21) and the fullscreen Pane (#20, cmd-f) — plus a map of the whole
visual system onto the shipped crates. Read-only study; no code was changed.

**Canon sources** (operator-accepted artboards): `Cockpit.dc.html`, `Main.dc.html`,
`DirectionDense.dc.html` (canon for transcript density), `PromptBox.dc.html`,
`ZoomLadder.dc.html`, `Wall.dc.html`. The other `Direction*` boards and
`DecisionRail.dc.html` are rejected and nothing below draws from them.

**Vocabulary** is CONTEXT.md's: Thread (durable conversation), Session (live provider
process), Pane (view cell), Cockpit (the grid), Composer (prompt line), Provider
(claude/codex), Decision (operator-blocked question). The nav lists **Threads**, never
"tabs" or "files".

---

## 1 · Aperture tokens — the palette the comps already agree on

Every color below appears verbatim in at least two canon boards. Names are roles, not
hues, so the retune in #22 is a values change, not a rename.

### Grounds
| Token | Value | Where the comps use it |
|---|---|---|
| `GROUND` | `#050505` | window/canvas behind everything (already `BG_WINDOW`) |
| `SURFACE` | `#0e0e0e` | Pane body (already `BG_PANE`) |
| `INSET` | `#0a0a0a` | one step down: todo strip, code/diff card interiors, **nav bar** |
| `RAISED` | `#191919` | chips, menus, inline-code pills |
| `COMPOSER` | `#161616` | the Composer's own ground |

### Lines
| Token | Value | Use |
|---|---|---|
| `HAIRLINE` | `rgba(255,255,255,0.045)` | internal dividers (header/status separators) |
| `EDGE` | `rgba(255,255,255,0.07)` | Pane/card outer border, progress track, **row hover** |
| `EDGE_STRONG` | `rgba(255,255,255,0.12)` | focused-card border, chips with borders, Composer top |

### Ink
| Token | Value | Use |
|---|---|---|
| `INK` | `#f3f4f7` | primary text, Prompt lines, running Thread names |
| `INK_SECONDARY` | `#a7abb4` | agent prose, dimmed titles |
| `INK_TERTIARY` | `#8b8f97` | captions, activity lines |
| `INK_MUTED` | `#7f8187` | labels, hints, idle |
| `INK_FAINT` | `#54575f` | line numbers, separators `·`, comments, parked hints (absorbs today's `TEXT_THINKING 0x5a5d63`) |

### Accent + signals
| Token | Value | Use |
|---|---|---|
| `ACCENT` | `#c7ccd6` | ❯ prompt glyph, links, progress fill, cursor, **focus ring** |
| `ACCENT_HOVER` | `#d7dbe3` | link hover |
| `ACCENT_WASH` | `rgba(199,204,214,0.14)` | mode chip (`⏵ auto-edit`), @-mention pill |
| `GOOD` | `#7fc99b` | running LED, pass badge, diff `+` (replaces both `LED_RUNNING 0x6fa8dc` and `DIFF_ADDED 0x7fb069`) |
| `GOOD_WASH` | `rgba(127,201,155,0.13)` | pass/exit-ok chip ground, diff `+` line ground |
| `GOOD_STRONG` | `rgba(127,201,155,0.30)` | intra-line diff emphasis (v2 — see §5.3) |
| `WAIT` | `#d8c082` | Decision amber: LED, ring, "needs you" (replaces `TEXT_NOTICE 0xd9a05b`) |
| `WAIT_WASH` | `rgba(216,192,130,0.13)` | Decision card ground, needs-you chip ground |
| `WAIT_EDGE` | `rgba(216,192,130,0.35)` | Decision card border |
| `FAIL` | `#e08c84` | fail badge, blocked LED/ring, diff `−` (replaces `DIFF_REMOVED 0xcf6f6f`) |
| `FAIL_WASH` | `rgba(224,140,132,0.13)` | fail chip ground, diff `−` line ground |
| `IDLE` | `#8b8f97` | idle/parked LED |

### Code classes (maps `transcript::Class`)
`Plain → ACCENT (#c7ccd6)` · `Keyword → #8fb3d9` · `Str → #9ec78a` ·
`Number → #d8c082` · `Comment → INK_FAINT (#54575f)`.

### Geometry & type
| Token | Value | Notes |
|---|---|---|
| `GAP_WALL / GAP` | 6 / 8 px | grid gaps (wall / cockpit); grid padding 8–10 |
| `PANE_HEADER_H` | 24 px | Pane title row (Main's focused header grows to 38 at L1 — #22) |
| `STRIP_H` | 34 px | Cockpit top strip |
| `COMPOSER_H / HINT_H` | 40 / 22 px | Composer input row / hint row below it |
| `LED / LED_WALL` | 6 / 5 px | status dot diameter |
| `BAR_H` | 6 px (5 at wall) | progress pill; track `EDGE`, fill `ACCENT`, radius 999 |
| `R_CHIP / R_CARD` | 4 / 5 px | chips / cards. **Pane cells are square** (drop `rounded_sm`) |
| `RING` | inset 1.5 px | focus `ACCENT`, Decision `WAIT`, blocker `FAIL` — drawn as an inner overlay quad (gpui has no inset shadow): absolute full-size child, `border(px(1.5))`, no hit-test |
| Type | title 11.5/600 · body mono 12.5 lh 1.45 (per DirectionDense) · chip 10 · chip-sm 9.5 · CAPS label 10 ls 0.10em · wall status 9 |
| Faces | `FONT_MONO` = Menlo / Consolas (today's `MONO_FONT`), `FONT_UI` = system UI — #22 already settles the JetBrains Mono/Geist → shipping mapping; bundling is its own decision |

Dimming: a done/parked cell at `opacity 0.6–0.75` (Wall/Cockpit boards) — spec 0.7.

---

## 2 · Left nav bar — the Thread list (#21)

**Stance.** The nav is a column of glance instruments, not a file tree: each row is a
Wall cell flattened to one line — LED, name, binding, one right-hand signal. No
disclosure triangles, no nesting, no icons beyond the LED and the amber chip. Rows keep
stable positions (creation order); the amber chip plus cmd-d do the jumping — sorting
churn is what makes a glance surface unreadable.

### Geometry
- Width **208 px** expanded, **40 px** collapsed rail. Ground `INSET #0a0a0a` (one step
  below the Panes' `SURFACE`, so it reads as chrome), `border-right: 1px EDGE`.
- Header row **34 px** (aligns with the Cockpit strip): CAPS label `THREADS` (10 px,
  ls 0.10em, `INK_MUTED`) left; mono count `7 · 2 waiting` right — count `INK_MUTED`,
  the `· 2 waiting` fragment `WAIT` when nonzero (same voice as the strip's
  "N needs you").
- Rows **28 px**, padding 0 10 px, internal gap 6 px. Two sections: running Threads
  (creation order — the grid's own order), then a divider (`HAIRLINE`) with CAPS label
  `PARKED — 4` (10 px, `INK_MUTED`, 22 px row), then parked Threads.

### Row anatomy (one line, never wraps)
```
[LED 6px] [name] [binding hint] ──flex── [provider] [signal]
```
- **LED** — 6 px dot, radius 999: `GOOD` streaming · `WAIT` blocked on a Decision ·
  `FAIL` closed · `IDLE` idle · parked = hollow (1 px ring `INK_FAINT`, no fill).
- **Name** — `thread-07` today (see §5.2 #8 on display names), 11.5 px `FONT_UI` 600,
  `INK` when running, `INK_SECONDARY` when idle, `INK_MUTED` when parked. Ellipsis.
- **Binding hint** — `main` or the worktree's leaf name, 10 px mono `INK_MUTED`
  (`INK_FAINT` parked). First thing to truncate.
- **Provider** — 9.5 px mono `INK_FAINT`: `cl` / `cx`. (The full `claude · fable` chip
  belongs to the Pane header, not a 208 px row.)
- **Signal slot** (right-aligned, one of):
  1. Decision pending → chip `needs you`: 9.5 px mono `WAIT` on `WAIT_WASH`, radius 4,
     padding 1 px 5 px — the exact chip from the Cockpit board's issue-triage cell.
  2. else todos → `3/4`, 10 px mono `INK_SECONDARY` (from `transcript.todos()`, O(1)).
  3. else nothing. Parked rows never show a signal (their log is not in memory —
     honesty over decoration).

### States
| State | Rendering |
|---|---|
| hover | row bg `HAIRLINE`-tone `rgba(255,255,255,0.045)` (pointer, lands with #15) |
| focused Thread | row bg `EDGE` `rgba(255,255,255,0.07)` + **2 px left inset bar `ACCENT`** — the Pane focus ring translated to a row |
| Decision pending | LED `WAIT`, name `INK`, `needs you` chip; if also focused, the steel bar stays and the chip carries the amber (ring language: position = steel, urgency = amber) |
| blocked/closed | LED `FAIL` |
| parked | hollow LED, muted text, whole row opacity 1.0 (rows are already muted by ink — dimming on top would kill legibility at 28 px) |
| pressed | bg `rgba(255,255,255,0.07)` while mouse is down |

### Interaction
- **Click a running row** → `focused = pane_for(thread)`, notify. (The render-time
  focus snap then lands the keyboard in that Pane's Composer or Decision card — the
  snap works *with* this; full hover/selection remains #15.)
- **Click a parked row** → revive: exactly `reopen_thread`'s body targeted at that
  ThreadId (`cockpit.revive`, push `PaneView`, focus it). The row moves up into the
  running section — a real state change, so movement is correct here.
- **cmd-b / ctrl-b** toggles collapsed (VS Code muscle memory; cmd-t/w/f are spoken
  for by #20). Two keymap-table rows, tested for both spellings like every other key.
- No horizontal resize in v1. No context menus in v1.

### Collapsed rail (40 px)
LED column only: running Threads as 8 px dots, 24 px row pitch, centered; a Decision
dot gets a `WAIT_WASH` halo (16 px circle behind it) so amber still reads across the
room; parked as 6 px hollow dots below the hairline divider. Clicking a dot behaves
like clicking its row. Header shrinks to the count `7` (mono 10 px, `INK_MUTED`).
The rail is the Wall's legend rotated vertical — nothing to read, only to notice.

### Semantic-zoom interplay
The nav's width participates in `CockpitView::cell()`: cells get
`viewport.width − nav_width` to divide. Opening the nav can legitimately drop Panes a
Level — size decides, that is the whole zoom contract, no special case. At the default
1440 px window with 24 Panes, 1440−208 leaves ~197 px cells: still Wall, unchanged.

---

## 3 · Fullscreen Pane — cmd-f (#20)

**Model.** Fullscreen is a Cockpit view mode, not a Pane property:
`fullscreen: bool` on `CockpitView`, always showing `self.focused`. That one decision
buys every flow for free: cmd-] / cmd-[ page Threads *while fullscreen* (browser-tab
feel), cmd-d lands the next Decision fullscreen, cmd-w parks the focused Thread and
the clamped focus means the next Thread fills the screen, cmd-t opens-and-focuses so
the new Thread is what you see. No per-Pane state, nothing to reconcile.

**What renders.**
- The **Cockpit strip stays** (34 px): `N threads · N waiting on you` in `WAIT` is the
  operator's tether to the rest of the swarm — it is the reason to leave fullscreen,
  so it must remain in view. All Sessions keep streaming (the pump is Cockpit-level
  and untouched).
- The **nav hides entirely** while fullscreen; its collapsed/expanded preference is
  kept and restored on exit. (The strip's amber count covers the peripheral signal;
  a rail beside a fullscreen Pane is chrome tax.)
- The focused Thread's Pane fills the whole content area, keeping the 6 px gutter and
  its own border + inset 1.5 px `ACCENT` ring — it still reads as a Pane that grew
  (Main.dc's focused card at room scale), not as a different app mode. Header, todo
  strip, transcript, status line, queued line, Decision card, Composer: the ordinary
  L1 stack.
- **Level is forced to `Level::Transcript`** while fullscreen — one expression at the
  top of `render` (`if self.fullscreen { Level::Transcript } else { self.level_now(window) }`).
  A 1440×900 single cell would compute L1 anyway; the override is what makes the AC
  "fullscreen = L1 regardless" true on a small window too.

**Restore gesture.** cmd-f toggles back. **Escape is answered: no.** Escape is bound
to `cockpit::Interrupt`, and interrupting a running agent while reading it at L1 is
the more valuable meaning — stealing it for "exit fullscreen" would make the panic
key ambiguous. cmd-f is symmetric, cheap to hit, and matches the browser-zoom-toggle
muscle it borrows. (A pointer minimize glyph in the Pane header — Main.dc's `—` icon
— can arrive with #15/#22; keyboard is the v1 contract.)

**Edge cases.** Parking the last Pane clears `fullscreen` and falls back to the empty
grid. A Decision on a *different* Thread while fullscreen: strip count ticks amber,
cmd-d jumps there without leaving fullscreen. Grid math (`columns`, `cell`) is
skipped entirely in fullscreen — render only the one Pane; never lay out 23 hidden
siblings behind it.

---

## 4 · Implementation map

Binding constraint (operator): *use existing deep modules, no shortcuts, no code
that becomes impossible later.* The read of the crates below is: **core already
carries almost every number the comps show; the gaps are two small core folds, one
store peek, and app-side rendering.** No new subsystems.

### 4.1 Where Aperture tokens live
One module in the app crate: **`crates/ferrite/src/theme.rs`**. Every color, wash,
size, gap, and type-size constant from §1, named once, `pub(crate)`. It absorbs the
17 color consts at the top of `pane.rs` (`BG_WINDOW`, `TEXT_NOTICE`, `LED_RUNNING`,
`DIFF_*`, `CODE_*`, …) and `MONO_FONT` from `main.rs` (as `FONT_MONO`, beside
`FONT_UI`). `cockpit.rs`, `pane.rs`, `composer.rs` (its two hex literals at the
cursor/selection quads), and the new `nav.rs` import from it; no hex literal outside
`theme.rs`. Core stays color-blind — `ferrite-core` never learns what a pixel is,
exactly as `docview.rs` promises today. First landing is a **verbatim move** (zero
visual diff, mechanical review); the value retune to §1 is #22's diff, one review,
one eyeball.

### 4.2 What core already carries vs. what is missing
Existing structures that already power the comps, verbatim:

| Comp element | Carrier (exists today) |
|---|---|
| status LED / wall label | `Transcript::status()` — O(1) |
| ▰▰▰▱ 3/4 todo strip | `Transcript::todos()` → `Todos{done,total}` — O(1) counters |
| pass/fail badge | `Instruments::of().tests` (`Tests::Passed/Failed`) |
| `+18 −2 · 2 files` | `Instruments::of().{added,removed,files}` |
| running-activity count | `Instruments::of().running` |
| ctx bar + `62%` | `Transcript::usage()` (`total_tokens`, `context_window`) |
| `$0.84` | `Transcript::last_cost()` |
| `claude-… · sess` header chip | `Transcript::model()`, `session_id()` |
| binding chip (`main` / worktree) | `Cockpit::workspace()` → `binding_label` |
| queued-prompt line + unqueue | `Cockpit::queued()/unqueue()` + `queued_line` (shipped; add the comps' `⌫ unqueue` hint text — app render only) |
| Decision card + `a always` gating | `Cockpit::pending()`, `Decision::standing_answer()` |
| `· N needs you` amber summary | `Cockpit::blocked().len()` (strip exists; recolor) |
| every transcript block kind | `Body::{Prompt,Paragraph,Heading,Bullet,Thinking,Notice,Meta,Code+tokens,Tool+diff}` |
| park/revive for the nav | `Cockpit::parked()/revive()/park()` + `park_order` |

**Missing pieces, each with its owning module:**

1. **`⎿` continuation row under tool rows** (DirectionDense: `⎿ exit 0 · 3.1s · …`,
   `⎿ 1,449 lines · …`). Core discards tool output except errors. Add
   `ToolBlock.result_line: Option<String>` — first line of `ToolCompleted.output`,
   trimmed (~80 chars) — folded in **core `transcript.rs`** (`settle_tool`). Render
   (the `⎿` glyph, `INK_MUTED`, 22 px indent) in **app `pane.rs`**.
2. **Exit-code chips** (`exit 0` green). `SessionEvent` carries only `is_error` — the
   literal code does not exist in the event model. Honest v1: chip from `ToolState`
   (`Ok` → `GOOD_WASH` `✓`, `Failed` → `FAIL_WASH` `✗`). A real exit code is a
   **protocol extension in core `session_event.rs`** (both provider adapters) — later
   ticket, not this pass.
3. **Test counts** (`✓ 41 passed`). `Instruments.tests` is pass/fail only; counts
   would mean parsing arbitrary runner output — brittle. v1: color-only badge
   (`tests pass` / `✗ failing`). If ever built: a summary parser beside
   `is_test_run` in **core `docview.rs`**.
4. **Durations** (`8.2s`, `running 2m14s · turn 6`). Nothing in core holds a clock
   (deliberately — folds are pure). Turn-level is cheap and honest: `busy_since:
   Option<Instant>` on core `cockpit.rs`'s `Thread`, set in `fold()` where `busy`
   flips, exposed as `busy_for(thread)`. Per-tool durations need per-event
   timestamps — **defer**; comps survive without them.
5. **Nav metadata for parked Threads.** `parked()` returns ids; provider + binding
   live only in the log header, and `Store::load()` replays the whole log — never in
   a render path. Add **`Store::peek(id) -> ThreadMeta{provider, workspace}`** in
   core `store.rs`: read line 1 (the `Header`), skip the records. App caches
   `Vec<NavParkedRow>` and rebuilds it only on park/revive/delete — never per frame.
6. **Wall-cell activity line** (`◐ vitest`, `1/5 · reading`). At L3 keep it O(1):
   status word + `todos()` only. The *name* of the running tool would want
   `Instruments::of` per wall cell per frame — that is the 116 fps budget; show it
   at L2 only, where `Instruments::of` is already paid.
7. **Sparklines** — appear in no canon board. Not data, not a gap: **nothing to build.**
8. **Thread display names** (`font-ligatures` instead of `thread-07`). No name field
   exists anywhere (store header, core, app). Real feature: header schema bump +
   rename flow — its own ticket. v1 nav shows `thread-NN` + binding hint, which is
   what the Panes already say.

### 4.3 What must NOT be built (comp elements that would demand a parallel path)
- **Link behavior on file paths / `open diff`** (Main, Dense boards render `<a>`s).
  Real links mean hit-testing, hover, and an editor/diff destination — an
  editor-grade element Ferrite doesn't have. v1: paths render in `ACCENT`, inert.
  Hover underline can ride #15; *opening* anything stays out of scope.
- **Intra-line diff emphasis** (Main.dc's `GOOD_STRONG` word-span inside a `+` line).
  Needs word-level diffing — a second diff engine beside the provider's hunks. v1:
  line-level `GOOD_WASH`/`FAIL_WASH` grounds with `GOOD`/`FAIL` ink, which the comps
  also show. Token reserved so v2 is a render change.
- **Slash-command and @-file menus** (PromptBox 02/03). Fuzzy matcher + file index +
  overlay stack = the Composer's next deep slice, not a visual pass. #22's Composer
  scope is the chrome: prompt row + hint row + queued/permission stack. Menus get
  their own ticket.
- **`exit 0` literals / test counts / per-tool timings** — §4.2 #2–#4 fallbacks.
- **Animated zoom transitions** between L1/L2/L3. The ladder is a hard cut today and
  the comps are stills; tweening levels means rendering two representations of every
  Pane mid-transition — the definition of a parallel render path. Keep the cut.
- **JetBrains Mono / Geist bundling** — #22 already maps to Menlo/Consolas + system
  UI; bundling is a separately-decided license/size question.

### 4.4 Build order (each stage compiles, tests green, reviewable alone)
1. **`theme.rs` — verbatim token move** (rides #22's opening commit or lands solo).
   No value changes; `pane.rs`/`main.rs`/`composer.rs` re-point. Zero-pixel diff.
2. **#15 slice the nav needs**: mouse-down-to-focus on Panes (click sets `focused`;
   the existing render-time focus snap then does the right thing), hover fills from
   tokens. Text selection + wheel scroll continue inside #15 independently.
3. **#20**: keymap rows `cmd/ctrl-t → cockpit::NewThread` (alias beside cmd-n) and
   `cmd/ctrl-f → cockpit::ToggleFullscreen` (new action); `fullscreen: bool`;
   level override + single-Pane render arm; keymap tests assert both spellings
   (the existing `keymap.rs` test pattern covers this for free).
4. **#21**: `Store::peek` in core (test: peek reads the header and never the records);
   `crates/ferrite/src/nav.rs` (pure `render_nav(state)` like `pane.rs`, a
   `NavState` the cockpit assembles per frame); parked-row cache invalidated on
   park/revive/delete; `cell()` subtracts nav width; `cmd/ctrl-b` collapse;
   `FERRITE_PERF` run at `--panes 24` before/after.
5. **#22 part 1 — chrome retune**: token values to §1; strip, Pane header, square
   corners, inset-ring overlay (focus/`WAIT`/`FAIL`), Wall cells per the Wall board
   (5 px dot · name · 5 px bar · status line), Composer 40+22 rows, Decision card
   recolored in place (`WAIT_WASH`/`WAIT_EDGE` — stays the in-Pane card, no rail).
6. **#22 part 2 — transcript density** per DirectionDense: `result_line` fold in
   core + `⎿` rows, gutter markers (`❯` ACCENT / `⏺` INK_TERTIARY), code/diff card
   chrome (`INSET` ground, `EDGE` border, path header row), L2 instrument chips on
   washes. Then the side-by-side eyeball AC at 6 Panes.

### 4.5 Perf — holding ~116 fps at the 24-Pane wall
- **The nav must be O(threads), never O(blocks).** Rows read `status()`, `pending()`,
  `todos()` — all O(1); parked rows come from the peek cache (§4.2 #5), rebuilt only
  on park/revive/delete. `Instruments::of` and `Store::load` are banned from nav
  code; `Store::peek` exists so nobody is tempted.
- **The nav paints inside `CockpitView::render`** — same entity, same pump, same
  `cx.notify` coalescing. No second timer, no own entity, no extra frames.
- **Fullscreen is a perf win, not a risk**: one Pane laid out instead of 24 — as long
  as the grid loop is skipped, not hidden. Early-return in `render`.
- **Ring overlay** adds one quad per flagged Pane; wall cells gain ~2 text runs + one
  bar quad each versus today (Wall-board layout) — same order as the current cells.
  `visible_blocks() == 0` at Wall stays the law.
- **Pre-existing hot path, flagged not fixed**: `Instruments::of` walks every Block
  per L2 Pane per frame (up to 2000 blocks × N panes). Shipped today; the visual
  pass adds no new callers at L3. If a 12-Pane L2 grid ever dips, the deepening is
  incremental instruments folded on `apply` in core — its own ticket, not a reason
  to grow render-side caches now.
- **String churn**: strip/nav labels are small `format!`s per frame (the strip
  already does this); parked-row `SharedString`s are cached with the peek cache.
  `result_line` is folded once per tool completion — zero per-frame cost.

### 4.6 Open questions for the operator
1. Thread display names (§4.2 #8) — ticket the store-header name field now, or live
   with `thread-NN` through v1?
2. Nav default state — expanded or collapsed on first launch? (Design assumes
   expanded at ≥1200 px windows, collapsed below.)
3. cmd-b for the nav toggle — accept, or reserve b and pick `cmd-\`?

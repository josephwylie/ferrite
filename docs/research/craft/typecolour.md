# Typography and colour: Claude, Codex, Ferrite

Every hex below was read from the PNGs with Pillow and numpy. Fills are the most common pixel (the mode) in a rectangle. Ink is the most extreme pixel in a glyph run. Claude and Codex coordinates are 1x px (1 px = 1 pt). Ferrite coordinates are **2x file px**, so halve them to get pt. Theme citations are `crates/ferrite-ui/src/theme.rs:LINE`.

## Method for type sizes

- **Cap height** is measured from the top of a stem, with partial-coverage rows counted fractionally, to the baseline. This is the measure that holds across renderers. Ferrite's 13 pt `W` measures 18.4 px at 2x, so 9.2 pt = 0.707 em, which is SF Pro's 0.705 cap height. gpui adds no spill.
- **x-height** is the rows at ≥40 % of peak ink density, down to the baseline. It is calibrated on Ferrite text whose size is known: 12 pt reads 6.6, 13 pt reads 7.1 and 14 pt reads 7.6. That is a ratio of about 0.548 with +0.2 px of anti-alias spill. Chromium (Claude and Codex are Electron) spills about 0.5 px more, so for those two apps cap height decides the size and x-height is only a cross-check.
- **Line pitch** is the distance between consecutive baselines.
- **Weight** is a stroke-thickness proxy: total ink coverage divided by the number of stem transitions. It is only compared within one app.
- **Face.** Codex's x/cap ratio is 0.81 (SF-like). Claude's is 0.76: same x-height as Codex, caps 0.6 px taller. So Claude's face is **not SF**, and its "15 px" is a different face at a larger size. Ferrite sets SF Pro, so its targets below are SF sizes matched to the references' cap and x-height.

## Table

| element | Claude | Codex | Ferrite now | target for Ferrite | evidence |
|---|---|---|---|---|---|
| **Window / main plane** | #191919 | #181818 main; #343334 title bar and icon rail | #1a1a1a everywhere (`CARD` :231, `FRAME` :234) | keep #1a1a1a | C answer-done (300-700, 600-900); X answer-done (450-780, 700-900), title (700-1700, 0-40); F conversation (24-512, 595-1548) |
| **Sidebar** | #292428: macOS vibrancy, the desktop tints through | #222222 (+10 over main) | #1a1a1a, the plane (rule 2) | keep the plane (owner's call, d880a138). Both references light the sidebar a step; if that is ever revisited, use #222222 | C (0-250, 300-900); X (60-410, 600-620) |
| **Transcript** | #191919 | #181818 | #1a1a1a | keep | as above |
| **Composer fill** | #303030 (+23) | #363636 (+30) | #212121 (+7) `FIELD` :246 | **#303030** | C answer-done (760-1400, 970-1005); X (830-1250, 975-990); F (1012-2023, 1593-1605) |
| **Composer ring** | 1 px #404040 | 1 px #3a3a3a | 1 px #242424 `FIELD_RING` :293 | **#404040** | C y=966/1009 at x=1000; X y=932/1029; F y=1591-1592 |
| **User bubble** | #252525, no ring | #2f2f2f, no ring | #242424 (`RAISED`) | **#262626**, matching Claude | C (1066-1455, 745-788), edge y=744; X (1180-1530, 750-758); F (1655-2107, 905-917) |
| **Inline code chip** | #2a2a2a; ink #f1f1f1; red #f3767b inside the bubble | #2f2f2f / #303030; ink #ffffff | #242424 `RAISED` :251; ink #f5f5f5 | **#2a2a2a** (Claude) | C (1060-1090, 526-545), (1135-1250, 305-325); X (1030-1060, 358-372); F (1154-1607, 1405-1410) |
| **Code / shell card** | tool card has no fill (#191919) and a 1 px #222222 ring; the command well is the same | fill #2a2a2a; ring #333333 (code), #4c4c4c (shell) | fill #1d1d1d `BLOCK` :239; ring #242424 | **#262626 fill, #333333 ring** (Codex): a fence should read as a block at a glance; Claude's ring-only style suits its tool cards, not code | C tools-expanded (725-1440, 140-150), ring y=135; X shell-card (810-1530, 640-650), ring y=617; X model-menu code card ring y=686; F (690-2107, 393-405), ring y=520 |
| **Menu / popover fill** | #303030 (+23) | #2b2a2b→#2f2e2f, a slight vertical vibrancy (+19 to +23) | #202020 (+6) `FLOAT` :255 | **#303030** | C model-menu (1045-1290, 885-900); X permission-menu (860-1270, 895-905); F modelpicker (1309-2023, 905-928) |
| **Menu edge** | 1 px #383838 | 1 px #2a2a2a / #292929 | 1 px #242424 (`HAIRLINE_STRONG` :295) | **#383838** | C y=730, x=1037; X y=826, x=845; F y=846-847 |
| **Menu shadow** | falls from #131313 at the edge to #181818 over about 12 px | from #171717 over about 3 px at the sides; the composer sits below it | from #121212 to #141414 over about 8 pt; CSS 0 6 14 at 50 % (:678-680) | keep, about right | C right of the menu (1294-1310, 900); X (1286-1296, 900); F y 1658-1676 |
| **Menu cursor / selected row** | slash picker #4f4f4f on #303030 (+31) | slash picker #3d3d3d on #2d2d2d (+16); permission menu row #2f2e2f (+4) | #2c2c2c on #202020 (+12) `FLOAT_SELECTED` :261 | **#3e3e3e** on the new #303030 (+14, Codex's step). Claude's +31 is heavy | C slash-picker (780-950, 592-598); X slash-picker (1100-1500, 612-618); F modelpicker (1547-1964, 988-1029) |
| **Nav selected row** | #494548 on #292428 (+32, vibrancy) | #333333 on #222222 (+17) | #212121 on #1a1a1a (+7) `SELECTED` :265 | **#2a2a2a** (+16) | C answer-done (10-240, 395-402); X (70-400, 828-836); F (24-512, 476-488) |
| **Hover row** | not captured | not captured | #ffffff at 4 %: #1a1a1a→#1e1e1e (`HOVER_WASH` :305) | about half the selected step: **white at 4 %** stays, but only once selected reaches +16. Inferred, not measured | n/a |
| **Seams / separators** | title row seam #353535; table rules #303030; agent card #222222 | title seam #3a393a; sidebar edge #444444; table head rule #3c3c3c, row rules #222222; "Worked for" rule #2b2b2b | one #242424 for everything (`BORDER` :286); also `LINE` #262626 :329 and `LINE2` #303030 :332 | **two greys: #303030 for region seams and table rules, #222222 for card rings**. Retire #242424 and #262626 | C y=44 at x=1000; table y=614, 651; X y=43-44; x=426; y=163, 204, 482; F nav seam x=527-528, head seam y=90-91 |
| **Scrollbar thumb** | #5e5e5e, 6 px overlay; picker #6d6d6d | #3c3c3c, 6 px; sidebar #343434 | #333333 `SCROLLBAR` :389, not visible in any render | **#5e5e5e**, 6 px wide | C x=1912-1917, y=720-1000; X x=1908-1913, y=900; X x=414-419 |
| **Primary text** | #efefef (body, title, bubble); sidebar #f0eff0 | #ffffff (body, title); sidebar rows #dedede | #f5f5f5 `INK_STRONG` :401 | **#efefef** (Claude): calmer than pure white | C (723-864, 834-847); X (804-938, 838-851), sidebar (93-291, 243-254); F (681-1187, 754-778) |
| **Secondary text** | #9c9c9c (tool headers, meta, "Ran agent", "Bash", footer "Medium"); in menus #a6a6a6 | #a3a3a3 tool rows; #c0bfc0 shell text; #cacaca unselected picker rows | #cfcfcf `INK` :404 | **two tiers would do.** Keep #cfcfcf only for icons and secondary chrome; tool rows → #9c9c9c | C (724-902, 491-504); X shell-card (827-996, 543-556), (813-991, 657-669); X slash (841-916, 647-657) |
| **Tertiary / meta** | #9c9c9c, the same as secondary; sidebar section labels #a3a1a2 | #8b8b8b ("Worked for", timestamps); section #909090; menu descriptions #969596 | #919191 `INK_SUBTLE` :408 | **#9c9c9c** | C (724-930, 212-225), (19-56, 247-256); X (804-902, 462-472), (1442-1472, 799-808); F (682-749, 1031-1049) |
| **Placeholder** | #878787 on #303030 | #686868 on #363636 | #919191 on #212121 | **#878787** on #303030 | C (747-890, 983-996); X (816-924, 950-960); F (776-976, 1616-1637) |
| **Link** | #69abed (the "Buy credits" pill) | #5685cc with a matching icon | #7aa8ee `LINK_INK` :644 | keep #7aa8ee | C model-menu (1216-1279, 973-988); X model-menu (860-919, 867-877); F (713-802, 1376-1390) |
| **Mono in prose** | inline code #f1f1f1, set **bold** where markdown bolds it; command #59eeec cyan, `$` #929292 | #ffffff in chips; shell #c0bfc0; syntax: keyword #f9a6c7, type #b997f0, number #e9aa8b | chips #f5f5f5; tool commands #cfcfcf; paths in `LINK_INK` | keep the ink; let bold mono be bold (see weights) | C (874-1011, 284-300); C tools-expanded (757-825, 183-195); X model-menu (825-879, 739-751), (941-960, 739-749), (984-1045, 740-750) |
| **Error red** | #f3767b (the "Failed" word, error mono) | none captured | #ef8a80 `RED` :431 / `BLOCKED` :465 | **#f3767b** (Claude): a touch more saturated, so it reads as an error at 13 pt | C tools-expanded (846-883, 108-118), (732-1395, 224-236); F error-turn (680-739, 531-549) |
| **Success** | none captured | grey "✓ Success" #949494, a shape and not a hue | done is a shape in `TEXT_DIM`; `GREEN` #93cf8c (:432) for live work and diff + | keep. Codex confirms Ferrite's rule | X shell-card (1459-1527, 716-726) |
| **Warning / risky mode** | "Manual" #e49b31 (amber-orange) | "Full access" #ff8454 (orange) and its check #ca6e4a; chip fill #464646 | "Accept edits" #e6c47c `ATTENTION` :463 (yellow) | **#e49b31** for the mode word; yellow reads as "pending" | C answer-done (1383-1424, 1025-1034); X (854-937, 1001-1014); F (1923-2062, 1689-1711) |
| **Accent** | selected check #237ddb | effort value and slider #3983f1 | none by rule (:91-94). But the context meter paints `BLUE` #8cb9f4 | one accent, **#3983f1**, for the selected check and meter fill only; or, if rule 6 holds, make the meter `TEXT_DIM` | C model-menu (1266-1280, 744-755); X model-menu (1359-1410, 907-917); F conversation meter (2315-2470, 1702-1714) |
| **Failure strip** | none captured | none captured | #292221 (`FAILURE_WASH` :320 over the plane) | keep | F error-turn (976-1964, 625-672) |
| **Body (prose) size** | cap 10.6, x 8.1 → about **15 px** in its non-SF face; pitch **21**; paragraph gap +11 | cap 10.0, x 8.1 → **14 px SF**; pitch **22** | 13/18 `FS_PROSE` :713, `LH_PROSE` :716 (cap 9.2) | **14/21 SF** (cap 9.9, matching Codex's 10.0 and Claude's x-height). `TX_LINE_RATIO` 1.4 → 1.5 | C baselines 87/108/129, then 161 (para); X baselines 68/90; F 641.5/659.5 pt |
| **User bubble text** | the body size, pitch 21 | the body size, pitch 20 | 13/18 | follows the body: 14/21 | C 409/430; X 372/392 |
| **Sidebar rows** | x 8.2 → the body size (≈14 SF); pitch **34** | x 8.3, cap ≈10 → **14**; pitch **31** | 13 on a pitch of about 30 (`FS_UI` :708) | **14**, pitch 32 | C (44-88, 93-104), rows at 64/98/132; X (93-291, 243-254), rows at 249/280/311; F (88-223, 112-132) |
| **Sidebar section label** | about 13 (x 7.3), #a3a1a2, regular, sentence case ("Pinned", "agenticOS") | **14**, #909090 ("Projects") | 12 `FS_META` :723, #919191 | **13**, the subtle ink | C (19-56, 247-256); X (69-121, 179-192); F (38-127, 377-399) |
| **Window title** | about 14 (x 8.2), #efefef, regular | **14** (x 8.2), #ffffff, medium (stroke 1.95 against 1.79 for rows) | 14 `FS_TITLE` :718, `W_STRONG` | keep 14 / 500 | C (275-540, 16-27); X (473-668, 16-27); F (542-927, 36-61) |
| **Meta / footer** | 13 (x 7.2): "Opus 5.5" #eeeeee medium, then "Medium" #9c9c9c regular | 13: "GPT-6 Astra" #fefefe, then "Medium" #9a9a9a | 12 #919191 throughout ("Default · Opus 5.5 (1M)") | **13**; name in primary at 500, value in subtle at 400 | C (1256-1365, 1025-1037); X (1312-1437, 1003-1013); F (1549-1892, 1689-1711) |
| **Menu items** | the body size (x 8.2); row pitch 28 single-line, 46 two-line | **13** (x 7.2); two-line pitch 43; picker pitch 29 | 13; row pitch 30 | **14** single-line, pitch 28 | C model-menu rows 750/778/806; C permission (1130-1159, 784-794); X permission (880-977, 866-878); F modelpicker rows at 847/898/948 (display) |
| **Menu descriptions / section heads** | **13** (x 7.1), #a6a6a6 | **13** (x 7.1), #969596 | 13 #919191 (descriptions inline on the row); section 12 | **13**, subtle, on their own line under the item | C (1054-1190, 987-999), (1131-1163, 757-766); X (880-1175, 882-894) |
| **Chips / pills** | "Recommended" and "Default": about 11 (x 6.2), #a6a6a6, a 1 px #595959 ring with no fill; footer pill #383838 | footer chip #464646 | 12 `FS_BADGE` :726 | **11** for an in-menu tag, with a ring and no fill | C permission-menu (1172-1253, 781-796); X (846-945, 997-1020) |
| **Tables** | head and cells at the body size; the head is heavier (stroke 1.99 against 1.73) | **12** (cap 9.0, x 6.3); the head is semibold (2.07 against 1.56) | the body size; the head is medium | the body size; head at **600** | C (724-793, 590-601); X (804-863, 583-593), (1195-1341, 618-630); F (680-1003, 568-594) |
| **Mono size** | about 13 in chips and code (x 7.1-7.2) | 12-13 in chips and shell (x 7.1-7.3) | 13 in prose chips; 12 for blocks (`TX_BLOCK_STEP` 1); 12 in chrome | **13** for chips and blocks (one step under the 14 body) | C (732-1395, 224-236); X shell (814-991, 657-669); F (797-866, 1412-1426) |

## Weights and case

- **Weights.** Ferrite has 400 and 500 only (`W_BODY`/`W_LABEL`/`W_STRONG` :745-747; rule 1 says "nothing heavier"). Both references go heavier:
  - Codex's "Codex" brand title has a stroke of 2.95 against 1.79 for sidebar rows (semibold).
  - Codex's table head is 2.07 against 1.56 for cells.
  - Claude bolds markdown-bold inline code: 2.96 against 1.95 for plain code, at C (874-1011, 284-300).
  - Claude's table head is 1.99 against 1.73 for cells.
  - Claude's footer model name is medium against a regular value (1.83 against 1.47).
  - **Target:** add 600 for `**strong**`, table heads and markdown headings (`W_STRONG`). Keep 500 for labels and tabs, and keep 400 for what is read. Ferrite's 500 measures only +7 % over 400 (1.60 against 1.49), so `**strong**` barely reads.
- **Headings** are body size by rule (`heading_scale` returns 1, :781). No markdown heading appears in the captures, so this is left unmeasured.
- **Case.** Both references use sentence case for every label, *including state and value words*:
  - Claude: "Failed" (tools-expanded 846, 108), "Medium", "Manual", "Bypass permissions", "Fast mode".
  - Codex: "Full access", "Worked for 23s", "Success", "Show more".
  - Ferrite lowercases state and value words by rule (:141-147, :508-515): "failed · 0.1s" (error-turn), "1 failed", the effort row "low medium high" (modelpicker 1014-1440, 1300-1328).
  - Ferrite also **contradicts itself**: its own footer says "High", capitalised (conversation 1847-1892, 1689-1711), while the picker says "high".
  - **Target:** capitalise a value or state word when it stands alone or leads a label, as both references do.

## Token audit (theme.rs)

- **The canonical tier names are dead.** `INK_STRONG` :401, `INK` :404 and `INK_SUBTLE` :408 are never named outside theme.rs. Render code uses six legacy aliases (`TEXT_STRONG` used 46 times, `TEXT` 37, `TEXT_DIM` 76, `TEXT_MUTED` 3, `TEXT_PROSE` 2, `TEXT_BAR` 6), all defined at :415-421 and all on three values. Either migrate to the tier names or delete them. Today the doc describes one API and the code uses another.
- **A no-op hover blend.** `crates/ferrite-cockpit/src/nav.rs:1801` mixes `TEXT_MUTED`→`TEXT` on hover. Both are #cfcfcf (:418, :420), so "Show parked threads" never changes ink. This is the aliasing above causing a real bug.
- **"One border" is three.** `BORDER` #242424 :286, `LINE` #262626 :329 and `LINE2` #303030 :332. `paint::LINE2` is used 17 times (for example `decision.rs:1449` and `project_editor.rs:301`) and `paint::LINE` 8 times. Six more aliases of `BORDER` (`HAIRLINE`, `CARD_RING`, `FIELD_RING`, `HAIRLINE_STRONG`, `ROW_RING`, `BAR_TOP`, :289-299) all hold one value. `ROW_RING` is used once (`nav.rs:708`). `settings.rs:501/531/834` paints `gpui::rgba(BORDER)` raw instead of through `paint::*`, against rule 4.
- **The float tokens are named twice.** `FLOAT_RAISED`/`FLOAT_SELECTED` (:260-261) are re-wrapped as `FLOAT_HOVER`/`FLOAT_SEL` (:2398-2399), then again as `PALETTE_HOVER`/`PALETTE_SEL` (:2345-2346), alongside `FLOAT_GROUND` = `paint::FLOAT` (:2395). That is three names for one fill.
- **Aliases with a single use.** `TAB_ACTIVE` is the same as `RAISED` (:248, :251); `paint::TAB_ACTIVE` is used once (`subagents.rs:536`). `paint::PANEL`, `paint::SUNKEN`, `paint::DECISION`, `paint::FIELD_COMPACT`, `paint::BAR_TOP` and `paint::PROMPT_BAND` are each used at most once (:347-378).
- **Docs contradicted by measurement.**
  - :766-768 (`TX_SOLO_STEP`) and :785-787 (`TX_LINE_RATIO`) call 13/18 "the Claude app's own reading size" and 1.4 "the Claude app's reading leading". Claude measures cap 10.6 on a 21 pitch, and Codex measures 14 px on 22.
  - :409-411 says the reference app draws icons in the secondary ink. Claude draws sidebar icons in its primary ink.
- **Tokens that lose to their neighbours.** `SCROLLBAR` #333333 (:389) shows nowhere in the renders. On #1a1a1a it is 1.5:1, against Claude's #5e5e5e.
- **The accent rule is broken.** Rule 6 says "no accent" (:91), yet the context meter paints `BLUE` #8cb9f4 (conversation-app (2315-2470, 1702-1714)).

## Summary

The references set 14-15 px type with 1.5 leading. They lift composer, menu and selection surfaces by 16 to 30 levels over the plane and draw seams at #303030-#3a3a3a. Ferrite sets everything a step smaller (13/18, with 12 for meta) and lifts every surface by only 6 or 7 levels (#1a1a1a → #202020/#212121). Its hairlines (#242424) are also the faintest of the three apps.

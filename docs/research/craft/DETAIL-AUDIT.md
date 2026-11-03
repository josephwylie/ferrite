# Detail audit: why Claude reads "sleeker, more purposeful"

This audit covers every surface pair I could match. I cropped the same component from Claude (`claude/`, 1x) and Ferrite (`ferrite-cp2/`, 2x, halved), scaled both to the same physical size, zoomed them 1.6–6×, and measured them with Pillow and numpy. All values are in pt. Hex values are measured pixels: a fill is the mode of a flat area, and an ink is the brightest pixel in a glyph run. Stroke widths are summed coverage across one stroke. Weights use the stroke proxy from typecolour.md and are compared only within one app. Evidence crops are in `detail/`, with Claude on the left (or top) and Ferrite on the right (or bottom).

**REMOVE** marks a fix that deletes something. Impact is **H**, **M** or **L**: how much the fix changes the look of an ordinary thread view.

## The short answer

Since the token pass, sizes, radii, row pitches and surface fills are mostly at parity: the bubble, cards, queue strip and approval all measure r 10, and the composer is 768 × 44 against Ferrite's 770 × 46. The remaining gap comes from five systemic habits, not from one component:

1. **Ferrite's icons are thinner than its text, smaller than Claude's, and grey.** Claude draws icons at 14–16 pt with a 1.25 pt stroke, which matches its text stems (1.27), in the same ink as the label (#efefef–#f0eff0). Ferrite draws 10–13 pt icons with a 0.9 pt stroke, thinner than its own text stems (1.22), in #cfcfcf or #9c9c9c. The cause is `ICON_STROKE = 1.0` at 16 (theme.rs:746) applied to icons drawn at 12–15, plus `ICON_INK = INK` (#cfcfcf, theme.rs:428). Every surface has icons, so this one setting makes the whole app look tentative ([22](detail/22-icon-stroke.jpg), [01](detail/01-sidebar-top.jpg), [16](detail/16-plus-menu.jpg)).
2. **Ferrite uses three ink tiers where Claude uses two.** Claude sets content in #efefef and metadata in #9c9c9c, with almost nothing in between. Ferrite puts a third tier, #cfcfcf, on things Claude sets in primary: side-panel rows, the footer model name, the empty-thread card, the queue strip, the composer `+`, the jump arrow and fence code. Those elements look half-disabled, and a view has one more grey to sort out.
3. **Ferrite shows text and controls that Claude shows only on hover, or never.** Examples: `⌘N`/`⌘K`, the copy + time row under the last answer, the send disc in an empty composer, the "Projects +" button, "No changes yet", "Shows once the provider reports what fills the window", "Limits not reported by this provider", "Last turn $0.42", the `rust` fence label, "Shell" and "✓ Success" in a tool card, the project or branch beside the title, "(1M)" after the model, and "Claude" where the model name should be.
4. **Ferrite has more hairline and fill tones.** Claude's transcript cards share one ring, #222222, with dividers in the same tone and no fill. Ferrite uses #222222 (card ring), #303030 (dividers *inside* that card, brighter than its edge), #333333 (code and shell), #3a352b (approval), plus fills #262626, #1e1e1e, #171717 and #272727.
5. **Selected states are too weak, and hover is too close to selected.** Claude's selected sidebar row is +32 over its plane. Ferrite's is +14 (#303030 on #222222), and its hover is +8 (#2a2a2a), so a hovered row looks almost selected and the selected row barely reads.

## Sidebar

Evidence: [01 top](detail/01-sidebar-top.jpg), [02 threads](detail/02-sidebar-threads.jpg), [03 foot](detail/03-sidebar-foot.jpg), [22 icon stroke](detail/22-icon-stroke.jpg).

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| S1 | Shortcut hints at rest | none | `⌘N`, `⌘K` at #9c9c9c, x 217–237 | **REMOVE**: show them in the tooltip | H |
| S2 | Nav icons | 14–16 pt, stroke 1.25, #f0eff0 (same as the label) | 10–14 pt, stroke 0.9, #cfcfcf | 16 pt, stroke 1.33 at 16 (`ICON_STROKE` 1.0 → 1.33, the Lucide 2/24 ratio), ink = label ink | H |
| S3 | Project headings | 13 pt label, #a3a1a2, `agenticOS ▾`, no icon; threads sit at the label's x (18) | folder icon + primary text (#efefef) + a 1 pt tree guide; threads indented to text x 44 | **REMOVE** the folder icons and tree guides. Draw the project as a 13 pt #9c9c9c label with ▾, and put thread text at x 18. One left edge for the tree | H |
| S4 | Selected row | #494548 on #292428 (+32), 32 tall, r≈7 | #303030 on #222222 (+14), 30 tall, r≈5 | #3a3a3a (+24) or higher; r 6–7 | M |
| S5 | Hover vs selected | hover not captured | hover #2a2a2a (+8) vs selected +14: hard to tell apart (`navhover-app`) | keep hover at +8 and lift selected per S4, so the two steps differ by about 2× | M |
| S6 | Status dot | 8 pt solid #fcb33c, no halo | 12 pt #e6c47c with a dark halo ring (#3f3a2f); separate red, white and green spinner marks | 8 pt solid, no halo. Use amber for needs-you (#e49b31-ish, the same as "Manual") | M |
| S7 | Header hairline | none in the sidebar (the title-bar seam spans the main pane only) | #303030 at y 43 across the sidebar | **REMOVE** the sidebar part of the seam | M |
| S8 | "Projects +" | no always-visible add | `+` at x 231 at rest | **REMOVE** at rest; show it on hover of the label | L |
| S9 | Foot strip | account row only | `Solo ◌ · UI overhaul ●` chip row above the account (no Claude counterpart) | **REMOVE** from the sidebar foot, or fold it into the account row | M |
| S10 | Counts | none | Inbox amber badge `2`, Parked `3` | keep Inbox; **REMOVE** the Parked count (it is not actionable) | L |
| S11 | Overflowing title | fades out (gradient) | `Retained transcript rend...` ellipsis | fade the last ~24 pt instead of an ellipsis | L |
| S12 | Header icons | 16 pt, #dcdcdc–#efefef | bell and sidebar toggle at 13 pt, #9c9c9c | 16 pt at primary ink (S2) | M |

## Title bar

Evidence: [04](detail/04-titlebar.jpg), [26 right side](detail/26-titlebar-right.jpg).

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| T1 | Redundant label beside the title | title + ▾ only | grey `ferrite` or `transcript-app-style · feat/pane-chrome` (#9c9c9c) | **REMOVE**: the project is in the sidebar and the branch is in the panel | H |
| T2 | Title truncation | full title (275–541) | truncated at ≈280 pt (`Preserve its...`) with 700+ pt free | let the title take the free width | M |
| T3 | Title chevron | 10 × 6, #ececec | 8 × 4.5, #9c9c9c | 10 × 6 at the title's ink | L |
| T4 | Right-side icons | six icons at 16 pt, #dcdcdc–#efefef, 32 pitch | one panel toggle at 13 pt, #9c9c9c | 16 pt, primary ink (S2) | L |
| T5 | Draft thread | "New session" | an empty bar (`draft-app`) | title the draft "New thread" | L |

The title-bar text x (Claude 275, Ferrite 277) and the bar height (44 with a #303030/#353535 seam) are at parity.

## User bubble

Evidence: [05](detail/05-user-bubble.jpg). This is at parity: fill #252525 vs #262626, r 10 vs 10, right edge on the column, text pad 16 vs 17. The one difference is height: 45 vs 41, because Ferrite's vertical pad is 2 pt tighter. Fix: pad y 10 → 12 (L).

## Answer prose

Evidence: [06](detail/06-prose.jpg), [07 action row](detail/07-answer-actions.jpg), [27 left edge](detail/27-left-edge.jpg).

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| P1 | Action row at rest | none under a finished answer (`answer-done`); on hover, four 12 pt icons at a 24 pitch + "51 minutes ago" | copy icon + `5:47 pm` always visible under the last answer | **REMOVE** at rest; show on hover | M |
| P2 | Action row x | first icon at 723 = column | copy icon at 325.5 = column + 5.5 | put the icon's ink on the column edge (320) | L |
| P3 | List bullet | 5 pt solid disc, #efefef, x column + 9 | 3.5 pt dot, #9c9c9c, x column + 3.5 | 5 pt disc in the body ink; bullet at +8, text at +22 | M |
| P4 | Inline code chip | #2a2a2a, 18 tall, 13 pt mono | #2a2a2a, 17 tall + 0.5 ring #242424, mono visibly larger relative to the body | drop the chip ring; check the mono step (13 under a 14 body) | L |

Line pitch is 21 in both. The paragraph and list rhythm is now within a point or two.

## Tool group: summary line and open card

Evidence: [08](detail/08-toolgroup-open.jpg).

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| G1 | Dividers brighter than the card edge | ring #222222, dividers #222222 | ring #222222, dividers **#303030** | dividers = ring = #222222 | H |
| G2 | Disclosure glyph | filled triangle ▸/▾, 6 × 4, #9c9c9c | stroked chevron › / ⌄, 7 × 7, thin | a filled 6 × 4 triangle for every in-transcript disclosure; keep chevrons for menus and the title | M |
| G3 | Summary → card gap | 13 | 8 | 12 | L |
| G4 | Summary length | `Ran 2 commands (1 failed)` | `Ran 6 commands, read 1 file and 1 more (2 failed)` | cap at two kinds (components.md) | L |

Row pitch (36/37), card x (column − 4) and row text inset (12/14) are at parity.

## Shell card (a command opened inside a group)

Evidence: [09](detail/09-shell-card.jpg). Codex comparison: `codex/shell-card.png`.

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| K1 | Card inside a card | the command sits in a hairline well (#222222 ring, **no fill**), output unboxed below | a filled card (#262626 fill, #333333 ring) inside the group's #222222 card | **REMOVE** the fill and the second ring: a hairline well for `$ cmd`, output on the plane | H |
| K2 | Label | none (the row above already says what it is) | `Shell`, 13 pt, #9c9c9c | **REMOVE** inside a group. Keep it only for a standalone card | M |
| K3 | Success marker | none (exit 0 says nothing) | `✓ Success`, bottom right | **REMOVE**. Failure keeps `Exit code N` in #f3767b | M |
| K4 | Command ink | syntax-coloured: command #6eb7f9, args green | grey | colour the command word and its args as Claude does | L |

## Todo card and working line

Evidence: [10](detail/10-todo-working.jpg).

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| W1 | Done mark | filled grey disc with a cut-out check (#9c9c9c) | outline circle-check (#9c9c9c, 0.9 stroke): reads like the pending mark | filled disc | M |
| W2 | Current mark | quiet grey arc (#626262) | green arc (#93ce8c) | neutral arc. Green adds a fourth hue to a calm card | L |
| W3 | Working glyph | 15 pt Claude spark, #da775d | 9 pt ring, #c96f52 | 14 pt glyph | M |
| W4 | Working text x | column + 32 | column + 20.5 | line up with the todo text (+36) or the prose (+0). Today it matches neither | L |
| W5 | Card x | 719 = column − 4 (same as the tool card) | 320 = column (but the tool card is at 316) | every transcript card at column − 4 | L |

## Approval card

Evidence: [11](detail/11-approval.jpg).

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| A1 | Body fill and ring | no fill (plane #191919), ring #222222 | body #1e1e1e, ring tinted amber #3a352b | plane body, ring #222222 | M |
| A2 | Command well | hairline ring only, no fill | sunken #171717 fill, no ring | hairline well (K1, the same component) | M |
| A3 | Description ink | #9c9c9c | #cfcfcf | #9c9c9c | L |
| A4 | Deny button | #3b3b3b on the plane (+34) | #242424 on #1e1e1e (+6): almost invisible | #3b3b3b | M |
| A5 | Well → buttons gap | 24 | 10 | 24 | M |
| A6 | Shortcut | keycaps `⌘` `↵` on the white button | inline `⌘↵` text | keycaps, or drop the hint | L |
| A7 | Pill | ringed dot outside the pill, orange #f1ac61 | dot inside the pill, yellow #e6c47c | amber, matching the "Manual" ink | L |

Width (480), header band (48) and r 10 are at parity.

## Composer and footer chips

Evidence: [12](detail/12-composer.jpg), [25 provider name](detail/25-footer-provider.jpg).

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| C1 | Trailing control at rest | mic glyph, 14 pt, #f1f1f1 (no disc) | a 28 pt send disc with a #9c9c9c arrow, visible while the composer is empty | **REMOVE** the disc at rest; show send once there is text | M |
| C2 | `+` | 14 pt, #d5d5d5, stroke 1.44 | 11 pt, #cfcfcf, stroke 1.0 | 14 pt, stroke 1.33, #e0e0e0+ | M |
| C3 | Model name | `Opus 5.5`, #eeeeee, medium (weight proxy 1.92 vs 1.77 body) | `Opus 5.5 (1M)` #cfcfcf, *lighter* than body (1.46 vs 1.64); in live and draft threads it reads **`Claude`** | the model name, never the provider; **REMOVE** `(1M)`; #efefef at 500 | H |
| C4 | Pressed chip while its menu is open | #373737 fill behind the chip | none | a 24 pt #373737 chip fill while open | M |
| C5 | Mode colour | Manual in #e49b31; Bypass in the chip | Manual in #e49b31; Accept edits grey | at parity | – |

Height, width, ring #404040, fill #303030 and the footer baseline (16 below) are at parity.

## Queue strip

Evidence: [13](detail/13-queue.jpg). The structure is at parity (#252525 vs #242424, 44 tall, 8 gap, r 10).

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| Q1 | Label weight and ink | `1 message queued` at medium, #f0f0f0; the message lighter | label and message both #cfcfcf, same weight | label at #efefef 500, message at #9c9c9c | L |
| Q2 | Action icons | 12–14 pt, #a1a1a1 | 8–11 pt, #cfcfcf | 14 pt (S2) | L |
| Q3 | Attachment chips | n/a | filenames in link blue | primary ink. Blue reads as a link and adds a hue | L |

## Menus

Evidence: [14 model](detail/14-model-menu.jpg), [15 permission](detail/15-permission-menu.jpg), [16 plus](detail/16-plus-menu.jpg), [17 slash](detail/17-slash-menu.jpg).

The model-menu findings (current item first, no resting highlight, accent check, inset dividers, no value text on More models, wider padding, a pressed chip, left alignment) apply to **every** menu:

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| M1 | Resting highlight | none in model, permission or plus | #3e3e3e row at rest in all three | **REMOVE**: highlight only under the pointer or the keyboard cursor | H |
| M2 | Selected mark | accent ✓ #237ddb, 15 pt, **left of** the digit | white ✓ 11 pt, right of the digit | accent check before the digit | M |
| M3 | Dividers | inset to the text (≈1054–1278) | full-bleed | inset | L |
| M4 | Tags | `Recommended`/`Default`: ring #595959, no fill, 11 pt | filled pill #404040, 13 pt | ring, no fill, 11 pt | M |
| M5 | Risk colour in the menu | none: names are all primary | `Manual` drawn amber inside the menu | colour only the footer chip | L |
| M6 | Digit alignment on two-line rows | digit on the name line | digit centred on the two lines | top-align with the name | L |
| M7 | Plus menu anchor | left edge 711, just left of the `+` (718); `+` gets a pressed fill | box x 113–340: it opens **leftward over the sidebar** (the composer starts at 303) | anchor the left edge to the `+` button and press the `+` | H |
| M8 | Plus menu icons | 13 pt, #efefef, stroke 1.25 | 10 pt, #9c9c9c | 14–16 pt, primary (S2) | M |
| M9 | Slash names | sans, no `/` prefix, never truncated | mono with `/` prefix; `/code-revi…` is truncated by a fixed name column | **REMOVE** the `/` and the mono; size the name column to the longest name | M |
| M10 | Slash cursor-row glyph | none | `↵` at the right | **REMOVE** | L |
| M11 | Slash descriptions | in a tooltip | inline, #cfcfcf | inline is fine (Codex), but use #9c9c9c | L |
| M12 | Menu row pitch | 28 single-line, 46 two-line | 30 / 48 | 28 / 46 | L |

## Side panel

Evidence: [18 top](detail/18-panel-top.jpg), [19 context](detail/19-panel-context.jpg), [23 filler](detail/23-panel-filler.jpg).

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| R1 | Filler copy | none: a section with nothing to say is not drawn | `No changes yet`, `Shows once the provider reports what fills the window` (sections.rs:68), `Limits not reported by this provider` | **REMOVE**: hide the empty section or row | H |
| R2 | Section headers | #eeeeee, medium | #9c9c9c, regular | primary at 500 | M |
| R3 | Row text and icons | text #eeeeee; icon 16 pt #9c9c9c | text #cfcfcf; icon 12 pt #cfcfcf | text primary; icon 16 pt subtle (the icon is quieter than the word, not louder) | M |
| R4 | Redundant labels | `ferrite · ~/Desktop/Projects`, `main` | `transcript-app-style · /Users/jos…trees`, `transcript-app-style from main` (the same word twice, three times with the title bar) | show the branch only when it differs from the folder (components.md) | M |
| R5 | Section hairline | #252525 | #303030 | #252525 | L |
| R6 | Context head | `27% used` (primary) + `109.9k / 400k` (grey) on one line, then the bar | `Context 32% ⌄`, the bar, then `64k / 200k tokens` on its own line | Claude's one-line head | L |
| R7 | Rows with no counterpart | none | `Last turn $0.42`; one row per connector with a status | **REMOVE** Last turn (or move it to a tooltip); collapse connectors to `Connectors ▸ 3 connected` | M |
| R8 | Legend marks | 10 pt rounded squares | 7 pt circles | 10 pt squares | L |
| R9 | Changes | one row `±  +3,550 −0  [Create PR]` | `1 file changed` + a filled file card (#262626) + a full-width `Review changes` button | one summary row with the action on the right | M |

## Empty or new thread

Evidence: [20](detail/20-empty-thread.jpg). Card width (320), ring #222222 and r 10 are at parity.

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| E1 | Row text and icons | #efefef, 14 pt icons | #cfcfcf, 12 pt icons | primary, 16 pt (S2) | M |
| E2 | Mark | orange mascot sitting on the card's top edge | #303030 ghost logo floating 15 above | either commit to it (the logo in colour, on the edge) or **REMOVE** it | L |
| E3 | Footer | `Opus 5.5 · Medium · Bypass` | `Claude · High` | the model name (C3) | M |

## Scrollbar

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| B1 | Thumb | 6 pt, #5e5e5e, visible on a scrolled thread (x 1912–1917) | no thumb in any capture, including scrolled ones | show a 6 pt #5e5e5e overlay thumb (`SCROLLBAR` #333333 is 1.5:1 against the plane) | L |

## Jump-to-bottom button

Evidence: [21](detail/21-jump.jpg).

| # | Difference | Claude | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| J1 | Body | 36 pt disc, opaque fill #1f1f1f, ring #282828, soft shadow: it floats over text | 32 pt, no fill (#1a1a1a = the plane), ring #303030 | 36 pt, fill #1f1f1f, ring #282828, shadow | L |
| J2 | Arrow | 12 pt, #efefef, about 1.5 stroke | 11 pt, #cfcfcf, 1.0 stroke | primary, stroke per S2 | L |

## Code fence (no Claude capture; compared with Codex)

Evidence: [24](detail/24-code-fence.jpg).

| # | Difference | Codex | Ferrite | Fix | Impact |
|---|---|---|---|---|---|
| F1 | Language label | a header row (`</> Rust` in the UI face) + copy and wrap icons | lowercase mono `rust` at the right edge of the first line, #9c9c9c, always visible (rich.rs:644–680) | **REMOVE** at rest; show copy on hover. If a label is wanted, take Codex's header | M |
| F2 | Radius | r≈16 (Codex) / cards r 10 | r≈5, inside a column of r 10 cards | r 8–10, to join the card family | L |
| F3 | Code ink | syntax colours, plain text #ffffff | plain text #cfcfcf | primary for plain text | L |

## Cross-surface checks

- **One left edge** ([27](detail/27-left-edge.jpg)). Claude: prose, summary line, table and the action row's first icon all sit on 723, and every card hangs at 719. Ferrite: prose 320, summary 321, the tool card 316 but the todo, code and approval cards 320, bullets 323.5, the action icon 325.5. The fix is one rule: text on the column, every card ring at column − 4.
- **Greys in one view.** Claude's thread view (answer-done) shows text in #efefef, #9c9c9c and placeholder #878787, with lines in #222222, #303030, #353535 and #404040. Ferrite's (conversation-app) shows text in #efefef, **#cfcfcf** and #9c9c9c, with lines in #222222, #242424 (chip ring), #303030, #333333 and #404040. Removing the #cfcfcf text tier (fix 2) and the #333333/#242424 rings (G1, K1, P4) brings it down to Claude's count.
- **Weights.** Claude's emphasis is deliberate: the footer model name is heavier than the body (1.92 vs 1.77) and the queue label is medium. Ferrite's footer model name is *lighter* than its body (1.46 vs 1.64). Titles, sidebar and body are at parity.
- **Radii** are now at parity for the bubble, cards, queue and approval (all ≈10). The odd ones out are the code fence (≈5) and the selected nav row (≈5 vs ≈7).
- **Optical centring** found no defects. The composer `+` is centred (837 vs 837), and the status dots and chevrons sit on the cap centre. Misalignment is horizontal (above), not vertical.
- **Hover-only states.** Claude hides everything that is an action rather than content: assistant actions, user actions, row `⋯`, add buttons and shortcuts. Ferrite already does this for the row `⋯` (`navhover-app`) and should apply it to S1, S8, P1, C1 and F1.

## Ferrite surfaces with no Claude counterpart that add noise

These appear in a normal thread with no Claude equivalent: the sidebar foot `Solo · UI overhaul` strip (S9), the Parked count (S10), the project folder icons and tree guides (S3), `Projects +` (S8), the project or branch label in the title bar (T1), `(1M)` (C3), the empty-composer send disc (C1), the shell card's `Shell` and `✓ Success` (K2, K3), the fence's `rust` (F1), the slash `↵` (M10), `Last turn $0.42` and the per-connector list (R7), and the three filler sentences in the panel (R1).

## Top 15 fixes, ranked by visual impact

| Rank | Fix | Type | Where it shows |
|---|---|---|---|
| 1 | Icon stroke 1.0 → 1.33 at 16 (`ICON_STROKE`, theme.rs:746); row and menu icons 14–16 pt; `ICON_INK` → the label's ink (S2, S12, T4, M8, C2, E1, Q2, J2) | change | every surface |
| 2 | Retire #cfcfcf as a *text* tier: panel rows, footer model, empty card, queue, slash descriptions and fence code go to #efefef or #9c9c9c (R3, C3, E1, Q1, M11, F3, A3) | change | panel, composer, menus, transcript |
| 3 | **REMOVE** the panel filler: `No changes yet`, `Shows once the provider reports…`, `Limits not reported…`, `Last turn $0.42` (R1, R7) | remove | side panel, always on screen |
| 4 | **REMOVE** the resting highlight in every menu; accent check before the digit; ringed tags; pressed chip while open (M1, M2, M4, C4) | remove | all menus |
| 5 | **REMOVE** `⌘N`/`⌘K` and `Projects +` at rest (S1, S8) | remove | sidebar |
| 6 | **REMOVE** the title-bar project and branch label, and let the title use the free width (T1, T2) | remove | title bar |
| 7 | Footer: the model name, never the provider; **REMOVE** `(1M)`; name in #efefef at 500 (C3, E3) | remove + change | composer footer |
| 8 | Plus menu: anchor to the `+` instead of opening over the sidebar (M7) | fix | plus menu |
| 9 | Sidebar tree: **REMOVE** the folder icons and guides; project as a 13 pt grey label with ▾; threads at x 18 (S3) | remove | sidebar |
| 10 | One ring for transcript cards: tool-card dividers #303030 → #222222; **REMOVE** the shell card's fill and second ring, plus `Shell` and `✓ Success` inside a group (G1, K1–K3) | remove | tool groups |
| 11 | Selected nav row +14 → about +24, with hover kept at +8; status dot 8 pt solid with no halo; **REMOVE** the sidebar header hairline and the `Solo` foot strip (S4–S7, S9) | change + remove | sidebar |
| 12 | **REMOVE** the answer action row and the composer send disc at rest; show them on hover or with text (P1, C1) | remove | transcript, composer |
| 13 | Approval: plane body, #222222 ring, hairline well, Deny #3b3b3b, 24 gap above the buttons, #9c9c9c description (A1–A5) | change | approval card |
| 14 | One left edge: text on the column, every card at column − 4, bullets at +8, the action icon on the column (W5, P2, P3) | change | transcript |
| 15 | **REMOVE** the `rust` fence label at rest; slash menu in sans with no `/`, no truncation and no `↵` (F1, M9, M10) | remove | code fences, slash menu |

Just below the top 15: the todo done mark as a filled disc and the 14 pt working glyph (W1, W3); side-panel section heads in primary 500 and the one-row Changes summary (R2, R9); the jump button's opaque fill (J1); the scrollbar thumb (B1).

## Limits

- Claude's sidebar sits on macOS vibrancy (#292428). Ferrite's plane is #222222, so plane deltas are compared, not absolute values.
- There are no Claude captures of a code fence, a sidebar hover row or a slash-menu description column. Codex stands in for the fence.
- Claude's UI face is not SF, so glyph-shape differences in the crops are expected and not listed.
- Ferrite renders at 2x and Claude at 1x, so Ferrite's hairlines look crisper in the crops. That is not a design difference.

# Checkpoint 5 review

Renders: `ferrite-cp5/` (deleted after sbs5 was made, for disk space; re-render to inspect), built at 5b739a84 with `FERRITE_REF_SIZES=app` (1440×900 pt at 2x). Side-by-sides: `sbs5/`, made with `.tools/sbs.py` at true size (1 pt = 1 px). The pairs are the same as sbs4 (see CP4-REVIEW.md for the table).

Detail checks were made on crops of the 2x renders, and the gap was measured from pixel columns, not from the downscaled sheets.

## Checks asked for

1. **Footer usage ring: partial pass.**
   - It sits in Claude's spot, right of the mode chip, in 10 of 13 footers: conversation, composer, subagents, toolgroups-open, panel-context, plusmenu, modelpicker, modemenu, nav and contextmenu.
   - It is absent in approval, live and empty-transcript. Those are the three Manual-mode scenes. `plan_usage_ring` returns None when the provider reports no window, so these fixtures probably feed no rate limits. Claude shows the ring in its approval, running and new-session captures, so the pairs still differ.
   - The ring is styled differently. Ferrite's arc is `INK_SUBTLE` (light grey) on a near-invisible `LINE` track, so at true size it reads as a loading spinner. Claude draws an accent-blue arc on a clearly visible grey track.
2. **Usage popover: pass, with one gap.**
   - The plan name is there (`Plan usage limits · Max`).
   - The bar fills accent blue on a grey track.
   - The reset text is there (`Resets Sat 10:40 pm`) before the percentage, and the percentage is full ink.
   - It has one row, like Claude's.
   - Still missing: `See all →` at the right of the header.
   - Small difference: Ferrite writes `pm` in lower case, where Claude writes `AM`.
3. **Row ⋯ menu: pass.**
   - Hairline rules separate the four groups.
   - `Delete thread` is red.
   - Every row has a single-letter key: P, R, F, N, E, C, S, A and D.
   - The keys are muted grey, as in Claude.
   - The anchor still differs. See difference #4.
4. **Status dots only on running and needs-you rows: fail on one row.**
   - Running rows (Retained transcript rendering, Geist type pass) show the orbs, not dots.
   - Nav density audit, which is waiting on an approval, has an amber dot. Its group, UI overhaul, rolls the amber dot up.
   - Finished rows (Checkout copy review, Board card sync, Release notes 0.9, Screenshot harness) are clean.
   - **Fix flaky pump test still shows a red dot.** Its turn ended in an error, and the scene calls `mark_notices_read()`. The scene's own comment says "the finished and failed turns here have been seen, so the two runs and the one approval are the list's only marks". Either a seen failure should drop its dot, or the comment is wrong. It shows in nav, palettefilter and sidebar.jpg.
5. **Context bar in rounded category runs: pass.**
   - The bar is split into separate pill-ended runs with gaps between them: Messages, System tools, MCP tools, System prompt, a Memory files sliver and Autocompact buffer. The rest is the free track.
   - The runs are about 4 pt tall, as in Claude.
   - The palette differs. Ferrite's runs are pastel (light blue, peach, green, lilac). Claude's are saturated (blue, orange-red, teal, amber, light grey, dark grey). Ferrite's System prompt is lilac, where Claude's is amber.
6. **+ menu 3–4 pt above the composer: pass against Claude, though not against the 3–4 pt figure.**
   - In pixel columns, Ferrite's menu edge ends at y 812.5 pt and the composer's top border is at 814 pt. That leaves a gap of about 1 pt.
   - Claude's capture measures the same way: menu edge at 967, composer border at 966. Its menu sits on the composer's top edge, with its shadow falling across it.
   - So the two now match within about 1 pt. The 3–4 pt in CP4 looks like a mismeasure of Claude, probably taken to the edge of the shadow.
7. **Working orbs not clipped: pass.**
   - I checked at 3x zoom: the inline working line (live, subagents, queued), the agent rows (Audit the nav, Audit the composer, Audit settings), the todo checklist row and the sidebar running rows.
   - All four glows are round, with soft falloff on every side and nothing cut at the box edge.

## Remaining differences, ranked

1. **The usage ring reads as a spinner, and it is missing from three scenes.** Every Claude footer ends in a blue-arc ring. Ferrite's is grey on an invisible track, and it is absent from approval, live and empty-transcript. This shows in nearly every pair.
2. **The working mark is white, where Claude's is accent orange** (running.jpg). Claude's working glyph and its approval radio are orange. Ferrite's orbs are neutral white. This may be deliberate, but it is the most visible colour difference in the transcript.
3. **A red dot stays on a seen failed row** (check 4). It is the one dot left that Claude wouldn't draw.
4. **The row menu's anchor and direction differ** (row-menu.jpg).
   - Claude opens the menu downward from the row's `…`, inside the sidebar.
   - Ferrite's bottom edge lines up with the row, and the menu grows upward over the sidebar's right edge and the transcript, although there is plenty of room below.
   - The item sets still differ. Claude has Open in ›, Mark as unread, Fork, Move to section › and Archive. Ferrite has Toggle fullscreen, New thread in this project, Reveal in Finder, Copy path, Refresh PR status and Park.
5. **The context bar palette is pastel, where Claude's is saturated** (side-panel.jpg). The legend swatches follow the bar, so the whole Context block reads softer than Claude's.
6. **The side panel is still missing pieces** (side-panel.jpg).
   - The environment row has no `Remote Control` toggle and no `Save`.
   - Changes uses a file row plus `Review changes`, where Claude shows a single `+3,550 −0  Create PR` line.
7. **No mic in the composer.** Claude has a mic in every composer. Ferrite now shows the stop square while running, but has no mic.
8. **The new-session and hero layouts** (new-session.jpg, hero-codex.jpg).
   - Claude's new session is a centred environment card with its import and auto-mode actions.
   - Ferrite uses a Codex-style hero, and its composer adds a ferrite/main chip strip.
   - Against Codex, the CP4 points stand. The mark is filled grey, where Codex's is outlined. The project name isn't underlined as a picker. The model, effort and mode controls sit outside the composer box.
9. **The right panel is open in every scene,** including a brand-new thread. This is scene setup, but every Ferrite frame reads busier than Claude's.
10. **The popover has no `See all →`** (check 2).
11. **There is no message action row** (tool-groups.jpg). Claude ends a finished answer with copy, branch, pin and speak icons and `Just now`. Ferrite has nothing under the answer.
12. **The + menu has 3 items to Claude's 5** (`Connectors ›` and `Plugins ›`). That is content, not styling.
13. **Stale hover grounds in palettefilter** (palette.jpg). These are unchanged from CP4: `Menu photography brief` with its `…`, `swarmdeck` with its `+`, and the focused row. They are a harness artefact, carried over from navhover.
14. **The sidebar ground is neutral** (39,39,39), where Claude's is tinted (41,36,40). This is unchanged from CP4.
15. **Close (no action).**
    - The model menu and mode menu match in row height, width and keys.
    - The approval card's width, chips and buttons match.
    - The open tool group's rhythm matches.
    - The queued-message bar matches in layout and icons.

# Checkpoint 4 review

Renders: `ferrite-cp4/` (deleted after sbs4 was made, for disk space; re-render to inspect) (`FERRITE_REF_SIZES=app`, 1440×900 pt at 2x), built after polish lanes C (chrome), D (density) and E (working mark). Side-by-sides: `sbs4/`, made with `.tools/sbs.py`.

**True size.** Unlike sbs2 and sbs3, every sbs4 pair is at 1 pt = 1 px: Ferrite's 2x render is scaled by 0.5, and the 1x Claude and Codex captures are left at 1.0. The pairs are top-aligned on a near-black ground. Claude's windows are 1920 pt wide and Ferrite's are 1440 pt, so Ferrite's frame is smaller, but type, rows and cards can now be compared directly. `sbs.py` does this by default; `--scale L,R` overrides it.

## Pairs

| File | Reference | Ferrite scene |
| --- | --- | --- |
| answer.jpg | claude/answer-done | conversation |
| approval.jpg | claude/approval | approval |
| model-menu.jpg | claude/model-menu | modelpicker |
| new-session.jpg | claude/new-session | empty-transcript |
| permission-menu.jpg | claude/permission-menu | modemenu |
| plus-menu.jpg | claude/plus-menu | plusmenu |
| queued.jpg | claude/queued | composer |
| running.jpg | claude/running | live |
| side-panel.jpg | claude/side-panel | panel-context |
| subagents.jpg | claude/thread-bottom | subagents |
| tool-groups.jpg | claude/failed-open | toolgroups-open |
| hero-codex.jpg | codex/home | empty-transcript |
| plus-menu-codex.jpg | codex/plus-menu | plusmenu |
| side-panel-codex.jpg | codex/side-panel | panel-context |
| sidebar.jpg | claude/new-session, left 20% | nav, left 30% |
| usage-popover.jpg (new) | claude/usage-popover | usagepopover |
| row-menu.jpg (new) | claude/row-menu | contextmenu |
| palette.jpg (new, loose) | claude/slash-picker | palettefilter |

**No confirm pair.** `claude/confirm-dialog.png` exists, but no Ferrite scene opens a confirm dialog. It needs a scene before it can be compared.

**The palette pair is loose.** There is no Claude palette capture, so the slash picker stands in as the nearest floating list. Only list styling is comparable in it.

## Checks asked for

- **Back/forward arrows: pass.** They sit in the sidebar head slot (nav, sidebar.jpg), at about 88 to 118 pt from the window edge. The traffic-light slot to their left is clear. Bell, search and the sidebar toggle sit at the right of the same slot.
- **Project names untruncated: pass.** `flavorlab-site ⌄`, `swarmdeck ⌄`, `ferrite ⌄` and `transcript-app-style ⌄` show in full in every scene. This was CP3's #1.
- **Working mark not clipped: pass.** I dumped pixels from the 2x renders in three places:
  - the sidebar running rows (nav: Retained transcript rendering, Geist type pass);
  - the inline working line (nav, subagents, subagent, live);
  - the agent rows (subagents: Audit the nav, Audit the composer, Audit settings).

  In every one, the upper shard comes to its pointed tip with transparent rows above it, and nothing is flattened at the box edge. The marks measure 12 to 14 pt tall.

## Fixed since CP3

- Project headings are untruncated (CP3 #1).
- The footer model reads `Opus 5.5` everywhere, never `Default` (CP3 #2). The model menu's checked row matches it (CP3 #11).
- The side panel order is now Environment, Context, Connectors, Changes, as in Claude (CP3 #3, order only).
- The placeholder while running reads `↵ to send, ⌘↵ to queue for later` (CP3 #4, text only).
- Tables fit their content and stay left-aligned (CP3 #6).
- The permission menu's rows are neutral, and only the footer chip is amber (CP3 #8).
- The `+` sits on a pressed ground while its menu is open (CP3 #9, styling only).
- Sidebar row pitch is 34 pt, matching Claude (CP3 #10, pitch only).
- The approval's key hint is two chips, `⌘` and `↩` (CP3 #12).

## Remaining differences, ranked

1. **The footer has no usage ring outside the popover scene.** Claude's footer always ends with the plan-usage ring, right of `Manual` or `Bypass permissions`. Ferrite's footer ends at the mode chip in every scene except `usagepopover`. This shows in every pair.
2. **The usage popover differs (usage-popover.jpg).**
   - Ferrite has no plan name in the header (Claude: `Plan usage limits · Ultra`) and no `See all →`.
   - Ferrite has no `Resets Tue 3:30 AM` before the percentage.
   - Ferrite's bars fill white. Claude's fill accent blue.
   - Ferrite shows two rows (5-hour and Weekly), where Claude shows one. That may come from the data, but Ferrite's popover is about 40% taller as a result.
   - Ferrite's percentages are muted. Claude's are full ink.
3. **The row menu differs (row-menu.jpg).**
   - Ferrite separates groups with gaps. Claude draws hairline rules between them.
   - Ferrite's `Delete thread` is neutral. Claude's `Delete` is red.
   - Claude shows a single-letter mnemonic on every row (P, U, R, F, A, D). Ferrite shows chorded shortcuts on three rows only, and they are smaller and fainter than the labels.
   - Claude opens the menu under the row's `…` button, inside the sidebar. Ferrite's scene opens it at the pointer, so it hangs over the transcript. That is partly the scene setup.
   - The item sets differ. Claude has `Open in ›`, Mark as unread, Fork, `Move to section ›` and Archive. Ferrite has Toggle fullscreen, New thread in this project, Reveal in Finder, Copy path, Refresh PR status and Park.
   - Row height matches at 28 pt.
4. **Status dots on nearly every sidebar row (sidebar.jpg, nav).** White, amber and red dots sit on almost all thread rows. Claude shows a dot only when a thread needs attention. This is still the busiest visible difference in the sidebar.
5. **The side panel is missing pieces (side-panel.jpg).**
   - There is no `Remote Control` toggle and no `Save` on the environment row.
   - Claude's context bar is segmented, one colour per category. Ferrite's is a single fill.
   - Ferrite's Changes block lists the file row and `Review changes`, where Claude shows a single `+3,550 −0  Create PR` line. Ferrite's is denser, which may be deliberate.
6. **No mic in the composer.** Claude has a mic in every composer, plus the stop square while running. Ferrite has neither the mic nor its slot.
7. **The hero (hero-codex.jpg, new-session.jpg).** The CP3 differences stand:
   - The project name in the headline isn't underlined as a picker.
   - The mark is a filled grey logo, where Codex's is outlined.
   - The model, effort and mode controls sit outside the composer, where Codex puts its controls row inside.
   - Codex's headline also reads a little larger at true size.

   The composer widths are now close: about 733 pt for Codex against 767 for Ferrite.
8. **The right panel is open in every scene, including a brand-new thread.** Claude's captures have it closed except in side-panel. This is probably scene setup, but it makes Ferrite's frame read busier in every pair.
9. **The + menu sits further above the composer (plus-menu.jpg).** Ferrite leaves a gap of about 12 pt, against Claude's 3 to 4 pt. Ferrite also lists 3 items to Claude's 5 (`Connectors ›` and `Plugins ›` are missing). That is content, not styling.
10. **The sidebar ground is neutral.** Ferrite's is (39,39,39). Claude's is vibrancy-tinted (41,36,40). This is subtle at true size.
11. **The palettefilter scene shows stale hover grounds.** `Menu photography brief` (with its `…`), the `swarmdeck` heading (with its `+`) and other rows all show hover grounds while the palette is open. The ferrite-cp3 render has the same pixels, so it isn't new. It looks like hover state carried over from the `navhover` scene, which renders earlier in the same run. This is a harness artefact, not product behaviour, but it makes the palette render misleading.
12. **Close (no action).**
    - The approval card's width, the type and the button chips match at true size.
    - The open tool group's rows and command well match Claude's rhythm.
    - The model and mode menus match in row height and width. Claude's Fast mode section is provider-specific.

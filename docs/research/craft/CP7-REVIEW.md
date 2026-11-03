# Checkpoint 7 review

Renders: `ferrite-cp7/`, built at 401c7d8d with `FERRITE_REF_SIZES=app` (1440×900 pt at 2x). They were deleted after the sheets were made; re-render to inspect. Side-by-sides: `sbs7/`, made with `.tools/sbs.py` at true size (1 pt = 1 px).

## Pairs

The pairs are the same as sbs6 (see CP4-REVIEW.md), with these changes:

| File | Reference | Ferrite scene |
| --- | --- | --- |
| answer.jpg | claude/answer-done | answer (was conversation) |
| running.jpg | claude/running | running (was live) |
| row-menu.jpg | claude/row-menu | row-menu (was contextmenu; both render the same menu) |
| answer-hover.jpg (new) | claude/hover-assistant | answer-hover |

`plus-menu.jpg` (plusmenu) and `queued.jpg` (composer) keep their scenes. There is no Claude capture of the page pane, lightbox, file cards or toasts, so I checked those by eye in `output-cards`, `page-pane`, `lightbox` and `toasts` without a pair.

## Ledger lanes, by eye

**Done tonight**
- Reply to a selection pill and Quote rows: **not visible.** No paired scene shows a selection.
- Sidebar 13px on `#212121`: **pass.** sidebar.jpg: the row text matches Claude's size, and the column is darker than the transcript.
- Thinner text: **pass.** Body text weight is close to Claude's in answer.jpg.

**Lane M (motion).** This lane can't be judged from stills. The 1px stroke on the tooltips and toasts is in place: toasts reads cleanly.

**Lane W**
- Working line uses the tool's words, in muted ink with white orbs: **pass.** running.jpg: "Waiting 15 seconds then printing done", as Claude has.
- Live token count with an arrow: **pass.** "27s · ↓ 557 tokens".
- Thinking effort: **not visible.** No scene shows thinking.
- No duplicate running row: **pass.**
- Running rows in the UI font: **pass.**
- Placeholder gap before ↵: **pass.** "▏ ↵ to send" has a clear gap after the caret.
- Errors once: **pass.** "List a directory that doesn't exist · Failed" appears once.
- Relative times: **pass.** answer-hover shows "just now".
- Date dividers: **pass.** "Wednesday 4 November" in answer and running.
- PageUp and PageDown: **not visible.**

**Lane C**
- Image thumbnail cards in the tray: **pass.** queued.jpg: an 88pt card, with the file chip beside it.
- Image message in a scene: **pass.** The composer scene sends one.
- `/skill` chips and source tags: **not visible.** No slash-picker scene.
- + menu with Connectors › and Plugins ›: **pass.** plus-menu.jpg has the same five rows as Claude.
- Queued rows each with their own actions: **pass.** queued.jpg: two rows, each with ↵, edit and ×.

**Lane A**
- Answer actions Copy, Fork and ⋯: **pass.** answer-hover.jpg. Claude also shows Pin and Read aloud, which the ledger leaves out on purpose.
- Thread ▾ menu: **not visible.**
- Sidebar row menu: **pass with a note.** row-menu.jpg has Claude's order, rules and letter keys, and Open in › opens beside the menu. "Mark as unread" is missing in this scene because the thread is on screen (`context_menu.rs:369`), so the omission is by design.
- Allow and switch to Auto: **not visible.** It sits inside the split button's ▾.
- Stop an agent from the keyboard, palette rewind and fork, side chat: **not visible.**

**Lane P**
- Page pane: **pass.** page-pane shows the title menu, expand, × and the can't-show note.
- Preview and file cards: **pass.** output-cards: globe thumbnail, Open and ⋯, with Open in › on the file card.
- Lightbox: **pass.** The dim overlay has arrows, "2 of 3", zoom, download and open.
- Right panel closed by default: **pass.** answer and running have no panel.
- Toasts clear of the panel: **pass.**
- Long titles fade instead of ending in "…": **fail.** See regressions 1 and 2.
- No ↑N counts on project headings: **pass.**
- Branch in the title when the panel is hidden: **pass.** "main" follows the title in answer and is gone in answer-hover, where the panel is open.

**Lane X.** Codex rewind can't be seen in the scenes. The subagent drill-in is an owner item.

**Lane G**
- Upstream #65952: **not visible.**
- 1px icons: **pass.** The header, sidebar and panel icons are crisp, with no thinning or broken strokes at 2x.

## Regressions

1. **Project headings fade even when they fit.** Seen in sidebar.jpg, row-menu.jpg and every scene with a project heading: "flavorlab-sit(e)", "swarmdec(k)", "ferrit(e)" and "transcript-app-styl(e)" all lose their last letter or two to the fade. CP4 to CP6 showed them whole. `project_row` pins the label box to the measured name width, and `fitted` draws `components::line_fade` every time, so the last `LINE_FADE` (24pt) of a name that fits gets covered. Likely source: `crates/ferrite-cockpit/src/nav.rs`, `fitted` and its `project_row` call (commit 76e2c416). The fade should be drawn only when the line overflows.
2. **Long thread titles still end in "…", under the fade.** In answer.jpg, running.jpg and approval.jpg, "UI test: color tokens and shell com…" has both the ellipsis and the fade. The fixture title is the full string, so the text element is cutting it. Likely source: the vendored gpui text element (`vendor/gpui-pre`, `elements/text.rs`), which ellipsizes a nowrap line that overflows. The `.overflow_hidden().whitespace_nowrap()` change in `nav.rs` `fitted` didn't stop it.
3. **Narrow table columns break inline code mid-token.** In answer.jpg and answer-hover.jpg, the Command column is about 110pt wide, so `ls /definitely-missing-dir` wraps as `ls` / `/definitely-missin` / `g-dir`, and `Exit 1: No such file or directory` wraps too. Meanwhile the rules run the full measure, with empty space to the right. Claude sizes the columns to their content, all on one line. Likely source: `crates/ferrite-pane/src/rich.rs`, the table column measure, which seems to leave inline-code chips out of each cell's natural width.

I saw no overlap, no broken layout, and no wrong colours or text sizes elsewhere.

## Remaining differences from Claude, ranked

1. **Tables** (regression 3): column sizing and code wrapping are the most visible difference in an answer.
2. **Sidebar title endings** (regressions 1 and 2).
3. **Inline code in sent prompts shows raw backticks.** In answer, running and approval, Ferrite's bubble reads `` `ls /definitely-missing-dir` ``. Claude draws it as a red-ink code chip.
4. **Approval card.** Ferrite's command block is plain ink, where Claude's is green. Claude also titles the card with the tool's description ("List a directory that doesn't exist"); Ferrite shows the agent's reason instead.
5. **The row menu has extra items.** Ferrite adds New thread in this project N, Copy path C and Refresh PR status S, which Claude doesn't have. A disabled Fork wraps to two lines with "a turn is running; stop it first"; Claude has no two-line rows.
6. **Usage popover has no "See all →".** usage-popover.jpg.
7. **Model menu has no Fast mode section.** model-menu.jpg. This is data-driven, so it may be correct.
8. **Answer hover.** Claude's actions row has four icons (copy, share, pin, read aloud). Ferrite's has three (copy, fork, ⋯). This is intended.
9. **New session.** Claude puts the mascot and environment cards in the centre, with the composer at the bottom. Ferrite's hero is Codex-style, which the owner chose to keep.
10. **The footer has no mic icon,** which the owner deferred.

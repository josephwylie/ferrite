# Checkpoint 3 review

Renders: `ferrite-cp3/` (`FERRITE_REF_SIZES=app`, 1440×900 pt at 2x), built after the motion pass. Side-by-sides: `sbs3/`, made with `.tools/sbs.py`.

**Scale caveat.** Each pair scales both images to 900 px high, the same way as `sbs2/`. The Claude and Codex captures are 1050 pt windows and Ferrite's is 900 pt, so in every pair except `sidebar.jpg` Ferrite looks about 17% larger than it really is. Don't judge type or row sizes from those pairs. `sidebar.jpg` puts both at 1 pt = 1 px, so sizes there can be compared directly.

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
| hero-codex.jpg (new) | codex/home | empty-transcript |
| plus-menu-codex.jpg (new) | codex/plus-menu | plusmenu |
| side-panel-codex.jpg (new) | codex/side-panel | panel-context |
| sidebar.jpg (new, pt-matched crop) | claude/new-session, sidebar | nav, sidebar |

**Usage popover: no pair.** `claude/usage-popover.png` exists, but no visual-reference scene opens Ferrite's usage popover. It needs a scene before it can be compared.

## Remaining differences, ranked

1. **Project headings truncate with room to spare.** The sidebar shows `ferri…`, `flavorlab-s…`, `swarmde…` and `transcript-app-st…` even though the column has space for the full name. Claude shows `ferrite ▾` and `agenticOS ▾` in full. This shows in every pair, and it is the most visible defect left.
2. **The footer model label reads `Default` in some scenes.** new-session, running and approval show `Default  High  Manual`. The answer, menu and panel scenes show `Opus 5.5  High`. Claude always shows the model name (`Opus 5.5  Medium`). The cause may be the fixture's model alias, but on screen it looks like a missing model name.
3. **Side panel order and content (side-panel.jpg).**
   - Ferrite's order is Environment, Changes, Context, Connectors. Claude's is Environment, Context, Connectors, Changes, with Changes last.
   - Ferrite has no `Remote Control` toggle and no `Save` on the environment row.
   - Ferrite draws rules between sections. Claude separates them with space only.
   - The Codex panel (side-panel-codex.jpg) is a floating card, not a column. It isn't a target, just context.
4. **Running state (running.jpg, subagents.jpg).**
   - Ferrite's working mark is a small, muted arc spinner. Claude's is a larger coral four-dot glyph.
   - Ferrite's placeholder while running reads `↵ to send, esc to stop`. Claude's reads `↵ to send, ⌘↵ to queue for later`, and Claude also shows a mic beside the stop square.
5. **The hero and new thread (new-session.jpg, hero-codex.jpg).** Ferrite follows Codex here, and these are the differences from it:
   - Codex's composer is much narrower than Ferrite's: about half its main column, against about 90% for Ferrite.
   - Codex's composer is taller and two-line, with an empty first line and the controls row inside it.
   - Codex's project name in the headline is underlined as a picker. Ferrite's is plain text.
   - Codex's mark is an outlined glyph. Ferrite's is a filled grey logo.

   Claude's new session is a different design altogether: a mascot, a `Default` / `Choose environment` card, and the environment row under the composer. Ferrite has none of it, by choice.
6. **Tables (answer.jpg).** Ferrite stretches a table to the full column and right-aligns numbers at the far edge. Claude's columns fit their content and stay left-aligned. Ferrite's table reads as sparse.
7. **The right panel shows on every scene, including a brand-new thread.** Claude's captures have it closed except in side-panel.png. This is probably scene setup, not a default. Either way, the transcript column it leaves is narrower than Claude's.
8. **The permission menu (permission-menu.jpg) tints `Manual` amber inside the menu.** Claude's menu rows are all neutral, and only the footer chip carries the colour.
9. **The + menu (plus-menu.jpg, plus-menu-codex.jpg).**
   - While its menu is open, Claude's `+` sits on a pressed square ground. Ferrite's `+` shows no ground.
   - Ferrite lists 3 items, against Claude's 5 (`Connectors ›` and `Plugins ›`). Codex lists about 9 under `Add` and `Plugins`. That gap is content, not styling.
10. **The sidebar at true size (sidebar.jpg).**
    - Ferrite's rows are 32 pt apart. Claude's are 34.
    - Ferrite puts a status dot (white, amber or red) on almost every thread row. Claude shows a dot only when a thread needs attention.
    - Claude's sidebar ground is vibrancy-tinted (41,36,40). Ferrite's is neutral (39,39,39).
    - Ferrite puts Search in the header icons, where Claude has a `Search` row. This is deliberate.
11. **The model menu (model-menu.jpg).** The checked row reads `Opus 5.5 (1M)` while the footer reads `Opus 5.5`: two names for one model. Claude's `Fast mode` section is absent. That is provider-specific and fine.
12. **The approval card (approval.jpg) is close.** One difference: Ferrite shows the key hint `⌘↩` as a single chip, where Claude shows two separate key chips (`⌘` `↩`).
13. **Subagents and tool groups (subagents.jpg, tool-groups.jpg) match in structure.** The card's rows and its `Running agent` / `Ran agent` labels match. Any difference in row height can't be judged at this scale.
14. **Queued (queued.jpg) is at parity in structure.** The strip shows a count, the text and three actions, laid out as Claude's is.

## Not checked here

Motion doesn't show in stills. The motion pass in this checkpoint, with chevrons, minimap and pinned jumps, and the Panel's hunk unfold all snapping, needs a recording to confirm.

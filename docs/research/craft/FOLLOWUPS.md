# Cross-lane follow-ups (orchestrator applies or routes at the checkpoint)

## From Lane G (approval)
- [x] subagents.rs patch applied.
- Placeholder "Answer above": Lane E.

## From Lane C (tool groups)
- [x] Lane B / transcript.rs: disclosure_chevron(expanded, targeted || shown) at ~1242; drop `eased`, `Disclosure.toggle` (~1150, 1244), `GroupCx.expanded`. Then Lane C drops `_eased`, `Disclosure::toggle`, `DisclosureToggle`, `GroupCx::expanded`.
- [x] theme.rs: add TOOL_CARD_ROW_H 36, TOOL_CARD_INSET (SPACE_3), TOOL_CARD_OUTSET (SPACE_1); delete FOLD_EASE_MS, OUTPUT_PREVIEW_LINES.
- Core (optional): ToolBlock.exit_code for Codex commands.
- [x] visual_reference/mockup.rs:594 stale "· 1 failed" comment.
- A lone call reading as its description: grouping in rows.rs (Lane B).

## From Lane D (live state)
- [x] theme.rs: MOTION_WORKING_DOTS_MS 600, WORKING_DOTS_STEPS 18; delete MOTION_SWAP_MS, MOTION_SWAP_FROM_OPACITY, METER_* tokens.
- [x] cockpit.rs ~1936 working_tail call: drop `running:` and `composer_empty:` fields; then Lane D deletes them from WorkingFacts. Fix turn_running doc (~248) "esc to interrupt".
- [x] Lane E composer.rs:89 send_is_stop doc stale.
- Lane C text.rs turn_stamp: interrupted -> "Interrupted" no duration; failed latest Main turn -> None (keep stamp for older turns/subagents).
- transcript_glue.rs failure_tail: strip words = TurnOutcome::Error(message), not "Stopped … ago".
- Decide: running call shown twice (group_live row + working line) -> drop group_live row (Claude shows one line).
- Lane A: split-right button lived in the strip; needs a title-bar home.
- Todo current mark: arc spinner doesn't exist yet; one shared arc spinner for nav + todo.

## From Lane F (menus)
- [x] Confirm theme.rs edits: MODEL_PICKER_W 260, EFFORT_MENU_W, MODE_MENU_W, ATTACH_MENU_W, SUBMENU_GAP, mode_is_risky(); MODEL_PICKER_NUDGE deleted. subagents.rs: submenu: None.
- [x] Lane E pane/composer.rs: mode_chip uses theme::mode_is_risky; move "Manual" rename into permission_mode_label, then delete cockpit mode_label; mode_is_default dead.
- [x] popover_footer dead: delete in pane/menus.rs + re-export pane.rs:37.
- [x] Lane E: model_chips returns one item -> Option; check Lane E didn't also merge model+effort chip. (Now `model_chip` -> Option.)
- Skill tag on slash rows: MenuRow field + 3 constructors in pane.rs.
- Model menu right-aligned to its button (Claude), currently left-anchored.

## From Lane E (composer)
- [x] Lane F already merged model+effort chip; reconcile with pane::model_effort_chip (use it in model_chips + draft_model_chips), then delete pane::model_chip/effort_chip. (Both chips use it. The draft's model menu now carries the Effort submenu as the live one does, so the draft's separate effort chip, `BandChip::Effort` and `pane::effort_chip` are deleted.)
- [x] Delete PaneWiring.unqueue + pass-through (pane.rs ~665, 871, 919, 1048, 1184, 1391) and cockpit `unqueue:` block; then ComposerStack.unqueue.
- [x] cockpit.rs send_button doc: "Queue a follow-up" -> "↵ to send, esc to stop".
- Core: per-message queue actions need addressing by id (cancel_queue pops back). Queue strip is one row + count for now.
- Skipped: "This computer" chip, usage card plan name + "See all".

## From Lane H (side panel + empty state) — cockpit won't build until 1–4 land
1. [x] cockpit.rs PR tab: delete `mod pr_tab;` (~14), `pr_tab` field (~475–477) + Default (~1550), cfg(test) pr_tab_reads (~593–594) + init (~1607); open_pr (~1805) drop `.or_else(|| self.pr_tab.url(thread))`; delete `self.tend_pr_tab(cx);` (~2257).
2. [x] cockpit.rs panel: ~2709 board_right_rest() -> board_right() (+ doc ~2700); ~10010 `let sliding = self.reader_slides.moving();`; ~10140 ride_panel(window, cx) -> tend_panel(window).
3. [x] palette.rs ~1211 self.open_panel(thread, cx); ~1462 panel_rest_column_w() -> panel_column_w().
4. [x] cockpit.rs ~10949 DraftState literal: `start: if level == Level::Transcript && !self.grid_board() { self.draft_start(index, cx) } else { Vec::new() },` (optional: share labels with draft_band_chips).
5. [x] Visual reference (feature-gated): visual_reference.rs:1105 open_panel; mockup_scenes.rs Shot::panel() -> bool + open_panel, drop fixture_unfold_description (195), PR scenes -> Changes PR row or cut; feature_scenes.rs:395 drop fixture_hold_panel. (PR scenes now open the panel and show its PR row; `refresh-pr-described-open` and `reader-slide` cut as duplicates.)
6. [x] Dead: facts::pr_adopt, PrWatch::adopt (pr.rs), PullRequest::of_wire (core); Preview::let_go + closing hold (ferrite-pane preview.rs); stale PR-tab docs components.rs:1566, 1860, usage.rs:390. (`of_wire` is still called by `of_view`, so it's private now, not deleted. `facts::age_label` deleted too.)
7. Lane E usage.rs: PANEL_ROW_ICON_X now 0 → heading hover wash needs own inset.
- Open decision: keep transcript column fixed when panel opens (Claude) — needs board/Pane layout change; currently re-centres in one frame.
- Note: PR tab + Details deleted wholesale (gh snapshot reads, readiness, comments, description, Outputs). Confirm with owner.

## From Lane A (chrome + nav)
- [x] Lane H: drop STATUS_BAR_H uses (panel.rs:551, panel/sections.rs:51), then delete the constant (bridged at 0).
- [x] Lane D already hides tab_strip with one subject (check). (Confirmed: `tab_strip` returns None with no Subagents.)
- Thread ▾ menu: add "Copy link"/"Fork" verbs later (menu range).
- Activity preview: real last-assistant-message field on ThreadFacts (architecture phase).
- Free-disk warning moved to sidebar foot (<20 GB).
- Split-right button (lost with the strip, from Lane D): give it a title-bar home.

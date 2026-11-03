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

## Transcript step 1, E-core (delivery) follow-ups
- [ ] Composer reads `open.queue()` (id + delivery) instead of queued()/queued_all(); per-item edit/remove call edit_queued_item / remove_queued_item; hide edit/remove for Steering.
- [ ] Then delete mirror queued()/queued_all() and cockpit's untargeted cancel_queued/edit_queued.

## Transcript step 1, lane B (core) follow-ups for step 2 UI
- [x] caption: None added to the 5 ToolBlock literals (pane.rs tests, core_transcript_parity.rs).
- [ ] Lane C: WorkKind::of -> tools::policy(name).kind; rows.rs groups()/OWN_ROW -> policy().grouping (StandsAlone/OwnRow break groups); draws_nothing gets !tool.draws_row(); show caption + failure suffix; is_command_run from transcript::tools (pane.rs:3846, :4128, text.rs:669), then delete the docview re-export.
- [ ] Lane D: failure strip now shows for a session closed mid-turn (TurnOutcome::Error(UNFINISHED)); check wording.
- Commit order: Step 0, then pure move (snapshot in scratchpad/pure-move/), then lane B rest.

## Transcript step 2, lane C follow-ups
- [x] pane.rs `argument_is_path` / `PATH_TOOLS` → `matches!(policy(name).kind, ToolKind::Read | Edit | Write)`.
- [ ] Run cockpit transcript_rows.rs / core_transcript_parity.rs at checkpoint 2.

## Subagents lane (after step 2)
- Group agent calls into Claude's "Ran N agents" card (policy: Agent → RollsUp-within-agents, or a dedicated agents group) with per-row task, spinner, done/failed, summary.
- Open a subagent from its row (match Claude's behaviour — capture needed); replace the Main/agent tab strip and "Back to Main".
- No duplicate running row (agent row + working line).
- Designed subagent view states; rows use §13 data (progress summaries, last tool, usage).

## Provider options (audit) — applied / pending
- [x] Claude spawn env: EMIT_TOOL_USE_SUMMARIES, EMIT_SESSION_STATE_EVENTS, ENABLE_TODO_TOOLS; initialize agentProgressSummaries.
- [x] activity.rs: invalidate after the Closed append, so a mid-turn close fails its turn (lane D test frame_parity::a_session_closed_mid_turn_leaves_the_strip_without_retry).
- [x] thinking_tokens estimate in the live line (working.rs).
- [ ] Later: supportedDialogKinds (refusal_fallback_prompt), question previews (CLAUDE_CODE_QUESTION_PREVIEW_FORMAT=markdown + pane render), --include-hook-events with hook UI; Codex approvalsReviewer auto_review, collaborationMode plan, thread/compact/start, review/start.
- [ ] Lane D leftovers: turn_stamp Interrupted / None for failed latest Main turn (needs row context); "N background agents completed" row (core block on background-origin turn + quiet row).

## Transcript step 2, lane E follow-ups
- [x] `pane::queued_takes_back` must exclude `Sending` (core can't cancel unacknowledged items by id) — or make core cancellable. (Excludes `Sending`; core cancellation still open.)
- [ ] Checkpoint 2 must run: core `native_queue_history_records_consumption_once` (rewritten, not rerun), cockpit escape/queue tests, lifecycle/notifications/subagent_cockpit.
- [x] rustfmt on pane/text.rs. (`cargo fmt --all` at checkpoint 2.)

## Checkpoint 2 (integrator)
- [x] `RowCx::narration` deleted (always false), with render_block's grey/narrow narration branches and `prose`'s quiet path.
- [x] `file_links::wire_inline`: the path chip's hover underline is `INLINE_CODE_INK`, not `LINK_INK`.
- [x] theme: `PROMPT_BUBBLE` #262626 and `INLINE_CODE_CHIP` #2a2a2a split from `RAISED` #242424 (typecolour.md).
- [x] gpui-base lib tests compile again (pre-existing breakage at HEAD): `indoc!` → `concat!` (a patched non-member's dev-dependencies never resolve), missing test imports in inline.rs, `Styled::font_weight` in style.rs. `cargo test -p gpui-base` is refused (non-member with dev-dependencies); run them with `cargo build -p gpui-base --lib --tests` and the printed binary.

## Open tool row (no box in box) follow-ups
- [x] Diffs inside an open group still use the filled `block_box` — give them the same hairline-well treatment as Claude's diff (polish lane A: `pane::diff_well`, `CARD_RING`, no fill).
- [ ] Decide: standalone call card shows Claude's "Bash" label? (left out for consistency); command syntax colour (left in INK).

## Wave 2 lane T follow-ups
- [x] Menu row height 30 → Claude's 28 (`FLOAT_ROW_H`, ~30 uses) — polish pass (note rows 48 → 46).
- [x] Panel Connectors row icon: `BLOCKS` (Claude) instead of `PLUG` (lane P).
- [x] thinking_tokens estimate: new non-persisted Live event (session_event, protocol, engine, store skip, claude wire, codex match, transcript) → working.rs shows max(reported, estimate). After lane R lands.
- [ ] Plugins: per-plugin submenus need nested submenus.
- Checkpoint tests: composer_chips, stab, stale_chrome, provider_picker, composer_controls, boards, image_viewer, core_transcript_parity, ferrite-ui menu/icons.

## Wave 2 lane N follow-ups
- [x] Delete the strip's split button (now in the title bar): `split_beside_button`, `PaneWiring.split`, `tab_strip` `split` param, cockpit `split:` line (~11077). (Integrator: already gone with A2's strip removal.)
- [x] Panel file-preview tabs must stop short of the title tools (`panel_toggle_inset() + ICON_BUTTON + TITLE_TOOLS_GAP + tools width`). (`title_tools_room`, shared with the view head.)
- [ ] Verify gpui matches `ctrl-{` / `ctrl-}` on Windows for Next/Previous pane.
- Checkpoint tests: cockpit::history::*, view_head, cockpit/tests/navigation.rs, keymap, icons, panel toggle_sits_in_one_place. Never compiled.

## Wave 2 lane P follow-ups
- [x] Remove `ContextDetails.usable_window` / `auto_compact_threshold` (unused now) after lane R. (Old records still load; the compat fixture keeps the keys.)
- [x] Plan name in the usage card ("· Ultra") from initialize `account.subscriptionType` (claude.rs). (`SessionEvent::AccountPlan`; `Claude Max` reads `Max`.)
- [x] `context_meter_segments` doc mentions a panel foot that no longer exists.
- [x] Apply lane T's 4-arg `composer_footer` at cockpit.rs:8530. (Already applied: lane T's signature is 3-arg, `lead` moved to `env_strip`.)
- Checkpoint tests: clock day_label, panel::changes create_pr prompt, review::create_pr, panel::connectors_fold, contract_native_controls context refresh, provider_controls context card, composer_chips context/ring, pane rate_limit_resets.

## Wave 2 lane S follow-ups
- [x] Owner (2026-11-03): keep Inbox as is for now (opens via ⌘I/palette); don’t fold into the bell.
- [ ] Pins not removed when a Thread is deleted (harmless).
- Checkpoint tests: nav_list new 4 + changed, floats_parity notifications, thread_groups:341, nav.rs head test, settings round-trip; watch ui_c.rs and floats_parity for Show more.

## Wave 2 lane A2 (subagents) follow-ups
- [x] Codex: agent rows don't open (agents keyed by thread id); "spawnAgent" not in the tool policy as Agent. (Bridge names spawns `tools::AGENT` and aliases the call id to the child.)
- [ ] Agent row keyboard stop (details toggle gone).
- [ ] Faces only where the agent ran live (replay has no agent state).
- [ ] Drill-in header design is a guess — confirm against a Claude capture.
- Checkpoint: `cargo test -p ferrite-core -p ferrite-pane -p ferrite-cockpit --lib -- background_origin every_tool_has_one_policy agents transcript::rows a_call_line_reads a_group_summary working subagents stab:: ui_c transcript_rows core_transcript_parity`

## Wave 2 lane R (edit/rewind) follow-ups
- [ ] Codex path: `thread/revert` / `thread/fork beforeTurnId` (not started; Codex threads show no buttons).
- [ ] Palette commands for Edit / Rewind / Fork.
- [ ] Live capture to confirm `claude-rewind-2.1.317.jsonl` reply shapes.
- [x] Re-run v12-torn compat fingerprint (repinned unverified). (Integrator: holds; v11-activity repinned for the dropped `ContextDetails` fields.)
- Checkpoint: full core/pane/cockpit; known cockpit failures from run 7: composer_chips::the_context_section_reads_the_window_and_the_ring_the_plan, composer_controls::the_model_gives_way_before_the_permission_mode, panel_preview::focus_moving_to_another_pane_takes_the_keyboard_from_an_image.

## Wave 2 checkpoint (integrator)
- [x] Builds clean (`--workspace --tests`, `visual-reference`); one stray `ScrollableElement` import in pane.rs.
- [x] Usage popover: the kit `Popover` framed the card in its own surface (`p_3`) and kept its 4px foot, so the card sat 10px short of the ring's right edge and over its top. Now `.appearance(false)` and `.bottom(USAGE_POP_OFFSET)`.
- [x] `focus_moving_to_another_pane…` pressed `cmd-]`, which is Forward now (lane N); Next Pane is `cmd-}`.
- [x] `the_model_gives_way…`: with the env words over the box (lane T) the model starts to cut only near 680px, so the test breaks at any cut, not 40px.

## Polish D (density) follow-ups
- [ ] Three tooltips in pane.rs / panel/preview.rs use the kit default — switch to `ferrite_ui::menu::tooltip`.
- [ ] Slash menu: check no column still uses the code font (lane D saw mono names).
- [ ] Palette, dialogs/confirms, toast: need Claude captures (⌘K search, a confirm) to size.
- [ ] sbs pairs must compare at 1pt = 1px (Ferrite renders 1440×900 vs Claude 1920×1050) — scaling both to 900 high makes ours look ~17% bigger.

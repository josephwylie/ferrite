# Ledger — every promise and decision, tracked

This is the single list of what has been promised or asked for. A promise
counts as made only once it is written here. It was rebuilt on 2026-11-04 from
a full audit of the session log against the code: the three parts are in the
session scratchpad as `ledger1-3.md`. Each row is ticked only with evidence,
meaning a commit and a test or render.

## Owner decisions (standing)

These are not built:

- Inbox
- Remote Control
- Mic and voice input
- Plan mode
- The ChatGPT side
- Read aloud
- A pending bubble in the transcript; the queue card above the composer is enough (2026-11-04)

Settings: the owner fixes it themselves.

These are kept as they are:

- The red style on every Remove button.
- White working orbs.
- The Codex-style hero.
- The `/` project name.

Decided 2026-11-04:

- Keep Claude's measured type and colour values.
- The in-app terminal is in, like Claude's.
- Ferrite's own selection colour stays.

## Done tonight

- [x] Reply to a selection: a centred pill, with Quote rows on the composer and on the sent message (`c35a2791`).
- [x] Sidebar text at 13px, and a darker sidebar at `#212121` (`c35a2791`).
- [x] Thinner text: no CoreGraphics glyph dilation (`e64ce45f`).
- [x] Dev builds use `debug = "line-tables-only"`.

- [ ] **Task list, partly working (owner, 2026-11-04 19:46):** a Claude turn made a 4-step plan (`68ed21a6`, `update_plan`), but the agent never advanced it, and after the turn ended the card stayed under the answer with step 1 still spinning (`ferrite-plan-stale.png`). Being fixed per handoff §12 "Transcript replay and completion".
- [x] **Seen by the owner, 2026-11-04:** the terminal takes clicks and typing (`b8a13756`), and Restart server restarts in place (`81e2bf97`).

## Lane M — motion, from Claude's source (`motion-claude-source.md`)

- [x] **Seen by the owner in the installed app, 2026-11-04:** live motion plays. Sent and answer entrances start at first paint, whole answers keep their full fade, and the token count ticks calmly (`102c2e76`).

- [x] All 14 gaps, the ⌘B snap, the row-hover snap, and the preview MP4 (`2735a69f`).
- [x] Composer height eases instead of jumping. Claude's `ChatComposer` snaps (doc row "Composer grow"), so the 200 ms comes from Claude's one height ease, the comment card's envelope, on the ease-out quart (`theme::MOTION_COMPOSER_GROW_MS`, `motion::COMPOSER_GROW`). The box tweens its row count; only the composer re-measures while it runs. Test: `composer::tests::the_box_eases_to_its_new_height`.
- [x] Crossfade on thread and view switch: Claude snaps; matched. `motion-claude-source.md` "Thread switch" row: a warm switch lands at once (rows fade over 300 ms only on a slow load), and "Leave alone" lists the thread switch and the panel among what snaps.
- [x] The working-line shimmer leaves the ~30 fps clock. `Shimmer` reads `motion::frame_loop(.., LoopClock::Css, ..)` and its text leases one display frame while it is lit and on screen (`motion::lease_frame`), as the orbs do; nothing is requested at rest. Test: `components::tests::a_lit_shimmer_leases_display_frames_not_the_pulse_clock`.
- [x] The kit's toasts follow Ferrite's motion. `vendor/gpui-component` (0.6.0, patched in `notification.rs`): in over `MOTION_TOAST_IN_MS` sliding the card's own height, out over 200 ms fading and scaling to 0.85, on the quart, read from `DefaultToastMotion`. Test: `render_performance::a_kit_toast_slides_its_own_height_on_the_toast_tokens`.
- [x] Three tooltips still use the old default style (`panel/preview.rs:371` and others). Every one now draws `menu::tooltip`: the pane's band chip and tool buttons, the attachment and quote × buttons, Back to Main (through the kit patch `Button::tooltip_builder`). `CopyButton::tooltip` was renamed `tooltip_text`, since it already drew the measured one. Test: `menu::tests::no_tooltip_falls_back_to_the_kits_default` scans the crates.

## Lane W — the working line and transcript rows

- [x] The working line uses the tool's description, not the raw command.
- [x] Muted ink for the working line; the orbs stay white.
- [x] The token count is live for the whole turn, thinking included, with an arrow icon and rolling, eased numbers.
- [x] While the model is thinking, the working line says so and shows the thread's effort.
- [x] No duplicate running row above the working line.
- [x] "Running …" rows use the UI font; only code is mono.
- [x] The placeholder leaves a gap before the "↵".
- [x] Errors show once: no `failed ·` stamp over the failure strip.
- [x] Relative message times ("52 minutes ago"), with the exact time on hover.
- [x] Date dividers in long threads.
- [x] PageUp and PageDown scroll the transcript.

## Lane C — the composer and picker

- [x] Image attachments show as thumbnail cards in the composer tray (`6733a514`): the picture on an 88px card, × on hover or keyboard focus, a click opens the lightbox. The card is the sent image's frame too.
- [x] Add an image message to a render scene (`6733a514`): the `composer` scene sends one and has one in the tray.
- [x] **Seen by the owner in the installed app, 2026-11-04:** a skill is blue ink (`SKILL_INK`, no chip) in the composer and in a sent bubble, with a hover card. This is Claude's look (`claude/parity/skill-sent-bubble.png`, `skill-hover.png`) and supersedes the chip below.
- [x] ~~`/skill` tokens are chips~~ (superseded) `/skill` tokens are chips in the composer, as `@` mentions are, sent as plain `/name`. `7a300afe` only staged a picked `/name` and painted it in the link blue: the composer's "chip" was ink on no ground, for `@` mentions too, so the owner saw blue text and no chip. A typed, pasted or restored `/name` was never staged, so it was not even blue. Fixed: the line paints a rounded `MENTION_CHIP` ground behind every chip, and it knows the Session's commands (`Composer::set_commands`, synced from the Thread's commands, or a draft's discovered ones), so any whole `/name` naming one chips, however it got there. Tests: `composer_chips::a_typed_skill_on_a_codex_thread_draws_as_a_chip`, `a_skill_picked_after_prose_draws_as_a_chip`, `a_restored_draft_keeps_its_skill_chip_and_sends_plain_text`, and `composer::tests::a_known_command_chips_unpicked`.
- [x] Slash-picker rows carry a source tag (Skill, Plugin, built-in) (`7a300afe`). Claude's come from its initialize list (`builtin`, `plugin:` names, the `(user)`/`(project)` suffixes, MCP prompts). Codex's skills give theirs from their scope; Codex reports nothing for its built-ins or prompts, so those rows are untagged.
- [x] The + menu matches Claude, with Connectors › and Plugins ›, and a submenu for each plugin (`7a300afe`). Plugins lists the plugins that bring commands, each a submenu of them. A plugin with no commands has nothing to pick, so it is left out, and Codex reports no plugins. Claude's other plugin and connector actions have no data behind them in Ferrite, so they are left out.
- [x] Each queued message can be edited and removed, and sent now (`73efd684`): a row each, with its own actions. Send now sends that message next. It steers into the running turn where the provider steers (Codex) and stops the turn where it cannot (Claude).

## Lane A — actions and menus

- [x] Actions under each answer: Copy, Fork and ⋯, as Claude has (`transcript/answer.rs`). Fork takes the conversation through that answer (before the next prompt, or all of it for the latest) and opens the new thread focused. ⋯ holds Copy message, Reply (Claude's "Attach message as context", as a Quote row) and Fork from here. Pin and Read aloud are left out because nothing backs them. Copy as Markdown is left out because Copy already copies the Markdown. Scene: `answer-hover`.
- [x] The thread ▾ menu gains Copy link and Fork. The ▾ opens the same menu as the sidebar row. There is no `ferrite://` scheme, so Copy link copies `ferrite thread focus <id>`, the CLI command that lands every window on the thread. Fork forks the whole conversation.
- [x] The sidebar row menu gains Claude's items, in Claude's order with rules and letter keys (`cockpit/context_menu.rs`): Open in › (Finder, Terminal; Ferrite knows no editors), Pin P, Mark as unread U, Rename R, Fork F, Toggle fullscreen T (it moved off F), Copy link K, Move to group › (Claude's "Move to section"; projects are checkouts and can't be moved between), and Park thread A as Claude's Archive. Submenus open beside the menu, on hover or on a press. Scenes: `contextmenu`, `row-menu`.
- [x] The approval split button gains "Allow and switch to Auto": the provider's own mode from `Settings::auto_mode_for` (Claude `auto`, Codex `never`), with no list hard-coded in the UI.
- [x] Stop a subagent row from the keyboard. An agent's row is now a Tab stop: ↵ opens the agent and ⌫ stops it (`cockpit::StopAgent`, the same stop op Escape uses). It is listed in the shortcuts sheet.
- [x] Palette commands for rewind and fork: "Rewind to last message" opens the last prompt's Rewind menu, and "Fork thread" forks the whole conversation.
- [x] "Start a side chat" from a selection, beside Reply. It forks the whole thread into a new focused thread, with the passage as a Quote row in its Composer. **Difference:** Claude's side chat is a floating, ephemeral panel that shares the session (`/btw`, ⌘;, tabs, "Clear side chat"). Ferrite has no place for that, so its side chat is an ordinary forked thread.

## Lane P — the panel, preview and sidebar

- [x] A page and preview pane: open a rendered page or URL beside the transcript, with a title menu, expand and close (`fa36929a`). An `.html` file draws live through the visuals engine under `--features cef`; a local URL, or any page without Chromium, shows the can't-show note with Open in browser. The engine is sandboxed and loads no URLs.
- [x] Preview cards in the transcript: a thumbnail, Open and ⋯ (`cdc328f3`). The thumbnail is a globe glyph; no page snapshot yet.
- [x] A lightbox: a dimmed overlay with arrows, "1 of N", zoom and download (`bcef8604`, `cda2c1c5`).
- [x] The right panel is closed by default (`8a9ac2e3`).
- [x] Toasts no longer cover the right panel (`4eb04ac0`, `2cba4d9c`).
- [x] Long sidebar titles fade out instead of ending in "…" (`76e2c416`).
- [x] Remove the "↑N" ahead/behind counts from project headings (`fdc65449`).
- [x] When the panel is hidden, the window title shows the branch (`69525ef2`).
- [x] File cards, with Open in, for files the agent produces (`cdc328f3`). Open in offers Finder and the default app; Ferrite knows no list of editors.

## Lane X — Codex parity and checks

- [x] Edit and rewind on Codex threads. Codex 0.166's app-server has a real rollback, so this is not fork-and-resend. Each prompt goes out with `clientUserMessageId`; a rewind pages `thread/turns/list` to the turn that prompt started and sends `thread/revert {beforeTurnId}`. A fork sends `thread/fork {beforeTurnId}` (experimental), then lets the new thread go (`providers/codex/rewind.rs`). Codex restores no files, so the new `ControlKind::RewindFiles` capability hides "Rewind conversation and code" wherever it isn't reported. A prompt steered into another's turn, or sent before this change, is refused in words. Tests: `codex::rewind::tests` (against `tests/fixtures/codex-rewind-0.166.0.jsonl`, written from the schema), `contract_codex_controls::codex_rewinds_by_the_prompts_client_id_and_never_restores_files`, and `forks::the_rewind_menu_offers_the_code_only_where_the_agent_restores_files`.
- [ ] Check the subagent drill-in against a Claude capture. It needs the Claude app on screen, so it waits for the owner.

## Lane G — GPUI and assets

- [x] Apply upstream #65952: fresh updates for reused cached views. Hand-merged into `vendor/gpui-pre` (`view.rs`, `window.rs`, `app/entity_map.rs`; see `vendor/README.md`). A deferred draw records what it reads and restores it when reused, a cached view adds what its paint reads, and released entities are skipped. Upstream's scope stack is not taken; Ferrite's earlier `detect_accessed_entities` patch already scopes reads. Test: `ferrite-ui` `vendored_gpui::a_reused_cached_view_keeps_what_it_read_subscribed`, upstream's own test.
- [x] Icons at a true 1px stroke. The cause: each SVG baked its `stroke-width` against its viewBox (Lucide's 2 on 24, 1.333 on 16), so the stroke scaled with the drawn size: 1.33px at 16, 1.17 at 14. Now `icons::icon` names the drawn size in the asset key, and `Assets` rewrites every `stroke-width` to `ICON_STROKE` (now 1.0) × viewBox ÷ size. Logomarks are untouched. Test: `icons::tests::line_icons_bake_the_stroke_class_and_logomarks_do_not` checks every line icon at 10–18 px.

## CP7 fixes

Renders: `ferrite-cp8/` (`FERRITE_REF_SIZES=app`; answer, answer-hover, approval, row-menu, running. There is no `sidebar` state; the sidebar pair is the nav, which every one of these scenes shows).

- [x] Project headings fade only when they overflow (regression 1). `nav::cut_ground` passes `fitted` the row's ground only where the measured line runs past its box, for project headings and for Thread, Group, Activity and Notice rows. Evidence: "ferrite", "flavorlab-site", "swarmdeck" and "transcript-app-style" are whole in row-menu and approval. Test: `nav::tests::only_a_cut_line_fades`.
- [x] Long thread titles end in the fade alone (regression 2). The "…" was the sidebar row, not the window title: `rename_target_thread` and `rename_target_group` wrapped the title in `.truncate()`. They now clip with `overflow_hidden().whitespace_nowrap()`. Evidence: "UI test: color tokens and shell comm" fades out with no "…" in answer, answer-hover and running. Test: `nav::tests::a_title_is_clipped_not_ellipsized`.
- [x] Tables keep inline code on one line (regression 3). The vendor scroll table measured each cell as one run in the body face. `Paragraph::line_widths` (`vendor/gpui-base/src/text/node.rs`) now measures each run in its own face: inline code in the code family, plus its wash's pad on each side, and bold in strong. The spare width is shared out in proportion, as Claude's are, and `with_table_columns_fit` is removed. Evidence: answer has `ls /definitely-missing-dir` and `No such file or directory` on one line each, with the Result column starting near the middle, as in claude/answer-done. Test: `rich::file_link_tests::a_table_that_fits_keeps_its_code_on_one_line`.
- [x] Inline code in sent prompts is Claude's red chip. A prompt that quotes code in backticks goes through the prompt's Markdown path, with its code spans left as code (`pane::code_spans`). The band draws them in `PROMPT_CODE_INK` (`RED` `#f3767b`) on `PROMPT_CODE_CHIP` (`#303030`), both sampled from claude/running.png. Evidence: answer, answer-hover and running show `ls /definitely-missing-dir` and `sleep 15 && echo done` as red chips in the bubble. Test: `core_transcript_parity::a_prompts_code_is_drawn_as_code`.
- [x] The approval card shows the tool's own description first (`decision::approval_prose`). That is the Bash `description` input; the agent's reason is the fallback. The command reads in `SHELL_COMMAND_INK` `#99ea71`, sampled from claude/approval.png. Evidence: approval shows "Print a spaced line, then read the fixture" over a green command. The fixture now carries a description. Test: `decision::tests::an_approval_says_the_tools_description_first`.
- [x] A disabled Fork in the row menu stays one line. `MenuItem::disabled_because` puts the reason in the row's tooltip, never in a second line. Evidence: row-menu's Fork is a dim single row. Test: `menu::tests::a_disabled_rows_reason_is_its_tooltip_not_a_second_line`.
- Usage popover "See all →" is left out because nothing backs it. The extra row-menu items and the model menu are unchanged.

## Waiting on the owner (asked in the morning summary)

- The type scale with base 13, and the owner's hex palette from early on. Since then, Claude's measured values have been applied instead; it needs a confirm.
- Bring back the PR and Details tabs?
- Save on Environment?
- A Solo view tab?
- Grey side-panel row icons?
- "Edit" or "Update" wording for each provider?
- The Projects removal: PR #190 still needs to land. Then cargo-sweep, rust-trim and mem-guard come back on, and merged worktrees are removed.
- Push this branch and open its PR.
- Threaded comments on pages; this follows the page pane.
- Provider options, CLI session import and home suggestions; these are deferred.

## Next build (owner-approved 2026-11-04, in order)

Each item is ticked only after it's seen working in the real installed app.

0. **Image reads.** A `Read` of a picture draws the picture under its row, as Claude does: a thumbnail at the sent-image box (480 × 320), opening in the lightbox, with the row open or shut; opened, the row shows the path. Seen in Claude Desktop 2026-11-04 on a throwaway thread (not saved as a capture; the real record shape is in `wire.rs`'s test). BUILT, not ticked: tests pass (core `transcript` and `claude/wire.rs`, cockpit `image_viewer.rs`); not yet seen in the installed app. The result is `ToolResult::Image` (schema 17, no bytes kept); the picture is drawn from the file the call names, so a relative path or a file since deleted draws nothing. Codex `imageView` maps to the same result, from its item shape, with no Codex capture. A Read inside a shut group stays hidden with the group.
1. **Reply.** A floating card over the selection that becomes a `Reply` card in the composer tray (`claude-sidechat-menus.md`). Owner 2026-11-04: the card is good, but the tray card must use Ferrite's attachment tile; being fixed. BUILT, not ticked: tests pass (`transcript_pointer.rs`); not yet seen in the installed app. The selection wash is drawn by the card (a press clears the real selection), so Esc does not bring the real selection back.
2. **Side chat.** A right-docked, tabbed pane (`claude-sidechat-menus.md`). Owner 2026-11-04: it is refused while a turn runs, but Claude allows that. Lane B is fixing it. BUILT, not ticked: tests pass (`tests/side_chat.rs`); not yet seen in the installed app. Opening needs the agent to be able to fork (no turn running); a fork that was never closed (quit mid-chat) stays a normal thread.
3. **Subagent drill-in.** In the same right pane. BUILT, not ticked: tests pass (`tests/subagents.rs`); not yet seen in the installed app or compared side by side. Opens from the row only; a notice's jump and the history arrows still give the Pane the agent's transcript (`select_subject`). `Model X` shows only when the agent's transcript names its model.
4. **Right-click menus.** Each matches its target: answer, prompt, link, file. BUILT, not ticked: answer, prompt and link (`tests/forks.rs`, `tests/link_clicks.rs`); not yet seen in the installed app. The file menu waits on the Files tree (item 5). Pin as chapter only marks the answer in its transcript (no chapter list, not saved). Open in Files opens the existing reader until Files exists.
5. **Files, Changes, session rail and terminal.** Title-bar tools as Claude's (`claude-files-changes.md`), replacing Show in Finder. BUILT, not ticked: tests pass (`tests/tools.rs`, `files.rs`, `changes.rs`, `terminal.rs`, the `ferrite-terminal` crate); not yet seen in the installed app. What stands:
    - **Title bar:** split, session rail, Terminal, Changes (⇧⌘D), Files (⇧⌘F) and `⋯`, the open one pressed. Browser skipped. `⋯` holds Reveal in Finder and Copy path (Claude's own rows were never captured). ⇧⌘D moved from Review branch, which keeps its palette command.
    - **Rail:** Claude's rail is a docked column, not a floating card (`terminal-tabs.png`), so Ferrite's existing panel column is the rail and its door toggles it. Connectors stays as it was; Remote Control and Save are not built. Create PR and the diffstat were already there.
    - **Dock:** Files and Changes share a column (Files above Changes), the Terminal has a column of its own, beside the rail.
    - **Files:** tree with filter, git colours (file level), folder disclosure and the right-click menu; editor with breadcrumb, numbered lines, current-line wash, soft wrap, syntax colour, editing and Auto save; preview (italic) tabs; every file path opened in a Pane's preview opens here. Prose (.md, .txt) is rendered by the reader and is **read-only**. Not built: the filter's `?` content search, File tree position, Tab key indents, Indent guides, Show whitespace, Scroll past end; settings are not saved across launches; a file the agent changes under an open editor is not re-read; ↑/↓ step logical lines (not wrapped rows).
    - **Changes:** scope dropdown (All changes, Uncommitted, Compare against a branch), per-file head with +N, reviewed circle, open and fold, the panel's hunks (one line of context, no line numbers) and the five `⋯` rows. Not built: search, the file-list view, Side by side, Group files by folder, Highlight changed words; reviewed marks are not saved.
    - **Terminal:** `alacritty_terminal` PTY running the login shell in the thread's folder; grid with the mono face, block cursor, scrollback, selection, copy/paste, resize; tabs with `+`/x, `⋯` (Cursor style, Blink cursor, Copy on select, Scrollback), right-click menu and the `Attach as context` pill, which makes the Reply tile. No links, search or splits. Closing the last tab or the column ends its shells.
6. **Turn states:**
   - Deny ends the turn, with "(1 stopped)": BUILT, not ticked. `ToolState::Stopped` (a denied call, or a call a stopped turn cut off) never reads red; the group says `(1 stopped)`, the row `stopped`. The Claude deny response now sends `interrupt: true`, the SDK's own field: checked against a live capture (`claude-permission-deny-interrupt-2.1.318`, test `a_denial_with_interrupt_ends_the_turn_as_an_interrupt`): the turn closes `aborted_tools`, read as an interrupt not an error, and the same session answers the next prompt.
   - Stop says "You stopped after Ns": BUILT, not ticked, once the turn's time is known; a turn that only ended (a revived log) still says `interrupted`.
   - the todo card stays, with a blocked state: BUILT, not ticked. The last plan stays under the thread's rows when idle (until the next plan replaces it, or a thread has none), and the step a stop landed on shows a bar in a ring. It is not kept per turn: a later turn's plan replaces it.
   - a live line that expands: BUILT, not ticked. Claude Desktop has no "Working for Ns" header (captured 2026-11-04: glyph, the step, the clock and a `▸`; that header is Codex's, XTR1), so the line gets Claude's `▸` after its facts, which opens the call it carries (its row shows in the line's place; `the_working_lines_chevron_opens_the_call_it_carries`). Claude's own expanded state was not captured. Codex's collapsing turn header is not built.
   - the working line stays during an approval: BUILT, not ticked, in a Solo Pane only (a Group Pane's Decision island keeps all its small body, and showing the line there broke its form: `group_question_uses_measured_space_and_restores_inline_form`); the line shows over the waiting Decision and its clock keeps counting. Tests: core `transcript` and `claude/wire.rs`, pane `text.rs` and `working.rs`, cockpit `decision_cards.rs`.
7. **Background work reports back to its row** (HD64). BUILT, not ticked: tests pass (`tests/background_handback.rs`, on the real `claude-background-2.1.318` capture). The launch row stays running past its turn, with `View live output ▸` opening the right-docked Background tasks pane, and settles with a muted `Background command completed · …` line where the work finished. The rail has a `Background tasks` section. There is no pill. Not built:
    - the pane's trash and expand, since the icon set has no trash;
    - an agent's nested child tasks;
    - a Codex background terminal still settles at its turn's end;
    - a stopped task reads `failed`.
8. **Edit and Write stream** while their input arrives (HD39). BUILT, not ticked (`tests/edit_drafts.rs`): the first draft opens a running row, the streaming input grows its would-be change, and the settled call is the same row.
9. **End-of-turn wrap-up box** (HD58). Restyled to Claude's captured box (`claude/parity/wrapup-*.png`, section "Wrap-up" in `claude-sidechat-menus.md`): hairline ring, no ground, a primary and a secondary button; used or left behind, it is spent (no buttons, the sent step shown) and a repeat press sends nothing; a recap alone draws no box; `blocked` is Ferrite's own guess (not captured); the primary's `⌘↵` keycaps are not drawn. Tests: `provider_controls.rs`. BUILT for Claude, not ticked (`tests/wrap_up.rs`): Ferrite's in-band server always offers `wrap_up`. Its box holds the recap, the question and up to two next steps, and each step sends its own words. Codex is not offered it. There is no Claude capture of the box, so its look uses the existing card and button styles.
10. **Changes refreshes** after background work and pushes (HD27). BUILT, not ticked: the PR is read again at once after a `git commit`, `git push` or `gh pr …` command settles, or after background work is handed back (`tests/pr.rs`). Changes still counts the agent's own edits, not a git diff.
11. **Mid-turn messages are confirmed** (HD5), and rewinds are validated (HD79). BUILT, not ticked:
    - `user_message_uuids` on a request or a result starts a folded prompt;
    - a resumed rewind passes `--resume-drops-turn`;
    - a rewind cuts at the first prompt under its id, so an old log's echo no longer keeps the real prompt;
    - Rewind gives the prompt's words back to the Composer.
12. **Claude steering is surfaced:** "Steering", and Send now folds the message in instead of interrupting. A real fold with tools is captured first. BUILT, not ticked (`claude-steer-2.1.318`): a prompt queued while a turn runs is `Folding`, which the strip shows as `Steering` while keeping edit and remove. Send now hands it over again without interrupting, and a folded prompt keeps the turn's clock.
13. **Question option previews** (HD68), and **scroll position per thread** (HD83). Previews BUILT, not ticked (`decision_cards.rs`): the option in hand (the cursor's, else the last picked) shows its preview as plain text in the command well, in the transcript and the overlay; no HTML previews. Scroll per thread: already true in a session, so only a test was added (`interaction_regressions.rs`); it is not saved across a restart.
14. **Review comment threads** (HD71). NOT BUILT: not reached.
15. **The rest of `STOCKTAKE-STATUS.md`:** MISSING and PARTIAL rows, most noticed first. Done, BUILT not ticked:
    - the transcript scrollbar (TR18): the thin overlay bar, hover mode, over the list (`interaction_regressions.rs`; only that it is mounted is tested, not its painting or a drag);
    - a send scrolls its prompt to the top with the room below (XD5), now reached by a real send (`scroll_to_new_prompt`; tested), a first prompt on a new thread still scrolls to the foot;
    - the running placeholder says `↵ to steer this turn`: both keys bind `Submit`, so ⌘↵ was not made to queue and the false hint is gone (the ⌘↵ binding and its comment in `keymap.rs` are left for the other lane).
    Drafts that persist: BUILT, not ticked (`ferrite_core::drafts`, `tests/drafts.rs`): per thread, kept across a Pane closing and a restart; the line's text only (not its attachments beyond what the prompt text carries).
    Not done: the permission chip and usage ring on a new thread, code block language headers (the fence's overlay has no header strip; it is the vendor text view's layout), Read rows opening in the panel (the row's whole-width press overlay covers its path, so a click toggles the row; ⌘-click still opens it), View live output for foreground commands.
16. **Unread, bell and Needs-approval indicators** (SH6, SH2, AP8). BUILT, not ticked (`nav.rs`, `tests/nav_list.rs`): an unseen finish's row dot is blue (`theme::UNREAD`, Codex's, from the stocktake) and no longer white; the Activity bell wears a dot on its corner while a Thread needs the operator (`ATTENTION`) or a finish is unseen (`UNREAD`), the former winning, counting every open Thread whatever is folded (`NavState::alert`). The row's needs-you dot and the Activity row's `Needs approval` word were already built. Not built: a numeric badge, the Home rail badge (Ferrite has no Home rail), and the folded nav's own bell. The blue departs from ADR 0033's "no blue chrome"; the stocktake asked for it, so this is the owner's call to keep.
17. **The thread loading state** (XQ7, MO5). BUILT, not ticked (`pane.rs`, `model_effort_chip`; test `the_model_chip_shimmers_while_the_session_starts`): while a Thread's Session starts the footer's model name shimmers (Codex's skeleton chip) until the Session says which model serves. Ferrite reads a Thread's history from the store at once, so there is no history load to show a centred spinner over; the Session start already has its own mark and `Starting` line at the transcript's foot. A centred spinner was not built, and only the model chip is a skeleton (the permission word and ring are absent on a new Thread, item 18).
18. **Code block headers** (XTR5). BUILT, not ticked (`rich.rs` `CodeActions`, `theme::CODE_HEADER_H`; test `a_fences_header_holds_its_language_and_its_actions`): a fence has a header row in its top padding, the language at the left at rest and `Copy` (and html `Preview`) at the right, blending in under the pointer as before, both inset as the code is. No vendor change was needed: the overlay stays over the block and the block's style makes the room, so the block is `CODE_HEADER_H` taller. It reverses DETAIL-AUDIT F1 ("no language at rest"), as asked. Not built: the wrap toggle. There is no Claude capture of a fence, so the header's type size and gap are a judgement, not a measurement.
19. **The bell matches the ChatGPT app's** (SH2, AP8). BUILT, not ticked (`cockpit.rs` `activity`, `nav::Activity`, `clock::recent_day`, `Store::last_activity`; spec in `claude-files-changes.md`, "The notification bell"):
    - the "Today" bug: every open Thread's log gets marks appended at startup, which moved its modification time to now, so untouched Threads listed under Today. The day is now the newest prompt or completion time on the log's tail (`Store::last_activity`; the same time drives the nav's ages), tested on a log whose mtime is newer than its last turn;
    - `Priority` ranks a request (oldest first), a failure, a finish unseen, then work; every other Thread is under `Today`, `Yesterday` or its weekday, six days back; the empty text is `Nothing needs attention`;
    - the `Priority` heading's `⋯` (always shown; stays on the first heading when `Priority` is hidden): `Show` `Priority section` and `Pinned` (a section after Priority), `Mark all as read` (`mark_notices_read`; greyed with no unseen finish), `Archive chats` (parks every Thread the day sections list; reversible from Parked); a row shows pin and park under the pointer, and the age as its tooltip. BUILT, not ticked. Not built: `Show Scheduled` (Ferrite has no scheduled tasks), and the choices are not kept across a restart.
    -  The ChatGPT app showed no unread or waiting thread, so those states are from its code, not seen; no PNGs are kept (a shell capture caught the owner's Terminal and was deleted).
20. **The terminal takes the keyboard.** BUILT, not ticked (`tests/terminal.rs`): the focus pump handed the keyboard back to the Composer every frame, so no key reached the shell; it now leaves a focused shell alone, and the door's focus waits for the grid to be drawn. Tests send real keystrokes through the production keymap: letters, Backspace, Enter, arrows, ⌃C and ⌘V.
21. **The title tools stay at the window's right edge.** BUILT, not ticked (`view_head.rs` `head_drop`, `tests/tools.rs`): the rightmost column's head row (rail tabs, the dock's heads, the side chat's and an agent's) now stands under them. The rail's tab strip, and each of those heads, sit 44px lower while a column is open.
22. **Permission word and usage ring on a new thread** (NS5). BUILT, not ticked, NOT RUN (`cockpit.rs` `draft_mode_chip`; test `a_draft_says_its_permission_mode`): a draft shows the mode a new Thread starts in (read-only: there is no Session to change it) and the account's usage ring. The mode chip is tested; the ring (which needs account limits on a draft) is not.
23. **The flaky terminal tests.** The cause was not a fork deadlock: the PTY's drop waited on a shell that cannot exit while its output is unread, and alacritty's macOS default wraps it in setuid-root `login`, which cannot be signalled. The shell runs directly now, and a dropped Terminal never waits (`session.rs`). `cargo test -p ferrite-cockpit` went from over 40 minutes to about 3.5, three runs in a row.
24. **Not built:** Read rows opening their file in Files (the call line's press overlay covers the path; the plan is to split the call into its verb and its path run and paint the path after the overlay, which changes how a call line copies), and review comment threads (HD71).

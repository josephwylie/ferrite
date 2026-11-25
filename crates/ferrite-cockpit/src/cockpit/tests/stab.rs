//! Stabilization tests: behaviour and state gaps closed after the work
//! packages merged.
#[allow(unused_imports)]
use super::*;

/// Reduced motion holds the working line's mark still; otherwise it moves.
#[gpui::test]
fn the_working_mark_rests_under_reduced_motion(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("working-mark-motion", 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "Inspect progress".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    super::hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::ReasoningSummaryDelta {
            text: "**Checking marks**".into(),
            summary_index: 0,
        })
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("progress-mark-live").is_some());
    assert!(cx.debug_bounds("progress-mark-still").is_none());

    cx.update(|_, cx| cx.set_reduce_motion(true));
    tick(cx);
    assert!(cx.debug_bounds("progress-mark-still").is_some());
    assert!(cx.debug_bounds("progress-mark-live").is_none());
}

/// The command key is drawn, never typed: the empty board's keycaps and the
/// context menu's shortcuts both place the one `⌘` glyph on macOS.
#[gpui::test]
fn keycaps_and_menu_shortcuts_draw_the_command_glyph(cx: &mut TestAppContext) {
    if ferrite_ui::keymap::PLATFORM != ferrite_ui::keymap::Platform::Mac {
        return;
    }
    let (core, _fake) = cockpit("command-glyph", 1);
    let thread = core.threads()[0];
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-w", CloseThread, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    // At rest no key is on screen: `?` is the way in (ADR 0014).
    assert!(cx.debug_bounds("command-key").is_none());

    view.update(cx, |view, cx| {
        view.open_context_menu(
            MenuTarget::Pane(thread),
            gpui::point(px(400.), px(300.)),
            cx,
        )
    });
    tick(cx);
    assert!(
        cx.debug_bounds("command-key").is_some(),
        "the Pane menu's shortcuts draw the glyph"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);

    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    assert!(cx.debug_bounds("empty-board").is_some());
    assert!(
        cx.debug_bounds("command-key").is_some(),
        "the empty board's keycaps draw the glyph"
    );
}

/// A long popover list scrolls with the keyboard: the draft's Project
/// chip over two dozen Projects keeps every row the arrows land on inside
/// its capped list, going down past the fold and back up.
#[gpui::test]
fn the_popover_keeps_its_keyboard_cursor_in_view(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("popover-cursor-in-view", 1);
    let base = scratch("popover-cursor-in-view-projects");
    for n in 0..24 {
        let dir = base.join(format!("project-{n:02}"));
        std::fs::create_dir_all(&dir).unwrap();
        core.register_project(core.first_machine(), &dir).now().unwrap();
    }
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    view.update(cx, |view, cx| {
        view.open_draft(DraftTarget::Main, cx);
        view.open_band_menu(pane::BandChip::Project, false, cx);
    });
    tick(cx);
    let rows = view.read_with(cx, |view, _| view.band_menu().unwrap().rows.len());
    assert!(
        rows as f32 * ferrite_ui::theme::FLOAT_ROW_H > ferrite_ui::theme::MENU_MAX_H,
        "the premise: {rows} rows overflow the list"
    );
    let visible = |cx: &mut gpui::VisualTestContext| {
        let selected = view.read_with(cx, |view, _| view.band_menu().unwrap().selected);
        let list = cx.debug_bounds("composer-menu-rows").expect("the list");
        let selector: &'static str =
            Box::leak(format!("composer-menu-row-{selected}").into_boxed_str());
        let row = cx.debug_bounds(selector).expect("the cursor row");
        assert!(
            row.top() >= list.top() - px(0.5) && row.bottom() <= list.bottom() + px(0.5),
            "row {selected} {row:?} is outside the list {list:?}"
        );
        selected
    };
    let first = visible(cx);
    for _ in 0..rows {
        view.update(cx, |view, cx| view.step_popover(1, cx));
        tick(cx);
        visible(cx);
    }
    assert!(visible(cx) + 2 >= rows, "the cursor reached the end");
    for _ in 0..rows {
        view.update(cx, |view, cx| view.step_popover(-1, cx));
        tick(cx);
        visible(cx);
    }
    assert!(visible(cx) <= first.max(1));
    let _ = std::fs::remove_dir_all(&base);
}

/// The watchdog sweeps on the executor's clock, not the wall clock: a slow
/// or loaded machine that takes longer than `SWEEP_INTERVAL` of real time
/// over a test must not drop a sweep (and its repaint and git refresh) into
/// the middle of it. Only the executor's time moving past the interval does.
#[gpui::test]
fn the_watchdog_sweeps_on_the_executor_clock(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sweep-clock", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let before = view.read_with(cx, |view, _| view.swept);
    std::thread::sleep(SWEEP_INTERVAL + Duration::from_millis(50));
    view.update(cx, |view, cx| view.pump(cx));
    assert_eq!(
        view.read_with(cx, |view, _| view.swept),
        before,
        "real time alone never sweeps"
    );
    cx.executor().advance_clock(SWEEP_INTERVAL);
    view.update(cx, |view, cx| view.pump(cx));
    assert!(
        view.read_with(cx, |view, _| view.swept) > before,
        "the executor's time does"
    );
}

/// The bottom-left toast card is gone (C8): with the nav open, finishes off
/// the board toast nothing — the list says it — so the Parked shelf's
/// header never moves and there is no `+N` bubble. The Inbox still lists
/// them.
#[gpui::test]
fn the_parked_fold_holds_still_with_no_toast_card(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("toast-parked", 5);
    let threads = core.threads();
    core.park(threads[4]).now().unwrap();
    for (n, thread) in threads[..4].iter().enumerate() {
        let _ = core.send(*thread, format!("task {n}"));
    }
    core.pump();
    core.focus_thread(threads[0]);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let resting = cx
        .debug_bounds("nav-shelf-parked")
        .expect("the Parked shelf's header");

    for stream in &fake.streams.borrow()[1..4] {
        stream
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
    }
    tick(cx);
    // The kit's entrance slide runs on the wall clock and its stack springs
    // on the executor's: let both settle before reading where toasts are.
    std::thread::sleep(Duration::from_millis(600));
    for _ in 0..8 {
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        use gpui::component::WindowExt as _;
        assert_eq!(
            window.notifications(cx).len(),
            0,
            "the nav is open: the list says it, nothing toasts"
        );
    });
    assert_eq!(
        view.read_with(cx, |view, cx| view.cockpit.read(cx).notifications(view.cockpit.read(cx).first_machine()).unread()),
        3,
        "the Inbox lists every finish"
    );
    for thread in &threads[1..4] {
        let toast: &'static str = format!("toast-{}", thread.thread().get()).leak();
        assert!(cx.debug_bounds(toast).is_none(), "no toast for {thread:?}");
    }
    assert!(cx.debug_bounds("toast-more").is_none(), "no `+N` bubble");
    assert_eq!(
        cx.debug_bounds("nav-shelf-parked"),
        Some(resting),
        "the row never moves"
    );
}

/// A Solo Pane's agent strip stands at every width, and with room it fits
/// every tab without folding them into `+N`.
#[gpui::test]
fn the_solo_agent_strip_fits_its_tabs_with_room(cx: &mut TestAppContext) {
    use ferrite_core::activity::{
        ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject, TranscriptCoverage,
    };
    let (mut core, fake) = cockpit("head-title-floor", 1);
    let thread = core.threads()[0];
    core.rename_thread(thread, "Audit every surface of the overhaul").now()
        .unwrap();
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    for name in [
        // Short names, all working: every live tab fits a wide strip.
        "nav", "cmp", "set", "tok",
    ] {
        let key = AgentKey::new(Provider::Claude, "ui-fixture", name);
        let mut info = AgentInfo::new(key.clone());
        info.name = Some(name.into());
        info.parent = Some(Subject::Main);
        info.coverage = TranscriptCoverage::Live;
        for event in [
            ActivityEvent::Discovered(info),
            ActivityEvent::Status {
                key,
                state: AgentStatus::Working,
            },
        ] {
            fake.streams.borrow()[0]
                .send(SessionEvent::Activity(event))
                .unwrap();
        }
    }
    // Solo has no Pane head (the view head names it), and Main has no
    // strip for its agents: they are its transcript's rows.
    for width in [900., 1800., 2400.] {
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        tick(cx);
        tick(cx);
        assert!(cx.debug_bounds("pane-head-1").is_none(), "Solo has no head");
        assert!(cx.debug_bounds("pane-subagent-1").is_none(), "no strip");
    }
}

/// A draft in a narrow Pane keeps its controls inside the Composer: the
/// Project and branch chips on the strip over the box (Codex's) truncate
/// without leaving it, and the model chip (its effort beside the model)
/// stays in the block.
#[gpui::test]
fn a_narrow_draft_keeps_its_controls_inside_the_composer(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("narrow-draft-controls", 1);
    let dir = scratch("narrow-draft-controls-a-rather-long-project-directory-name");
    std::fs::create_dir_all(&dir).unwrap();
    core.register_project(core.first_machine(), &dir).now().unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(720.), px(900.)));
    view.update(cx, |view, cx| view.open_draft(DraftTarget::Main, cx));
    tick(cx);
    let block = cx
        .debug_bounds("composer-block")
        .expect("the draft Composer");
    let model = cx.debug_bounds("draft-model-picker").expect("model");
    // SPEC §5.5: the chip rides the footer under the box, within its
    // edges.
    assert!(
        model.right() <= block.right() + px(0.5),
        "the model chip {model:?} stays inside the box {block:?}"
    );
    let strip = cx.debug_bounds("composer-env").expect("the env strip");
    assert!(
        (strip.bottom() - block.top()).abs() <= px(1.5)
            && strip.left() >= block.left()
            && strip.right() <= block.right(),
        "the strip stands on the box, inside its sides: {strip:?} / {block:?}"
    );
    for chip in ["band-chip-0", "band-chip-1"] {
        let chip = cx.debug_bounds(chip).expect("a setup chip");
        assert!(
            strip.contains(&chip.center()) && chip.right() <= strip.right(),
            "the setup chips ride the strip: {chip:?} / {strip:?}"
        );
    }
    for chip in ["band-chip-0", "band-chip-1"] {
        let chip = cx.debug_bounds(chip).expect("a setup chip");
        assert!(
            chip.size.width >= px(48.),
            "{chip:?} keeps a readable label"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A question's own answer is not a second field (rule 2.8.6): one bare
/// line, a list line's text height, its text starting on the option
/// labels' column — the glyph column holds its digit as it holds theirs.
#[gpui::test]
fn the_own_answer_is_one_bare_line_on_the_label_column(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("own-answer-field", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(900.)));
    fake.streams.borrow()[0].send(question("own")).unwrap();
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let serial = view.read_with(cx, |view, cx| {
        view.cockpit.read(cx).thread(thread).unwrap().pending_decisions()[0]
            .handle
            .serial
    });
    let field = cx
        .debug_bounds(Box::leak(
            format!("request-other-{}-{serial}-0", thread.thread().get()).into_boxed_str(),
        ))
        .expect("the own-answer field");
    let choice = cx.debug_bounds("question-choice-0-0").expect("an option");
    assert_eq!(field.size.height, px(ferrite_ui::theme::QUESTION_OTHER_H));
    // An option reads `1  label` (SPEC §5.9): its padding, the key's
    // one mono cell, the gap, then the label.
    let labels = choice.left()
        + px(ferrite_ui::theme::DECISION_OPTION_PAD_X
            + ferrite_ui::theme::FS_META * ferrite_ui::theme::CODE_ADVANCE
            + ferrite_ui::theme::DECISION_OPTION_GAP);
    assert!(
        (field.left() - labels).abs() <= px(0.5),
        "the line's text starts on the labels' column: {:?} / {labels:?}",
        field.left()
    );
}

/// A draft's body holds the start hero (Codex's `What should we build in
/// <project>?`, its project word the Project switcher), and neither it nor
/// an empty Thread centres a second copy of the guidance: the Composer's
/// placeholder says how to start, once (rule 2.11.4).
#[gpui::test]
fn a_draft_body_says_how_to_start(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("draft-empty", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    assert!(
        cx.debug_bounds("transcript-empty").is_none(),
        "an empty Thread has no centred block"
    );
    view.update(cx, |view, cx| view.open_draft(DraftTarget::Main, cx));
    tick(cx);
    let empty = cx
        .debug_bounds("draft-empty")
        .expect("the draft's body spacer");
    let block = cx.debug_bounds("composer-block").unwrap();
    assert!(empty.bottom() <= block.top(), "{empty:?} / {block:?}");
    let hero = cx.debug_bounds("start-hero").expect("the start hero");
    assert!(empty.contains(&hero.center()), "{hero:?} in {empty:?}");
    let word = cx
        .debug_bounds("start-hero-project")
        .expect("the hero's project word");
    cx.simulate_mouse_down(word.center(), MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(word.center(), MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.band_menu()
                .is_some_and(|open| open.kind.chip == pane::BandChip::Project),
            "the project word switches the Project"
        );
    });
    let rows = cx
        .debug_bounds("composer-menu-rows")
        .expect("the Project menu is drawn");
    assert!(
        rows.top() >= word.bottom(),
        "it drops under the word: {rows:?} / {word:?}"
    );
    view.update(cx, |view, cx| {
        view.close_popup(cx);
    });
    tick(cx);
    assert!(
        cx.debug_bounds("transcript-empty").is_none(),
        "no centred empty state anywhere"
    );
    let ghost = cx.debug_bounds("prompt-placeholder").expect("the ghost");
    assert!(ghost.top() >= block.top(), "the guidance is the Composer's");
    assert!(
        cx.debug_bounds("prompt-placeholder-hint").is_none(),
        "the draft's ghost names no keys (ADR 0014)"
    );
}

/// The hero's project word is a medium-weight switcher, and under 448 of
/// window height the lead words give way and leave it standing alone.
#[gpui::test]
fn the_hero_drops_its_lead_words_in_a_short_window(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("hero-short", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    view.update(cx, |view, cx| view.open_draft(DraftTarget::Main, cx));
    tick(cx);
    let tall = cx.debug_bounds("start-hero").expect("the start hero");
    let word = cx
        .debug_bounds("start-hero-project")
        .expect("the project word");
    cx.simulate_resize(gpui::size(px(1200.), px(ferrite_ui::theme::HERO_SHORT_WINDOW - 8.)));
    tick(cx);
    let short = cx.debug_bounds("start-hero").expect("the hero stays");
    assert!(
        short.size.width < tall.size.width,
        "the lead words are gone: {tall:?} -> {short:?}"
    );
    assert!(
        cx.debug_bounds("start-hero-project").is_some(),
        "the project word stays ({word:?})"
    );
}

/// On the empty board the titlebar says nothing either (ADR 0014): no
/// name, no build badge (F-10: Settings › About keeps that).
#[gpui::test]
fn the_empty_board_titlebar_says_nothing(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("empty-titlebar", 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-w", CloseThread, None)]));
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    assert!(cx.debug_bounds("empty-board").is_some());
    for gone in ["titlebar-location", "titlebar-word-0", "titlebar-dev-badge"] {
        assert!(cx.debug_bounds(gone).is_none(), "{gone}");
    }
    assert!(
        cx.debug_bounds("composer-meta").is_none(),
        "no Pane's status line on the empty board"
    );
}

/// Expanding the nav must not remove the window's top drag region. The
/// board sits below that region so its controls still receive their clicks.
#[gpui::test]
fn the_titlebar_remains_draggable_across_the_window(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("titlebar-band", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let check = |cx: &mut gpui::VisualTestContext| {
        let board = view.read_with(cx, |view, _| view.board.get());
        assert_eq!(
            board.y,
            ferrite_ui::theme::WIN_CHROME_H,
            "the cards start under the head row"
        );
        if ferrite_ui::frame::CUSTOM || cfg!(target_os = "macos") {
            let drag = cx.debug_bounds("titlebar-drag").expect("the drag region");
            assert!(
                drag.contains(&gpui::point(px(600.), px(16.))),
                "the middle of the window's top must drag: {drag:?}"
            );
            assert!(
                drag.contains(&gpui::point(px(120.), px(16.))),
                "the band over the nav drags: {drag:?}"
            );
            if cfg!(target_os = "macos") {
                let moves = cx.window_move_requests();
                cx.simulate_click(gpui::point(px(600.), px(16.)), gpui::Modifiers::none());
                assert_eq!(cx.window_move_requests(), moves + 1);
            }
        }
    };
    check(cx);
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    tick(cx);
    check(cx);
}

/// The checks card grows to its own tally: the counts line is never cut
/// while the card is under its cap.
#[gpui::test]
fn the_checks_card_grows_to_its_tally(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("checks-card-width", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    let thread = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads()[0]);
    let pr = pr_with_checks();
    let tally = pr.tally();
    let text = [
        (tally.failing, "failed"),
        (tally.pending, "running"),
        (tally.passing, "passed"),
        (tally.skipped, "skipped"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, word)| format!("{count} {word}"))
    .collect::<Vec<_>>()
    .join(" · ");
    view.update(cx, |view, cx| {
        view.facts.pr_answered(thread, Some(pr));
        cx.notify();
    });
    tick(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_checks(window, cx)));
    tick(cx);
    let card = cx.debug_bounds("context-checks-card").expect("the card");
    assert!(card.size.width >= px(ferrite_ui::theme::CHECKS_CARD_W));
    assert!(card.size.width <= px(ferrite_ui::theme::CHECKS_CARD_MAX_W));
    let tally = cx.debug_bounds("checks-tally").unwrap();
    let natural = cx.update(|window, _| {
        window
            .text_system()
            .shape_line(
                text.clone().into(),
                px(ferrite_ui::theme::FS_UI),
                &[gpui::TextRun {
                    len: text.len(),
                    font: gpui::font(ferrite_ui::theme::FONT_UI),
                    color: gpui::rgb(ferrite_ui::theme::INK_STRONG).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width
    });
    assert!(
        tally.size.width + px(0.5) >= natural,
        "the tally `{text}` is whole: {tally:?} for {natural:?}"
    );
}

/// The Project card in the window (`ferrite_settings::project_editor` has
/// the card's own tests): it draws the Project as the registry has it,
/// and keeps Remove project shut while a Thread records it; its presses
/// reach the window, which removes a directory and redraws the card,
/// renames the Project on Done, and removes one no Thread records.
#[gpui::test]
fn the_project_card_reads_and_answers_the_window(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("project-card-window", 1);
    let thread = core.threads()[0];
    let busy = core.project_of(thread).expect("the Thread's Project");
    let extra = scratch("project-card-window-extra");
    std::fs::create_dir_all(&extra).unwrap();
    core.add_project_directories(busy, &[extra.clone()]).now().unwrap();
    let idle = repo_in(&scratch("project-card-window-idle"));
    let idle = core.register_project(core.first_machine(), &idle).now().unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    let press = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
        let at = cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector}"));
        cx.simulate_click(at.center(), gpui::Modifiers::none());
        tick(cx);
    };
    let extra = format!("project-directory:{}", ferrite_core::git_read::canonical(&extra).display());

    view.update(cx, |view, cx| view.open_project_editor(busy, cx));
    tick(cx);
    assert!(debug_bounds(cx, extra.clone()).is_some(), "drawn from the registry");
    let card = view.read_with(cx, |view, cx| view.project_card(cx).expect("the card"));
    let heard = ferrite_ui::testing::record(cx, &card, ferrite_settings::project_editor::ProjectEditorEvent::clone);
    press(cx, "remove-project");
    assert_eq!(heard.take(), [], "the window says it is in use, so the card takes no press");
    view.read_with(cx, |view, cx| {
        assert!(view.cockpit.read(cx).project(busy).is_some(), "a Thread records it");
        assert!(view.project_card(cx).is_some(), "the sheet stays up");
    });
    press(cx, "remove-project-directory-1");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.cockpit.read(cx).project(busy).unwrap().additional_roots, Vec::<std::path::PathBuf>::new());
    });
    assert!(debug_bounds(cx, extra).is_none(), "the card is drawn again");
    let name = view.read_with(cx, |view, cx| view.project_card(cx).unwrap().read(cx).name.clone());
    name.update(cx, |name, cx| name.set("Renamed".into(), cx));
    press(cx, "project-confirm");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.cockpit.read(cx).project(busy).unwrap().title, "Renamed");
        assert!(view.project_card(cx).is_none(), "Done closes it");
    });

    view.update(cx, |view, cx| view.open_project_editor(idle, cx));
    tick(cx);
    press(cx, "remove-project");
    view.read_with(cx, |view, cx| {
        assert!(view.cockpit.read(cx).project(idle).is_none(), "no Thread records it");
        assert!(view.project_card(cx).is_none());
    });
}

/// Theme rule 1, sampled where each face is set: the chrome is in the UI
/// face (Geist), and the transcript, code and machine text are in the code
/// face (Geist Mono).
#[gpui::test]
fn the_ui_and_code_faces_follow_what_the_text_is(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{FONT_CODE, FONT_UI};
    use gpui::Styled as _;
    let (core, fake) = cockpit("font-roles", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(1000.)));
    tick(cx);
    cx.simulate_input("Check the fixture");
    cx.simulate_keystrokes("enter");
    tick(cx);
    let stream = fake.streams.borrow()[0].clone();
    for (id, name, input) in [
        (
            "run",
            "Bash",
            serde_json::json!({ "command": "cargo test -p ferrite" }),
        ),
        (
            "read",
            "Read",
            serde_json::json!({ "file_path": "src/lib.rs" }),
        ),
    ] {
        stream
            .send(SessionEvent::ToolStarted {
                id: id.into(),
                name: name.into(),
                input,
            })
            .unwrap();
        stream
            .send(SessionEvent::ToolCompleted {
                id: id.into(),
                output: "ok".into(),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            })
            .unwrap();
    }
    stream
        .send(SessionEvent::TextDelta {
            text: "Use `cargo test` here.\n\n```rust\nlet x = 1;\n```\n\n".into(),
        })
        .unwrap();
    stream
        .send(SessionEvent::ToolStarted {
            id: "edit".into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": "/workspace/x.txt" }),
        })
        .unwrap();
    stream
        .send(SessionEvent::ToolCompleted {
            id: "edit".into(),
            output: "applied".into(),
            is_error: false,
            result: ferrite_core::ToolResult::FileEdit {
                path: "/workspace/x.txt".into(),
                hunks: vec![ferrite_core::Hunk {
                    old_start: 1,
                    old_lines: 2,
                    new_start: 1,
                    new_lines: 2,
                    lines: vec![" alpha".into(), "-bravo".into(), "+delta".into()],
                    section: None,
                }],
            },
        })
        .unwrap();
    stream
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    drop(stream);
    tick(cx);
    tick(cx);
    open_folds(&view, cx);

    let (namespace, blocks, composer) = view.read_with(cx, |view, cx| {
        let pane = &view.panes[0];
        let thread = view.cockpit.read(cx).thread(pane.thread().unwrap()).unwrap();
        let blocks: Vec<_> = thread
            .transcript()
            .blocks()
            .iter()
            .map(|block| (block.id, block.body.clone()))
            .collect();
        (pane.text_namespace(), blocks, pane.composer.entity_id())
    });
    let id_of = |pick: &dyn Fn(&Body) -> bool| {
        blocks
            .iter()
            .find(|(_, body)| pick(body))
            .map(|(id, _)| *id)
            .expect("the fixture's block")
    };
    let face = |cx: &mut gpui::VisualTestContext, prefix: String| {
        cx.update(|_, cx| ferrite_pane::rich::testing::font_family(&prefix, cx))
            .unwrap_or_else(|| panic!("{prefix} was drawn"))
    };
    let prompt = id_of(&|body| matches!(body, Body::Prompt(_)));
    let group = id_of(&|body| matches!(body, Body::Tool(tool) if tool.name == "Bash"));
    let edit = id_of(&|body| matches!(body, Body::Tool(tool) if tool.name == "Edit"));

    // Activity reads in the app's words, as the prompt does: a group's
    // summary, a call's line (its argument an inline code span, below).
    // A quick turn closes on no stamp (`pane::turn_stamp`).
    for (what, prefix) in [
        ("prompt", format!("literal-{namespace}-{prompt:?}-0")),
        ("group summary", format!("literal-{namespace}-{group:?}-0")),
        ("call line", format!("thinking-{namespace}-{edit:?}")),
    ] {
        assert_eq!(face(cx, prefix).as_ref(), FONT_UI, "{what} is UI text");
    }
    // Code: the diff's lines and the Composer's line.
    // A settled edit's diff shows once it is disclosed.
    for disclosure in [pane::DisclosureId::Tool("edit".into())] {
        if let Some(control) = view.read_with(cx, |view, cx| {
            view.panes[0].tool_bounds(disclosure.clone(), cx)
        }) {
            cx.simulate_click(control.center(), gpui::Modifiers::none());
            tick(cx);
        }
    }
    let diff_faces: Vec<_> = (0..10)
        .filter_map(|ordinal| {
            let prefix = format!("literal-{namespace}-{edit:?}-{ordinal}");
            cx.update(|_, cx| {
                let text = ferrite_pane::rich::testing::full_text(&prefix, cx)?;
                ["alpha", "bravo", "delta"]
                    .iter()
                    .any(|line| text.trim() == *line)
                    .then(|| ferrite_pane::rich::testing::font_family(&prefix, cx))
                    .flatten()
            })
        })
        .collect();
    assert!(!diff_faces.is_empty(), "the diff is drawn");
    assert!(
        diff_faces.iter().all(|face| face.as_ref() == FONT_CODE),
        "every diff line is code: {diff_faces:?}"
    );
    assert_eq!(
        cx.update(|_, cx| ferrite_pane::composer::testing::face(composer, cx))
            .as_deref(),
        Some(ferrite_ui::theme::FONT_UI),
        "the Solo Composer's line is chrome (SPEC §3.5)"
    );
    // Code in prose: fenced blocks and inline spans.
    let style = ferrite_pane::rich::style(px(16.));
    assert_eq!(
        style.code_block().text.font_family.as_deref(),
        Some(FONT_CODE)
    );
    assert_eq!(style.inline_code_font().as_deref(), Some(FONT_CODE));

    // The surfaces chrome inherits its face from.
    let ui = |mut div: gpui::Div| div.style().text.font_family.clone();
    for (what, family) in [
        ("nav", ui(ferrite_nav::card(gpui::div()))),
        (
            "menu row",
            ui(ferrite_ui::components::menu_row_content(
                &ferrite_ui::components::MenuItem::new("Rename"),
                false,
                false,
            )),
        ),
        (
            "floating surface",
            ui(ferrite_ui::components::floating_surface()),
        ),
        ("settings sheet", ui(ferrite_ui::sheet::sheet(400., 300.))),
        ("settings label", ui(ferrite_ui::components::text_ui())),
        ("metadata", ui(ferrite_ui::components::text_meta())),
    ] {
        assert_eq!(family.as_deref(), Some(FONT_UI), "{what} is UI text");
    }
    for (what, family) in [
        (
            "keys",
            ui(ferrite_ui::components::key_combo(
                "cmd-F",
                ferrite_ui::theme::INK_STRONG,
            )),
        ),
        (
            "command well",
            ui(ferrite_pane::decision::well(false, gpui::div())),
        ),
    ] {
        assert_eq!(family.as_deref(), Some(FONT_CODE), "{what} is code");
    }
}

/// SPEC §5.5 (Claude's): at rest the Solo Composer is a box of one row —
/// `+`, the line, send — with no key hint; the bar carries no status line.
/// Enter sends and Esc interrupts.
#[gpui::test]
fn the_composer_is_one_row_over_its_toolbar(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("composer-one-row", 1);
    let thread = core.threads()[0];
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    spoken(&fake, 0);
    tick(cx);
    let block = cx.debug_bounds("composer-block").unwrap();
    assert_eq!(
        block.size.height,
        px(2. * ferrite_ui::theme::COMPOSER_ROW_PAD + ferrite_ui::theme::COMPOSER_ATTACH),
        "one row: the buttons' height in the row's padding"
    );
    assert!(cx.debug_bounds("composer-meta").is_none(), "no status line");
    assert!(
        cx.debug_bounds("prompt-placeholder").is_some(),
        "the resting line says what to do"
    );
    assert!(cx.debug_bounds("prompt-placeholder-hint").is_none());
    assert!(cx.debug_bounds("status-keys").is_none(), "no key hints");
    // `?` left the bar (SPEC §5.7): the palette's `keyboard shortcuts`
    // and the Help menu are the keys' doors now.
    assert!(cx.debug_bounds("bottom-bar-help").is_none());
    for verb in ["send", "stop"] {
        let hint: &'static str = Box::leak(
            format!("composer-{verb}-{:?}", PaneIdentity::Thread(thread)).into_boxed_str(),
        );
        assert!(cx.debug_bounds(hint).is_none(), "no `{verb}` hint is drawn");
    }

    cx.simulate_input("go");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(fake.sent.borrow().as_slice(), ["go"], "Enter sends");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 1, "Esc still interrupts");
}

/// The prompt heads its turn: the operator's line is at prose size, the
/// same size as the answer under it, in its bubble. Under the last turn's
/// answer and its actions row, a block sits above the bubble, and the
/// bubble's padding above its text.
#[gpui::test]
fn the_prompt_heads_its_turn_at_prose_size(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("prompt-heads-turn", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    tick(cx);
    for (prompt, answer) in [
        ("first ask", "First answer."),
        ("second ask", "Second answer."),
    ] {
        cx.simulate_input(prompt);
        cx.simulate_keystrokes("enter");
        tick(cx);
        let stream = fake.streams.borrow()[0].clone();
        stream
            .send(SessionEvent::TextDelta {
                text: format!("{answer}\n\n"),
            })
            .unwrap();
        stream
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
        tick(cx);
    }
    // A send scrolls its prompt to the top, so the first answer is above the
    // view and its bounds are as last drawn: bring both turns into view.
    view.update(cx, |view, cx| {
        let transcript = view.panes[0].transcript().unwrap();
        transcript.update(cx, |transcript, cx| {
            transcript.page(ferrite_pane::transcript::Page::Top, cx)
        });
    });
    tick(cx);
    let (namespace, prompts, answers) = view.read_with(cx, |view, cx| {
        let pane = &view.panes[0];
        let thread = view.cockpit.read(cx).thread(pane.thread().unwrap()).unwrap();
        let ids = |kind: fn(&Body) -> bool| -> Vec<_> {
            thread
                .transcript()
                .blocks()
                .iter()
                .filter(|block| kind(&block.body))
                .map(|block| block.id)
                .collect()
        };
        (
            pane.text_namespace(),
            ids(|body| matches!(body, Body::Prompt(_))),
            thread
                .transcript()
                .blocks()
                .iter()
                .filter(|block| block.markdown.is_some())
                .map(|block| block.markdown_run.unwrap_or(block.id))
                .collect::<Vec<_>>(),
        )
    });
    let size = cx.update(|_, cx| {
        ferrite_pane::rich::testing::font_size(
            &format!("literal-{namespace}-{:?}-0", prompts[1]),
            cx,
        )
    });
    assert_eq!(
        size,
        Some(px(ferrite_ui::theme::tx_size(Default::default(), true))),
        "prose size"
    );
    let answer = cx.update(|_, cx| {
        ferrite_pane::rich::testing::bounds(
            &format!("markdown-{namespace}-{:?}", answers[0]),
            0,
            cx,
        )
        .unwrap()
    });
    let mut line = |id| {
        cx.update(|_, cx| {
            ferrite_pane::rich::testing::bounds(&format!("literal-{namespace}-{id:?}-0"), 0, cx)
                .unwrap()
        })
    };
    // Measured, not just declared: the first turn's answer to the second
    // prompt is its meta row and T3's row padding.
    use ferrite_ui::theme::{MESSAGE_ACTION, MESSAGE_ACTIONS_GAP, PROMPT_BUBBLE_PAD_Y, TX_ROW_PAD};
    assert_eq!(
        line(prompts[1]).top() - answer.bottom(),
        px(MESSAGE_ACTIONS_GAP + MESSAGE_ACTION + TX_ROW_PAD + PROMPT_BUBBLE_PAD_Y),
        "turns sit the meta row, the padding and the bubble's padding apart"
    );
}

/// A Decision card's head names its kind and says nothing more while it
/// simply waits: no `waiting` beside the card that is plainly waiting.
#[gpui::test]
fn a_waiting_decision_head_names_only_its_kind(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("decision-head-quiet", 1);
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(900.)));
    fake.streams.borrow()[0].send(question("quiet")).unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("question-island").is_some(),
        "the card is up"
    );
    assert!(
        cx.debug_bounds("decision-status").is_none(),
        "a waiting card carries no status word"
    );
}

/// The status line rides the bottom bar (ADR 0014), so the Composer's top
/// never moves: not when a mode is announced, not under a Decision (rule
/// 2.6.6).
#[gpui::test]
fn the_solo_composer_holds_its_place_through_a_decision(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("solo-status-line", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    spoken(&fake, 0);
    tick(cx);
    let rest = cx.debug_bounds("composer-block").expect("the Composer");
    fake.streams.borrow()[0]
        .send(SessionEvent::PermissionMode {
            mode: "acceptEdits".into(),
        })
        .unwrap();
    tick(cx);
    assert_eq!(cx.debug_bounds("composer-block"), Some(rest));
    fake.streams.borrow()[0]
        .send(decision("solo-status-line"))
        .unwrap();
    tick(cx);
    assert_eq!(
        cx.debug_bounds("composer-block"),
        Some(rest),
        "a Decision never moves the Composer"
    );
}

/// A window with no Server says so once, keeps saying it on screen, and
/// keeps trying; it never runs the agents itself (ADR 0025).
#[gpui::test]
fn a_window_without_a_server_says_so_once(cx: &mut TestAppContext) {
    let dir = scratch("no-server");
    let core = Cockpit::attach(ferrite_client::cockpit::Attach {
            saved_as: ferrite_core::MachineKey(None),
            browser: false,
            driven: false,
        store: dir.clone(),
        server: ferrite_client::cockpit::Endpoint::Local {
            executable: dir.join("no-such-ferrite"),
        },
        client: ferrite_core::protocol::Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
    });
    assert!(matches!(
        core.server(core.first_machine()),
        Some(ferrite_client::cockpit::ServerState::Detached { .. })
    ));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    tick(cx);
    cx.update(|window, cx| {
        use gpui::component::WindowExt;
        assert_eq!(window.notifications(cx).len(), 1, "said once");
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(view.leaving_warning(cx), None, "quitting ends nothing");
    });
}

/// Settings › Server's Restart answers its press: the restart is under
/// way, or why it is not, on screen. The page offers Start at login only
/// in a released build, as the window says. Restart's act once reached
/// for its window by the window's handle from inside the press, while
/// gpui had the window out for the event, so every press did nothing.
#[gpui::test]
fn the_restart_button_answers_its_press(cx: &mut TestAppContext) {
    let dir = scratch("restart-press");
    let core = Cockpit::attach(ferrite_client::cockpit::Attach {
            saved_as: ferrite_core::MachineKey(None),
            browser: false,
            driven: false,
        store: dir.clone(),
        server: ferrite_client::cockpit::Endpoint::Local {
            executable: dir.join("no-such-ferrite"),
        },
        client: ferrite_core::protocol::Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    view.update(cx, |view, cx| {
        view.show_settings(cx).update(cx, |settings, cx| {
            settings.show(settings::PageKey::Server, cx);
        });
        cx.notify();
    });
    tick(cx);
    assert_eq!(
        cx.debug_bounds("settings-start-at-login").is_some(),
        !crate::DEV,
        "Start at login is a released build's: the window says which this is"
    );
    let restart = cx
        .debug_bounds("settings-server-restart")
        .expect("Settings › Server offers Restart");
    // A fresh test window takes the first press into it for itself (any
    // button's, not this one's): the second is the operator's.
    for _ in 0..2 {
        cx.simulate_click(restart.center(), gpui::Modifiers::none());
        tick(cx);
    }
    view.read_with(cx, |view, cx| {
        assert!(
            matches!(
                view.cockpit.read(cx).server_restart(view.cockpit.read(cx).first_machine()),
                Some(ferrite_client::cockpit::ServerRestart::Failed { .. })
            ),
            "a Server that does not answer cannot restart, and the window says so"
        );
    });
}

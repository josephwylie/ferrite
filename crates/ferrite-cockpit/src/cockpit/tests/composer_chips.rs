//! The chips under the Composer and the menus they hang.
#[allow(unused_imports)]
use super::*;

/// The panel's Context section reads the Session's reported usage: the
/// context window's counts and the account windows, following live
/// readings and never inventing a window. It is the panel's one context
/// reading: no foot repeats it. With no account window reported, one line
/// says so.
#[gpui::test]
fn the_context_section_shows_context_and_subscription_windows(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("context-tab", 1);
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    let usage = |total_tokens: u64, context_window: Option<u64>| SessionEvent::TokenUsage {
        total_tokens,
        input_tokens: total_tokens,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        context_window,
    };
    fake.streams.borrow()[0]
        .send(usage(64_000, Some(200_000)))
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("context-usage-current-64000").is_some());
    assert!(
        cx.debug_bounds("context-usage-limits-unknown").is_some(),
        "no account window reported: one line, no bars"
    );
    assert!(cx.debug_bounds("context-usage-five-hour-0").is_none());
    // Resets inside their windows: a reset already past reads nothing.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    fake.streams.borrow()[0]
        .send(SessionEvent::RateLimits {
            five_hour: Some(ferrite_core::RateLimitWindow {
                used_fraction: 0.52,
                resets_at: Some(now + 3_600),
            }),
            weekly: Some(ferrite_core::RateLimitWindow {
                used_fraction: 0.08,
                resets_at: Some(now + 2 * 86_400),
            }),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(usage(124_000, Some(200_000)))
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("context-usage-current-124000").is_some());
    assert!(cx.debug_bounds("context-usage-maximum-200000").is_some());
    assert!(cx.debug_bounds("context-usage-five-hour-52").is_some());
    assert!(cx.debug_bounds("context-usage-weekly-8").is_some());
    assert!(cx
        .debug_bounds("context-usage-five-hour-reset-reported")
        .is_some());
    assert!(cx
        .debug_bounds("context-usage-weekly-reset-reported")
        .is_some());
    assert!(
        cx.debug_bounds("context-usage-limits-unknown").is_none(),
        "both windows reported: no note"
    );
    fake.streams.borrow()[0].send(usage(31_000, None)).unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("context-usage-current-31000").is_some(),
        "the section follows live usage"
    );
    assert!(
        cx.debug_bounds("context-usage-maximum-unknown").is_some(),
        "unknown limit is not invented"
    );
}

/// The model picker rests above its chip in the Composer's toolbar
/// (SPEC §5.5): its foot `MODEL_PICKER_GAP` over the chip, its right
/// edge at the chip's (Claude's), inside the Composer, every row one menu
/// row high.
#[gpui::test]
fn the_model_picker_rests_above_the_bar_inside_the_window(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("model-picker-place", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(900.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.open_provider_picker(thread, cx));
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    let menu = cx.debug_bounds("choice-menu").expect("the picker is open");
    let chip = debug_bounds(cx, format!("model-picker-{}", thread.get()))
        .expect("the model chip rides the toolbar");
    let composer = cx.debug_bounds("composer-block").unwrap();
    assert!(
        (menu.bottom() - (chip.top() - px(ferrite_ui::theme::MODEL_PICKER_GAP))).abs() <= px(1.),
        "FL-13: the picker's foot 5px over its chip: {menu:?} / {chip:?}"
    );
    assert!(
        (menu.size.width - px(ferrite_ui::theme::MODEL_PICKER_W)).abs() <= px(1.),
        "Claude's compact model menu: {menu:?}"
    );
    assert!(menu.right() <= composer.right() + px(1.5));
    assert!(
        (menu.right() - chip.right()).abs() <= px(1.5),
        "anchored on its chip, not the window's edge: {menu:?} / {chip:?}"
    );
    let row = cx.debug_bounds("choice-row-1").expect("a live model row");
    // Terminal-native (WP-E): a float row is one grid row.
    assert_eq!(row.size.height, px(ferrite_ui::theme::FLOAT_ROW_H));
}

/// Claude's model menu: the effort is a row under the models, `Effort
/// <level> ›`, its levels in a submenu that opens beside the menu (feet
/// level, `SUBMENU_GAP` apart) — by → from the keys or the pointer resting
/// on the row — and a pick there sets the Thread's effort and closes both.
#[gpui::test]
fn the_model_picker_opens_its_effort_as_a_submenu(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{EFFORT_MENU_W, SUBMENU_GAP};
    let (core, _fake) = cockpit("model-picker-effort", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let ladder = view.read_with(cx, |view, _| view.effort_ladder(thread));
    let Some((steps, _, _)) = ladder else {
        panic!("the premise: the default model takes an effort");
    };
    view.update(cx, |view, cx| view.open_provider_picker(thread, cx));
    tick(cx);
    let menu = cx.debug_bounds("choice-menu").expect("the picker is open");
    let row = cx
        .debug_bounds("choice-submenu-row-0")
        .expect("the effort row rides the model picker");
    assert!(
        cx.debug_bounds("choice-submenu").is_none(),
        "shut until asked"
    );
    cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
    tick(cx);
    let sub = cx
        .debug_bounds("choice-submenu")
        .expect("resting on the row opens the submenu");
    assert!(
        (sub.size.width - px(EFFORT_MENU_W)).abs() <= px(1.),
        "{sub:?}"
    );
    assert!(
        (sub.left() - menu.right() - px(SUBMENU_GAP)).abs() <= px(1.5),
        "beside the menu: {sub:?} / {menu:?}"
    );
    assert!((sub.bottom() - menu.bottom()).abs() <= px(1.), "feet level");
    let last = steps.len() - 1;
    let pick = cx
        .debug_bounds(Box::leak(format!("choice-sub-{last}").into_boxed_str()))
        .expect("every level is a row");
    cx.simulate_click(pick.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.popover.is_none(), "a pick closes the picker");
        assert_eq!(
            view.cockpit.thread(thread).unwrap().effort(),
            Some(steps[last].as_str())
        );
    });
}

/// A Group Pane's Composer carries no chips (SPEC §5.9): the mode,
/// model and effort are Solo's.
#[gpui::test]
fn a_group_panes_composer_carries_no_chips(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("group-no-chips", 2);
    let group = group_all(&mut core);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    view.update(cx, |view, cx| {
        view.enter_group(group, cx);
        view.focus_pane(1);
        cx.notify();
    });
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[1].thread().unwrap());
    assert!(cx.debug_bounds("composer-footer").is_none());
    assert!(debug_bounds(cx, format!("model-picker-{}", thread.get())).is_none());
}

/// A draft's Project menu hangs off its chip in the Composer's toolbar
/// (SPEC §5.5): its left edge at the chip's, resting above it.
#[gpui::test]
fn a_draft_band_menu_hangs_off_its_chip_in_the_bar(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("band-menu-place", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    view.update(cx, |view, cx| view.open_draft(DraftTarget::Main, cx));
    tick(cx);
    let chip = cx.debug_bounds("band-chip-0").expect("the Project chip");
    cx.simulate_mouse_down(chip.center(), MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(chip.center(), MouseButton::Left, gpui::Modifiers::none());
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    view.read_with(cx, |view, _| {
        assert!(view
            .popover
            .as_ref()
            .is_some_and(|open| matches!(open.kind, Kind::Band(pane::BandChip::Project))));
    });
    let rows = cx
        .debug_bounds("composer-menu-rows")
        .expect("the menu is drawn");
    assert!(
        (rows.left() - chip.left()).abs() <= px(1.5),
        "the menu stands on its chip: {rows:?} / {chip:?}"
    );
    assert!(
        rows.bottom() <= chip.top() - px(ferrite_ui::theme::FLOAT_OFFSET),
        "it rests above its chip: {rows:?} / {chip:?}"
    );
}

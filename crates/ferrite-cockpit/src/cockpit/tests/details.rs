//! The Thread details card (`cockpit/details.rs`): the Thread's lightweight
//! context, reachable beside the tool the Panel holds, with its own
//! visibility, focus and dismissal.
use super::*;
use crate::cockpit::surfaces::SurfaceId;

fn solo<'a>(
    name: &str,
    width: f32,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    ThreadKey,
    &'a mut gpui::VisualTestContext,
) {
    let (core, _fake, _checkout) = bound_cockpit(name, Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(width), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, cx)
}

fn composer_focused(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|window, cx| {
        view.read(cx).panes[0]
            .read(cx).composer
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    })
}

fn open(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    view.read_with(cx, |view, cx| view.panes[0].read(cx).details.is_open())
}

#[gpui::test]
fn the_card_opens_beside_the_panels_files(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("details-beside-files", 1440., cx);
    view.update(cx, |view, cx| {
        view.upsert_surface(thread, SurfaceId::Files, cx)
    });
    tick(cx);
    assert!(
        cx.debug_bounds("details-card").is_none(),
        "closed until asked"
    );

    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    let card = cx.debug_bounds("details-card").expect("⌘⇧B opens the card");
    let files = cx
        .debug_bounds("files")
        .expect("the Panel still shows Files");
    assert!(
        card.right() <= files.left(),
        "the card floats over the transcript, not the tool"
    );
    assert!(!composer_focused(&view, cx), "the card has the keyboard");
    assert_eq!(
        panel_tabs(&view, thread, cx).1,
        Some(SurfaceId::Files),
        "opening it leaves the Panel's tool alone"
    );
}

#[gpui::test]
fn the_title_bar_toggle_opens_and_shuts_it(cx: &mut TestAppContext) {
    let (view, _, cx) = solo("details-toggle", 1440., cx);
    let toggle = cx
        .debug_bounds("details-toggle")
        .expect("the title bar's toggle");
    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(open(&view, cx));
    assert!(
        cx.debug_bounds("panel-toggle").is_some(),
        "beside the Panel's own toggle"
    );
    assert!(cx.debug_bounds("details-card").is_some());

    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        !open(&view, cx),
        "a second press on the toggle shuts it, not reopens it"
    );
    assert!(cx.debug_bounds("details-card").is_none());
}

#[gpui::test]
fn escape_shuts_the_card_and_returns_the_keyboard_to_the_composer(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("details-escape", 1440., cx);
    view.update(cx, |view, cx| {
        view.upsert_surface(thread, SurfaceId::Files, cx)
    });
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!open(&view, cx));
    assert!(cx.debug_bounds("details-card").is_none());
    assert!(
        cx.debug_bounds("files").is_some(),
        "Escape leaves the tool open"
    );
    assert!(composer_focused(&view, cx));
}

#[gpui::test]
fn a_press_outside_shuts_the_card(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("details-click-out", 1440., cx);
    view.update(cx, |view, cx| {
        view.upsert_surface(thread, SurfaceId::Files, cx)
    });
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    let card = cx.debug_bounds("details-card").unwrap();
    cx.simulate_click(card.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(open(&view, cx), "a press inside it keeps it");

    let files = cx.debug_bounds("files").unwrap();
    cx.simulate_click(files.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(!open(&view, cx), "a press outside shuts it");
    assert!(cx.debug_bounds("files").is_some(), "the tool stays");
}

#[gpui::test]
fn a_narrow_pane_opens_details_as_a_popover(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("details-narrow", 760., cx);
    cx.simulate_keystrokes("cmd-alt-b");
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none());
    let surface = panel_tabs(&view, thread, cx).1;
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    assert!(open(&view, cx));
    assert!(cx.debug_bounds("details-card").is_some());
    assert!(
        cx.debug_bounds("panel").is_none(),
        "popover leaves panel visibility alone"
    );
    assert_eq!(panel_tabs(&view, thread, cx).1, surface);
}

#[gpui::test]
fn a_narrow_pane_keeps_the_panels_files_and_floats_the_card(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("details-narrow-files", 1440., cx);
    view.update(cx, |view, cx| {
        view.upsert_surface(thread, SurfaceId::Files, cx)
    });
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    assert!(
        cx.debug_bounds("details-card").is_some(),
        "the card floats beside Files at the default window size"
    );
    assert_eq!(
        panel_tabs(&view, thread, cx).1,
        Some(SurfaceId::Files),
        "Files stays the Panel's tool"
    );
}

#[gpui::test]
fn inline_details_persist_beside_the_conversation(cx: &mut TestAppContext) {
    let (_view, _, cx) = solo("details-inline", 2400., cx);
    assert!(
        cx.debug_bounds("details-card").is_some(),
        "inline starts visible"
    );
    let card = cx.debug_bounds("details-card").unwrap();
    assert_eq!(card.size.width, px(280.));
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(
        cx.debug_bounds("details-card").is_some(),
        "Escape keeps inline context"
    );
    let toggle = cx.debug_bounds("details-toggle").unwrap();
    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("details-card").is_none());
    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("details-card").is_some());
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    tick(cx);
    assert!(
        cx.debug_bounds("details-card").is_none(),
        "narrow popover starts closed"
    );
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    assert!(
        cx.debug_bounds("details-card").is_some(),
        "narrow context is available on demand"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    cx.simulate_resize(gpui::size(px(2400.), px(900.)));
    tick(cx);
    assert!(
        cx.debug_bounds("details-card").is_some(),
        "inline context returns without toggling"
    );
    cx.simulate_resize(gpui::size(px(2400.), px(200.)));
    tick(cx);
    assert!(
        cx.debug_bounds("details-card").is_none(),
        "short canvases cannot reserve an inline card"
    );
}

#[gpui::test]
fn the_page_keys_leave_the_transcript_alone_while_the_card_has_them(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("details-page-keys", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: (0..80)
                .map(|line| format!("history line {line:03}\n\n"))
                .collect(),
        })
        .unwrap();
    tick(cx);
    let offset = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.panes[0]
                .read(cx).transcript()
                .unwrap()
                .read(cx)
                .scroll()
                .offset()
                .y
        })
    };
    let rest = offset(cx);
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    for key in ["pageup", "pagedown", "home", "end"] {
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        assert_eq!(offset(cx), rest, "{key} did not scroll the transcript");
    }
    assert!(open(&view, cx), "and did not shut the card");
}

#[gpui::test]
fn focus_moving_to_another_thread_shuts_the_card_it_left(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("details-two-threads", 2);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    tick(cx);
    let cards = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.panes
                .iter()
                .map(|pane| pane.read(cx).details.is_open())
                .collect::<Vec<_>>()
        })
    };
    view.update(cx, |view, cx| {
        view.focus_pane(0, cx);
        cx.notify();
    });
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    assert_eq!(cards(cx), [true, false]);

    view.update(cx, |view, cx| {
        view.focus_pane(1, cx);
        cx.notify();
    });
    tick(cx);
    assert_eq!(cards(cx), [false, false], "the card it left shuts");

    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    assert_eq!(cards(cx), [false, true], "the new Thread has its own");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(cards(cx), [false, false], "and Escape shuts it");
}

#[gpui::test]
fn the_card_hangs_below_the_find_bar(cx: &mut TestAppContext) {
    let (_, _, cx) = solo("details-find", 1440., cx);
    cx.simulate_keystrokes("cmd-f");
    tick(cx);
    let find = cx.debug_bounds("find-bar").expect("the find bar");
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    let card = cx.debug_bounds("details-card").expect("the card");
    assert!(
        card.top() >= find.bottom(),
        "clear of the find bar: {card:?} vs {find:?}"
    );
}

#[gpui::test]
fn the_card_leaves_with_its_exit_and_snaps_when_motion_is_reduced(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (view, _, cx) = solo("details-exit", 1440., cx);
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!open(&view, cx));
    let leaving = cx.debug_bounds("details-card").is_some();
    for _ in 0..40 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    assert!(
        cx.debug_bounds("details-card").is_none(),
        "gone once its exit ran (was leaving: {leaving})"
    );
}

#[gpui::test]
fn inline_visibility_outlives_pane_recreation(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("details-durable-inline", 2400., cx);
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    assert!(cx.debug_bounds("details-card").is_none());
    view.update(cx, |view, cx| {
        view.flush_panel_persistence(cx);
        let value = ferrite_pane::details::DetailsCard::new(cx);
        view.panes[0].update(cx, |pane, _| pane.details = value);
        assert!(!view.panel.surfaces.get(thread).unwrap().details_inline_open);
        let restored = crate::cockpit::surfaces::Surfaces::saved_in(&view.prefs.dir, thread.thread()).unwrap();
        assert!(!restored.details_inline_open);
        cx.notify();
    });
    tick(cx);
    assert!(
        cx.debug_bounds("details-card").is_none(),
        "rebuilding the pane keeps the thread's choice"
    );
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    assert!(open(&view, cx));
    cx.simulate_resize(gpui::size(px(2400.), px(900.)));
    tick(cx);
    assert!(
        !open(&view, cx),
        "inline mode clears transient popover visibility"
    );
    assert!(
        cx.debug_bounds("details-card").is_none(),
        "popover does not change the inline choice"
    );
}

#[gpui::test]
fn details_preferences_can_empty_and_restore_the_card(cx: &mut TestAppContext) {
    use ferrite_core::settings::{ContextCardItem, Settings};
    let (view, thread, cx) = solo("details-item-preferences", 2400., cx);
    view.update(cx, |view, cx| {
        view.change_settings(
            |settings| {
                for item in ContextCardItem::ALL {
                    settings.context_card_preferences.set(item, false);
                }
            },
            cx,
        );
    });
    tick(cx);
    let choose = cx
        .debug_bounds("details-choose-items")
        .expect("an empty card can be refilled");
    cx.simulate_click(choose.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        let menu = view.context_menu().unwrap();
        assert_eq!(menu.target, MenuTarget::Details(thread));
        let items: Vec<_> = menu.rows.iter().flatten().collect();
        assert_eq!(items.len(), 13);
        assert!(items[..12].iter().all(|(item, _)| !item.checked));
        assert!(!items[12].0.disabled);
        let saved = Settings::load(&view.prefs.dir).unwrap();
        assert!(ContextCardItem::ALL
            .into_iter()
            .all(|item| !saved.context_card_preferences.visible(item)));
    });
    let reset = cx.debug_bounds("context-menu-row-13").unwrap();
    cx.simulate_click(reset.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("details-choose-items").is_none());
    assert!(cx.debug_bounds("details-item-environment-0").is_some());
    view.read_with(cx, |view, _| {
        let saved = Settings::load(&view.prefs.dir).unwrap();
        assert!(saved.context_card_preferences.is_default());
        assert!(!saved
            .context_card_preferences
            .visible(ContextCardItem::Usage));
        assert!(!saved
            .context_card_preferences
            .visible(ContextCardItem::Context));
    });
}

/// Lineage as T3 draws it: a quiet `Lineage · N running` heading, a row per
/// running agent, and the finished ones folded under one counted
/// `Previous agents` row; the fold is the Thread's, so a re-render keeps it,
/// and the card grows and shrinks with it. A Subagent's card instead leads
/// back to its parent: a press on that row returns the Pane to Main.
#[gpui::test]
fn lineage_lists_running_agents_and_folds_the_previous_ones(cx: &mut TestAppContext) {
    use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject};
    let (core, fake) = cockpit("details-lineage-fold", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(2400.), px(900.)));
    let emit = |event| {
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(event))
            .unwrap()
    };
    let key = |name: &str| AgentKey::new(Provider::Claude, "ui-fixture", name);
    for (name, status) in [
        ("Atlas", AgentStatus::Working),
        ("Birch", AgentStatus::Idle),
        ("Cedar", AgentStatus::Failed),
    ] {
        let mut info = AgentInfo::new(key(name));
        info.name = Some(name.into());
        info.parent = Some(Subject::Main);
        emit(ActivityEvent::Discovered(info));
        emit(ActivityEvent::Status {
            key: key(name),
            state: status,
        });
    }
    tick(cx);
    let height = |cx: &mut gpui::VisualTestContext| {
        f32::from(cx.debug_bounds("details-card").unwrap().size.height)
    };
    assert!(
        cx.debug_bounds("details-lineage-running-1").is_some(),
        "the heading says how many run"
    );
    assert!(cx.debug_bounds("details-lineage-agent-Atlas").is_some());
    let title = cx.debug_bounds("details-lineage-title-Atlas").expect("its title");
    assert_eq!(title.size.height, px(16.), "T3's `leading-4`: a whole line box");
    assert!(
        cx.debug_bounds("details-lineage-previous-count-2").is_some(),
        "the finished agents fold under their count"
    );
    for row in ["details-lineage-agent-Birch", "details-lineage-agent-Cedar"] {
        assert!(cx.debug_bounds(row).is_none(), "{row} is folded at first");
    }
    let folded = height(cx);

    let previous = cx.debug_bounds("details-lineage-previous").unwrap();
    cx.simulate_click(previous.center(), gpui::Modifiers::none());
    tick(cx);
    for row in ["details-lineage-agent-Birch", "details-lineage-agent-Cedar"] {
        assert!(cx.debug_bounds(row).is_some(), "a press opens {row}");
    }
    assert!(height(cx) > folded, "the card grows with the fold");

    emit(ActivityEvent::Status {
        key: key("Atlas"),
        state: AgentStatus::Waiting,
    });
    tick(cx);
    assert!(
        cx.debug_bounds("details-lineage-agent-Cedar").is_some(),
        "the fold outlives a re-render"
    );

    let previous = cx.debug_bounds("details-lineage-previous").unwrap();
    cx.simulate_click(previous.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("details-lineage-agent-Birch").is_none());
    assert_eq!(height(cx), folded, "and shrinks when it shuts");

    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    cx.update(|window, app| {
        view.update(app, |view, cx| {
            view.select_subject(thread, Subject::Subagent(key("Atlas")), window, cx)
        })
    });
    tick(cx);
    assert!(
        cx.debug_bounds("details-lineage-agent-Atlas").is_none(),
        "a Subagent's card lists its own agents, not its siblings"
    );
    let parent = cx
        .debug_bounds("details-lineage-parent")
        .expect("a Subagent's card leads back to its parent");
    cx.simulate_click(parent.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        view.read_with(cx, |view, cx| view.panes[0].read(cx).is_main()),
        "the parent row returns the Pane to Main"
    );
    assert!(cx.debug_bounds("details-lineage-parent").is_none());
}

/// `Previous agents` pages as T3's does: six at first, then a
/// `Show N more` row that adds twelve a press, N never more than are left.
/// The card grows with each page, and a card opened afresh starts again at
/// six.
#[gpui::test]
fn previous_agents_show_six_then_twelve_more_a_press(cx: &mut TestAppContext) {
    use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject};
    let (core, fake) = cockpit("details-lineage-pages", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(2400.), px(1400.)));
    let emit = |event| {
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(event))
            .unwrap()
    };
    for index in 0..20 {
        let key = AgentKey::new(Provider::Claude, "ui-fixture", &format!("a{index}"));
        let mut info = AgentInfo::new(key.clone());
        info.name = Some(format!("Agent{index:02}"));
        info.parent = Some(Subject::Main);
        emit(ActivityEvent::Discovered(info));
        emit(ActivityEvent::Status {
            key,
            state: AgentStatus::Idle,
        });
    }
    tick(cx);
    let previous = cx.debug_bounds("details-lineage-previous").unwrap();
    cx.simulate_click(previous.center(), gpui::Modifiers::none());
    tick(cx);
    let height = |cx: &mut gpui::VisualTestContext| {
        f32::from(cx.debug_bounds("details-card").unwrap().size.height)
    };
    let shown = |cx: &mut gpui::VisualTestContext| {
        (0..20)
            .filter(|index| {
                let row: &'static str = format!("details-lineage-agent-Agent{index:02}").leak();
                cx.debug_bounds(row).is_some()
            })
            .count()
    };
    assert_eq!(shown(cx), 6, "the first page is six");
    let first = height(cx);
    let more = cx
        .debug_bounds("details-lineage-show-more-12")
        .expect("a row offers the next twelve");
    cx.simulate_click(more.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(shown(cx), 18, "a press shows twelve more");
    assert!(height(cx) > first, "the card grows with the page");
    let more = cx
        .debug_bounds("details-lineage-show-more-2")
        .expect("the row offers only what is left");
    cx.simulate_click(more.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(shown(cx), 20);
    assert!(cx.debug_bounds("details-lineage-show-more-2").is_none());

    let toggle = cx.debug_bounds("details-toggle").unwrap();
    for _ in 0..2 {
        cx.simulate_click(toggle.center(), gpui::Modifiers::none());
        tick(cx);
    }
    assert_eq!(shown(cx), 6, "a card opened afresh starts at six");
}

/// A Lineage row gives the Pane that agent's full chat.
#[gpui::test]
fn a_lineage_row_opens_its_agent_in_the_pane(cx: &mut TestAppContext) {
    use ferrite_core::activity::{
        ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject, TranscriptCoverage,
    };
    let (core, fake, _checkout) = bound_cockpit("details-lineage-open", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(2400.), px(900.)));
    let key = AgentKey::new(Provider::Claude, "ui-fixture", "toolu_a");
    let mut info = AgentInfo::new(key.clone());
    info.name = Some("Atlas".into());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    for event in [
        ActivityEvent::Discovered(info),
        ActivityEvent::Status {
            key: key.clone(),
            state: AgentStatus::Working,
        },
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(event))
            .unwrap();
    }
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let tabs = panel_tabs(&view, thread, cx).0;
    let row = cx.debug_bounds("details-lineage-agent-Atlas").expect("the agent's row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[0].read(cx).selected, Subject::Subagent(key.clone()));
    });
    assert_eq!(panel_tabs(&view, thread, cx).0, tabs, "no Panel tab opens for it");
}

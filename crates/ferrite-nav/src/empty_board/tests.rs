use std::time::Duration;

use ferrite_ui::testing::{click, mount, record, shows, Heard};
use gpui::{Entity, KeyBinding, TestAppContext, VisualTestContext};

use super::*;

mod start {
    gpui::actions!(empty_board_tests, [First, Second, Third, Fourth]);
}

/// What the window keeps of the empty board, as a test keeps it: how many
/// ways to start it offers, the Threads used last (Thread number, whether a
/// Pane shows it), the CLIs probed, and whether it answers at all.
struct Kept {
    ways: usize,
    recent: Vec<(u64, bool)>,
    cli_versions: Option<(SharedString, SharedString)>,
    answers: bool,
}

fn kept(ways: usize) -> Kept {
    Kept {
        ways,
        recent: Vec::new(),
        cli_versions: None,
        answers: true,
    }
}

fn thread(number: u64) -> ThreadKey {
    ThreadKey::fixture(1, number)
}

/// The ways to start, each its own action so a row run in another's place
/// says so.
fn ways(count: usize) -> Vec<StartRow> {
    let ways: [(&'static str, Box<dyn gpui::Action>); 4] = [
        ("empty_board_tests::First", Box::new(start::First)),
        ("empty_board_tests::Second", Box::new(start::Second)),
        ("empty_board_tests::Third", Box::new(start::Third)),
        ("empty_board_tests::Fourth", Box::new(start::Fourth)),
    ];
    ways.into_iter()
        .take(count)
        .map(|(action, run)| StartRow::new(action, ferrite_ui::icons::PLUS, action, run))
        .collect()
}

/// The empty board in a window of its own, reading `kept` as the Cockpit
/// reads itself, its keys bound as the floats package binds them, and what
/// it asks heard.
fn open(
    kept: Kept,
    cx: &mut TestAppContext,
) -> (Entity<EmptyBoardView>, Entity<Kept>, Heard<String>, &mut VisualTestContext) {
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("up", Previous, Some(KEY_CONTEXT)),
            KeyBinding::new("down", Next, Some(KEY_CONTEXT)),
            KeyBinding::new("enter", Run, Some(KEY_CONTEXT)),
        ])
    });
    let kept = cx.new(|_| kept);
    let inputs = Source::of(&kept, |kept: &Kept, _: &(), _| {
        kept.answers.then(|| EmptyBoardInputs {
            cli_versions: kept.cli_versions.clone(),
            rows: ways(kept.ways),
            recent: kept
                .recent
                .iter()
                .map(|&(number, open)| RecentRow {
                    thread: thread(number),
                    status: RowStatus::Idle,
                    name: format!("Thread {number}").into(),
                    project: Some("ferrite".into()),
                    age: Some("3m".into()),
                    open,
                })
                .collect(),
        })
    });
    let (view, cx) = mount(cx, |_, cx| EmptyBoardView::new(inputs, cx));
    let heard = record(cx, &view, |event: &EmptyBoardEvent| match event {
        EmptyBoardEvent::Run(action) => format!("run {}", action.name()),
        EmptyBoardEvent::Focus(thread) => format!("focus {}", thread.thread().get()),
        EmptyBoardEvent::Revive(thread) => format!("revive {}", thread.thread().get()),
    });
    (view, kept, heard, cx)
}

fn tick(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(64));
    cx.run_until_parked();
}

/// The list holds the keyboard, as the window's focus pump hands it.
fn hold_keys(view: &Entity<EmptyBoardView>, cx: &mut VisualTestContext) {
    let focus = view.read_with(cx, |view, _| view.focus().clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    tick(cx);
}

fn cursor(view: &Entity<EmptyBoardView>, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |view, _| view.cursor())
}

/// F-13: ↓ and ↑ walk the ways to start, both ways round, and ⏎ runs the
/// one the cursor is on.
#[gpui::test]
fn its_keys_walk_the_ways_to_start_wrapping_and_run_the_one_chosen(cx: &mut TestAppContext) {
    let (view, _kept, heard, cx) = open(kept(3), cx);
    hold_keys(&view, cx);
    assert_eq!(cursor(&view, cx), 0, "the first way, at first");
    cx.simulate_keystrokes("down down");
    assert_eq!(cursor(&view, cx), 2);
    cx.simulate_keystrokes("down");
    assert_eq!(cursor(&view, cx), 0, "it wraps");
    cx.simulate_keystrokes("up");
    assert_eq!(cursor(&view, cx), 2, "both ways");
    assert!(heard.take().is_empty(), "walking asks nothing");
    cx.simulate_keystrokes("enter");
    assert_eq!(heard.take(), ["run empty_board_tests::Third"]);
}

/// A press on a way to start runs that way, and the cursor moves to it.
#[gpui::test]
fn a_press_runs_its_way_and_takes_the_cursor_there(cx: &mut TestAppContext) {
    let (view, _kept, heard, cx) = open(kept(3), cx);
    click(cx, "empty-board-command-1");
    assert_eq!(heard.take(), ["run empty_board_tests::Second"]);
    assert_eq!(cursor(&view, cx), 1);
}

/// R13: a recent Thread a Pane shows is focused, one no Pane shows is
/// brought back, each by its own Thread.
#[gpui::test]
fn a_recent_thread_is_focused_while_shown_and_brought_back_when_not(cx: &mut TestAppContext) {
    let mut world = kept(3);
    world.recent = vec![(3, true), (5, false)];
    let (_view, _kept, heard, cx) = open(world, cx);
    click(cx, "empty-board-recent-3");
    click(cx, "empty-board-recent-5");
    assert_eq!(heard.take(), ["focus 3", "revive 5"]);
}

/// It draws what the window offers and nothing else: each way to start,
/// the Recent list only with a Thread in it, the CLIs' line only once
/// probed. It draws again as the window's notify says the offer moved, and
/// draws nothing once the window answers nothing.
#[gpui::test]
fn it_draws_what_the_window_offers(cx: &mut TestAppContext) {
    let (_view, kept, _heard, cx) = open(kept(3), cx);
    assert!(shows(cx, "empty-board"));
    for at in 0..3 {
        assert!(shows(cx, &format!("empty-board-command-{at}")), "way {at}");
    }
    assert!(!shows(cx, "empty-board-command-3"), "only the ways offered");
    assert!(!shows(cx, "empty-board-recent"), "no Recent with no Thread in it");
    assert!(!shows(cx, "empty-board-facts"), "no CLI line before the probe lands");

    kept.update(cx, |kept, cx| {
        kept.ways = 4;
        kept.recent = vec![(7, false)];
        kept.cli_versions = Some(("2.1.316 \u{b7} /usr/bin/claude".into(), "not found".into()));
        cx.notify();
    });
    tick(cx);
    assert!(shows(cx, "empty-board-command-3"), "a fourth way, once offered");
    assert!(shows(cx, "empty-board-recent"));
    assert!(shows(cx, "empty-board-recent-7"));
    assert!(shows(cx, "empty-board-facts"));

    kept.update(cx, |kept, cx| {
        kept.answers = false;
        cx.notify();
    });
    tick(cx);
    assert!(!shows(cx, "empty-board"), "nothing once the window answers nothing");
}

/// The ways offered can shrink under the cursor (the last parked Thread
/// reopened): ⏎ runs the last one left.
#[gpui::test]
fn the_cursor_holds_on_the_last_way_as_the_ways_shrink(cx: &mut TestAppContext) {
    let (view, kept, heard, cx) = open(kept(4), cx);
    hold_keys(&view, cx);
    cx.simulate_keystrokes("up");
    assert_eq!(cursor(&view, cx), 3);
    kept.update(cx, |kept, cx| {
        kept.ways = 3;
        cx.notify();
    });
    tick(cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(heard.take(), ["run empty_board_tests::Third"]);
    assert_eq!(cursor(&view, cx), 2);
}

/// The empty board's keys come from the platform's own key table and read
/// as glyphs: `⌘N`, `⌘⇧N`, `⌘O`, `⌘G`, `⌘K` (rule 2.11.4, F-13).
#[test]
fn the_empty_board_spells_keys_from_the_key_table() {
    let (primary, glyph) = match ferrite_ui::keymap::PLATFORM {
        ferrite_ui::keymap::Platform::Mac => ("cmd", "\u{2318}"),
        ferrite_ui::keymap::Platform::Windows => ("ctrl", "\u{2303}"),
    };
    let spelled = |action: &str| key_label(action).map(|keys| ferrite_ui::components::key_glyphs(&keys));
    assert_eq!(
        key_label("cockpit::NewThread").as_deref(),
        Some(format!("{primary}-N").as_str())
    );
    assert_eq!(spelled("cockpit::NewThread"), Some(format!("{glyph}N")));
    assert_eq!(
        key_label("cockpit::NewWorktreeThread").as_deref(),
        Some(format!("{primary}-shift-N").as_str())
    );
    assert_eq!(
        spelled("cockpit::NewWorktreeThread"),
        Some(format!("{glyph}\u{21e7}N"))
    );
    assert_eq!(spelled("cockpit::ReopenThread"), Some(format!("{glyph}O")));
    assert_eq!(spelled("palette::Toggle"), Some(format!("{glyph}K")));
}

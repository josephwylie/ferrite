use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ferrite_core::Rehold;
use ferrite_ui::testing::{click, mount, record, shows};
use gpui::{Bounds, Modifiers, MouseButton, TestAppContext, VisualTestContext};

use super::*;

/// How long a real shell gets to answer. A loaded Windows runner has taken
/// longer than 10 seconds to start one.
const SHELL_LOST_AFTER: Duration = Duration::from_secs(60);

/// What the window keeps of the Terminal, as a test keeps it: the shell its
/// Panel shows, whether its Thread is focused, and the `⋯` menu's standing.
struct Kept {
    shown: Option<(ThreadKey, u64)>,
    focused: bool,
    up: bool,
    /// Where the `⋯` button was last laid out, as the window records it for
    /// its menu to hang from.
    recorded: Rc<Cell<Option<Bounds<Pixels>>>>,
}

fn thread() -> ThreadKey {
    ThreadKey::fixture(1, 7)
}

fn kept() -> Kept {
    Kept {
        shown: Some((thread(), 1)),
        focused: true,
        up: false,
        recorded: Rc::default(),
    }
}

/// The Terminal in a window of its own, reading `kept` as the Cockpit
/// reads its Panel.
fn open(kept: Kept, cx: &mut TestAppContext) -> (Entity<TerminalsView>, Entity<Kept>, &mut VisualTestContext) {
    let kept = cx.new(|_| kept);
    let inputs = Source::of(&kept, |kept: &Kept, _: &(), _| {
        let (thread, number) = kept.shown?;
        let recorded = kept.recorded.clone();
        Some(TerminalInputs {
            thread,
            number,
            focused: kept.focused,
            more: MenuDoor::new(kept.up, move |bounds, _, _| recorded.set(Some(bounds))),
        })
    });
    let (view, cx) = mount(cx, |_, _| TerminalsView::new(inputs));
    (view, kept, cx)
}

fn tick(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(64));
    cx.run_until_parked();
}

/// A folder a shell can start in.
fn folder() -> PathBuf {
    std::env::temp_dir()
}

/// One more shell for `thread`, in `directory`, drawn.
fn started(view: &Entity<TerminalsView>, thread: ThreadKey, directory: &Path, cx: &mut VisualTestContext) -> u64 {
    let number = view.update(cx, |view, cx| view.open(thread, directory, None, cx));
    tick(cx);
    number
}

/// The shown shell, drawn, its folder `folder()`.
fn shell_shown(cx: &mut TestAppContext) -> (Entity<TerminalsView>, Entity<Kept>, &mut VisualTestContext) {
    let (view, kept, cx) = open(kept(), cx);
    assert_eq!(started(&view, thread(), &folder(), cx), 1);
    (view, kept, cx)
}

#[test]
fn scrollback_reads_with_thousands_separators() {
    assert_eq!(scrollback_label(1_000), "1,000 lines");
    assert_eq!(scrollback_label(10_000), "10,000 lines");
    assert_eq!(scrollback_label(100_000), "100,000 lines");
}

#[test]
fn a_remote_terminal_never_falls_back_to_a_local_shell() {
    let directory = std::env::temp_dir();
    let bridge = Bridge::new(
        "remote fixture",
        directory.join("unused-terminal-bridge.sock"),
        Box::new(|| unreachable!("terminal startup does not dial the Server")),
    );
    let error = spawn_shell(Some(&bridge), &directory, 1000, (80, 24))
        .err()
        .expect("a non-SSH remote must fail even when the folder exists locally");
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
}

/// The shell the Panel shows is drawn: its row with `⋯`, then the grid.
/// With none shown, or the window gone, nothing is.
#[gpui::test]
fn the_shown_shell_is_its_row_and_its_grid(cx: &mut TestAppContext) {
    let (view, kept, cx) = shell_shown(cx);
    for drawn in ["terminal-pane", "terminal-more", "terminal"] {
        assert!(shows(cx, drawn), "{drawn}");
    }
    assert!(!shows(cx, "terminal-failed"));
    kept.update(cx, |kept, cx| {
        kept.shown = None;
        cx.notify();
    });
    tick(cx);
    assert!(!shows(cx, "terminal-pane"), "no shell shown");
    kept.update(cx, |kept, cx| {
        kept.shown = Some((thread(), 1));
        cx.notify();
    });
    tick(cx);
    assert!(shows(cx, "terminal"), "shown again");
    drop(kept);
    view.update(cx, |_, cx| cx.notify());
    tick(cx);
    assert!(!shows(cx, "terminal-pane"), "the window is gone");
}

/// A shell that cannot start (its folder is gone) leaves a tab that says
/// so and asks the window to try again, which starts it once the folder
/// is back.
#[gpui::test]
fn a_shell_that_cannot_start_says_so_and_asks_to_retry(cx: &mut TestAppContext) {
    let (view, _kept, cx) = open(kept(), cx);
    let heard = record(cx, &view, TerminalsEvent::clone);
    let gone = std::env::temp_dir().join(format!("ferrite-tools-no-folder-{}", std::process::id()));
    assert_eq!(started(&view, thread(), &gone, cx), 1);
    assert!(shows(cx, "terminal-failed"), "the tab says it could not start");
    assert!(!shows(cx, "terminal"), "no grid");
    click(cx, "terminal-retry");
    assert_eq!(heard.take(), [TerminalsEvent::Retry { thread: thread(), number: 1 }]);
    view.update(cx, |view, cx| view.retry(thread(), 1, &folder(), None, cx));
    tick(cx);
    assert!(!shows(cx, "terminal-failed"));
    assert!(shows(cx, "terminal"), "the retry started the shell");
}

/// `⋯` asks the window for its menu, and its bounds are where the window
/// hangs it.
#[gpui::test]
fn the_more_button_asks_for_its_menu_where_it_stands(cx: &mut TestAppContext) {
    let (view, kept, cx) = shell_shown(cx);
    let heard = record(cx, &view, TerminalsEvent::clone);
    let drawn = cx.update(|window, _| window.debug_bounds("terminal-more")).unwrap();
    let recorded = kept.read_with(cx, |kept, _| kept.recorded.get()).expect("its bounds, recorded");
    assert!(
        recorded.size == drawn.size && drawn.contains(&recorded.center()),
        "{recorded:?} is where it was drawn, {drawn:?}"
    );
    click(cx, "terminal-more");
    assert_eq!(heard.take(), [TerminalsEvent::More { thread: thread() }]);
}

/// The `⋯` menu holds Claude's four rows, as set: the cursor, its blink,
/// copy on select, the scrollback.
#[gpui::test]
fn the_more_menu_holds_claudes_four_rows(cx: &mut TestAppContext) {
    let (view, _kept, cx) = shell_shown(cx);
    let rows = view.read_with(cx, |view, _| labels(view.more_rows()));
    assert_eq!(
        rows,
        [
            ("Cursor style".into(), false, Some("Block".into())),
            ("Blink cursor".into(), true, None),
            ("Copy on select".into(), false, None),
            ("Scrollback".into(), false, Some("10,000 lines".into())),
        ]
    );
    let subs = view.read_with(cx, |view, _| {
        view.more_rows().into_iter().flatten().map(|(_, row)| row).collect::<Vec<_>>()
    });
    assert_eq!(
        subs,
        [
            Row::Sub(Sub::CursorStyle),
            Row::Verb(Verb::ToggleBlink),
            Row::Verb(Verb::ToggleCopyOnSelect),
            Row::Sub(Sub::Scrollback),
        ]
    );
}

/// What a row sets is the window's: the menu and its submenus show it, for
/// a shell started since too.
#[gpui::test]
fn a_more_menu_choice_shows_in_its_rows(cx: &mut TestAppContext) {
    let (view, _kept, cx) = shell_shown(cx);
    view.update(cx, |view, cx| {
        for verb in [
            Verb::ToggleBlink,
            Verb::ToggleCopyOnSelect,
            Verb::Cursor(Cursor::Bar),
            Verb::Scrollback(1_000),
        ] {
            view.run_verb(thread(), 1, verb, cx);
        }
    });
    started(&view, thread(), &folder(), cx);
    let rows = view.read_with(cx, |view, _| labels(view.more_rows()));
    assert_eq!(
        rows,
        [
            ("Cursor style".into(), false, Some("Bar".into())),
            ("Blink cursor".into(), false, None),
            ("Copy on select".into(), true, None),
            ("Scrollback".into(), false, Some("1,000 lines".into())),
        ]
    );
    let ticked = |sub| {
        view.read_with(cx, |view, _| {
            view.sub_rows(sub)
                .into_iter()
                .filter(|(item, _)| item.checked)
                .map(|(item, verb)| (item.label.to_string(), verb))
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(ticked(Sub::CursorStyle), [("Bar".to_string(), Verb::Cursor(Cursor::Bar))]);
    assert_eq!(ticked(Sub::Scrollback), [("1,000 lines".to_string(), Verb::Scrollback(1_000))]);
}

/// A right-click in the shell asks for its menu at the press; with nothing
/// selected the menu has no Attach row and Copy is greyed.
#[gpui::test]
fn with_no_selection_the_menu_has_no_attach_row_and_copy_is_greyed(cx: &mut TestAppContext) {
    let (view, _kept, cx) = shell_shown(cx);
    let heard = record(cx, &view, TerminalsEvent::clone);
    let grid = cx.update(|window, _| window.debug_bounds("terminal")).unwrap();
    cx.simulate_mouse_down(grid.center(), MouseButton::Right, Modifiers::none());
    tick(cx);
    assert!(
        matches!(heard.take()[..], [TerminalsEvent::Menu { thread: asked, .. }] if asked == thread()),
        "the menu, for this Thread"
    );
    let rows = view.read_with(cx, |view, _| {
        view.context_rows()
            .into_iter()
            .flatten()
            .map(|(item, verb)| (item.label.to_string(), item.disabled, verb))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        rows,
        [
            ("Copy".to_string(), true, Verb::Copy),
            ("Paste".into(), false, Verb::Paste),
            ("Select all".into(), false, Verb::SelectAll),
            ("Clear terminal".into(), false, Verb::Clear),
        ]
    );
}

/// A shell that exits, even with a failed status, ends its tab and says so,
/// rather than saying it failed.
#[gpui::test]
fn a_local_shell_with_a_failed_exit_ends_its_tab(cx: &mut TestAppContext) {
    let (view, _kept, cx) = shell_shown(cx);
    let heard = record(cx, &view, TerminalsEvent::clone);
    view.read_with(cx, |view, _| view.shell(thread(), 1))
        .unwrap()
        .update(cx, |shell, _| shell.write(b"exit 7\r"));
    let until = Instant::now() + SHELL_LOST_AFTER;
    while !view.read_with(cx, |view, _| view.numbers(thread())).is_empty() {
        assert!(Instant::now() < until, "the local shell's tab never ended");
        std::thread::sleep(Duration::from_millis(20));
        tick(cx);
    }
    assert_eq!(heard.take(), [TerminalsEvent::Ended { thread: thread(), number: 1 }]);
    assert!(!shows(cx, "terminal-failed"));
}

/// The shown shell takes the keyboard once it is drawn, when its Thread is
/// the focused one, and ⌘J back to it asks again; another Thread's shell
/// never takes it.
#[gpui::test]
fn the_shown_shell_takes_the_keyboard_its_thread_opened_it_for(cx: &mut TestAppContext) {
    let (view, kept, cx) = open(Kept { focused: false, ..kept() }, cx);
    started(&view, thread(), &folder(), cx);
    let holds = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| view.read(cx).holds_focus(thread(), 1, window, cx))
    };
    assert!(!holds(cx), "its Thread is not the focused one");
    kept.update(cx, |kept, cx| {
        kept.focused = true;
        cx.notify();
    });
    tick(cx);
    assert!(!holds(cx), "the open's ask was spent");
    view.update(cx, |view, cx| {
        view.want_focus();
        cx.notify();
    });
    tick(cx);
    assert!(holds(cx), "⌘J back to it");
}

/// A Thread with the same id on another machine is another Thread (ADR
/// 0045): each has its own Terminal 1, and ending one leaves the other's.
#[gpui::test]
fn a_terminal_keeps_two_machines_threads_apart(cx: &mut TestAppContext) {
    let (view, _kept, cx) = shell_shown(cx);
    let there = ThreadKey::fixture(u32::MAX, thread().thread().get());
    assert_eq!(started(&view, there, &folder(), cx), 1, "a Terminal 1 of its own");
    assert_eq!(view.read_with(cx, |view, _| view.numbers(there)), [1], "the other machine's Thread holds it");
    assert_eq!(view.read_with(cx, |view, _| view.numbers(thread())), [1], "and this Thread its own");
    view.update(cx, |view, cx| view.end(there, 1, cx));
    assert!(view.read_with(cx, |view, _| view.numbers(there)).is_empty());
    assert_eq!(view.read_with(cx, |view, _| view.numbers(thread())), [1], "this Thread's shell runs on");
}

/// A machine let go ends its shells, each tab saying why; held again, its
/// Threads' tabs are theirs under their new keys, and another machine's
/// stay.
#[gpui::test]
fn a_machine_let_go_ends_its_shells_and_held_again_they_move(cx: &mut TestAppContext) {
    let (view, kept, cx) = shell_shown(cx);
    let other = ThreadKey::fixture(2, 7);
    started(&view, other, &folder(), cx);
    view.update(cx, |view, cx| view.let_go(thread().machine(), cx));
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.shell(thread(), 1)).is_none(), "its shell ended");
    assert!(view.read_with(cx, |view, _| view.shell(other, 1)).is_some(), "another machine's runs on");
    let says = cx.update(|window, _| window.debug_text("terminal-failed"));
    assert!(
        says.is_some_and(|text| {
            text.iter().any(|line| line.contains("The remote terminal ended when its machine was turned off."))
        }),
        "its tab says why"
    );
    let now = ThreadKey::fixture(3, 0).machine();
    view.update(cx, |view, cx| view.held_again(Rehold { was: thread().machine(), now }, cx));
    let moved = thread().held_again(now);
    assert_eq!(view.read_with(cx, |view, _| view.numbers(moved)), [1], "its tab, under its new key");
    assert!(view.read_with(cx, |view, _| view.numbers(thread())).is_empty());
    assert_eq!(view.read_with(cx, |view, _| view.numbers(other)), [1]);
    kept.update(cx, |kept, cx| {
        kept.shown = Some((moved, 1));
        cx.notify();
    });
    tick(cx);
    assert!(shows(cx, "terminal-failed"), "it still says why, there");
}

/// Each row's label, tick and detail.
fn labels<V>(rows: Vec<Option<(menu::Item, V)>>) -> Vec<(String, bool, Option<String>)> {
    rows.into_iter()
        .flatten()
        .map(|(item, _)| (item.label.to_string(), item.checked, item.detail.as_ref().map(|d| d.to_string())))
        .collect()
}

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use ferrite_ui::testing::{click, mount, record, shows, Heard};
use gpui::{px, Focusable, KeyBinding, Modifiers, MouseButton, TestAppContext, VisualTestContext};

use super::*;
use crate::notifications::{Detail, RowTarget, State};
use crate::{NavItem, NavTail, RowStatus, ThreadRow};

/// What the window keeps that the nav draws, as a test keeps it.
struct Kept {
    /// Each open Thread's number and the section it is in, in the
    /// window's order.
    listed: Vec<(u64, Section)>,
    /// The parked Threads' numbers.
    parked: Vec<u64>,
    /// The Thread the board shows.
    shown: Option<u64>,
    machines: Vec<MachineNotice>,
    open_shelves: Vec<Shelf>,
    menu: Option<NavMenu>,
    door_up: Option<NavMenu>,
    /// Each Thread's name, when not `Thread n`.
    names: HashMap<u64, &'static str>,
    /// The branch each Thread's checkout is on, if it has one.
    branches: HashMap<u64, &'static str>,
    /// The provider each Thread runs, if the window says.
    providers: HashMap<u64, Provider>,
    /// Whether the Factory button stands.
    factory: bool,
    /// Whether it answers at all.
    answers: bool,
}

fn kept(listed: &[(u64, Section)]) -> Kept {
    Kept {
        listed: listed.to_vec(),
        parked: Vec::new(),
        shown: None,
        machines: Vec::new(),
        open_shelves: Vec::new(),
        menu: None,
        door_up: None,
        names: HashMap::new(),
        branches: HashMap::new(),
        providers: HashMap::new(),
        factory: false,
        answers: true,
    }
}

fn thread(number: u64) -> ThreadKey {
    ThreadKey::fixture(0, number)
}

impl Kept {
    fn row(&self, number: u64) -> ThreadRow {
        let name = self.names.get(&number).map_or_else(|| format!("Thread {number}"), |name| name.to_string());
        ThreadRow {
            key: thread(number),
            name: name.into(),
            status: RowStatus::Idle,
            project: None,
            selected: self.shown == Some(number),
            tail: NavTail::None,
            branch: self.branches.get(&number).map(|&branch| branch.into()),
            worktree: None,
            pr: None,
            provider: self.providers.get(&number).copied(),
            activity_at: None,
            working_since: None,
            pinned: false,
            retitling: false,
            picked: false,
            elsewhere: None,
        }
    }

    fn listed(&self) -> Vec<Listed> {
        self.listed
            .iter()
            .map(|&(number, section)| (thread(number), section, None))
            .collect()
    }

    /// What the nav draws: each listed Thread in its section, the active
    /// ones latest arrival first, as the window orders them.
    fn look(&self, asked: &NavAsked) -> NavLook {
        let mut state = NavState::default();
        for &(key, section, _) in &asked.listed {
            let row = self.row(key.thread().get());
            match section {
                Section::Pinned => state.pinned.push(row),
                Section::Active => state.active.push(row),
                Section::Working => state.working.push(row),
                Section::Snoozed => state.snoozed.push(row),
            }
        }
        state.active.sort_by_key(|row| asked.arrivals.latest_first(row.key));
        state.parked = self.parked.iter().map(|&number| self.row(number)).collect();
        // Each Active card comes in the Inbox's order, as its row: here
        // none asks anything, so each opens its Thread.
        let inbox = state
            .active
            .iter()
            .map(|row| NoticeRow {
                target: RowTarget::Thread(row.key.thread()),
                thread: row.key,
                title: row.name.clone(),
                state: State::Idle,
                detail: Detail::None,
                when: "".into(),
                read: true,
                elsewhere: false,
            })
            .collect();
        let open = state
            .open_rows()
            .map(|row| {
                (
                    row.key,
                    OpenThread {
                        queued: false,
                        snooze_until: None,
                    },
                )
            })
            .collect();
        NavLook {
            inputs: NavInputs {
                state,
                machines: self.machines.clone(),
                inbox,
                open_shelves: self.open_shelves.clone(),
                width: ferrite_ui::theme::NAV_WIDTH,
                shown: self.shown.map(thread),
                notice: None,
                filtered: None,
                factory: self.factory.then_some(false),
                menu: self.menu,
                door_up: self.door_up,
                open,
                rename: None,
                limits: RateLimits::default(),
                free_disk: None,
                update: None,
            },
            factories: Vec::new(),
        }
    }
}

/// What the nav asked, in words a test compares.
fn said(event: &NavEvent) -> String {
    match event {
        NavEvent::Open(thread) => format!("open {}", thread.thread().get()),
        NavEvent::OpenResult(thread) => format!("result {}", thread.thread().get()),
        NavEvent::Notice(Verb::OpenThread(thread)) => format!("press {}", thread.thread().get()),
        NavEvent::Menu(menu, at) => {
            let at = match at {
                MenuAt::Point(_) => "at the pointer",
                MenuAt::Under(_) => "under its row",
            };
            format!("menu {menu:?} {at}")
        }
        NavEvent::Door(menu) => format!("door {menu:?}"),
        NavEvent::Park(threads) => {
            let threads: Vec<u64> = threads.iter().map(|thread| thread.thread().get()).collect();
            format!("park {threads:?}")
        }
        NavEvent::Snooze(thread, until) => {
            let until = until.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            format!("snooze {} until {until}", thread.thread().get())
        }
        NavEvent::Undo(act) => format!("undo {act:?}"),
        other => format!("{other:?}"),
    }
}

/// The nav in a window of its own, reading `kept` as the window reads
/// itself, looking as it notifies, its keys bound as the window binds
/// them, where its menu buttons stand recorded, and what it asks heard.
fn open(
    kept: Kept,
    cx: &mut TestAppContext,
) -> (
    Entity<NavView>,
    Entity<Kept>,
    Heard<String>,
    Rc<RefCell<Vec<NavMenu>>>,
    &mut VisualTestContext,
) {
    cx.update(|cx| {
        let tree = Some("NavTree");
        cx.bind_keys([
            KeyBinding::new("up", crate::SelectPrevious, tree),
            KeyBinding::new("down", crate::SelectNext, tree),
            KeyBinding::new("enter", crate::Open, tree),
            KeyBinding::new("left", crate::Fold, tree),
            KeyBinding::new("right", crate::Unfold, tree),
            KeyBinding::new("escape", crate::CloseView, tree),
            KeyBinding::new("cmd-z", crate::Undo, tree),
        ])
    });
    let kept = cx.new(|_| kept);
    let doors = Rc::new(RefCell::new(Vec::new()));
    let recorded = doors.clone();
    let sources = NavSources {
        listed: Source::of(&kept, |kept: &Kept, _: &(), _| kept.answers.then(|| kept.listed())),
        look: Source::of(&kept, |kept: &Kept, asked: &NavAsked, _| Some(kept.look(asked))),
        model_name: Source::of(&kept, |_: &Kept, &(thread, provider): &(ThreadKey, Provider), _| {
            Some(format!("{provider:?}'s model of {}", thread.thread().get()).into())
        }),
        door: Rc::new(move |menu: NavMenu, up| {
            let recorded = recorded.clone();
            MenuDoor::new(up, move |_, _, _| recorded.borrow_mut().push(menu))
        }),
    };
    let (view, cx) = mount(cx, |_, cx| NavView::new(sources, cx));
    let heard = record(cx, &view, said);
    view.update(cx, |nav, cx| {
        let wake = cx.observe(&kept, |nav, _, cx| nav.look(cx));
        nav.watch(vec![wake], cx);
    });
    cx.run_until_parked();
    (view, kept, heard, doors, cx)
}

fn change(kept: &Entity<Kept>, cx: &mut VisualTestContext, change: impl FnOnce(&mut Kept)) {
    kept.update(cx, |kept, cx| {
        change(kept);
        cx.notify();
    });
    cx.run_until_parked();
}

fn names(view: &Entity<NavView>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |nav, _| {
        let state = &nav.inputs().expect("it read the window").state;
        state.open_rows().chain(&state.parked).map(|row| row.name.to_string()).collect()
    })
}

/// It draws what the window lists, every section in its place, and draws
/// again only as that moves: a notify that moves nothing it draws draws
/// nothing. A window that answers nothing leaves it as it was.
#[gpui::test]
fn it_draws_what_the_window_lists_and_redraws_only_as_that_moves(cx: &mut TestAppContext) {
    let mut first = kept(&[(1, Section::Active), (2, Section::Pinned), (3, Section::Working)]);
    first.parked = vec![4];
    first.open_shelves = vec![Shelf::Working, Shelf::Parked];
    let (view, kept, _heard, _doors, cx) = open(first, cx);
    for selector in ["nav-thread-1", "nav-thread-2", "nav-thread-3", "nav-thread-4", "nav-shelf-working"] {
        assert!(shows(cx, selector), "{selector}");
    }
    let drawn = crate::testing::nav_renders();
    change(&kept, cx, |_| {});
    change(&kept, cx, |_| {});
    assert_eq!(crate::testing::nav_renders(), drawn, "nothing moved");
    change(&kept, cx, |kept| {
        kept.names.insert(1, "Fold regression");
    });
    assert!(crate::testing::nav_renders() > drawn, "a name moved");
    assert!(names(&view, cx).contains(&"Fold regression".to_string()));
    change(&kept, cx, |kept| {
        kept.answers = false;
        kept.names.insert(1, "Unseen");
    });
    assert!(names(&view, cx).contains(&"Fold regression".to_string()), "nothing read");
}

/// A Thread that comes into Active again comes first: the nav notes when
/// each Thread arrived in its section, and hands the window those arrivals
/// to order the list by.
#[gpui::test]
fn a_thread_back_in_active_comes_first(cx: &mut TestAppContext) {
    let (view, kept, _heard, _doors, cx) =
        open(kept(&[(1, Section::Active), (2, Section::Active), (3, Section::Working)]), cx);
    assert_eq!(names(&view, cx)[..2], ["Thread 1", "Thread 2"]);
    cx.executor().advance_clock(Duration::from_secs(1));
    change(&kept, cx, |kept| kept.listed[2].1 = Section::Active);
    assert_eq!(names(&view, cx)[0], "Thread 3", "it arrived last");
}

/// A card's press asks for its Thread; a right press asks for its menu at
/// the pointer; a slim row's Revive, a snoozed one's Wake, a shelf's
/// header and New thread ask for theirs.
#[gpui::test]
fn its_presses_say_what_they_ask(cx: &mut TestAppContext) {
    let mut first = kept(&[(1, Section::Active), (2, Section::Snoozed)]);
    first.parked = vec![3];
    first.open_shelves = vec![Shelf::Snoozed, Shelf::Parked];
    let (_view, _kept, heard, _doors, cx) = open(first, cx);
    click(cx, "nav-thread-1");
    assert_eq!(heard.take(), ["press 1"]);
    let card = cx.debug_bounds("nav-thread-1").unwrap().center();
    cx.simulate_mouse_down(card, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(heard.take(), [format!("menu {:?} at the pointer", NavMenu::Thread(thread(1)))]);
    hover(cx, "nav-thread-2");
    click(cx, "nav-wake-2");
    assert_eq!(heard.take(), [format!("{:?}", NavEvent::Wake(thread(2)))]);
    hover(cx, "nav-thread-3");
    click(cx, "nav-revive-3");
    assert_eq!(heard.take(), [format!("{:?}", NavEvent::Revive(thread(3)))]);
    click(cx, "nav-shelf-parked");
    assert_eq!(heard.take(), ["SetShelfOpen(Parked, false)"]);
    click(cx, "nav-new-thread");
    assert_eq!(heard.take(), ["NewThread"]);
}

/// Handed the keys, the list walks its rows in its order, cards then each
/// shelf's header and rows, and stops at its ends; ⏎ on a header folds
/// it, ⏎ on a card asks for it and gives the keys back.
#[gpui::test]
fn its_keys_walk_its_rows_and_open_the_one_chosen(cx: &mut TestAppContext) {
    let mut first = kept(&[(1, Section::Active), (2, Section::Active), (3, Section::Working)]);
    first.open_shelves = vec![Shelf::Working];
    let (view, _kept, heard, _doors, cx) = open(first, cx);
    let cursor = |cx: &mut VisualTestContext| {
        view.read_with(cx, |nav, _| nav.cursor.as_ref().map(|cursor| cursor.item))
    };
    hold_keys(&view, cx);
    assert_eq!(cursor(cx), Some(NavItem::Thread(thread(1))));
    cx.simulate_keystrokes("down down");
    assert_eq!(cursor(cx), Some(NavItem::Shelf(Shelf::Working)), "the shelf's header");
    cx.simulate_keystrokes("enter");
    assert_eq!(heard.take(), ["SetShelfOpen(Working, false)"]);
    cx.simulate_keystrokes("down down");
    assert_eq!(cursor(cx), Some(NavItem::Thread(thread(3))), "it stops at the end");
    cx.simulate_keystrokes("up up");
    assert_eq!(cursor(cx), Some(NavItem::Thread(thread(2))));
    cx.simulate_keystrokes("enter");
    assert_eq!(heard.take(), ["open 2"]);
    assert!(!view.read_with(cx, |nav, _| nav.keys_held), "it gave the keys back");
    assert_eq!(cursor(cx), None);
}

/// The list takes the keys, its cursor on its first card, as ⌘I hands
/// them.
fn hold_keys(view: &Entity<NavView>, cx: &mut VisualTestContext) {
    view.update_in(cx, |nav, window, cx| {
        let start = nav.first_card(nav.inputs().unwrap());
        nav.take_keys(start, window, cx)
    });
    cx.run_until_parked();
}

fn center(cx: &mut VisualTestContext, selector: String) -> gpui::Point<Pixels> {
    cx.debug_bounds(selector.leak()).expect("drawn").center()
}

/// The pointer rests on what `selector` names: a row's own buttons show.
fn hover(cx: &mut VisualTestContext, selector: &str) {
    let at = center(cx, selector.into());
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.run_until_parked();
}

/// ⌘-click toggles a card into the pick and ⇧-click spans it in the
/// order the list draws; a menu on a picked card acts on the pick, on any
/// other card on that card alone, which lets the pick go. A plain press
/// clears it and opens.
#[gpui::test]
fn picks_span_the_drawing_order_and_a_menu_takes_them(cx: &mut TestAppContext) {
    let listed = [1, 2, 3, 4].map(|number| (number, Section::Active));
    let (view, _kept, heard, _doors, cx) = open(kept(&listed), cx);
    let picked = |cx: &mut VisualTestContext| {
        view.read_with(cx, |nav, _| {
            nav.picked.iter().map(|thread| thread.thread().get()).collect::<Vec<_>>()
        })
    };
    let press = |cx: &mut VisualTestContext, number: u64, modifiers: Modifiers| {
        let at = center(cx, format!("nav-thread-{number}"));
        cx.simulate_click(at, modifiers);
        cx.run_until_parked();
    };
    press(cx, 1, Modifiers::command());
    press(cx, 3, Modifiers::shift());
    assert_eq!(picked(cx), [1, 2, 3]);
    assert!(heard.take().is_empty(), "a pick opens nothing");
    press(cx, 2, Modifiers::command());
    assert_eq!(picked(cx), [1, 3], "it takes it back out");
    assert_eq!(view.read_with(cx, |nav, _| nav.menu_threads(thread(3))), [thread(1), thread(3)]);
    assert_eq!(view.read_with(cx, |nav, _| nav.menu_threads(thread(4))), [thread(4)]);
    let card = center(cx, "nav-thread-4".into());
    cx.simulate_mouse_down(card, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert!(picked(cx).is_empty(), "a menu off the pick lets it go");
    heard.take();
    press(cx, 1, Modifiers::command());
    press(cx, 2, Modifiers::none());
    assert!(picked(cx).is_empty());
    assert_eq!(heard.take(), ["press 2"]);
}

/// A press on a card's Park sweeps across the cards the pointer is dragged
/// over, and the release parks them all.
#[gpui::test]
fn a_park_press_sweeps_and_its_release_parks(cx: &mut TestAppContext) {
    let listed = [1, 2, 3].map(|number| (number, Section::Active));
    let (_view, _kept, heard, _doors, cx) = open(kept(&listed), cx);
    hover(cx, "nav-thread-1");
    let park = center(cx, "nav-card-park-1".into());
    cx.simulate_mouse_down(park, MouseButton::Left, Modifiers::none());
    let over = center(cx, "nav-thread-3".into());
    cx.simulate_mouse_move(over, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(over, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(heard.take(), ["park [1, 2, 3]"]);
}

/// Custom… on a Snooze menu opens the field; ⏎ on a time it reads asks the
/// window and stands until it answers, a refusal staying in it, done
/// closing it; a line it cannot read stays, with why.
#[gpui::test]
fn the_snooze_field_asks_the_window_and_waits_for_its_answer(cx: &mut TestAppContext) {
    let now = std::time::UNIX_EPOCH + Duration::from_secs(1_791_399_990);
    let _clock = ferrite_core::clock::ThreadFixture::install(now.into());
    let (view, _kept, heard, _doors, cx) = open(kept(&[(1, Section::Active)]), cx);
    view.update_in(cx, |nav, window, cx| nav.edit_snooze(thread(1), window, cx));
    cx.run_until_parked();
    view.update(cx, |nav, cx| nav.finish_snooze(true, cx));
    cx.run_until_parked();
    let hour = now + Duration::from_secs(60 * 60);
    let hour = hour.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    assert_eq!(heard.take(), [format!("snooze 1 until {hour}")], "an hour, as it is seeded");
    let error = |cx: &mut VisualTestContext| {
        view.read_with(cx, |nav, _| nav.snooze.as_ref().map(|editor| editor.error.clone()))
    };
    assert_eq!(error(cx), Some(None), "it waits for the answer");
    view.update(cx, |nav, cx| nav.snooze_answered(Some("refused".into()), cx));
    assert_eq!(error(cx), Some(Some("refused".into())), "a refusal stays in it");
    let line = view.read_with(cx, |nav, _| nav.snooze_line().cloned().unwrap());
    line.update(cx, |line, cx| line.set("whenever".into(), cx));
    view.update(cx, |nav, cx| nav.finish_snooze(true, cx));
    assert!(heard.take().is_empty(), "nothing it can read");
    assert!(matches!(error(cx), Some(Some(why)) if why.as_ref() != "refused"));
    view.update(cx, |nav, cx| nav.snooze_answered(None, cx));
    assert_eq!(error(cx), None, "done, it closes");
}

/// A Park or a Snooze leaves the undo notice, which ⌘Z takes back while it
/// stands, once; it fades out on its own after `NAV_UNDO_SHOWN_MS`.
#[gpui::test]
fn the_undo_notice_takes_back_the_last_act_while_it_stands(cx: &mut TestAppContext) {
    let (view, _kept, heard, _doors, cx) = open(kept(&[(1, Section::Active)]), cx);
    let parked = UndoAct::Parked(vec![thread(1)]);
    view.update(cx, |nav, cx| nav.offer_undo(parked.clone(), cx));
    cx.run_until_parked();
    // Faded in, it says what it undoes and the key it is bound to.
    cx.executor()
        .advance_clock(Duration::from_millis(ferrite_ui::theme::MOTION_UNDO_NOTICE_MS));
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    let chord = if cfg!(target_os = "macos") { "⌘Z" } else { "⌃Z" };
    let words = cx.update(|window, _| window.debug_text("nav-undo-notice"));
    let words: Vec<String> = words.expect("the notice").iter().map(ToString::to_string).collect();
    assert_eq!(words, [format!("Parked, Undo {chord}")], "its key, as bound");
    hold_keys(&view, cx);
    cx.simulate_keystrokes("cmd-z cmd-z");
    assert_eq!(heard.take(), [format!("undo {parked:?}")], "once");
    assert!(!shows(cx, "nav-undo-notice"));
    view.update(cx, |nav, cx| nav.offer_undo(UndoAct::Snoozed(thread(1)), cx));
    cx.executor()
        .advance_clock(Duration::from_millis(ferrite_ui::theme::NAV_UNDO_SHOWN_MS));
    cx.run_until_parked();
    assert!(!shows(cx, "nav-undo-notice"), "it went on its own");
    cx.simulate_keystrokes("cmd-z");
    assert!(heard.take().is_empty());
}

/// Each menu button records where it stands for its menu to hang from,
/// and its press asks for its own menu: the Filter's, the Factory's, a
/// card's Snooze. Shut, it gives a tooltip at rest; with its menu up from
/// it, none, which would be drawn over the menu.
#[gpui::test]
fn a_menu_button_asks_for_its_menu_and_records_where_it_stands(cx: &mut TestAppContext) {
    let mut first = kept(&[(1, Section::Active)]);
    first.factory = true;
    let (_view, kept, heard, doors, cx) = open(first, cx);
    for menu in [NavMenu::Filter, NavMenu::Factory] {
        assert!(doors.borrow().contains(&menu), "laid out, {menu:?} recorded its place");
    }
    click(cx, "nav-filter");
    click(cx, "nav-factory");
    hover(cx, "nav-thread-1");
    click(cx, "nav-card-snooze-1");
    let snooze = NavMenu::Snooze(thread(1));
    assert_eq!(heard.take(), ["door Filter".to_string(), "door Factory".into(), format!("door {snooze:?}")]);
    assert!(doors.borrow().contains(&snooze), "the card's, as it stood");
    rest_on(cx, "nav-filter");
    assert!(shows(cx, "tooltip"), "shut: its tooltip");
    hover(cx, "nav-thread-1");
    change(&kept, cx, |kept| kept.door_up = Some(NavMenu::Filter));
    rest_on(cx, "nav-filter");
    assert!(!shows(cx, "tooltip"), "up: no tooltip over its menu");
}

/// The pointer rests on what `selector` names long enough for a tooltip.
fn rest_on(cx: &mut VisualTestContext, selector: &str) {
    let at = center(cx, selector.into());
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_move(at + gpui::point(px(1.), px(0.)), None, Modifiers::none());
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
}

/// A machine being reached again says so only once that has taken
/// `RECONNECT_GRACE`; a press on its notice asks to reach it now, and one
/// that needs a sign-in asks for that.
#[gpui::test]
fn a_machine_reached_again_says_so_only_after_its_grace(cx: &mut TestAppContext) {
    let machine = ThreadKey::fixture(1, 1).machine();
    let mut first = kept(&[(1, Section::Active)]);
    first.machines = vec![MachineNotice {
        machine,
        words: "Reconnecting to box".into(),
        sign_in: false,
        reconnecting: true,
    }];
    let (_view, kept, heard, _doors, cx) = open(first, cx);
    assert!(!shows(cx, "nav-machine-notice-1"), "within its grace");
    cx.executor().advance_clock(RECONNECT_GRACE);
    cx.run_until_parked();
    assert!(shows(cx, "nav-machine-notice-1"), "the nav looked again as it ended");
    click(cx, "nav-machine-notice-1");
    assert_eq!(heard.take(), [format!("{:?}", NavEvent::Reconnect(machine))]);
    change(&kept, cx, |kept| {
        kept.machines[0].sign_in = true;
        kept.machines[0].reconnecting = false;
    });
    click(cx, "nav-machine-notice-1");
    assert_eq!(heard.take(), [format!("{:?}", NavEvent::SignIn(machine))]);
}

/// The field's words narrow the list to the Threads whose title holds
/// them, open and parked; ↓ moves the highlight and ⏎ asks for the one
/// it is on.
#[gpui::test]
fn the_search_narrows_the_list_and_opens_its_pick(cx: &mut TestAppContext) {
    let mut first = kept(&[(1, Section::Active), (2, Section::Active), (3, Section::Active)]);
    first.parked = vec![4];
    first.names = HashMap::from([(1, "fold regression"), (3, "Fold toast"), (4, "unfold rail")]);
    let (view, _kept, heard, _doors, cx) = open(first, cx);
    let search = view.read_with(cx, |nav, _| nav.search.clone());
    search.update(cx, |line, cx| line.set("FOLD".into(), cx));
    cx.run_until_parked();
    for (number, shown) in [(1, true), (2, false), (3, true), (4, true)] {
        assert_eq!(shows(cx, format!("nav-result-{number}").leak()), shown, "{number}");
    }
    cx.update(|window, cx| window.focus(&search.focus_handle(cx), cx));
    view.update(cx, |nav, cx| nav.step_search(1, cx));
    view.update(cx, |nav, cx| nav.open_search_cursor(cx));
    assert_eq!(heard.take(), ["result 3"]);
    view.update(cx, |nav, cx| nav.clear_search(cx));
    cx.run_until_parked();
    assert!(shows(cx, "nav-thread-2"), "the list is back");
}

/// A row's hover card names the model its Thread runs, as the window
/// names it for that Thread and its provider.
#[gpui::test]
fn a_hover_card_names_the_model_as_the_window_does(cx: &mut TestAppContext) {
    let mut first = kept(&[(1, Section::Active), (2, Section::Active)]);
    first.providers = HashMap::from([(1, Provider::Claude), (2, Provider::Codex)]);
    let (_view, _kept, _heard, _doors, cx) = open(first, cx);
    for (number, named) in [(1, "Claude's model of 1"), (2, "Codex's model of 2")] {
        rest_on(cx, &format!("nav-thread-{number}"));
        let card = crate::testing::drawn_hover_card();
        assert!(card.iter().any(|line| line == named), "{named} in {card:?}");
    }
}

/// A Thread's row is its card (sidebar.md §3): a 78px box with its
/// checkout line — 8 + 20, 4, 20, 2, 16 + 8 — its content 10px in, and a
/// card with nothing for line 3 hugs its two lines (60). Cards pitch at
/// their 2px item padding.
#[gpui::test]
fn a_thread_card_is_three_lines_in_a_fixed_box(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{NAV_CARD_H, NAV_CARD_H_SHORT, NAV_ROW_PAD_X};
    let mut first = kept(&[(1, Section::Active), (2, Section::Active)]);
    first.branches.insert(1, "t3/improve-sidebar-architecture");
    let (_view, _kept, _heard, _doors, cx) = open(first, cx);
    let at = |cx: &mut VisualTestContext, name: &str, number: u64| {
        let key: &'static str = format!("{name}-{number}").leak();
        cx.debug_bounds(key).unwrap_or_else(|| panic!("{key} drawn"))
    };
    let card = at(cx, "nav-thread", 1);
    let line1 = at(cx, "nav-card-line1", 1);
    let line2 = at(cx, "nav-card-line2", 1);
    let line3 = at(cx, "nav-card-line3", 1);
    let title = at(cx, "nav-title", 1);
    assert_eq!(card.size.height, px(NAV_CARD_H));
    assert_eq!(line1.top() - card.top(), px(8.0), "8px over line 1");
    assert_eq!(line1.size.height, px(20.0));
    assert_eq!(line2.top() - line1.bottom(), px(4.0));
    assert_eq!(line2.size.height, px(20.0));
    assert_eq!(line3.top() - line2.bottom(), px(2.0));
    assert_eq!(line3.size.height, px(16.0));
    assert_eq!(card.bottom() - line3.bottom(), px(8.0), "8px under line 3");
    assert!(
        (title.left() - (card.left() + px(NAV_ROW_PAD_X))).abs() < px(0.5),
        "the title 10px in: {title:?} in {card:?}"
    );
    let bare = at(cx, "nav-thread", 2);
    assert_eq!(bare.size.height, px(NAV_CARD_H_SHORT), "no checkout: it hugs");
    assert!(!shows(cx, "nav-card-line3-2"));
    let (upper, lower) = if card.top() < bare.top() { (card, bare) } else { (bare, card) };
    assert!(
        lower.top() - upper.bottom() >= px(4.0),
        "2px item padding above and below each card"
    );
}

/// A wheel turn over the list, `y` pixels.
fn wheel(cx: &mut VisualTestContext, y: f32) {
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: gpui::point(px(100.), px(200.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(y))),
        ..Default::default()
    });
    cx.run_until_parked();
}

fn active(count: u64) -> Kept {
    let listed: Vec<_> = (1..=count).map(|number| (number, Section::Active)).collect();
    kept(&listed)
}

/// The list fades at whichever edge has rows past it, 12px deep, never
/// slicing a row against the head or the foot: the foot while rows remain
/// below, the top once it has scrolled, and neither edge with nothing past
/// it.
#[gpui::test]
fn the_list_fades_at_the_edges_rows_pass(cx: &mut TestAppContext) {
    let (_view, _kept, _heard, _doors, cx) = open(active(24), cx);
    cx.simulate_resize(gpui::size(px(1000.), px(320.)));
    cx.run_until_parked();
    assert!(shows(cx, "nav-fade-foot"), "rows below: the foot fades");
    assert_eq!(
        cx.debug_bounds("nav-fade-foot").unwrap().size.height,
        px(ferrite_ui::theme::NAV_EDGE_FADE)
    );
    assert!(!shows(cx, "nav-fade-top"), "at the top: nothing above");
    wheel(cx, -40.);
    assert!(shows(cx, "nav-fade-top"), "scrolled: the top fades");
    assert!(shows(cx, "nav-fade-foot"));
    wheel(cx, -40_000.);
    assert!(shows(cx, "nav-fade-top"));
    assert!(!shows(cx, "nav-fade-foot"), "at the end: nothing below");
}

/// A list scrolled in a short window and grown back until it fits has
/// nothing above it: its top fade goes with the frame that clamps the
/// scroll, whatever the render before it read.
#[gpui::test]
fn a_list_grown_back_to_fit_drops_its_top_fade(cx: &mut TestAppContext) {
    let (_view, _kept, _heard, _doors, cx) = open(active(6), cx);
    cx.simulate_resize(gpui::size(px(1000.), px(320.)));
    cx.run_until_parked();
    wheel(cx, -40_000.);
    assert!(shows(cx, "nav-fade-top"), "scrolled: the top fades");
    cx.simulate_resize(gpui::size(px(1000.), px(1200.)));
    cx.run_until_parked();
    assert!(!shows(cx, "nav-fade-top"), "it all fits: no fade stays");
}

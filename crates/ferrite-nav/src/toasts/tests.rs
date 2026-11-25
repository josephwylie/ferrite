use std::collections::HashSet;
use std::time::Duration;

use ferrite_core::activity::DecisionHandle;
use ferrite_core::notifications::RequestKind;
use ferrite_core::ThreadId;
use ferrite_ui::testing::{click, mount, record, shows, Heard};
use gpui::{Entity, TestAppContext, VisualTestContext};

use super::*;
use crate::notifications::{Detail, State, ALERT_DELAY, DONE_TOAST, NEEDS_YOU_TOAST};

/// One machine the window holds, as a test keeps it: its Notices (id,
/// Thread, read) and its live requests (id, Thread, read).
struct Held {
    machine: MachineId,
    local: bool,
    notices: Vec<(NoticeId, ThreadKey, bool)>,
    requests: Vec<(DecisionNoticeId, ThreadKey, bool)>,
}

/// What the window keeps that the toasts read, as a test keeps it.
struct Kept {
    held: Vec<Held>,
    /// The window has focus.
    present: bool,
    /// The Threads on the board.
    on_board: HashSet<ThreadKey>,
    /// The Threads whose provider chimes.
    chimes: HashSet<ThreadKey>,
    /// The Threads a system notification is wanted for.
    posts: HashSet<ThreadKey>,
    /// The machines that have not answered yet.
    silent: HashSet<MachineId>,
    /// Whether it answers a tend at all.
    answers: bool,
    /// Whether its records are there to word a row.
    worded: bool,
}

fn here(thread: u64) -> ThreadKey {
    ThreadKey::fixture(0, thread)
}

fn on(machine: u32, thread: u64) -> ThreadKey {
    ThreadKey::fixture(machine, thread)
}

fn notice(id: u64) -> NoticeId {
    NoticeId::from_u64(id)
}

fn request(thread: ThreadKey, serial: u64) -> DecisionNoticeId {
    DecisionNoticeId {
        thread: ThreadId::new(thread.thread().get()),
        handle: DecisionHandle {
            generation: 1,
            serial,
            request_id: format!("r{serial}"),
        },
    }
}

/// This computer (machine 0) and another (machine 1), neither saying
/// anything yet; the window has focus and shows Thread 1 here.
fn kept() -> Kept {
    let held = |machine: u32, local| Held {
        machine: on(machine, 1).machine(),
        local,
        notices: Vec::new(),
        requests: Vec::new(),
    };
    Kept {
        held: vec![held(0, true), held(1, false)],
        present: true,
        on_board: HashSet::from([here(1)]),
        chimes: HashSet::new(),
        posts: HashSet::new(),
        silent: HashSet::new(),
        answers: true,
        worded: true,
    }
}

impl Kept {
    fn of(&mut self, machine: MachineId) -> &mut Held {
        self.held.iter_mut().find(|held| held.machine == machine).unwrap()
    }

    /// Whether `alert`'s record is there and unread.
    fn wants(&self, alert: &Alert) -> bool {
        let held = self.held.iter().find(|held| held.machine == alert.thread.machine());
        held.is_some_and(|held| match &alert.target {
            RowTarget::Notice(id) => held.notices.iter().any(|(at, _, read)| at == id && !read),
            RowTarget::Decision(id) => held.requests.iter().any(|(at, _, read)| at == id && !read),
            RowTarget::Thread(_) => false,
        })
    }

    fn tending(&self, asked: &Asked) -> Tending {
        let news = self
            .held
            .iter()
            .map(|held| News {
                machine: held.machine,
                quiet_first: !held.local,
                fresh: held
                    .notices
                    .iter()
                    .filter(|(id, _, _)| asked.presented.get(&held.machine).is_none_or(|seen| id > seen))
                    .cloned()
                    .collect(),
                requests: held.requests.clone(),
            })
            .collect();
        let standing = asked
            .standing
            .iter()
            .map(|alert| Standing {
                live: (!self.silent.contains(&alert.thread.machine())).then(|| self.wants(alert)),
                in_view: self.present && self.on_board.contains(&alert.thread),
            })
            .collect();
        let due = asked
            .due
            .iter()
            .map(|alert| Due {
                chimes: self.chimes.contains(&alert.thread),
                posting: self.posts.contains(&alert.thread).then(|| posting(alert)),
            })
            .collect();
        Tending {
            news,
            standing,
            due,
        }
    }

    fn row(&self, alert: &Alert) -> Option<Row> {
        let number = alert.thread.thread().get();
        self.worded.then(|| Row {
            target: alert.target.clone(),
            thread: alert.thread,
            title: format!("Thread {number}").into(),
            state: if alert.needs_you() {
                State::NeedsYou(RequestKind::Permission)
            } else {
                State::Done
            },
            detail: Detail::Words(format!("about Thread {number}").into()),
            when: "".into(),
            read: false,
            elsewhere: alert.thread.machine().get() != 0,
        })
    }
}

/// The system notification the test window posts for `alert`: tagged by
/// its Thread.
fn posting(alert: &Alert) -> Posting {
    let tag = format!("post-{}-{}", alert.thread.machine().get(), alert.thread.thread().get());
    Posting {
        tag,
        needs_you: alert.needs_you(),
        word: if alert.needs_you() { "needs you" } else { "done" },
        title: format!("Thread {}", alert.thread.thread().get()),
        detail: String::new(),
        agent: None,
        cwd: None,
    }
}

/// What the toasts said, in words a test compares.
fn said(event: &ToastEvent) -> String {
    match event {
        ToastEvent::Answer(machine, id, answer) => {
            format!("answer {} {} {answer:?}", machine.get(), id.handle.request_id)
        }
        ToastEvent::Open(verb) => format!("open {verb:?}"),
        ToastEvent::Rang(Rang {
            withdraw,
            sound,
            post,
        }) => {
            let post: Vec<&str> = post.iter().map(|posting| posting.tag.as_str()).collect();
            format!("rang withdraw={withdraw:?} sound={sound:?} post={post:?}")
        }
    }
}

/// The toasts in a window of their own, reading `kept` as the Cockpit
/// reads itself, tending as it notifies, hung with the nav folded, and
/// what they say heard.
fn open(
    kept: Kept,
    cx: &mut TestAppContext,
) -> (Entity<ToastsView>, Entity<Kept>, Heard<String>, &mut VisualTestContext) {
    let kept = cx.new(|_| kept);
    let sources = ToastSources {
        tending: Source::of(&kept, |kept: &Kept, asked: &Asked, _| {
            kept.answers.then(|| kept.tending(asked))
        }),
        row: Source::of(&kept, |kept: &Kept, alert: &Alert, _| kept.row(alert)),
    };
    let (view, cx) = mount(cx, |_, _| ToastsView::new(sources));
    let heard = record(cx, &view, said);
    view.update(cx, |toasts, cx| {
        let wake = cx.observe(&kept, |toasts, _, cx| toasts.tend(cx));
        toasts.watch(vec![wake], cx);
        toasts.place(
            Place {
                folded: true,
                nav_w: 0.,
                board_right: 0.,
            },
            cx,
        );
    });
    cx.run_until_parked();
    (view, kept, heard, cx)
}

/// Change what the window keeps, and notify as it does.
fn change(kept: &Entity<Kept>, cx: &mut VisualTestContext, change: impl FnOnce(&mut Kept)) {
    kept.update(cx, |kept, cx| {
        change(kept);
        cx.notify();
    });
    cx.run_until_parked();
}

fn wait(cx: &mut VisualTestContext, time: Duration) {
    cx.executor().advance_clock(time);
    cx.run_until_parked();
}

/// A machine's Thread asks for a Decision.
fn ask(kept: &Entity<Kept>, thread: ThreadKey, serial: u64, cx: &mut VisualTestContext) -> DecisionNoticeId {
    let id = request(thread, serial);
    change(kept, cx, |kept| {
        kept.of(thread.machine()).requests.push((id.clone(), thread, false))
    });
    id
}

/// A machine's Thread finishes a turn: Notice `id`, unread.
fn finish(kept: &Entity<Kept>, thread: ThreadKey, id: u64, cx: &mut VisualTestContext) {
    change(kept, cx, |kept| {
        kept.of(thread.machine()).notices.push((notice(id), thread, false))
    });
}

fn standing(view: &Entity<ToastsView>, cx: &mut VisualTestContext) -> Vec<(u32, u64)> {
    view.read_with(cx, |view, _| {
        view.needs_you()
            .toasts()
            .iter()
            .map(|toast| (toast.alert.thread.machine().get(), toast.alert.thread.thread().get()))
            .collect()
    })
}

const TICK: Duration = Duration::from_millis(16);

/// FL-18, ADR 0017: a request off the board waits its second, then
/// toasts, chimes the needs-you chime and posts its system notification,
/// once; the toast draws its row with its answers.
#[gpui::test]
fn a_request_waits_its_second_then_toasts_chimes_and_posts(cx: &mut TestAppContext) {
    let mut kept = kept();
    kept.chimes.insert(here(2));
    kept.posts.insert(here(2));
    let (view, kept, heard, cx) = open(kept, cx);
    ask(&kept, here(2), 7, cx);
    wait(cx, ALERT_DELAY - TICK);
    assert!(standing(&view, cx).is_empty() && !shows(cx, "toast-2"), "it waits its second");
    assert!(heard.take().is_empty());
    wait(cx, TICK);
    assert_eq!(standing(&view, cx), [(0, 2)]);
    assert!(shows(cx, "toast-2") && shows(cx, "toast-allow-2") && shows(cx, "toast-deny-2"));
    assert_eq!(
        heard.take(),
        ["rang withdraw=[] sound=Some(NeedsYou) post=[\"post-0-2\"]"]
    );
    change(&kept, cx, |_| {});
    wait(cx, TICK);
    assert_eq!(standing(&view, cx), [(0, 2)], "one toast");
    assert!(heard.take().is_empty(), "it rang once");
}

/// A finish chimes the done chime; one of a Thread whose provider is
/// muted chimes nothing, and one not wanted posted posts nothing. Ringing
/// together, a request's chime wins.
#[gpui::test]
fn what_rings_chimes_only_where_its_provider_chimes(cx: &mut TestAppContext) {
    let mut kept = kept();
    kept.chimes.insert(here(2));
    let (view, kept, heard, cx) = open(kept, cx);
    finish(&kept, here(2), 1, cx);
    wait(cx, ALERT_DELAY);
    assert_eq!(heard.take(), ["rang withdraw=[] sound=Some(Done) post=[]"]);

    finish(&kept, here(3), 2, cx);
    wait(cx, ALERT_DELAY);
    assert_eq!(standing(&view, cx), [(0, 2), (0, 3)], "it toasts unchimed");
    assert!(heard.take().is_empty(), "Thread 3's provider is muted");

    change(&kept, cx, |kept| {
        kept.chimes.insert(here(4));
    });
    finish(&kept, here(2), 3, cx);
    ask(&kept, here(4), 1, cx);
    wait(cx, ALERT_DELAY);
    assert_eq!(heard.take(), ["rang withdraw=[] sound=Some(NeedsYou) post=[]"]);
}

/// A finish read, or a request answered, within its second never toasts;
/// nor does one on a Thread the operator is looking at, while the window
/// has focus. Away from the window, the same rings.
#[gpui::test]
fn what_is_read_or_in_view_within_its_second_never_rings(cx: &mut TestAppContext) {
    let mut kept = kept();
    kept.chimes.extend([here(1), here(2), here(3)]);
    let (view, kept, heard, cx) = open(kept, cx);
    finish(&kept, here(2), 1, cx);
    change(&kept, cx, |kept| kept.of(here(2).machine()).notices[0].2 = true);
    ask(&kept, here(3), 1, cx);
    change(&kept, cx, |kept| kept.of(here(3).machine()).requests.clear());
    finish(&kept, here(1), 2, cx);
    wait(cx, ALERT_DELAY);
    assert!(standing(&view, cx).is_empty(), "read, answered, in view");
    assert!(heard.take().is_empty());

    change(&kept, cx, |kept| kept.present = false);
    finish(&kept, here(1), 3, cx);
    wait(cx, ALERT_DELAY);
    assert_eq!(standing(&view, cx), [(0, 1)], "nobody is looking");
    assert_eq!(heard.take(), ["rang withdraw=[] sound=Some(Done) post=[]"]);
}

/// A toast goes as its record stops wanting the operator, and its system
/// notification is taken back; one that comes into view goes too.
#[gpui::test]
fn a_toast_goes_as_its_record_stops_wanting_and_its_posting_is_taken_back(
    cx: &mut TestAppContext,
) {
    let mut kept = kept();
    kept.posts.extend([here(2), here(3)]);
    let (view, kept, heard, cx) = open(kept, cx);
    ask(&kept, here(2), 1, cx);
    finish(&kept, here(3), 1, cx);
    wait(cx, ALERT_DELAY);
    assert_eq!(standing(&view, cx), [(0, 2), (0, 3)]);
    heard.take();
    change(&kept, cx, |kept| kept.of(here(2).machine()).requests.clear());
    assert_eq!(standing(&view, cx), [(0, 3)], "the request resolved");
    assert_eq!(heard.take(), ["rang withdraw=[\"post-0-2\"] sound=None post=[]"]);
    change(&kept, cx, |kept| {
        kept.on_board.insert(here(3));
    });
    assert!(standing(&view, cx).is_empty(), "in view");
    assert!(!shows(cx, "toast-3"));
    assert!(heard.take().is_empty(), "still unread, its posting stands");
}

/// ADR 0017: a toast goes on its own, a finish's after 5s and a request's
/// after 8s, each on the toasts' own clock with nothing notified; the
/// request still waits, its posting standing.
#[gpui::test]
fn a_toast_goes_on_its_own_clock(cx: &mut TestAppContext) {
    let mut kept = kept();
    kept.posts.insert(here(3));
    let (view, kept, heard, cx) = open(kept, cx);
    finish(&kept, here(2), 1, cx);
    wait(cx, ALERT_DELAY);
    wait(cx, DONE_TOAST - TICK);
    assert!(shows(cx, "toast-2"), "it stands its five seconds");
    wait(cx, TICK);
    assert!(standing(&view, cx).is_empty() && !shows(cx, "toast-2"), "then goes");

    ask(&kept, here(3), 1, cx);
    wait(cx, ALERT_DELAY);
    heard.take();
    wait(cx, NEEDS_YOU_TOAST - TICK);
    assert!(shows(cx, "toast-3"), "a request stands eight");
    wait(cx, TICK);
    assert!(standing(&view, cx).is_empty() && !shows(cx, "toast-3"));
    assert!(heard.take().is_empty(), "the request still waits: nothing taken back");
}

/// ADR 0045: what another machine held when the window first heard it is
/// not news; what it says after rings, its toast named by its machine.
/// This computer's first hearing is news.
#[gpui::test]
fn another_machines_first_hearing_is_quiet(cx: &mut TestAppContext) {
    let mut kept = kept();
    kept.of(on(1, 4).machine())
        .requests
        .push((request(on(1, 4), 1), on(1, 4), false));
    kept.of(here(2).machine())
        .requests
        .push((request(here(2), 1), here(2), false));
    let (view, kept, _heard, cx) = open(kept, cx);
    wait(cx, ALERT_DELAY);
    assert_eq!(standing(&view, cx), [(0, 2)], "box's standing request is not news");
    ask(&kept, on(1, 5), 2, cx);
    wait(cx, ALERT_DELAY);
    assert_eq!(standing(&view, cx), [(0, 2), (1, 5)]);
    assert!(shows(cx, "toast-1-5"), "named by its machine");
}

/// A toast's buttons say what was asked, about its record on its machine,
/// and the window takes the toast down as it hears them: it goes at once,
/// no face fading where it stood. One that goes on its own fades.
#[gpui::test]
fn its_buttons_answer_and_open_on_its_machine_and_go_at_once(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (view, kept, heard, cx) = open(kept(), cx);
    // The window acts on what was asked, and takes the toast down.
    cx.update(|_, cx| {
        cx.subscribe(&view, |toasts, event: &ToastEvent, cx| {
            let (machine, target) = match event {
                ToastEvent::Answer(machine, id, _) => (*machine, RowTarget::Decision(id.clone())),
                ToastEvent::Open(Verb::OpenDecision(machine, id)) => {
                    (*machine, RowTarget::Decision(id.clone()))
                }
                _ => return,
            };
            toasts.update(cx, |toasts, cx| toasts.drop_toast(machine, &target, cx));
        })
        .detach()
    });
    let machine = on(1, 5).machine();
    let open_seven = format!("open {:?}", Verb::OpenDecision(machine, request(on(1, 7), 7)));
    for (thread, button, said) in [
        (5, "toast-allow-1-5", "answer 1 r5 Allow".to_string()),
        (6, "toast-deny-1-6", "answer 1 r6 Deny".to_string()),
        (7, "toast-open-1-7", open_seven),
    ] {
        ask(&kept, on(1, thread), thread, cx);
        wait(cx, ALERT_DELAY);
        heard.take();
        click(cx, button);
        assert_eq!(heard.take(), [said]);
        assert!(standing(&view, cx).is_empty(), "{button}");
        assert!(!shows(cx, "toast-leaving"), "{button}: it went at once");
    }

    // One that goes on its own leaves softly.
    ask(&kept, here(2), 9, cx);
    wait(cx, ALERT_DELAY);
    change(&kept, cx, |kept| kept.of(here(2).machine()).requests.clear());
    assert!(shows(cx, "toast-leaving"), "it fades");
}

/// A toast shows only with the nav folded, and only while its record is
/// there to word it; a window that answers nothing rings nothing.
#[gpui::test]
fn a_toast_shows_only_folded_and_worded(cx: &mut TestAppContext) {
    let (view, kept, heard, cx) = open(kept(), cx);
    ask(&kept, here(2), 1, cx);
    wait(cx, ALERT_DELAY);
    assert!(shows(cx, "toast-2"));
    let hang = |folded| Place {
        folded,
        nav_w: 0.,
        board_right: 0.,
    };
    view.update(cx, |toasts, cx| toasts.place(hang(false), cx));
    cx.run_until_parked();
    assert!(!shows(cx, "toast-2"), "the nav is open");
    view.update(cx, |toasts, cx| toasts.place(hang(true), cx));
    cx.run_until_parked();
    assert!(shows(cx, "toast-2"));
    change(&kept, cx, |kept| kept.worded = false);
    assert_eq!(standing(&view, cx), [(0, 2)]);
    assert!(!shows(cx, "toast-2"), "nothing to word it with");

    change(&kept, cx, |kept| {
        kept.worded = true;
        kept.answers = false;
        kept.chimes.insert(here(3));
    });
    finish(&kept, here(3), 1, cx);
    heard.take();
    wait(cx, ALERT_DELAY);
    assert_eq!(standing(&view, cx), [(0, 2)], "nothing heard");
    assert!(heard.take().is_empty());
}

/// It draws again only as what it shows moves: a window's notify that
/// changes nothing it reads draws nothing.
#[gpui::test]
fn it_draws_only_as_what_it_shows_moves(cx: &mut TestAppContext) {
    let (_view, kept, _heard, cx) = open(kept(), cx);
    let idle = crate::testing::toast_renders();
    for _ in 0..3 {
        change(&kept, cx, |_| {});
    }
    assert_eq!(crate::testing::toast_renders(), idle, "nothing to show");
    ask(&kept, here(2), 1, cx);
    let asked = crate::testing::toast_renders();
    wait(cx, ALERT_DELAY);
    assert!(crate::testing::toast_renders() > asked, "it drew its toast");
    let up = crate::testing::toast_renders();
    for _ in 0..3 {
        change(&kept, cx, |_| {});
    }
    assert_eq!(crate::testing::toast_renders(), up, "the toast stands as it was");
}

/// The window let a machine go: its toasts go and what was heard of it,
/// its Notices' watermark too, is forgotten; the other machine's stand.
#[gpui::test]
fn forgetting_a_machine_takes_its_toasts_down(cx: &mut TestAppContext) {
    let (view, kept, _heard, cx) = open(kept(), cx);
    finish(&kept, here(2), 1, cx);
    finish(&kept, on(1, 5), 1, cx);
    ask(&kept, here(2), 1, cx);
    ask(&kept, on(1, 5), 1, cx);
    wait(cx, ALERT_DELAY);
    assert_eq!(standing(&view, cx), [(0, 2), (1, 5)]);
    let (stays, gone) = (here(2).machine(), on(1, 5).machine());
    let marks = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            (view.needs_you().presented(stays), view.needs_you().presented(gone))
        })
    };
    assert_eq!(marks(cx), (Some(notice(1)), Some(notice(1))), "both heard");
    view.update(cx, |toasts, cx| toasts.forget(&[gone], cx));
    cx.run_until_parked();
    assert_eq!(standing(&view, cx), [(0, 2)]);
    assert!(shows(cx, "toast-2") && !shows(cx, "toast-1-5"));
    assert_eq!(marks(cx), (Some(notice(1)), None), "only its watermark went");
}

/// The window took one toast down (its button, or the list, acted on
/// it): that one goes at once, the other of its machine stands.
#[gpui::test]
fn a_toast_the_window_takes_down_goes_alone(cx: &mut TestAppContext) {
    let (view, kept, _heard, cx) = open(kept(), cx);
    let first = ask(&kept, here(2), 1, cx);
    ask(&kept, here(3), 1, cx);
    wait(cx, ALERT_DELAY);
    assert_eq!(standing(&view, cx), [(0, 2), (0, 3)]);
    let machine = here(2).machine();
    view.update(cx, |toasts, cx| toasts.drop_toast(machine, &RowTarget::Decision(first), cx));
    assert_eq!(standing(&view, cx), [(0, 3)], "its record still wants the operator");
}

/// A scene's requests that stood before the window opened are heard and
/// toast nothing; what comes after rings.
#[gpui::test]
fn hushed_what_stands_toasts_nothing(cx: &mut TestAppContext) {
    let mut kept = kept();
    kept.of(here(2).machine())
        .requests
        .push((request(here(2), 1), here(2), false));
    let (view, kept, _heard, cx) = open(kept, cx);
    view.update(cx, |toasts, cx| toasts.hush(cx));
    wait(cx, ALERT_DELAY);
    assert!(standing(&view, cx).is_empty());
    ask(&kept, here(3), 1, cx);
    wait(cx, ALERT_DELAY);
    assert_eq!(standing(&view, cx), [(0, 3)]);
}

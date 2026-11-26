//! The Decision cards in a window of their own (`ferrite_ui::testing`):
//! drawn from the `Cards` value the window hands the cell, each press said
//! to `DecisionsView` as the event the window answers. No Cockpit behind
//! them.

use ferrite_core::activity::Subject;
use ferrite_core::questions::{Choice, Question};
use ferrite_core::roster::PaneIdentity;
use ferrite_core::settings::ReadingSize;
use ferrite_core::{Decision, DecisionKind};
use ferrite_ui::testing::{click, mount, record, Heard};
use gpui::{TestAppContext, VisualTestContext};

use super::*;
use crate::decision::ApprovalChoice;
use crate::rich::TextCache;

/// Draws `Cards` as the cell draws them on its plane.
struct Host {
    cards: Cards,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col().children(self.cards.render(window, cx))
    }
}

fn thread() -> ThreadKey {
    ThreadKey::fixture(1, 7)
}

fn request(serial: u64, kind: DecisionKind, tool: &str, input: serde_json::Value) -> PendingDecision {
    PendingDecision {
        handle: DecisionHandle {
            generation: 1,
            serial,
            request_id: format!("req_{serial}"),
        },
        subject: Some(Subject::Main),
        decision: Decision {
            delivery: Default::default(),
            kind,
            policy: Default::default(),
            id: format!("req_{serial}"),
            tool_use_id: String::new(),
            tool_name: tool.into(),
            description: String::new(),
            input,
            suggestions: vec![],
        },
        submitting: false,
        reply_error: None,
    }
}

fn approval(serial: u64) -> PendingDecision {
    request(serial, DecisionKind::Approval, "Bash", serde_json::json!({"command": "cargo test"}))
}

fn question(serial: u64) -> PendingDecision {
    let choice = |label: &str| Choice {
        label: label.into(),
        description: format!("{label}, as the agent offers it"),
        preview: None,
    };
    let questions = vec![
        Question {
            id: None,
            question: "Which branch?".into(),
            header: "Branch".into(),
            multi_select: false,
            secret: false,
            allow_other: false,
            options: vec![choice("main"), choice("next")],
        },
        Question {
            id: None,
            question: "Run the tests?".into(),
            header: "Tests".into(),
            multi_select: false,
            secret: false,
            allow_other: false,
            options: vec![choice("Yes"), choice("No")],
        },
    ];
    request(serial, DecisionKind::Questions(questions), "AskUserQuestion", serde_json::Value::Null)
}

/// What a card said, by kind and handle serial.
fn said(event: &DecisionEvent) -> String {
    match event {
        DecisionEvent::Approve { request, choice, .. } => {
            format!("approve {} {choice:?}", request.handle.serial)
        }
        DecisionEvent::Respond { handle, .. } => format!("respond {}", handle.serial),
        DecisionEvent::SendQuestions { handle, .. } => format!("send {}", handle.serial),
        DecisionEvent::SendForm { handle, .. } => format!("send form {}", handle.serial),
        DecisionEvent::Picked { handle, .. } => format!("picked {}", handle.serial),
        DecisionEvent::SendNote { handle, .. } => format!("note {}", handle.serial),
        DecisionEvent::CloseNote { handle, .. } => format!("close note {}", handle.serial),
        DecisionEvent::Answered { handle, answers, .. } => {
            let picks: Vec<_> = answers.iter().map(|answer| answer.picks.clone()).collect();
            format!("answered {} {picks:?}", handle.serial)
        }
    }
}

/// The cards of `requests` on a Pane's plane, as the window hands them
/// over, what they say to their owner, and the owner, which the test keeps
/// as the window's root does: the cards hold it weakly.
fn open(
    requests: Vec<PendingDecision>,
    cx: &mut TestAppContext,
) -> (Entity<DecisionsView>, Heard<String>, &mut VisualTestContext) {
    let pane = PaneIdentity::Thread(thread());
    let owner = cx.new(|_| {
        let mut owner = DecisionsView::default();
        owner.open_pane(pane);
        owner
    });
    let forms = owner.read_with(cx, |owner, _| owner.forms(pane).cloned().expect("the Pane's forms"));
    let cards = Cards {
        owner: owner.downgrade(),
        asked: vec![],
        thread: thread(),
        provider: Some(Provider::Claude),
        context: None,
        switch: None,
        workspace: None,
        reading: ReadingSize::default(),
        rich: TextCache::default(),
        forms,
        request_error: None,
        short: false,
        overlay: true,
        narrow: false,
        inset: theme::tx_inset_at(900., true),
        requests: requests.into_iter().map(|request| (request, false)).collect(),
        kept_in: None,
    };
    let (_, cx) = mount(cx, |_, _| Host { cards });
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    cx.run_until_parked();
    let heard = record(cx, &owner, said);
    (owner, heard, cx)
}

/// An approval's card draws from the value it is handed, and its buttons
/// say their answers to the owner.
#[gpui::test]
fn an_approvals_buttons_say_their_answer(cx: &mut TestAppContext) {
    let (_owner, heard, cx) = open(vec![approval(3)], cx);
    let id = thread().thread().get();
    assert!(cx.debug_bounds("approval-well").is_some(), "the command it asks to run");

    click(cx, &format!("request-allow-{id}-3"));
    assert_eq!(heard.take(), vec![format!("approve 3 {:?}", ApprovalChoice::Allow)]);

    click(cx, &format!("request-deny-{id}-3"));
    assert_eq!(heard.take(), vec![format!("approve 3 {:?}", ApprovalChoice::DenyAndSteer)]);
}

/// A question's picks live in the Pane's forms: a pick says so and the
/// card redraws it, and Send says the answers are ready.
#[gpui::test]
fn a_questions_picks_keep_in_its_forms_and_send_says_so(cx: &mut TestAppContext) {
    let (owner, heard, cx) = open(vec![question(5)], cx);
    let id = thread().thread().get();

    click(cx, "question-choice-0-1");
    click(cx, "question-choice-1-0");
    assert_eq!(heard.take(), vec!["picked 5".to_string(), "picked 5".to_string()]);
    let pane = PaneIdentity::Thread(thread());
    let picks = owner.read_with(cx, |owner, _| {
        let handle = question(5).handle;
        owner.forms(pane).and_then(|forms| forms.answers(&handle))
    });
    let picks: Vec<_> = picks.expect("the form's answers").into_iter().map(|answer| answer.picks).collect();
    assert_eq!(picks, vec![vec![1], vec![0]]);

    click(cx, &format!("request-submit-{id}-5"));
    assert_eq!(heard.take(), vec!["send 5".to_string()]);
}

/// More than one request on the plane: each its own card, each press its
/// own request's.
#[gpui::test]
fn each_request_on_the_plane_is_its_own_card(cx: &mut TestAppContext) {
    let (_owner, heard, cx) = open(vec![approval(1), approval(2)], cx);
    let id = thread().thread().get();
    click(cx, &format!("request-allow-{id}-2"));
    click(cx, &format!("request-allow-{id}-1"));
    let allow = format!("{:?}", ApprovalChoice::Allow);
    assert_eq!(heard.take(), vec![format!("approve 2 {allow}"), format!("approve 1 {allow}")]);
}

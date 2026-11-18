use super::*;
use crate::browser::{Action, Control, Document, Operation, Outcome, Owner, Response};

struct Case {
    harness: Harness,
    window: Window,
    owner_window: Client,
    thread: ThreadId,
    owner: Owner,
    document: Document,
    clock: crate::clock::Fixture,
}

impl Case {
    fn new(name: &str) -> Self {
        let clock = crate::clock::Fixture::install(chrono::Local::now());
        let harness = start(name);
        let mut window = attach(&harness.address);
        let (thread, _) = open(&mut window, &harness);
        let owner_window = browser_test_window(&harness, thread);
        let response = harness
            .browser
            .for_thread(thread)
            .submit(browser_test_action(Action::Open {
                url: "https://example.com/form".into(),
            }))
            .recv_timeout(PATIENCE)
            .unwrap()
            .unwrap();
        Self {
            harness,
            window,
            owner_window,
            thread,
            owner: response.state.owner.unwrap(),
            document: response.state.tabs[0].document(),
            clock,
        }
    }

    fn write(&mut self) -> (Receiver<Result<Response, String>>, Op) {
        let agent = self.harness.browser.for_thread(self.thread);
        let receipt = browser_test_observe(&mut self.owner_window, &agent, self.document, 0)
            .observation
            .unwrap();
        let answer = agent.submit(Operation::Control {
            document: self.document,
            revision: 0,
            observation: Some(receipt),
            control: Control::Click {
                reference: "@save".into(),
            },
        });
        loop {
            if let ServerMessage::BrowserRequest {
                request,
                thread,
                owner,
                document,
                revision,
                dispatch,
                ..
            } = self
                .owner_window
                .messages
                .recv_timeout(PATIENCE)
                .unwrap()
            {
                let boot_id = dispatch.expect("dispatch capability");
                return (
                    answer,
                    Op::BrowserDispatch {
                        request,
                        boot_id,
                        thread,
                        owner,
                        document,
                        revision,
                    },
                );
            }
        }
    }

    fn claim(&mut self, op: Op) -> ServerMessage {
        browser_test_command(&mut self.owner_window, 20_000, op)
    }

    fn complete(&mut self, claim: Op, outcome: Outcome) {
        let Op::BrowserDispatch {
            request,
            boot_id,
            thread,
            owner,
            document,
            revision,
        } = claim
        else {
            unreachable!()
        };
        self.owner_window
            .send(&ClientMessage::BrowserReply {
                request,
                boot_id: Some(boot_id),
                thread,
                owner,
                document,
                revision,
                outcome,
            })
            .unwrap();
        browser_test_command(&mut self.owner_window, 20_001, Op::SettingsChanged);
    }

    fn state(&self) -> Response {
        self.harness
            .browser
            .for_thread(self.thread)
            .submit(Operation::State)
            .recv_timeout(PATIENCE)
            .unwrap()
            .unwrap()
    }

    fn finish(mut self) {
        self.window.command(99, Op::Stop { when: When::Now });
        assert_eq!(self.harness.server.join().unwrap(), 0);
    }
}

#[test]
fn queued_expiration_refuses_dispatch_and_ignores_late_success() {
    let mut case = Case::new("browser-queued-expiry");
    let (answer, claim) = case.write();
    case.clock.advance(Duration::from_secs(45));
    assert!(matches!(
        case.claim(claim.clone()),
        ServerMessage::Reply { error: Some(_), .. }
    ));
    let error = answer
        .recv_timeout(PATIENCE)
        .unwrap()
        .unwrap_err();
    assert!(error.contains("before dispatch authorization"), "{error}");
    assert!(!case.state().state.tabs[0].protected);
    case.complete(
        claim,
        Outcome::Success {
            value: serde_json::json!({"saved": true}),
        },
    );
    assert!(!case.state().state.tabs[0].protected);
    case.finish();
}

#[test]
fn persistence_backlog_does_not_extend_a_submitted_requests_lifetime() {
    let mut case = Case::new("browser-intake-expiry");
    let agent = case.harness.browser.for_thread(case.thread);
    let receipt = browser_test_observe(&mut case.owner_window, &agent, case.document, 0)
        .observation
        .unwrap();
    let gate = case.harness.browser_disk.hold();
    case.owner_window
        .send(&ClientMessage::Command(Command {
            id: 20_030,
            op: Op::Browser {
                thread: case.thread,
                action: Action::Select {
                    tab: case.document.tab,
                },
            },
        }))
        .unwrap();
    browser_test_command(&mut case.owner_window, 20_031, Op::SettingsChanged);
    let answer = agent.submit(Operation::Control {
        document: case.document,
        revision: 0,
        observation: Some(receipt),
        control: Control::Click {
            reference: "@save".into(),
        },
    });
    case.clock.advance(Duration::from_secs(55));
    assert!(matches!(answer.try_recv(), Err(mpsc::TryRecvError::Empty)));
    gate.send(()).unwrap();
    let error = answer
        .recv_timeout(PATIENCE)
        .unwrap()
        .unwrap_err();
    assert!(error.contains("before dispatch authorization"), "{error}");
    assert!(!case.state().state.tabs[0].protected);
    assert!(
        !case
            .owner_window
            .messages
            .try_iter()
            .any(|message| matches!(message, ServerMessage::BrowserRequest { .. })),
        "an expired provider request must never reach the native window"
    );
    case.finish();
}

#[test]
fn authorized_expiration_retains_uncertainty_after_late_success_and_refuses_replay() {
    let mut case = Case::new("browser-authorized-expiry");
    let (answer, claim) = case.write();
    assert!(matches!(
        case.claim(claim.clone()),
        ServerMessage::Reply {
            ok: Some(Done::BrowserDispatch {
                remaining_ms: 45_000
            }),
            error: None,
            ..
        }
    ));
    assert!(matches!(
        case.claim(claim.clone()),
        ServerMessage::Reply { error: Some(_), .. }
    ));
    case.clock.advance(Duration::from_secs(45));
    case.complete(
        claim.clone(),
        Outcome::Success {
            value: serde_json::json!({"saved": true}),
        },
    );
    let error = answer
        .recv_timeout(PATIENCE)
        .unwrap()
        .unwrap_err();
    assert!(error.contains("uncertain"), "{error}");
    assert!(case.state().state.tabs[0].protected);
    assert!(matches!(
        case.claim(claim),
        ServerMessage::Reply { error: Some(_), .. }
    ));
    let agent = case.harness.browser.for_thread(case.thread);
    let retry = agent.submit(Operation::Control {
        document: case.document,
        revision: 0,
        observation: None,
        control: Control::Click {
            reference: "@save".into(),
        },
    });
    assert!(retry.recv_timeout(PATIENCE).unwrap().is_err());
    let observed = browser_test_observe(&mut case.owner_window, &agent, case.document, 0);
    assert!(!observed.state.tabs[0].protected);
    case.finish();
}

#[test]
fn dispatch_rechecks_exact_identity_and_operator_intervention() {
    let mut case = Case::new("browser-claim-identity");
    let (answer, claim) = case.write();
    let mut stale = claim.clone();
    if let Op::BrowserDispatch { revision, .. } = &mut stale {
        *revision += 1;
    }
    assert!(matches!(
        case.claim(stale),
        ServerMessage::Reply { error: Some(_), .. }
    ));
    let revision = browser_test_input(
        &mut case.owner_window,
        20_002,
        case.thread,
        case.owner,
        case.document,
    );
    assert!(matches!(
        case.claim(claim.clone()),
        ServerMessage::Reply { error: Some(_), .. }
    ));
    case.complete(
        claim,
        Outcome::Failed {
            error: "operator cancelled queued work".into(),
        },
    );
    assert!(answer
        .recv_timeout(PATIENCE)
        .unwrap()
        .is_err());
    assert!(
        case.state().state.tabs[0].protected,
        "the operator still holds the page"
    );
    browser_test_command(
        &mut case.owner_window,
        20_003,
        Op::BrowserSettled {
            thread: case.thread,
            owner: case.owner,
            document: case.document,
            revision,
        },
    );
    assert!(!case.state().state.tabs[0].protected);
    case.finish();
}

#[test]
fn queued_completion_cannot_bypass_dispatch_authorization() {
    let mut case = Case::new("browser-claim-required");
    let (answer, claim) = case.write();
    case.complete(
        claim,
        Outcome::Success {
            value: serde_json::json!({"saved": true}),
        },
    );
    let error = answer
        .recv_timeout(PATIENCE)
        .unwrap()
        .unwrap_err();
    assert!(error.contains("before dispatch authorization"), "{error}");
    assert!(!case.state().state.tabs[0].protected);
    case.finish();
}

#[test]
fn another_server_boot_cannot_claim_or_complete_a_reused_request_number() {
    let mut case = Case::new("browser-claim-boot");
    let (answer, claim) = case.write();
    let mut old_boot = claim.clone();
    if let Op::BrowserDispatch { boot_id, .. } = &mut old_boot {
        *boot_id = "previous-server-boot".into();
    }
    assert!(matches!(
        case.claim(old_boot.clone()),
        ServerMessage::Reply { error: Some(_), .. }
    ));
    case.complete(
        old_boot,
        Outcome::Success {
            value: serde_json::json!({"saved": true}),
        },
    );
    assert!(matches!(answer.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert!(case.state().state.tabs[0].protected);
    assert!(matches!(
        case.claim(claim.clone()),
        ServerMessage::Reply {
            ok: Some(Done::BrowserDispatch { .. }),
            error: None,
            ..
        }
    ));
    case.complete(
        claim,
        Outcome::Success {
            value: serde_json::json!({"saved": true}),
        },
    );
    assert!(answer.recv_timeout(PATIENCE).unwrap().is_ok());
    assert!(!case.state().state.tabs[0].protected);
    case.finish();
}

#[test]
fn operator_reload_cancels_a_queued_write_without_uncertainty() {
    let mut case = Case::new("browser-queued-cancel");
    let (answer, claim) = case.write();
    browser_test_command(
        &mut case.owner_window,
        20_020,
        Op::Browser {
            thread: case.thread,
            action: Action::Reload {
                document: case.document,
            },
        },
    );
    assert!(matches!(
        case.claim(claim),
        ServerMessage::Reply { error: Some(_), .. }
    ));
    let error = answer
        .recv_timeout(PATIENCE)
        .unwrap()
        .unwrap_err();
    assert!(error.contains("before dispatch authorization"), "{error}");
    assert!(!case.state().state.tabs[0].protected);
    case.finish();
}

#[test]
fn legacy_browser_owner_is_refused_control_with_an_upgrade_message() {
    let harness = start("browser-legacy-owner");
    let mut window = attach(&harness.address);
    let (thread, _) = open(&mut window, &harness);
    let Attached { mut client, .. } = Client::attach(
        &harness.address,
        ClientMessage::Hello {
            generation: 1,
            client: Peer {
                name: "legacy-browser".into(),
                version: "0".into(),
            },
            capabilities: vec![Capability::ConversationBrowser],
        },
    )
    .unwrap();
    client
        .send(&ClientMessage::Watch {
            thread,
            resume: None,
        })
        .unwrap();
    browser_test_command(&mut client, 20_010, Op::SettingsChanged);
    let agent = harness.browser.for_thread(thread);
    let response = agent
        .submit(browser_test_action(Action::Open {
            url: "https://example.com/".into(),
        }))
        .recv_timeout(PATIENCE)
        .unwrap()
        .unwrap();
    let error = agent
        .submit(Operation::Control {
            document: response.state.tabs[0].document(),
            revision: 0,
            observation: None,
            control: Control::Observe,
        })
        .recv_timeout(PATIENCE)
        .unwrap()
        .unwrap_err();
    assert!(
        error.contains("Update the owning Ferrite window"),
        "{error}"
    );
    assert!(
        !agent
            .submit(Operation::State)
            .recv_timeout(PATIENCE)
            .unwrap()
            .unwrap()
            .state
            .tabs[0]
            .protected
    );
    window.command(99, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
}

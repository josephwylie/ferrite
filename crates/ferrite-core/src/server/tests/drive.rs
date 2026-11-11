//! The API's `window.*` actions reach the one driven window and come back
//! as its answer (ADR 0038).

use serde_json::json;

use super::*;
use crate::drive::{Action, Answer, Find, Keys};

/// A `ferrite --headless` window's end of the link.
fn attach_driven(address: &Path) -> Client {
    Client::attach(
        address,
        ClientMessage::Hello {
            generation: 1,
            client: Peer {
                name: "headless".into(),
                version: "0".into(),
            },
            capabilities: vec![Capability::Driven],
        },
    )
    .unwrap()
    .client
}

/// The next action `window` is handed, past whatever else it hears.
fn handed(window: &Client) -> (u64, Action) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let message = window
            .messages
            .recv_timeout(left)
            .expect("an action within five seconds");
        if let ServerMessage::Drive { request, action } = message {
            return (request, action);
        }
    }
}

/// Nothing `window` hears for a while is an action.
fn never_handed(window: &Client) {
    while let Ok(message) = window.messages.recv_timeout(Duration::from_millis(300)) {
        assert!(
            !matches!(message, ServerMessage::Drive { .. }),
            "a window that is not driven was handed {message:?}"
        );
    }
}

#[test]
fn a_window_action_reaches_only_the_driven_window_and_answers_with_its_reply() {
    let harness = start("drive-routed");
    let plain = attach(&harness.address);
    let mut driven = attach_driven(&harness.address);
    let mut caller = call(&harness);

    caller.ask("1", "window.find", json!({ "selector": "settings-close" }));
    let (request, action) = handed(&driven);
    assert_eq!(
        action,
        Action::Find(Find {
            selector: "settings-close".into()
        })
    );
    caller.quiet();
    driven
        .send(&ClientMessage::Driven {
            request,
            ok: Some(Answer::Found {
                x: 1300.0,
                y: 40.0,
                width: 24.0,
                height: 24.0,
            }),
            error: None,
        })
        .unwrap();
    let found = caller.next();
    assert_eq!(found["id"], "1", "{found}");
    assert_eq!(
        found["result"],
        json!({ "answer": "found", "x": 1300.0, "y": 40.0, "width": 24.0, "height": 24.0 })
    );

    caller.ask("2", "window.key", json!({ "keys": ["cmd-,"] }));
    let (request, action) = handed(&driven);
    assert_eq!(
        action,
        Action::Key(Keys {
            keys: vec!["cmd-,".into()]
        })
    );
    driven
        .send(&ClientMessage::Driven {
            request,
            ok: None,
            error: Some("`cmd-?` is not a keystroke".into()),
        })
        .unwrap();
    let refused = caller.next();
    assert_eq!(refused["id"], "2", "{refused}");
    assert_eq!(refused["error"]["code"], "failed");
    assert_eq!(refused["error"]["message"], "`cmd-?` is not a keystroke");

    never_handed(&plain.client);
}

#[test]
fn with_no_driven_window_a_window_action_is_refused_at_once() {
    let harness = start("drive-none");
    let _plain = attach(&harness.address);
    let mut caller = call(&harness);
    let refused = caller.answer("1", "window.key", json!({ "keys": ["cmd-,"] }));
    assert_eq!(refused["error"]["code"], "no_headless_window", "{refused}");
}

#[test]
fn a_driven_window_that_goes_mid_action_leaves_its_caller_refused() {
    let harness = start("drive-gone");
    let mut driven = attach_driven(&harness.address);
    let mut caller = call(&harness);
    caller.ask("1", "window.shot", json!({ "path": "/tmp/shot.png" }));
    handed(&driven);
    drop(driven);
    let refused = caller.next();
    assert_eq!(refused["id"], "1", "{refused}");
    assert_eq!(refused["error"]["code"], "failed");

    let none = caller.answer("2", "window.shot", json!({ "path": "/tmp/shot.png" }));
    assert_eq!(none["error"]["code"], "no_headless_window", "{none}");
}

//! Captured evidence crosses the owning window/Server boundary once, then
//! remains usable after the live page changes, including from remote windows.
use super::*;
use crate::browser::{Action, Protection};
use crate::browser_annotations::{Capture, Selection, Viewport};
use crate::protocol::WirePng;
use base64::Engine as _;

fn request(client: &mut Client, id: u64, op: Op) -> Result<Option<Done>, WireError> {
    let capture = matches!(&op, Op::BrowserCapture { .. });
    let message = ClientMessage::Command(Command { id, op });
    if capture {
        client.send_capture(message).unwrap();
    } else {
        client.send(&message).unwrap();
    }
    loop {
        if let ServerMessage::Reply { id: got, ok, error } = client
            .messages
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        {
            if got == id {
                return error.map_or(Ok(ok), Err);
            }
        }
    }
}

#[test]
fn browser_annotation_receipt_survives_navigation_and_fetches_only_retained_images() {
    let harness = start("ba-retain");
    let mut opener = attach(&harness.address);
    let (thread, _provider) = open(&mut opener, &harness);
    let mut owner_window = browser_test_window(&harness, thread);
    let opened = harness
        .browser
        .for_thread(thread)
        .submit(browser_test_action(Action::Open {
            url: "https://example.test/captured".into(),
        }))
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let owner = opened.state.owner.unwrap();
    let document = opened.state.tabs[0].document();
    let Some(Done::BrowserProtection { lease }) = request(
        &mut owner_window,
        901,
        Op::BrowserProtect {
            thread,
            owner,
            document,
            protection: Protection::Annotation,
        },
    )
    .unwrap() else {
        panic!("capture protection was not granted")
    };
    let png = include_bytes!("../../../tests/fixtures/browser-original.png");
    let capture = Capture {
        document,
        native_generation: 6,
        url: "https://example.test/captured".into(),
        viewport: Viewport {
            width_css: 1.0,
            height_css: 1.0,
            device_scale: 1.0,
        },
        selection: Selection::Region {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
    };
    let gate = harness.browser_disk.hold();
    owner_window
        .send_capture(ClientMessage::Command(Command {
            id: 902,
            op: Op::BrowserCapture {
                thread,
                owner,
                lease,
                capture: capture.clone(),
                png: WirePng {
                    base64: base64::engine::general_purpose::STANDARD.encode(png),
                    width: 1,
                    height: 1,
                },
            },
        }))
        .unwrap();
    // Cancellation acknowledges promptly, but cannot drop the protection
    // while the original image still waits for durable retention.
    request(
        &mut owner_window,
        903,
        Op::BrowserRelease {
            thread,
            owner,
            lease,
        },
    )
    .unwrap();
    assert!(request(
        &mut owner_window,
        904,
        Op::Browser {
            thread,
            action: Action::Navigate {
                document,
                url: "https://example.test/too-early".into()
            },
        }
    )
    .is_err());
    gate.send(()).unwrap();
    let annotation = loop {
        if let ServerMessage::Reply { id: 902, ok, error } = owner_window
            .messages
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        {
            assert!(error.is_none());
            let Some(Done::BrowserCapture { annotation }) = ok else {
                panic!("capture did not return retained evidence")
            };
            break annotation;
        }
    };
    request(
        &mut owner_window,
        904,
        Op::Browser {
            thread,
            action: Action::Navigate {
                document,
                url: "https://example.test/another-page".into(),
            },
        },
    )
    .unwrap();
    // Feedback refers to the retained capture, without requiring its old page.
    let feedback = annotation
        .with_feedback("Keep the old page as evidence".into(), vec![])
        .unwrap();
    assert_eq!(feedback.capture(), &capture);
    assert_eq!(feedback.original(), annotation.original());
    let Some(Done::BrowserCaptureImage { png: fetched }) = request(
        &mut owner_window,
        905,
        Op::BrowserCaptureImage {
            thread,
            hash: annotation.original_hash().to_owned(),
        },
    )
    .unwrap() else {
        panic!("remote history could not fetch retained evidence")
    };
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(fetched.base64)
            .unwrap(),
        png
    );
    assert!(request(
        &mut owner_window,
        906,
        Op::BrowserCaptureImage {
            thread,
            hash: "../log.jsonl".into()
        }
    )
    .is_err());
    opener.command(999, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
}

#[test]
fn browser_metadata_waits_off_pump_and_acknowledges_ordered_durable_results() {
    let harness = start("browser-disk-order");
    let mut opener = attach(&harness.address);
    let (thread, _provider) = open(&mut opener, &harness);
    let mut window = browser_test_window(&harness, thread);
    request(&mut window, 910, Op::BrowserAttach { thread }).unwrap();
    let gate = harness.browser_disk.hold();
    for id in [911, 912] {
        window
            .send(&ClientMessage::Command(Command {
                id,
                op: Op::Browser {
                    thread,
                    action: Action::Open {
                        url: format!("https://example.test/{id}"),
                    },
                },
            }))
            .unwrap();
    }
    // An ordinary command completes while the first durable write waits.
    window
        .send(&ClientMessage::Command(Command {
            id: 913,
            op: Op::SettingsChanged,
        }))
        .unwrap();
    loop {
        if let ServerMessage::Reply { id, .. } = window
            .messages
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        {
            assert_eq!(id, 913, "browser completion preceded its durable write");
            break;
        }
    }
    gate.send(()).unwrap();
    for expected in [911, 912] {
        loop {
            if let ServerMessage::Reply { id, ok, error } = window
                .messages
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
            {
                assert_eq!(id, expected);
                assert!(error.is_none());
                let Some(Done::Browser { state }) = ok else {
                    panic!("missing browser state")
                };
                assert_eq!(state.tabs.len(), (expected - 910) as usize);
                break;
            }
        }
    }
    let path = harness.dir.join("threads/browser-tabs.json");
    let saved = std::fs::read(&path).unwrap();
    let service =
        crate::browser::Service::restore(serde_json::from_slice(&saved).unwrap()).unwrap();
    assert_eq!(service.state(thread).tabs.len(), 2);
    let live = harness
        .browser
        .for_thread(thread)
        .submit(crate::browser::Operation::State)
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let owner = live.state.owner.unwrap();
    let document = live
        .state
        .tabs
        .iter()
        .find(|tab| Some(tab.id) == live.state.active)
        .unwrap()
        .document();
    let gate = harness.browser_disk.hold();
    window
        .send(&ClientMessage::Command(Command {
            id: 914,
            op: Op::Browser {
                thread,
                action: Action::Open {
                    url: "https://example.test/failed".into(),
                },
            },
        }))
        .unwrap();
    // The existing malformed-metadata safeguard must survive the worker move.
    std::fs::write(&path, b"malformed metadata").unwrap();
    let Some(Done::BrowserInput { revision }) = request(
        &mut window,
        915,
        Op::BrowserInput {
            thread,
            owner,
            document,
        },
    )
    .unwrap() else {
        panic!("input was not acknowledged while browser persistence waited")
    };
    assert_eq!(revision, 1);
    gate.send(()).unwrap();
    loop {
        if let ServerMessage::Reply { id: 914, ok, error } = window
            .messages
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        {
            assert!(ok.is_none());
            assert!(error.is_some());
            break;
        }
    }
    let state = harness
        .browser
        .for_thread(thread)
        .submit(crate::browser::Operation::State)
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(state.state.tabs.len(), 2);
    assert_eq!(
        state.revision, revision,
        "disk completion overwrote operator input"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"malformed metadata");
    opener.command(999, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
}

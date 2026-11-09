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
    let Some(Done::BrowserCapture { annotation }) = request(
        &mut owner_window,
        902,
        Op::BrowserCapture {
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
    )
    .unwrap() else {
        panic!("capture did not return retained evidence")
    };
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

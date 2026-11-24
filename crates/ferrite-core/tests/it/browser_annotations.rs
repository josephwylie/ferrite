use ferrite_core::browser::{Document, TabId};
use ferrite_core::browser_annotations::{self, Capture, Selection, Viewport};

// A captured original survives leaving the page, and the exact annotation can
// be restored from the same text the queue, providers and history receive.
#[test]
fn captured_feedback_survives_prompt_roundtrip_without_a_live_page() {
    let root = std::env::temp_dir().join(format!("ferrite-annotation-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let png = include_bytes!("../fixtures/browser-original.png");
    let capture = Capture {
        document: Document {
            tab: TabId(7),
            generation: 3,
        },
        native_generation: 8,
        url: "https://example.test/form".into(),
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
    let annotation =
        browser_annotations::retain(&root, capture, png, "Make this clearer".into(), vec![])
            .unwrap();
    let path = annotation.original().unwrap().to_path_buf();
    // Attachments are absolute; a Unix-style path is not one on Windows.
    let notes = root.join("notes.txt");
    let prompt = browser_annotations::compose(
        "Fix this",
        std::slice::from_ref(&notes),
        &[annotation.clone()],
    );
    assert!(prompt.contains("Make this clearer"));
    assert!(prompt.contains("https://example.test/form"));
    assert!(ferrite_core::prompt_files::paths(&prompt, None).contains(&path));
    let (text, files, restored) = browser_annotations::split(prompt);
    assert_eq!(text, "Fix this");
    assert_eq!(files, [notes]);
    assert_eq!(restored, [annotation]);
    assert_eq!(std::fs::read(restored[0].original().unwrap()).unwrap(), png);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn sent_annotation_reopens_through_the_thread_store() {
    use ferrite_core::{
        checkout::WorkspaceBinding,
        provider::Provider,
        store::Store,
        transcript::Input,
    };
    let root = std::env::temp_dir().join(format!("ferrite-browser-history-{}", std::process::id()));
    let store = Store::open(&root).unwrap();
    let capture = Capture {
        document: Document {
            tab: TabId(11),
            generation: 9,
        },
        native_generation: 4,
        url: "https://example.test/before-navigation".into(),
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
    let png = include_bytes!("../fixtures/browser-original.png");
    let annotation = browser_annotations::retain(
        store.dir(),
        capture,
        png,
        "Keep this feedback".into(),
        vec![],
    )
    .unwrap();
    let prompt = browser_annotations::compose("Update", &[], &[annotation.clone()]);
    let (thread, mut writer) = store
        .create(
            Provider::Claude,
            None,
            WorkspaceBinding::Main {
                checkout: root.clone(),
            },
        )
        .unwrap();
    writer.record_prompt(&prompt).unwrap();
    writer.flush_fully().unwrap();
    writer.close();
    let replay = store.load(thread).unwrap().inputs();
    let restored = replay
        .into_iter()
        .find_map(|input| match input {
            Input::Prompt(text) => Some(text),
            _ => None,
        })
        .unwrap();
    let (words, _, annotations) = browser_annotations::split(restored);
    assert_eq!(words, "Update");
    assert_eq!(annotations, [annotation]);
    assert_eq!(
        std::fs::read(annotations[0].original().unwrap()).unwrap(),
        png
    );
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

fn live_capture(screenshot: browser_annotations::LiveScreenshot) -> Capture {
    use browser_annotations::{LiveFeedback, Rect, RegionTarget};
    Capture {
        document: Document {
            tab: TabId(7),
            generation: 3,
        },
        native_generation: 4,
        url: "https://example.test/live".into(),
        viewport: Viewport {
            width_css: 10.,
            height_css: 10.,
            device_scale: 1.,
        },
        selection: Selection::Live {
            feedback: LiveFeedback {
                id: "annotation_1".into(),
                page_url: "https://example.test/live".into(),
                page_title: Some("Live page".into()),
                comment: "Use the new spacing".into(),
                elements: vec![],
                regions: vec![RegionTarget {
                    id: "region_1".into(),
                    rect: Rect {
                        x: 2.,
                        y: 3.,
                        width: 1.,
                        height: 1.,
                    },
                }],
                strokes: vec![],
                style_changes: vec![],
                screenshot,
                bounds: Rect {
                    x: 2.,
                    y: 3.,
                    width: 1.,
                    height: 1.,
                },
                created_at: "2026-11-16T00:00:00.000Z".into(),
            },
        },
    }
}

#[test]
fn unavailable_live_screenshot_retains_context_without_an_image_attachment() {
    let root = std::env::temp_dir().join(format!("ferrite-live-context-{}", std::process::id()));
    let capture = live_capture(browser_annotations::LiveScreenshot::Unavailable {
        reason: "Targets are outside the viewport".into(),
    });
    let annotation =
        browser_annotations::retain_optional(&root, capture, None, String::new(), vec![]).unwrap();
    assert!(annotation.original().is_none());
    assert!(annotation.original_hash().is_none());
    assert!(annotation.pixel_size().is_none());
    assert_eq!(annotation.comment(), "Use the new spacing");
    let prompt = browser_annotations::compose("Review the page", &[], &[annotation.clone()]);
    let (text, images, restored) = browser_annotations::split(prompt);
    assert_eq!(text, "Review the page");
    assert!(images.is_empty());
    assert_eq!(restored, vec![annotation]);
    assert!(!root.exists());
}

#[test]
fn live_screenshot_uses_crop_dimensions_and_rejects_missing_image_bytes() {
    let root = std::env::temp_dir().join(format!("ferrite-live-crop-{}", std::process::id()));
    let capture = live_capture(browser_annotations::LiveScreenshot::Captured {
        crop: browser_annotations::Rect {
            x: 2.,
            y: 3.,
            width: 1.,
            height: 1.,
        },
    });
    let png = include_bytes!("../fixtures/browser-original.png");
    let annotation = browser_annotations::retain_optional(
        &root,
        capture.clone(),
        Some(png),
        String::new(),
        vec![],
    )
    .unwrap();
    assert_eq!(annotation.pixel_size(), Some((1, 1)));
    assert!(
        browser_annotations::retain_optional(&root, capture, None, String::new(), vec![]).is_err()
    );
    let mut wrong_url = annotation.capture().clone();
    wrong_url.url = "https://example.test/another-document".into();
    assert!(browser_annotations::retain_optional(
        &root,
        wrong_url,
        Some(png),
        String::new(),
        vec![]
    )
    .is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn live_feedback_rejects_duplicate_targets_and_cross_document_elements() {
    let capture = live_capture(browser_annotations::LiveScreenshot::Unavailable {
        reason: "No visible crop".into(),
    });
    let Selection::Live { mut feedback } = capture.selection else {
        unreachable!()
    };
    feedback.regions.push(feedback.regions[0].clone());
    assert!(browser_annotations::validate_live_feedback(&feedback).is_err());
}

#[test]
fn legacy_receipt_image_fields_still_deserialize() {
    let root = std::env::temp_dir().join(format!("ferrite-legacy-receipt-{}", std::process::id()));
    let mut capture = live_capture(browser_annotations::LiveScreenshot::Unavailable {
        reason: "unused".into(),
    });
    capture.viewport.width_css = 1.;
    capture.viewport.height_css = 1.;
    capture.selection = Selection::Region {
        x: 0.,
        y: 0.,
        width: 1.,
        height: 1.,
    };
    let annotation = browser_annotations::retain(
        &root,
        capture,
        include_bytes!("../fixtures/browser-original.png"),
        "Legacy".into(),
        vec![],
    )
    .unwrap();
    let old_wire = serde_json::to_value(&annotation).unwrap();
    assert!(old_wire["original"].is_string());
    assert!(old_wire["pixel_width"].is_number());
    let restored: browser_annotations::Annotation = serde_json::from_value(old_wire).unwrap();
    assert_eq!(restored.original(), annotation.original());
    assert_eq!(restored.pixel_size(), Some((1, 1)));
    std::fs::remove_dir_all(root).unwrap();
}

// The window's display copy of a capture is a cache: a week after it was
// written the next caching drops it, and opening that capture writes it again.
#[test]
fn a_cached_capture_goes_a_week_after_it_was_written() {
    let root = std::env::temp_dir().join(format!("ferrite-capture-cache-{}", std::process::id()));
    let png = include_bytes!("../fixtures/browser-original.png");
    let mut capture = live_capture(browser_annotations::LiveScreenshot::Unavailable {
        reason: "unused".into(),
    });
    capture.viewport.width_css = 1.;
    capture.viewport.height_css = 1.;
    capture.selection = Selection::Region {
        x: 0.,
        y: 0.,
        width: 1.,
        height: 1.,
    };
    let annotation =
        browser_annotations::retain(&root, capture, png, "Cached".into(), vec![]).unwrap();
    let cached = annotation.cache_image(png).unwrap();
    let old = cached.with_file_name("0".repeat(64) + ".png");
    std::fs::write(&old, png).unwrap();
    let week_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(7 * 24 * 60 * 60);
    for stale in [&old, &cached] {
        std::fs::File::options().write(true).open(stale).unwrap().set_modified(week_ago).unwrap();
    }
    assert_eq!(annotation.cache_image(png).unwrap(), cached);
    assert!(!old.exists(), "a week-old cached capture outlived the next caching");
    assert!(cached.is_file(), "opening the capture wrote it again");
    assert!(annotation.cached_image().is_some());
    std::fs::remove_dir_all(root).unwrap();
}

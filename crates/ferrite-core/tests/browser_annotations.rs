use ferrite_core::browser::{Document, TabId};
use ferrite_core::browser_annotations::{self, Capture, Selection, Viewport};
use std::path::PathBuf;

// A captured original survives leaving the page, and the exact annotation can
// be restored from the same text the queue, providers and history receive.
#[test]
fn captured_feedback_survives_prompt_roundtrip_without_a_live_page() {
    let root = std::env::temp_dir().join(format!("ferrite-annotation-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let png = include_bytes!("fixtures/browser-original.png");
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
    let path = annotation.original().to_path_buf();
    let prompt = browser_annotations::compose(
        "Fix this",
        &[PathBuf::from("/tmp/notes.txt")],
        &[annotation.clone()],
    );
    assert!(prompt.contains("Make this clearer"));
    assert!(prompt.contains("https://example.test/form"));
    assert!(ferrite_core::prompt_files::paths(&prompt, None).contains(&path));
    let (text, files, restored) = browser_annotations::split(prompt);
    assert_eq!(text, "Fix this");
    assert_eq!(files, [PathBuf::from("/tmp/notes.txt")]);
    assert_eq!(restored, [annotation]);
    assert_eq!(std::fs::read(restored[0].original()).unwrap(), png);
    std::fs::remove_dir_all(root).unwrap();
}

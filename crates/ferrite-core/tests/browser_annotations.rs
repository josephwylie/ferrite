use ferrite_core::browser::{Document, TabId};
use ferrite_core::browser_annotations::{self, Capture, Selection, Viewport};

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
    assert_eq!(std::fs::read(restored[0].original()).unwrap(), png);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn sent_annotation_reopens_through_the_thread_store() {
    use ferrite_core::{
        store::{Provider, Store},
        transcript::Input,
        workspace::WorkspaceBinding,
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
    let png = include_bytes!("fixtures/browser-original.png");
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
    let replay = Store::reader(&root).unwrap().load(thread).unwrap().inputs();
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
    assert_eq!(std::fs::read(annotations[0].original()).unwrap(), png);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

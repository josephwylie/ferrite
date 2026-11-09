use super::*;
use ferrite_core::{
    browser::{Document, TabId},
    browser_annotations::{self, Capture, Selection, Viewport},
};
use gpui::{Modifiers, TestAppContext};

#[gpui::test]
fn annotation_editor_pointer_feedback_preserves_capture(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("ferrite-editor-{}", std::process::id()));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        1000,
        500,
        image::Rgba([240, 240, 240, 255]),
    ))
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let png = png.into_inner();
    let retained = browser_annotations::retain(
        &root,
        Capture {
            document: Document {
                tab: TabId(12),
                generation: 9,
            },
            native_generation: 7,
            url: "https://example.test/retained".into(),
            viewport: Viewport {
                width_css: 800.,
                height_css: 400.,
                device_scale: 1.25,
            },
            selection: Selection::Region {
                x: 20.,
                y: 30.,
                width: 90.,
                height: 60.,
            },
        },
        &png,
        String::new(),
        vec![],
    )
    .unwrap();
    let original = retained.clone();
    let (editor, cx) = cx.add_window_view(|_, cx| {
        AnnotationEditor::new(retained.clone(), retained.original().into(), cx)
    });
    cx.simulate_resize(size(px(600.), px(700.)));
    let rectangle = cx
        .debug_bounds("annotation-tool-Rectangle")
        .expect("Rectangle tool");
    cx.simulate_click(rectangle.center(), Modifiers::none());
    let bounds = editor.read_with(cx, |editor, _| editor.picture_bounds().unwrap());
    let from = bounds.origin + point(bounds.size.width * 0.25, bounds.size.height * 0.25);
    let to = bounds.origin + point(bounds.size.width * 0.75, bounds.size.height * 0.75);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.drawings(),
            &[Drawing::Rectangle {
                bounds: Rect {
                    x: 200.,
                    y: 100.,
                    width: 400.,
                    height: 200.
                },
                ink: Ink {
                    color: "#f3767b".into(),
                    width: 3.
                }
            }]
        );
    });
    editor.update(cx, |editor, cx| {
        editor.undo(cx);
        assert!(editor.drawings().is_empty());
        editor.redo(cx);
    });
    // Every drawing tool reaches the same public feedback collection through
    // its real toolbar and pointer handlers; no native BrowserView is involved.
    let blue = cx.debug_bounds("annotation-ink-Blue").unwrap();
    cx.simulate_click(blue.center(), Modifiers::none());
    let width = cx.debug_bounds("annotation-width-6").unwrap();
    cx.simulate_click(width.center(), Modifiers::none());
    for name in ["Pen", "Line", "Arrow", "Ellipse", "Steps"] {
        let tool = cx.debug_bounds(format!("annotation-tool-{name}")).unwrap();
        cx.simulate_click(tool.center(), Modifiers::none());
        let bounds = editor.read_with(cx, |editor, _| editor.picture_bounds().unwrap());
        let from = bounds.origin + point(bounds.size.width * 0.25, bounds.size.height * 0.25);
        let to = bounds.origin + point(bounds.size.width * 0.75, bounds.size.height * 0.75);
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
        editor.read_with(cx, |editor, _| {
            let ink = match (name, editor.drawings().last().unwrap()) {
                ("Pen", Drawing::Pen { points, ink }) => {
                    assert!(points.len() >= 2);
                    assert_eq!(points.first(), Some(&EvidencePoint { x: 200., y: 100. }));
                    assert_eq!(points.last(), Some(&EvidencePoint { x: 600., y: 300. }));
                    ink
                }
                ("Line", Drawing::Line { from, to, ink })
                | ("Arrow", Drawing::Arrow { from, to, ink }) => {
                    assert_eq!(from, &EvidencePoint { x: 200., y: 100. });
                    assert_eq!(to, &EvidencePoint { x: 600., y: 300. });
                    ink
                }
                ("Ellipse", Drawing::Ellipse { bounds, ink }) => {
                    assert_eq!(
                        bounds,
                        &Rect {
                            x: 200.,
                            y: 100.,
                            width: 400.,
                            height: 200.
                        }
                    );
                    ink
                }
                ("Steps", Drawing::Step { at, number, ink }) => {
                    assert_eq!(at, &EvidencePoint { x: 200., y: 100. });
                    assert_eq!(*number, 1);
                    ink
                }
                (_, drawing) => panic!("{name} produced {drawing:?}"),
            };
            assert_eq!(
                ink,
                &Ink {
                    color: "#8cb9f4".into(),
                    width: 6.
                }
            );
        });
    }
    let feedback = Rc::new(std::cell::RefCell::new(Vec::new()));
    let cancelled = Rc::new(Cell::new(false));
    let captured = feedback.clone();
    let cancellation = cancelled.clone();
    let _subscription = editor.update(cx, |_, cx| {
        cx.subscribe(&editor, move |_, _, event, _| match event {
            EditorEvent::Added { comment, drawings } => captured
                .borrow_mut()
                .push((comment.clone(), drawings.clone())),
            EditorEvent::Cancelled => cancellation.set(true),
        })
    });
    let text = cx.debug_bounds("annotation-tool-Text").unwrap();
    cx.simulate_click(text.center(), Modifiers::none());
    let label = cx.debug_bounds("annotation-label").unwrap();
    cx.simulate_click(label.center(), Modifiers::none());
    cx.simulate_input("Move this label");
    let bounds = editor.read_with(cx, |editor, _| editor.picture_bounds().unwrap());
    cx.simulate_click(bounds.center(), Modifiers::none());
    let comment = cx.debug_bounds("annotation-comment").unwrap();
    cx.simulate_click(comment.center(), Modifiers::none());
    cx.simulate_input("Make this clearer");
    let add = cx.debug_bounds("annotation-add").unwrap();
    cx.simulate_click(add.center(), Modifiers::none());
    assert_eq!(feedback.borrow().len(), 1);
    let feedback = feedback.borrow();
    assert_eq!(feedback[0].0, "Make this clearer");
    assert_eq!(feedback[0].1.len(), 7);
    assert!(
        matches!(&feedback[0].1[6], Drawing::Text { at, text, .. } if text == "Move this label" && (at.x - 400.).abs() < 0.001 && (at.y - 200.).abs() < 0.001)
    );
    let cancel = cx.debug_bounds("annotation-cancel").unwrap();
    cx.simulate_click(cancel.center(), Modifiers::none());
    assert!(cancelled.get());
    assert_eq!(retained, original);
    assert_eq!(std::fs::read(retained.original()).unwrap(), png);
    std::fs::remove_dir_all(root).unwrap();
}

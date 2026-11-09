use super::*;
use gpui::{TestAppContext, Modifiers};
use ferrite_core::{browser::{Document, TabId}, browser_annotations::{self, Capture, Selection, Viewport}};

#[gpui::test]
fn annotation_editor_pointer_feedback_preserves_capture(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("ferrite-editor-{}", std::process::id()));
    let png = include_bytes!("../../../ferrite-core/tests/fixtures/browser-original.png");
    let retained = browser_annotations::retain(&root, Capture {
        document: Document { tab: TabId(12), generation: 9 }, native_generation: 7,
        url: "https://example.test/retained".into(),
        viewport: Viewport { width_css: 800., height_css: 400., device_scale: 1.25 },
        selection: Selection::Region { x: 20., y: 30., width: 90., height: 60. },
    }, png, String::new(), vec![]).unwrap();
    let original = retained.clone();
    let (editor, cx) = cx.add_window_view(|_, cx| AnnotationEditor::new(retained.clone(), retained.original().into(), cx));
    cx.simulate_resize(size(px(600.), px(700.)));
    editor.update(cx, |editor, cx| editor.select_tool(Tool::Rectangle, cx));
    let bounds = editor.read_with(cx, |editor, _| editor.picture_bounds().unwrap());
    let from = bounds.origin + point(bounds.size.width * 0.25, bounds.size.height * 0.25);
    let to = bounds.origin + point(bounds.size.width * 0.75, bounds.size.height * 0.75);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.drawings(), &[Drawing::Rectangle { bounds: Rect { x: 200., y: 100., width: 400., height: 200. }, ink: Ink { color: "#f3767b".into(), width: 3. } }]);
    });
    editor.update(cx, |editor, cx| { editor.undo(cx); assert!(editor.drawings().is_empty()); editor.redo(cx); });
    assert_eq!(retained, original);
    assert_eq!(std::fs::read(retained.original()).unwrap(), png);
    std::fs::remove_dir_all(root).unwrap();
}

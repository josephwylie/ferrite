//! The Files pane (`cockpit/tools/files.rs`): the tree, the editor, its
//! tabs and menus, and the paths that open there.
use super::*;
use crate::cockpit::context_menu::Submenu;
use crate::cockpit::tools::{FilesSub, FilesVerb, Tool};

fn files_window<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadId, std::path::PathBuf, &'a mut gpui::VisualTestContext) {
    let (core, _fake, checkout) = bound_cockpit(name, Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, checkout, cx)
}

fn open(view: &Entity<CockpitView>, thread: ThreadId, path: &std::path::Path, keep: bool, cx: &mut gpui::VisualTestContext) {
    let path = path.to_path_buf();
    view.update(cx, |view, cx| view.open_in_files(thread, path, None, keep, cx));
    tick(cx);
}

fn tab_names(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) -> Vec<(String, bool)> {
    view.read_with(cx, |view, _| view.files_tab_names(thread))
}

#[gpui::test]
fn the_door_opens_files_with_its_empty_word_and_the_tree(cx: &mut TestAppContext) {
    let (view, _thread, _checkout, cx) = files_window("files-door", cx);
    view.update(cx, |view, cx| view.press_tool(Tool::Files, cx));
    tick(cx);
    assert!(cx.debug_bounds("files-empty").is_some(), "Open files appear here");
    assert!(cx.debug_bounds("files-filter-placeholder").is_some());
    // Folders first: `src`, then README.md.
    assert!(cx.debug_bounds("files-row-0").is_some());
    assert!(cx.debug_bounds("files-row-1").is_some());
    assert!(cx.debug_bounds("files-row-2").is_none(), "src is closed");
    let row = cx.debug_bounds("files-row-0").unwrap();
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("files-row-2").is_some(), "a press opens the folder");
}

#[gpui::test]
fn a_press_previews_and_the_next_replaces_it_until_kept(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-preview", cx);
    open(&view, thread, &checkout.join("src/lib.rs"), false, cx);
    assert_eq!(tab_names(&view, thread, cx), [("lib.rs".to_string(), true)]);
    open(&view, thread, &checkout.join("src/lib.rs"), true, cx);
    assert_eq!(tab_names(&view, thread, cx), [("lib.rs".to_string(), false)], "kept");
    std::fs::write(checkout.join("b.rs"), "b\n").unwrap();
    std::fs::write(checkout.join("c.rs"), "c\n").unwrap();
    open(&view, thread, &checkout.join("b.rs"), false, cx);
    open(&view, thread, &checkout.join("c.rs"), false, cx);
    assert_eq!(
        tab_names(&view, thread, cx),
        [("lib.rs".to_string(), false), ("c.rs".to_string(), true)],
        "the preview tab is replaced, the kept one stays"
    );
    assert!(cx.debug_bounds("files-breadcrumb").is_some());
    let close = cx.debug_bounds("files-tab-close-1").expect("the preview tab's x");
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(tab_names(&view, thread, cx), [("lib.rs".to_string(), false)], "closed");
}

#[gpui::test]
fn typing_edits_the_file_and_auto_save_writes_it_after_a_pause(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-edit", cx);
    let file = checkout.join("src/lib.rs");
    open(&view, thread, &file, false, cx);
    let editor = view.read_with(cx, |view, _| view.files_editor_of(thread).expect("a code file"));
    editor.update(cx, |editor, cx| editor.type_text("// hi\n", cx));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "l\n", "not yet");
    assert_eq!(
        tab_names(&view, thread, cx),
        [("lib.rs".to_string(), false)],
        "typing keeps a preview tab"
    );
    cx.executor().advance_clock(crate::cockpit::tools::AUTO_SAVE_AFTER);
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "// hi\nl\n");
    assert!(!editor.read_with(cx, |editor, _| editor.is_dirty()));
}

#[gpui::test]
fn prose_reads_rendered_not_in_the_code_face(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-prose", cx);
    open(&view, thread, &checkout.join("README.md"), false, cx);
    assert!(cx.debug_bounds("files-prose").is_some());
    assert!(view.read_with(cx, |view, _| view.files_editor_of(thread).is_none()));
}

#[gpui::test]
fn a_file_the_preview_opens_opens_here_instead(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-redirect", cx);
    let file = checkout.join("src/lib.rs");
    view.update_in(cx, |view, window, cx| {
        view.panes[0].preview.open(file.clone(), Some(1), &[], window, cx)
    });
    tick(cx);
    assert!(view.read_with(cx, |view, _| !view.panes[0].preview.is_open()));
    assert_eq!(tab_names(&view, thread, cx), [("lib.rs".to_string(), true)]);
    assert!(view.read_with(cx, |view, _| view.tool_pressed(Tool::Files)));
}

#[gpui::test]
fn the_filter_lists_matches_and_a_row_menu_copies_paths(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-menu", cx);
    view.update(cx, |view, cx| view.press_tool(Tool::Files, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.set_files_filter_for_test(thread, "lib", cx);
    });
    tick(cx);
    assert!(cx.debug_bounds("files-row-0").is_some());
    assert!(cx.debug_bounds("files-row-1").is_none(), "only lib.rs matches");
    let row = cx.debug_bounds("files-row-0").unwrap();
    cx.simulate_mouse_down(row.center(), gpui::MouseButton::Right, gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        let menu = view.context_menu.as_ref().expect("the row's menu");
        let names: Vec<_> = menu
            .rows
            .iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect();
        assert_eq!(names, ["Attach as context", "Copy", "Show in Finder"]);
    });
    let lib = checkout.join("src/lib.rs");
    for (verb, expect) in [
        (FilesVerb::CopyAbsolute, lib.to_string_lossy().into_owned()),
        (FilesVerb::CopyRelative, "src/lib.rs".to_string()),
        (FilesVerb::CopyName, "lib.rs".to_string()),
    ] {
        view.update(cx, |view, cx| view.run_files_verb(thread, verb, cx));
        assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), Some(expect));
    }
    view.read_with(cx, |view, _| {
        let sub = view.context_submenu_rows_for_test(MenuTarget::File(thread), Submenu::Files(FilesSub::Copy));
        assert_eq!(sub.len(), 3, "Absolute path, Relative path, Filename");
    });
    view.update(cx, |view, cx| view.run_files_verb(thread, FilesVerb::Attach, cx));
    assert_eq!(
        view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).attachment_count()),
        1,
        "Attach as context"
    );
}

#[gpui::test]
fn the_more_menu_holds_only_its_five_settings_and_they_take_effect(cx: &mut TestAppContext) {
    let (view, thread, _checkout, cx) = files_window("files-more", cx);
    view.update(cx, |view, cx| view.press_tool(Tool::Files, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::FilesMore(thread), gpui::point(px(900.), px(100.)), cx)
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        let rows = &view.context_menu.as_ref().unwrap().rows;
        let names: Vec<_> = rows.iter().flatten().map(|(item, _)| item.label.to_string()).collect();
        assert_eq!(
            names,
            ["Show file tree", "Preview tabs", "Auto save", "Word wrap", "Tab size"]
        );
        let checked: Vec<_> = rows.iter().flatten().map(|(item, _)| item.checked).collect();
        assert_eq!(checked, [true, true, true, true, false]);
    });
    view.update(cx, |view, cx| view.run_files_verb(thread, FilesVerb::ToggleTree, cx));
    tick(cx);
    assert!(cx.debug_bounds("files-tree-column").is_none(), "the tree hides");
    view.update(cx, |view, cx| view.run_files_verb(thread, FilesVerb::ToggleTree, cx));
    tick(cx);
    assert!(cx.debug_bounds("files-tree-column").is_some());
}

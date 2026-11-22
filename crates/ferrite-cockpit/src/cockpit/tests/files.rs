//! The Files pane (`cockpit/tools/files.rs`): the tree, the editor, its
//! tabs and menus, and the paths that open there.
use super::*;
use crate::cockpit::context_menu::Submenu;
use crate::cockpit::panel::Opener;
use crate::cockpit::surfaces::SurfaceId;
use crate::cockpit::tools::{Diff, FilesSub, FilesVerb};

fn files_window<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadKey, std::path::PathBuf, &'a mut gpui::VisualTestContext) {
    let (core, _fake, checkout) = bound_cockpit(name, Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, checkout, cx)
}

fn open(view: &Entity<CockpitView>, thread: ThreadKey, path: &std::path::Path, keep: bool, cx: &mut gpui::VisualTestContext) {
    let path = path.to_path_buf();
    view.update(cx, |view, cx| view.open_in_files(thread, path, None, keep, cx));
    tick(cx);
}

fn tab_names(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) -> Vec<(String, bool)> {
    view.read_with(cx, |view, cx| view.files.read(cx).tab_names(thread))
}

#[gpui::test]
fn the_door_opens_files_with_its_empty_word_and_the_tree(cx: &mut TestAppContext) {
    let (view, thread, _checkout, cx) = files_window("files-door", cx);
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Files, cx));
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
    let editor = view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).expect("a code file"));
    type_into("// hi\n", cx);
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
fn typing_replaces_selected_file_text_through_platform_input(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-replace-selection", cx);
    let file = checkout.join("src/lib.rs");
    open(&view, thread, &file, false, cx);
    type_into("", cx);
    cx.simulate_keystrokes("cmd-a");
    tick(cx);
    cx.simulate_input("replacement");
    tick(cx);
    cx.simulate_keystrokes("cmd-s");
    tick(cx);
    assert_eq!(std::fs::read_to_string(file).unwrap(), "replacement");
}

#[gpui::test]
fn prose_reads_rendered_until_its_source_is_asked_for(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-prose", cx);
    open(&view, thread, &checkout.join("README.md"), false, cx);
    assert!(cx.debug_bounds("files-prose").is_some());
    assert!(view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).is_none()));
    let toggle = cx.debug_bounds("files-show-source").expect("Show source");
    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("files-prose").is_none());
    assert!(cx.debug_bounds("editor").is_some());
    assert!(view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).is_some()));
}

#[gpui::test]
fn prose_edits_in_its_source_saves_and_reads_the_edit_rendered(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-prose-edit", cx);
    for name in ["README.md", "notes.txt"] {
        let file = checkout.join(name);
        std::fs::write(&file, "start\n").unwrap();
        open(&view, thread, &file, false, cx);
        let toggle = cx.debug_bounds("files-show-source").expect("Show source");
        cx.simulate_click(toggle.center(), gpui::Modifiers::none());
        tick(cx);
        type_into("// hi\n", cx);
        pause_and_settle(cx);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "// hi\nstart\n", "{name} saved");
        let toggle = cx.debug_bounds("files-show-rendered").expect("Show rendered");
        cx.simulate_click(toggle.center(), gpui::Modifiers::none());
        tick(cx);
        assert!(cx.debug_bounds("files-prose").is_some(), "{name} reads rendered again");
        let markup = view.read_with(cx, |view, cx| view.files.read(cx).rendered_markup(thread));
        assert!(markup.is_some_and(|markup| markup.contains("hi")), "{name} shows the edit");
    }
}

#[gpui::test]
fn a_line_for_a_markdown_file_opens_its_source_there(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-prose-line", cx);
    let file = checkout.join("README.md");
    let text: String = (1..=120).map(|line| format!("line {line}\n")).collect();
    std::fs::write(&file, text).unwrap();
    open(&view, thread, &file, false, cx);
    assert!(cx.debug_bounds("files-prose").is_some());
    view.update(cx, |view, cx| view.open_in_files(thread, file.clone(), Some(80), false, cx));
    tick(cx);
    assert!(cx.debug_bounds("files-prose").is_none(), "the source shows the line");
    let editor = view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).expect("source"));
    assert_eq!(editor.read_with(cx, |editor, _| editor.caret_line_for_test()), 80);
    let toggle = cx.debug_bounds("files-show-rendered").expect("back to rendered");
    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("files-prose").is_some());
    assert_eq!(tab_names(&view, thread, cx).len(), 1, "the file stayed one tab");
}

#[gpui::test]
fn an_image_from_the_tree_opens_the_lightbox_and_a_pdf_names_its_kind(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-media", cx);
    image::RgbaImage::from_pixel(8, 8, image::Rgba([90, 140, 200, 255]))
        .save(checkout.join("a.png"))
        .unwrap();
    std::fs::write(checkout.join("b.pdf"), [0x25, 0x50, 0x44, 0x46, 0xff, 0xfe, 0x00]).unwrap();
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Files, cx));
    tick(cx);
    let row = |name: &str, view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        let at = view
            .read_with(cx, |view, cx| view.files.read(cx).row_names(thread))
            .iter()
            .position(|row| row == name)
            .unwrap_or_else(|| panic!("{name} in the tree"));
        cx.debug_bounds(Box::leak(format!("files-row-{at}").into_boxed_str())).unwrap().center()
    };
    let at = row("a.png", &view, cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.panes[0].preview.lightbox().is_some()));
    assert!(tab_names(&view, thread, cx).is_empty(), "no tab for an image");
    view.update(cx, |view, cx| {
        view.panes[0].preview.close_lightbox();
        cx.notify();
    });
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.panes[0].preview.lightbox().is_none()), "closed");
    let at = row("b.pdf", &view, cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    tick(cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    tick(cx);
    assert_eq!(tab_names(&view, thread, cx).len(), 1, "the pdf has a tab");
    assert!(cx.debug_bounds("files-unshown").is_some());
    assert!(cx.debug_bounds("files-open-original").is_some(), "its own app is offered");
    assert_eq!(
        view.read_with(cx, |view, cx| view.files.read(cx).unshown_note(thread)).as_deref(),
        Some("PDF document")
    );
}

/// Whether `selector` sits fully inside the Files tabs row.
fn inside_the_row(selector: &'static str, cx: &mut gpui::VisualTestContext) -> Option<bool> {
    let strip = cx.debug_bounds("files-tabs")?;
    let bounds = cx.debug_bounds(selector)?;
    Some(bounds.left() >= strip.left() - px(1.) && bounds.right() <= strip.right() + px(1.))
}

/// Twenty tabs at a narrow width, scrolled by the chevron back to the first.
fn twenty_tabs<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadKey, std::path::PathBuf, &'a mut gpui::VisualTestContext) {
    let (view, thread, checkout, cx) = files_window(name, cx);
    cx.simulate_resize(gpui::size(px(1300.), px(900.)));
    for at in 0..20 {
        let file = checkout.join(format!("file-number-{at}.rs"));
        std::fs::write(&file, "x\n").unwrap();
        open(&view, thread, &file, true, cx);
    }
    tick(cx);
    (view, thread, checkout, cx)
}

/// F6 comes back to the file tab it left, by the file: a tab closed before
/// it moves it along the row, and the keyboard follows the file.
#[gpui::test]
fn f6_returns_to_the_file_tab_it_left_after_one_before_it_closes(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-f6-file", cx);
    for name in ["a.rs", "b.rs", "c.rs"] {
        let file = checkout.join(name);
        std::fs::write(&file, "x\n").unwrap();
        open(&view, thread, &file, true, cx);
    }
    let keyboard = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| view.read(cx).keyboard_label(window, cx).map(|name| name.to_string()))
    };
    let b = view.read_with(cx, |view, _| {
        let drawn = view.panel.focus.drawn();
        let (control, _) = drawn.iter().find(|(_, semantics)| semantics.name == "b.rs").unwrap();
        view.panel.focus.handle(control).unwrap()
    });
    cx.update(|window, cx| window.focus(&b, cx));
    tick(cx);
    assert_eq!(keyboard(cx).as_deref(), Some("b.rs"));
    super::panel_focus::press("escape", cx);
    assert_eq!(keyboard(cx), None, "back in the conversation");

    view.update(cx, |view, cx| view.files.update(cx, |files, cx| files.close_tab(thread, 0, cx)));
    tick(cx);
    assert_eq!(tab_names(&view, thread, cx).len(), 2, "a.rs closed");
    super::panel_focus::press("f6", cx);
    assert_eq!(keyboard(cx).as_deref(), Some("b.rs"), "the file it left, not its place");
}

#[gpui::test]
fn twenty_tabs_at_a_narrow_width_are_all_reachable(cx: &mut TestAppContext) {
    use crate::cockpit::panel::PanelControl;
    let (view, thread, checkout, cx) = twenty_tabs("files-twenty", cx);
    assert_eq!(inside_the_row("files-tab-19", cx), Some(true), "the shown tab is in view");
    assert!(cx.debug_bounds("files-tabs-left").is_some(), "more lies to the left");
    let left = cx.debug_bounds("files-tabs-left").unwrap();
    for _ in 0..40 {
        if cx.debug_bounds("files-tabs-left").is_none() {
            break;
        }
        cx.simulate_click(left.center(), gpui::Modifiers::none());
        cx.executor().advance_clock(std::time::Duration::from_millis(500));
        tick(cx);
    }
    assert_eq!(inside_the_row("files-tab-0", cx), Some(true), "the chevron reaches the first");
    let first = cx.debug_bounds("files-tab-0").unwrap();
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(view.read_with(cx, |view, cx| view.files.read(cx).active_path(thread)), Some(checkout.join("file-number-0.rs")));

    let first = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        let handle = view.read_with(cx, |view, _| view.panel.focus.handle(&PanelControl::FileTab(checkout.join("file-number-0.rs"))).unwrap());
        cx.update(|window, cx| window.focus(&handle, cx));
    };
    first(&view, cx);
    for _ in 0..19 {
        super::panel_focus::press("right", cx);
    }
    let focused = cx.update(|window, cx| view.read(cx).panel.focus.focused(window));
    assert_eq!(focused, Some(PanelControl::FileTab(checkout.join("file-number-19.rs"))));
    assert_eq!(inside_the_row("files-tab-19", cx), Some(true), "the arrow keys never park focus off screen");

    first(&view, cx);
    super::panel_focus::press("right right", cx);
    super::panel_focus::activate("enter", cx);
    assert_eq!(view.read_with(cx, |view, cx| view.files.read(cx).active_path(thread)), Some(checkout.join("file-number-2.rs")));
    super::panel_focus::press("backspace", cx);
    assert_eq!(tab_names(&view, thread, cx).len(), 19, "the key closed it");
    let focused = cx.update(|window, cx| view.read(cx).panel.focus.focused(window));
    assert_eq!(
        focused,
        Some(PanelControl::FileTab(checkout.join("file-number-3.rs"))),
        "the keyboard moved to the neighbour"
    );
    for _ in 0..19 {
        super::panel_focus::press("backspace", cx);
    }
    assert!(tab_names(&view, thread, cx).is_empty(), "every tab closed by key");
}

/// The tab whose opening first overflows the row lands whole in it, though
/// the chevron slots that overflow brings in narrow the row.
#[gpui::test]
fn the_tab_that_first_overflows_the_row_sits_whole_in_it(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-first-overflow", cx);
    cx.simulate_resize(gpui::size(px(1300.), px(900.)));
    for at in 0..20 {
        let file = checkout.join(format!("file-number-{at}.rs"));
        std::fs::write(&file, "x\n").unwrap();
        open(&view, thread, &file, true, cx);
        tick(cx);
        if cx.debug_bounds("files-tabs-left").is_some() {
            let selector: &'static str = Box::leak(format!("files-tab-{at}").into_boxed_str());
            let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
            assert!(frames > 0, "the row asks for the frame that aims at the tab again");
            cx.run_until_parked();
            assert_eq!(inside_the_row(selector, cx), Some(true), "tab {at} lies whole in the row");
            return;
        }
    }
    panic!("twenty tabs never overflowed the row");
}

/// A reveal in a row that is not drawn asks for no frames.
#[gpui::test]
fn a_reveal_in_a_hidden_row_asks_for_no_frames(cx: &mut TestAppContext) {
    let (view, thread, _checkout, cx) = twenty_tabs("files-hidden-reveal", cx);
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
    assert!(cx.debug_bounds("files-tabs").is_none(), "the row is hidden");
    view.update(cx, |view, cx| {
        view.files.update(cx, |files, _| files.reveal_tab(thread, 3));
        cx.notify();
    });
    tick(cx);
    for _ in 0..4 {
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(frames, 0, "the hidden row asks for nothing");
}

/// A tab revealed from the far end lands whole inside the row, and the
/// chevron that appears beside it takes none of the row's width.
#[gpui::test]
fn a_revealed_tab_sits_whole_in_the_row_whatever_the_chevrons_do(cx: &mut TestAppContext) {
    let (view, thread, _checkout, cx) = twenty_tabs("files-reveal", cx);
    let left = cx.debug_bounds("files-tabs-left").expect("more lies to the left");
    for _ in 0..40 {
        if cx.debug_bounds("files-tabs-left").is_none() {
            break;
        }
        cx.simulate_click(left.center(), gpui::Modifiers::none());
        cx.executor().advance_clock(std::time::Duration::from_millis(500));
        tick(cx);
    }
    let row = cx.debug_bounds("files-tabs").unwrap().size.width;
    assert!(cx.debug_bounds("files-tabs-right").is_some(), "at the start, more lies right");
    for at in [10, 18, 2] {
        view.update(cx, |view, cx| {
            view.files.update(cx, |files, _| files.reveal_tab(thread, at));
            cx.notify();
        });
        tick(cx);
        tick(cx);
        let selector: &'static str = Box::leak(format!("files-tab-{at}").into_boxed_str());
        assert_eq!(inside_the_row(selector, cx), Some(true), "tab {at} lies whole in the row");
        assert_eq!(cx.debug_bounds("files-tabs").unwrap().size.width, row, "the row kept its width");
    }
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
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Files));
}

#[gpui::test]
fn the_filter_lists_matches_and_a_row_menu_copies_paths(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-menu", cx);
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Files, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.files.update(cx, |files, cx| files.set_filter_for_test(thread, "lib", cx));
    });
    tick(cx);
    assert!(cx.debug_bounds("files-row-0").is_some());
    assert!(cx.debug_bounds("files-row-1").is_none(), "only lib.rs matches");
    let row = cx.debug_bounds("files-row-0").unwrap();
    cx.simulate_mouse_down(row.center(), gpui::MouseButton::Right, gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        let menu = view.context_menu().expect("the row's menu");
        let names: Vec<_> = menu
            .rows
            .iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect();
        assert_eq!(names, ["Attach as context", "Copy", "Show in Finder"]);
    });
    let relative = std::path::Path::new("src").join("lib.rs");
    let lib = checkout.join(&relative);
    for (verb, expect) in [
        (FilesVerb::CopyAbsolute, lib.to_string_lossy().into_owned()),
        (FilesVerb::CopyRelative, relative.to_string_lossy().into_owned()),
        (FilesVerb::CopyName, "lib.rs".to_string()),
    ] {
        view.update(cx, |view, cx| view.files.update(cx, |files, cx| files.run_verb(thread, verb, cx)));
        assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), Some(expect));
    }
    view.read_with(cx, |view, cx| {
        let sub = view.context_submenu_rows_for_test(MenuTarget::File(thread), Submenu::Files(FilesSub::Copy), cx);
        assert_eq!(sub.len(), 3, "Absolute path, Relative path, Filename");
    });
    view.update(cx, |view, cx| view.files.update(cx, |files, cx| files.run_verb(thread, FilesVerb::Attach, cx)));
    assert_eq!(
        view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).attachment_count()),
        1,
        "Attach as context"
    );
}

#[gpui::test]
fn the_more_menu_holds_only_its_five_settings_and_they_take_effect(cx: &mut TestAppContext) {
    let (view, thread, _checkout, cx) = files_window("files-more", cx);
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Files, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::FilesMore(thread), gpui::point(px(900.), px(100.)), cx)
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        let rows = &view.context_menu().unwrap().rows;
        let names: Vec<_> = rows.iter().flatten().map(|(item, _)| item.label.to_string()).collect();
        assert_eq!(
            names,
            ["Show file tree", "Preview tabs", "Auto save", "Word wrap", "Tab size"]
        );
        let checked: Vec<_> = rows.iter().flatten().map(|(item, _)| item.checked).collect();
        assert_eq!(checked, [true, true, true, true, false]);
    });
    view.update(cx, |view, cx| view.files.update(cx, |files, cx| files.run_verb(thread, FilesVerb::ToggleTree, cx)));
    tick(cx);
    assert!(cx.debug_bounds("files-tree-column").is_none(), "the tree hides");
    view.update(cx, |view, cx| view.files.update(cx, |files, cx| files.run_verb(thread, FilesVerb::ToggleTree, cx)));
    tick(cx);
    assert!(cx.debug_bounds("files-tree-column").is_some());
}

/// A changed file's tab offers `Show changes`, which opens the Changes
/// column on it; an unchanged file's tab has only `Open`.
#[gpui::test]
fn a_changed_files_tab_offers_show_changes(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-show-changes", cx);
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "operator@example.invalid"],
        &["config", "user.name", "operator"],
        &["add", "-A"],
        &["commit", "-q", "-m", "all"],
    ] {
        let out = std::process::Command::new("git").args(args).current_dir(&checkout).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }
    std::fs::write(checkout.join("fresh.rs"), "new\n").unwrap();
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Changes, cx));
    for _ in 0..400 {
        tick(cx);
        if view.read_with(cx, |view, cx| view.changes.read(cx).files_read(Diff::Changes(thread)).is_some()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    open(&view, thread, &checkout.join("src/lib.rs"), true, cx);
    assert!(cx.debug_bounds("files-open").is_some());
    assert!(cx.debug_bounds("files-show-changes").is_none(), "unchanged");
    open(&view, thread, &checkout.join("fresh.rs"), true, cx);
    let link = cx.debug_bounds("files-show-changes").expect("a changed file");
    view.update(cx, |view, cx| view.close_surface(thread, &SurfaceId::Changes, cx));
    tick(cx);
    assert!(cx.debug_bounds("changes").is_none(), "its tab is closed");
    cx.simulate_click(link.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("changes").is_some(), "Show changes opens it");
}

fn open_editor<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadKey, std::path::PathBuf, Entity<crate::cockpit::tools::Editor>, &'a mut gpui::VisualTestContext) {
    let (view, thread, checkout, cx) = files_window(name, cx);
    let file = checkout.join("src/lib.rs");
    open(&view, thread, &file, true, cx);
    let editor = view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).expect("a code file"));
    (view, thread, file, editor, cx)
}

/// Clicks the rendered editor, then types through the focused window as the
/// keyboard does: text as input, Enter as the bound key.
fn type_into(text: &str, cx: &mut gpui::VisualTestContext) {
    let editor = cx.debug_bounds("editor").expect("the editor is on screen");
    cx.simulate_click(editor.center(), gpui::Modifiers::none());
    tick(cx);
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            cx.simulate_keystrokes("enter");
        }
        if !line.is_empty() {
            cx.simulate_input(line);
        }
    }
    tick(cx);
}

/// Keystrokes that do not wait for background work, so a write stays in
/// flight while the user keeps typing.
fn press_without_waiting(keys: &str, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        for key in keys.split(' ') {
            window.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), cx);
        }
    });
}

fn pause_and_settle(cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(crate::cockpit::tools::AUTO_SAVE_AFTER);
    cx.run_until_parked();
    tick(cx);
}

#[gpui::test]
fn a_refused_write_keeps_the_buffer_and_retry_writes_it(cx: &mut TestAppContext) {
    use crate::cockpit::tools::SaveState;
    let (view, thread, file, editor, cx) = open_editor("files-refused", cx);
    editor.update(cx, |editor, _| editor.refuse_writes());
    type_into("// hi\n", cx);
    pause_and_settle(cx);
    assert!(matches!(editor.read_with(cx, |e, _| e.save_state().clone()), SaveState::Failed(_)));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "l\n");
    assert!(cx.debug_bounds("editor-status").is_some(), "the failure shows");

    let close = cx.debug_bounds("files-tab-close-0").unwrap();
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    pause_and_settle(cx);
    assert_eq!(tab_names(&view, thread, cx).len(), 1, "closing keeps the unsaved tab");
    assert!(editor.read_with(cx, |e, _| e.text().contains("// hi")));

    editor.update(cx, |editor, _| editor.allow_writes());
    let retry = cx.debug_bounds("editor-retry").expect("Retry");
    cx.simulate_click(retry.center(), gpui::Modifiers::none());
    pause_and_settle(cx);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "// hi\nl\n");
    assert_eq!(editor.read_with(cx, |e, _| e.save_state().clone()), SaveState::Saved);
    assert!(cx.debug_bounds("editor-status").is_none());
}

#[gpui::test]
fn discarding_a_failed_buffer_closes_the_tab(cx: &mut TestAppContext) {
    let (view, thread, file, editor, cx) = open_editor("files-discard", cx);
    editor.update(cx, |editor, _| editor.refuse_writes());
    type_into("// hi\n", cx);
    let close = cx.debug_bounds("files-tab-close-0").unwrap();
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    pause_and_settle(cx);
    assert_eq!(tab_names(&view, thread, cx).len(), 1);
    let discard = cx.debug_bounds("editor-discard").expect("Discard");
    cx.simulate_click(discard.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(tab_names(&view, thread, cx).is_empty());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "l\n");
}

#[gpui::test]
fn a_clean_buffer_follows_its_file_when_the_agent_edits_it(cx: &mut TestAppContext) {
    let (core, fake, checkout) = bound_cockpit("files-follow", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let file = checkout.join("src/lib.rs");
    open(&view, thread, &file, true, cx);
    let editor = view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).expect("a code file"));
    std::fs::write(&file, "agent\n").unwrap();
    agent_worked(&view, &fake, 0, cx);
    assert_eq!(editor.read_with(cx, |e, _| e.text().to_string()), "agent\n");
    assert!(!editor.read_with(cx, |e, _| e.is_dirty()));

    type_into("// mine\n", cx);
    std::fs::write(&file, "agent again\n").unwrap();
    agent_worked(&view, &fake, 0, cx);
    assert!(
        editor.read_with(cx, |e, _| e.text().to_string()).contains("// mine"),
        "an edited buffer is left to its owner"
    );
}

#[gpui::test]
fn a_clean_buffer_keeps_its_caret_when_it_follows_its_file(cx: &mut TestAppContext) {
    let (core, fake, checkout) = bound_cockpit("files-follow-caret", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let file = checkout.join("src/lib.rs");
    std::fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();
    open(&view, thread, &file, true, cx);
    let editor = view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).expect("a code file"));
    let at = cx.debug_bounds("editor").expect("the editor is on screen");
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes("down down");
    tick(cx);
    let caret = editor.read_with(cx, |e, _| e.caret_for_test());
    assert!(caret > 0, "the caret is off the first line");
    std::fs::write(&file, "a\nBETA\ngamma\n").unwrap();
    agent_worked(&view, &fake, 0, cx);
    assert_eq!(editor.read_with(cx, |e, _| e.text().to_string()), "a\nBETA\ngamma\n");
    assert_eq!(
        editor.read_with(cx, |e, _| e.caret_for_test()),
        caret - "lpha".len(),
        "the caret keeps its line and column when a line above it shrinks"
    );
}

#[gpui::test]
fn the_tree_reads_git_status_off_the_ui_thread(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-status", cx);
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git").args(args).current_dir(&checkout).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "a"]);
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Files, cx));
    cx.run_until_parked();
    let readme = checkout.join("README.md");
    std::fs::write(&readme, "changed\n").unwrap();
    view.update(cx, |view, cx| view.files.update(cx, |files, cx| files.refresh(thread, cx)));
    assert_eq!(
        view.read_with(cx, |view, cx| view.files.read(cx).status_for_test(thread, &readme)),
        None,
        "the UI thread did not wait for git"
    );
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, cx| view.files.read(cx).status_for_test(thread, &readme)),
        Some(crate::cockpit::tools::FileStatus::Modified)
    );
}

#[gpui::test]
fn closing_a_dirty_tab_writes_it_then_closes(cx: &mut TestAppContext) {
    let (view, thread, file, _editor, cx) = open_editor("files-close-dirty", cx);
    type_into("// hi\n", cx);
    let close = cx.debug_bounds("files-tab-close-0").unwrap();
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    tick(cx);
    assert!(tab_names(&view, thread, cx).is_empty());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "// hi\nl\n");
}

#[gpui::test]
fn an_external_edit_is_not_overwritten_until_the_user_chooses(cx: &mut TestAppContext) {
    use crate::cockpit::tools::SaveState;
    let (_view, _thread, file, editor, cx) = open_editor("files-external", cx);
    std::fs::write(&file, "agent\n").unwrap();
    type_into("// hi\n", cx);
    pause_and_settle(cx);
    assert_eq!(editor.read_with(cx, |e, _| e.save_state().clone()), SaveState::Conflict);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "agent\n", "the agent's edit survives");
    let reload = cx.debug_bounds("editor-reload").expect("Reload");
    assert!(cx.debug_bounds("editor-overwrite").is_some());

    cx.simulate_click(reload.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(editor.read_with(cx, |e, _| e.text().to_string()), "agent\n");
    assert!(!editor.read_with(cx, |e, _| e.is_dirty()));

    std::fs::write(&file, "agent again\n").unwrap();
    type_into("// mine\n", cx);
    pause_and_settle(cx);
    let overwrite = cx.debug_bounds("editor-overwrite").expect("Overwrite");
    cx.simulate_click(overwrite.center(), gpui::Modifiers::none());
    pause_and_settle(cx);
    assert!(std::fs::read_to_string(&file).unwrap().contains("// mine"));
    assert_eq!(editor.read_with(cx, |e, _| e.save_state().clone()), SaveState::Saved);
}

#[gpui::test]
fn a_failed_reload_says_so_and_keeps_the_buffer(cx: &mut TestAppContext) {
    use crate::cockpit::tools::SaveState;
    let (_view, _thread, file, editor, cx) = open_editor("files-reload-fails", cx);
    std::fs::write(&file, "agent\n").unwrap();
    type_into("// hi\n", cx);
    pause_and_settle(cx);
    std::fs::remove_file(&file).unwrap();
    let reload = cx.debug_bounds("editor-reload").expect("Reload");
    cx.simulate_click(reload.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(matches!(editor.read_with(cx, |e, _| e.save_state().clone()), SaveState::ReloadFailed(_)));
    assert!(editor.read_with(cx, |e, _| e.is_dirty()), "the buffer survives");
    assert!(cx.debug_bounds("editor-discard").is_none());
    assert!(cx.debug_bounds("editor-reload").is_some());
}

#[gpui::test]
fn a_late_write_does_not_mark_newer_edits_saved(cx: &mut TestAppContext) {
    use crate::cockpit::tools::SaveState;
    let (_view, _thread, file, editor, cx) = open_editor("files-late", cx);
    type_into("a", cx);
    press_without_waiting("cmd-s", cx);
    assert!(matches!(editor.read_with(cx, |e, _| e.save_state().clone()), SaveState::Saving(_)));
    press_without_waiting("b", cx);
    cx.run_until_parked();
    assert!(editor.read_with(cx, |e, _| e.is_dirty()), "the first write does not cover the second edit");
    assert_ne!(editor.read_with(cx, |e, _| e.save_state().clone()), SaveState::Saved);
    let first = std::fs::read_to_string(&file).unwrap();
    assert!(first.contains('a') && !first.contains('b'), "{first:?}");
    pause_and_settle(cx);
    assert!(std::fs::read_to_string(&file).unwrap().contains("ab"));
    assert_eq!(editor.read_with(cx, |e, _| e.save_state().clone()), SaveState::Saved);
}

fn rows_drawn(cx: &mut gpui::VisualTestContext) -> usize {
    (0..12)
        .filter(|at| cx.debug_bounds(Box::leak(format!("files-row-{at}").into_boxed_str())).is_some())
        .count()
}

#[gpui::test]
fn the_refresh_door_shows_a_file_made_after_the_tree_was_read(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-refresh", cx);
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Files, cx));
    tick(cx);
    let before = rows_drawn(cx);
    std::fs::write(checkout.join("made-later.rs"), "x\n").unwrap();
    tick(cx);
    assert_eq!(rows_drawn(cx), before, "nothing reads the disk by itself");
    let refresh = cx.debug_bounds("files-refresh").expect("the Refresh door");
    cx.simulate_click(refresh.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(rows_drawn(cx), before + 1, "the new file's row");
}

#[gpui::test]
fn coming_back_to_the_files_tab_shows_what_changed_while_away(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-return", cx);
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Files, cx));
    tick(cx);
    let before = rows_drawn(cx);
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Changes, cx));
    tick(cx);
    std::fs::write(checkout.join("made-while-away.rs"), "x\n").unwrap();
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Files, cx));
    tick(cx);
    assert_eq!(rows_drawn(cx), before + 1);
}

#[gpui::test]
fn a_quit_writes_what_auto_save_had_not_yet(cx: &mut TestAppContext) {
    let (view, _thread, file, _editor, cx) = open_editor("files-quit", cx);
    type_into("// hi\n", cx);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "l\n", "not yet");
    view.update(cx, |view, cx| view.files.update(cx, |files, cx| files.flush_for_quit(cx)));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "// hi\nl\n");
}

#[gpui::test]
fn leaving_asks_about_text_the_disk_refused(cx: &mut TestAppContext) {
    let (view, _thread, _file, editor, cx) = open_editor("files-quit-refused", cx);
    editor.update(cx, |editor, _| editor.refuse_writes());
    type_into("// hi\n", cx);
    pause_and_settle(cx);
    type_into("x", cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.leaving_warning(cx)).as_deref(),
        Some("1 file has unsaved changes. Leaving discards them."),
        "a typed edit after a refused write is still only in the buffer"
    );
}

/// Auto save's quit flush writes what it can. What it cannot, text with
/// auto save off or over a file that changed on disk, the quit asks about.
#[gpui::test]
fn leaving_asks_about_the_text_a_quit_will_not_write(cx: &mut TestAppContext) {
    let (view, thread, file, _editor, cx) = open_editor("files-quit-asks", cx);
    let warning = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| view.leaving_warning(cx))
    };
    assert_eq!(warning(&view, cx), None, "nothing typed");
    type_into("// hi\n", cx);
    assert_eq!(warning(&view, cx), None, "auto save writes it at quit");

    std::fs::write(&file, "changed elsewhere\n").unwrap();
    assert_eq!(
        warning(&view, cx).as_deref(),
        Some("1 file has unsaved changes. Leaving discards them."),
        "the file changed under the buffer"
    );
    view.update(cx, |view, cx| view.files.update(cx, |files, cx| files.flush_for_quit(cx)));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "changed elsewhere\n", "the quit did not overwrite");

    view.update(cx, |view, cx| view.files.update(cx, |files, cx| files.run_verb(thread, FilesVerb::ToggleAutoSave, cx)));
    std::fs::write(&file, "l\n").unwrap();
    assert!(
        warning(&view, cx).is_some(),
        "auto save off, the text is only in the buffer"
    );
}

#[gpui::test]
fn remote_files_use_remote_tree_text_saves_conflicts_and_reload_when_local_paths_collide(
    cx: &mut TestAppContext,
) {
    use crate::cockpit::tools::SaveState;
    use ferrite_core::file_operations::{
        fingerprint, Directory, Entry, Reply, Request, Snapshot, Status,
    };
    let (view, thread, root, cx) = files_window("remote-files-collision", cx);
    let path = root.join("collision.rs");
    std::fs::write(&path, "local must stay untouched").unwrap();
    let remote_text = Rc::new(RefCell::new("remote original".to_string()));
    let requests = Rc::new(RefCell::new(Vec::new()));
    let fail_reads = Rc::new(std::cell::Cell::new(false));
    let read_failure = fail_reads.clone();
    let content = remote_text.clone();
    let observed = requests.clone();
    let remote_path = path.clone();
    let remote_root = root.clone();
    view.update(cx, |view, cx| {
        view.remote_files_for_test(
            thread,
            move |request| {
                observed.borrow_mut().push(request.clone());
                if matches!(request, Request::Read { .. }) && read_failure.get() {
                    return Err(std::io::Error::other("temporary disconnect"));
                }
                Ok(match request {
                    Request::Read { .. } => Reply::Text {
                        text: content.borrow().clone(),
                        fingerprint: fingerprint(content.borrow().as_bytes()),
                    },
                    Request::Write { text, expected, .. } => {
                        if expected.is_some_and(|expected| {
                            expected != fingerprint(content.borrow().as_bytes())
                        }) {
                            Reply::Conflict
                        } else {
                            *content.borrow_mut() = text;
                            Reply::Written {
                                fingerprint: fingerprint(content.borrow().as_bytes()),
                            }
                        }
                    }
                    Request::Snapshot { .. } => {
                        let entry = Entry {
                            path: remote_path.clone(),
                            name: "remote-name.rs".into(),
                            directory: false,
                        };
                        Reply::Snapshot(Snapshot {
                            directories: vec![Directory {
                                path: remote_root.clone(),
                                entries: vec![entry.clone()],
                            }],
                            files: vec![entry],
                            status: vec![("collision.rs".into(), Status::Modified)],
                            truncated: false,
                        })
                    }
                    Request::Unknown => panic!("unknown operation"),
                })
            },
            cx,
        );
        view.open_in_files(thread, path.clone(), None, true, cx);
    });
    tick(cx);
    let editor = view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).unwrap());
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.text().to_string()),
        "remote original"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.files.read(cx).row_names(thread)),
        ["remote-name.rs"]
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.files.read(cx).status_for_test(thread, &path)),
        Some(crate::cockpit::tools::FileStatus::Modified)
    );
    assert!(
        cx.debug_bounds("files-open").is_none(),
        "a remote path has no local app action"
    );
    editor.update(cx, |editor, cx| {
        editor.type_text("edit ", cx);
        editor.save(cx);
    });
    tick(cx);
    assert_eq!(*remote_text.borrow(), "edit remote original");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "local must stay untouched"
    );
    fail_reads.set(true);
    editor.update(cx, |editor, cx| editor.follow_disk(cx));
    tick(cx);
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.save_state().clone()),
        SaveState::Saved
    );
    fail_reads.set(false);
    editor.update(cx, |editor, cx| editor.follow_disk(cx));
    tick(cx);
    editor.update(cx, |editor, cx| {
        editor.type_text("recovered ", cx);
        editor.save(cx);
    });
    tick(cx);
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.save_state().clone()),
        SaveState::Saved
    );
    assert!(remote_text.borrow().contains("recovered"));
    *remote_text.borrow_mut() = "remote agent changed it".into();
    editor.update(cx, |editor, cx| editor.follow_disk(cx));
    tick(cx);
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.text().to_string()),
        "remote agent changed it"
    );
    editor.update(cx, |editor, cx| editor.type_text("mine ", cx));
    *remote_text.borrow_mut() = "changed again".into();
    editor.update(cx, |editor, cx| editor.save(cx));
    tick(cx);
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.save_state().clone()),
        SaveState::Conflict
    );
    let reload = cx.debug_bounds("editor-reload").unwrap();
    cx.simulate_click(reload.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.text().to_string()),
        "changed again"
    );
    editor.update(cx, |editor, cx| editor.type_text("mine ", cx));
    *remote_text.borrow_mut() = "raced".into();
    editor.update(cx, |editor, cx| editor.save(cx));
    tick(cx);
    let overwrite = cx.debug_bounds("editor-overwrite").unwrap();
    cx.simulate_click(overwrite.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(*remote_text.borrow(), "mine changed again");
    editor.update(cx, |editor, cx| {
        editor.type_text("unsaved ", cx);
        assert!(editor.lost_by_leaving());
        editor.flush_for_quit();
        assert!(editor.is_dirty());
    });
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "local must stay untouched"
    );
    assert!(requests
        .borrow()
        .iter()
        .any(|request| matches!(request, Request::Write { expected: None, .. })));
}

#[gpui::test]
fn remote_files_failure_and_binary_never_fall_back_to_readable_local_text(cx: &mut TestAppContext) {
    use ferrite_core::file_operations::{Reply, Request};
    let (view, thread, root, cx) = files_window("remote-files-failure", cx);
    let path = root.join("collision.rs");
    std::fs::write(&path, "local text must not appear").unwrap();
    let unavailable = Rc::new(std::cell::Cell::new(true));
    let server_unavailable = unavailable.clone();
    view.update(cx, |view, cx| {
        view.remote_files_for_test(
            thread,
            move |request| {
                if server_unavailable.get() {
                    return Err(std::io::Error::other("Remote server unavailable"));
                }
                Ok(match request {
                    Request::Read { .. } => Reply::Text {
                        text: "remote recovered".into(),
                        fingerprint: ferrite_core::file_operations::fingerprint(
                            b"remote recovered",
                        ),
                    },
                    _ => Reply::Unknown,
                })
            },
            cx,
        );
        view.open_in_files(thread, path.clone(), None, true, cx);
    });
    tick(cx);
    assert!(view
        .read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread))
        .is_none());
    assert_eq!(
        view.read_with(cx, |view, cx| view.files.read(cx).unshown_note(thread)),
        Some("Remote server unavailable".into())
    );
    unavailable.set(false);
    let retry = cx
        .debug_bounds("files-retry")
        .expect("remote read errors can retry");
    cx.simulate_click(retry.center(), gpui::Modifiers::none());
    tick(cx);
    let editor = view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).unwrap());
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.text().to_string()),
        "remote recovered"
    );
    view.update(cx, |view, cx| {
        view.remote_files_for_test(
            thread,
            |request| {
                Ok(match request {
                    Request::Read { .. } => Reply::Binary,
                    _ => Reply::Unknown,
                })
            },
            cx,
        );
        view.open_in_files(thread, path, None, true, cx);
    });
    tick(cx);
    assert!(view
        .read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread))
        .is_none());
    assert!(view
        .read_with(cx, |view, cx| view.files.read(cx).unshown_note(thread))
        .unwrap()
        .contains("remote terminal"));
    assert!(cx.debug_bounds("files-open-original").is_none());
}

#[gpui::test]
fn remote_files_transcript_preview_routes_to_remote_editor_and_keeps_line_target(
    cx: &mut TestAppContext,
) {
    use ferrite_core::file_operations::{fingerprint, Reply, Request};
    let (view, thread, root, cx) = files_window("remote-files-preview", cx);
    let path = root.join("preview.md");
    std::fs::write(&path, "local text must not appear").unwrap();
    view.update(cx, |view, cx| {
        view.remote_files_for_test(
            thread,
            |request| {
                Ok(match request {
                    Request::Read { .. } => Reply::Text {
                        text: "remote\nsecond\nthird".into(),
                        fingerprint: fingerprint(b"remote\nsecond\nthird"),
                    },
                    _ => Reply::Unknown,
                })
            },
            cx,
        );
        view.open_preview_from(0, path.clone(), Some(3), cx);
    });
    tick(cx);
    let editor = view.read_with(cx, |view, cx| {
        assert!(
            view.panes[0].preview.document().is_none(),
            "the local preview never reads this path"
        );
        view.files.read(cx).shown_editor(thread).unwrap()
    });
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.text().to_string()),
        "remote\nsecond\nthird"
    );
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.caret_line_for_test()),
        3
    );
    assert!(
        cx.debug_bounds("files-show-rendered").is_none(),
        "remote source cannot load local Markdown images or file links"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "local text must not appear"
    );
}

#[gpui::test]
fn remote_files_inline_transcript_chip_uses_remote_bytes_despite_local_collision(
    cx: &mut TestAppContext,
) {
    use ferrite_core::file_operations::{fingerprint, Reply, Request};
    let (core, fake, root) = bound_cockpit("remote-files-inline-chip", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let path = root.join("chip.rs");
    std::fs::write(&path, "local collision must not open").unwrap();
    view.update(cx, |view, cx| {
        view.remote_files_for_test(
            thread,
            |request| {
                Ok(match request {
                    Request::Read { .. } => Reply::Text {
                        text: "remote chip\nsecond".into(),
                        fingerprint: fingerprint(b"remote chip\nsecond"),
                    },
                    _ => Reply::Unknown,
                })
            },
            cx,
        )
    });
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: format!("Open [remote source]({}:2).\n", path.display()),
        })
        .unwrap();
    tick(cx);
    let selector = Box::leak(format!("file-attachment-{}", path.display()).into_boxed_str());
    let chip = cx
        .debug_bounds(selector)
        .expect("the transcript's file chip");
    cx.simulate_click(chip.center(), gpui::Modifiers::none());
    tick(cx);
    let editor = view.read_with(cx, |view, cx| {
        assert!(view.panes[0].preview.document().is_none());
        view.files.read(cx).shown_editor(thread).unwrap()
    });
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.text().to_string()),
        "remote chip\nsecond"
    );
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.caret_line_for_test()),
        2
    );
    assert!(
        cx.opened_url().is_none(),
        "no local default application is opened"
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "local collision must not open"
    );
}

/// The Files bar's doors press by the pointer: the tree's door hides the
/// tree and shows it again, and `⋯` opens the Files settings.
#[gpui::test]
fn the_files_bars_doors_press_by_the_pointer(cx: &mut TestAppContext) {
    let (view, thread, _checkout, cx) = files_window("files-doors", cx);
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Files, cx));
    tick(cx);
    for shown in [false, true] {
        let toggle = cx.debug_bounds("files-tree-toggle").expect("the tree's door");
        cx.simulate_click(toggle.center(), gpui::Modifiers::none());
        tick(cx);
        assert_eq!(cx.debug_bounds("files-tree-column").is_some(), shown);
    }
    let more = cx.debug_bounds("files-more").expect("the bar's more door");
    cx.simulate_click(more.center(), gpui::Modifiers::none());
    tick(cx);
    let names = view.read_with(cx, |view, _| {
        let rows = &view.context_menu().expect("the more menu opened").rows;
        rows.iter().flatten().map(|(item, _)| item.label.to_string()).collect::<Vec<_>>()
    });
    assert_eq!(names, ["Show file tree", "Preview tabs", "Auto save", "Word wrap", "Tab size"]);
}

/// A deleted Thread's edit still reaches the disk, as auto save promised,
/// and its Files go once it has.
#[gpui::test]
fn a_deleted_threads_edit_reaches_the_disk_and_its_files_go(cx: &mut TestAppContext) {
    let (view, thread, file, editor, cx) = open_editor("files-thread-deleted", cx);
    editor.update(cx, |editor, cx| editor.type_text("// kept\n", cx));
    press_nav_verb(&view, thread, MenuVerb::Delete, cx);
    pause_and_settle(cx);
    assert!(std::fs::read_to_string(&file).unwrap().starts_with("// kept\n"), "the edit is on disk");
    tick(cx);
    assert_eq!(view.read_with(cx, |view, cx| view.held_for_test(thread, cx)), 0, "its Files went");
}

/// A deleted Thread's edit the disk refused is still counted when leaving,
/// so the warning before a quit names it.
#[gpui::test]
fn a_deleted_threads_refused_edit_is_still_named_when_leaving(cx: &mut TestAppContext) {
    let (view, thread, _file, editor, cx) = open_editor("files-thread-deleted-refused", cx);
    editor.update(cx, |editor, _| editor.refuse_writes());
    editor.update(cx, |editor, cx| editor.type_text("// lost\n", cx));
    press_nav_verb(&view, thread, MenuVerb::Delete, cx);
    pause_and_settle(cx);
    assert_eq!(view.read_with(cx, |view, cx| view.files.read(cx).unsaved_at_leaving(cx)), Some(1));
}

/// A Thread with the same id on another machine is another Thread (ADR
/// 0045): its Files hold tabs of their own, and closing one leaves this
/// Thread's.
#[gpui::test]
fn a_files_view_keeps_two_machines_threads_apart(cx: &mut TestAppContext) {
    let (view, thread, checkout, cx) = files_window("files-two-machines", cx);
    open(&view, thread, &checkout.join("src/lib.rs"), true, cx);
    let there = ferrite_core::ThreadKey::fixture(u32::MAX, thread.thread().get());
    view.update(cx, |view, cx| {
        assert_ne!(thread, there);
        view.files.update(cx, |files, cx| {
            files.make_for_test(there, checkout.clone(), cx);
            files.open(there, checkout.join("README.md"), None, true, true, cx);
        });
    });
    tick(cx);
    let theirs = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| view.files.read(cx).tab_names(there))
    };
    assert_eq!(tab_names(&view, thread, cx), [("lib.rs".to_string(), false)]);
    assert_eq!(theirs(&view, cx), [("README.md".to_string(), false)]);
    view.update(cx, |view, cx| view.files.update(cx, |files, cx| files.close_tab(there, 0, cx)));
    assert!(theirs(&view, cx).is_empty());
    assert_eq!(tab_names(&view, thread, cx), [("lib.rs".to_string(), false)], "this Thread's stay");
}

//! The Panel's Preview tab (`panel/preview.rs`, SPEC §5.6.3): a file opens
//! there at its line, the Panel wide and the line in view; its × and esc
//! close it, never the turn, and the Panel goes back to the tab it showed;
//! ⌘W in it closes it, not the Pane; each Thread keeps its own, the Panel
//! showing the focused Pane's; focus moving to another Pane takes the
//! keyboard out of an image; a draft previews in the Panel and keeps it
//! through its first send.

use super::*;
use crate::cockpit::panel::PanelTab;
use ferrite_ui::theme::{LH_UI, MOTION_RESIZE_MS, PANEL_W};

/// A source file of `lines` numbered lines, in its own scratch directory.
fn source(name: &str, lines: usize) -> std::path::PathBuf {
    let dir = scratch(name);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("lines.rs");
    let text: String = (1..=lines)
        .map(|n| format!("fn line_{n}() {{}}\n"))
        .collect();
    std::fs::write(&file, text).unwrap();
    file
}

/// Open `path` at `line` as a file link would, and let the Panel's ride
/// land.
fn preview(
    view: &Entity<CockpitView>,
    path: &std::path::Path,
    line: Option<u32>,
    cx: &mut gpui::VisualTestContext,
) {
    let path = path.to_path_buf();
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_preview(path, line, window, cx)));
    land(cx);
}

/// Draw until every ride has landed.
fn land(cx: &mut gpui::VisualTestContext) {
    tick(cx);
    cx.executor()
        .advance_clock(Duration::from_millis(MOTION_RESIZE_MS + 50));
    tick(cx);
}

fn panel_w(cx: &mut gpui::VisualTestContext) -> Option<f32> {
    cx.debug_bounds("panel")
        .map(|panel| f32::from(panel.size.width))
}

fn window<'a>(
    name: &str,
    panes: usize,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, Fake, &'a mut gpui::VisualTestContext) {
    let (mut core, fake) = cockpit(name, panes);
    if panes > 1 {
        let group = group_all(&mut core);
        core.enter_group(group).unwrap();
    }
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    (view, fake, cx)
}

/// A file opened at a line from the PR tab: the Preview tab appears beside
/// the Thread's and is shown, the Panel wide, the target line in view; its
/// × closes it and the Panel goes back to the PR tab, at its own width.
#[gpui::test]
fn a_file_at_a_line_opens_wide_and_closes_back_to_the_tab_before(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window("preview-line", 1, cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.open_panel_tab(thread, PanelTab::Pr, cx));
    land(cx);
    assert_eq!(panel_w(cx), Some(PANEL_W));
    let file = source("preview-line-file", 200);

    preview(&view, &file, Some(150), cx);
    assert!(cx.debug_bounds("panel-tab-Preview").is_some(), "its tab");
    assert!(
        cx.debug_bounds("panel-tab-Pr").is_some(),
        "beside the Thread's"
    );
    assert!(cx.debug_bounds("panel-preview").is_some(), "and shown");
    let wide = panel_w(cx).expect("shown");
    assert!(wide > PANEL_W + 100., "wide: {wide}");
    let scrolled = view.read_with(cx, |view, _| {
        view.panes[0]
            .preview_tab
            .scroll
            .0
            .borrow()
            .base_handle
            .offset()
            .y
    });
    assert!(
        scrolled < px(-120. * LH_UI),
        "line 150 is brought to the middle: {scrolled:?}"
    );

    let close = cx.debug_bounds("preview-close").expect("its ×");
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    land(cx);
    assert!(view.read_with(cx, |view, _| view.panes[0].preview.document().is_none()));
    assert!(
        cx.debug_bounds("panel-tab-Preview").is_none(),
        "its tab goes"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.panel_tab(thread)),
        PanelTab::Pr,
        "back to the tab before"
    );
    assert_eq!(panel_w(cx), Some(PANEL_W), "at the Panel's own width");
}

/// Another tab picked while a preview is open keeps the preview and its
/// tab, at the Panel's own width; its tab brings it back, wide.
#[gpui::test]
fn another_tab_keeps_the_preview_for_later(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window("preview-tabs", 1, cx);
    let file = source("preview-tabs-file", 20);
    preview(&view, &file, None, cx);
    let wide = panel_w(cx).unwrap();
    let changes = cx.debug_bounds("panel-tab-Changes").unwrap();
    cx.simulate_click(changes.center(), gpui::Modifiers::none());
    land(cx);
    assert_eq!(panel_w(cx), Some(PANEL_W));
    assert!(cx.debug_bounds("panel-preview").is_none());
    let tab = cx.debug_bounds("panel-tab-Preview").expect("its tab stays");
    cx.simulate_click(tab.center(), gpui::Modifiers::none());
    land(cx);
    assert_eq!(panel_w(cx), Some(wide));
    assert!(cx.debug_bounds("panel-preview").is_some());
}

/// Esc with the preview's own text holding the keyboard closes the
/// preview — not the Pane's turn — and the keyboard is the Pane's at once;
/// ⌘W in it closes it too, and the Pane stays.
#[gpui::test]
fn escape_and_cmd_w_in_the_preview_close_it_not_the_turn_or_the_pane(cx: &mut TestAppContext) {
    let (view, fake, cx) = window("preview-escape", 1, cx);
    let dir = scratch("preview-escape-file");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notes.md");
    std::fs::write(
        &file,
        "# Notes\n\nA paragraph long enough to select a few words in.\n",
    )
    .unwrap();
    let select = |cx: &mut gpui::VisualTestContext| {
        let text = cx
            .update(|_, cx| {
                ferrite_pane::rich::testing::bounds(&format!("document-{}", file.display()), 0, cx)
            })
            .expect("the document is drawn");
        let y = text.top() + px(12.);
        let from = gpui::point(text.left() + px(4.), y);
        let to = gpui::point(text.left() + px(60.), y);
        cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
        tick(cx);
        assert!(
            cx.update(|window, cx| view.read(cx).panes[0].preview.text_focused(window, cx)),
            "the premise: the preview's text holds the keyboard"
        );
    };
    preview(&view, &file, None, cx);
    select(cx);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |view, _| view.panes[0].preview.document().is_none()),
        "esc closed the preview"
    );
    assert_eq!(*fake.interrupts.borrow(), 0, "and interrupted nothing");
    cx.simulate_input("x");
    assert_eq!(
        view.read_with(cx, |view, cx| view.panes[0]
            .composer
            .read(cx)
            .text()
            .to_string()),
        "x",
        "the keyboard is the Pane's already"
    );
    land(cx);

    preview(&view, &file, None, cx);
    select(cx);
    cx.simulate_keystrokes("cmd-w");
    land(cx);
    assert!(view.read_with(cx, |view, _| view.panes[0].preview.document().is_none()));
    assert_eq!(
        view.read_with(cx, |view, _| view.panes.len()),
        1,
        "the Pane stays"
    );
}

/// Each Thread keeps its own preview: focus moving to another Pane shows
/// that Pane's Panel, at its own width, the first's preview kept; focus
/// coming back finds it, wide.
#[gpui::test]
fn each_thread_keeps_its_own_preview(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window("preview-threads", 2, cx);
    let file = source("preview-threads-file", 20);
    preview(&view, &file, Some(3), cx);
    let wide = panel_w(cx).unwrap();
    let focus = |index: usize, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| {
            view.focus_pane(index);
            cx.notify();
        });
        land(cx);
    };
    focus(1, cx);
    assert_eq!(panel_w(cx), Some(PANEL_W), "the other Thread's Panel");
    assert!(cx.debug_bounds("panel-tab-Preview").is_none());
    assert!(
        view.read_with(cx, |view, _| view.panes[0].preview.is_open()),
        "kept"
    );
    focus(0, cx);
    assert_eq!(panel_w(cx), Some(wide), "found as it was");
    assert!(cx.debug_bounds("panel-preview").is_some());
}

/// An image holds the keyboard in its viewer — yet focus moving to the
/// other Pane (⌘]) takes it there: typing reaches that Pane's Composer.
#[gpui::test]
fn focus_moving_to_another_pane_takes_the_keyboard_from_an_image(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window("preview-image-focus", 2, cx);
    let dir = scratch("preview-image-focus-file");
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join("shot.png");
    image::RgbaImage::from_pixel(40, 30, image::Rgba([90, 140, 200, 255]))
        .save(&image)
        .unwrap();
    preview(&view, &image, None, cx);
    assert!(
        cx.update(|window, cx| view.read(cx).panes[0]
            .preview
            .held_focus(window, cx)
            .is_some()),
        "the premise: the viewer holds the keyboard"
    );
    cx.simulate_keystrokes("cmd-]");
    land(cx);
    cx.simulate_input("x");
    let typed = |index: usize, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.panes[index].composer.read(cx).text().to_string()
        })
    };
    assert_eq!(typed(1, cx), "x", "the other Pane's Composer has it");
    assert_eq!(typed(0, cx), "");
}

/// A draft, which has no Panel of its own, shows the Panel for its preview
/// alone — the Preview tab, no other — and its first send, which makes it a
/// Thread, keeps it as it was: no ride, the same document, the Thread's
/// tabs joining it.
#[gpui::test]
fn a_draft_previews_in_the_panel_and_keeps_it_through_its_first_send(cx: &mut TestAppContext) {
    let base = scratch("preview-draft");
    let repo = repo_in(&base);
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake.clone()));
    bind_band_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    view.update(cx, |view, _| view.aim_launch(&repo));
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.panes[view.focused()].thread().is_none()),
        "the premise: a draft"
    );
    let file = source("preview-draft-file", 20);
    preview(&view, &file, None, cx);
    assert!(cx.debug_bounds("panel-tab-Preview").is_some());
    assert!(
        cx.debug_bounds("panel-tab-Changes").is_none(),
        "no Thread's tabs"
    );
    let wide = panel_w(cx).expect("the Panel shows for it");

    cx.simulate_input("run the tests");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.panes[view.focused()].thread().is_some()),
        "the draft became a Thread"
    );
    assert!(
        !view.read_with(cx, |view, _| view.panel_riding()),
        "no ride"
    );
    assert_eq!(panel_w(cx), Some(wide));
    assert!(
        cx.debug_bounds("panel-preview").is_some(),
        "the same preview"
    );
    assert!(
        cx.debug_bounds("panel-tab-Changes").is_some(),
        "the Thread's tabs join"
    );
}

/// ⌥⌘B while a preview shows puts it away with the Panel: a hidden Panel
/// holds no preview.
#[gpui::test]
fn hiding_the_panel_puts_its_preview_away(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window("preview-hide", 1, cx);
    let file = source("preview-hide-file", 20);
    preview(&view, &file, None, cx);
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    land(cx);
    assert_eq!(panel_w(cx), None);
    assert!(view.read_with(cx, |view, _| view.panes[0].preview.document().is_none()));
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    land(cx);
    assert_eq!(panel_w(cx), Some(PANEL_W), "back on its own tab");
}

//! A finished turn's cards (`ferrite_pane::transcript::outputs`): a file it
//! wrote gets a file card whose `Open in ›` offers Finder and the default
//! app, and whose press reads it in the Panel; a page it wrote or named
//! gets a page card whose `Open` shows it beside the transcript — the
//! Panel's page card, its title menu, expand and × — and, with no engine
//! in this build, the "can't show" note with the browser a click away.

use super::*;

/// A write call that made `path` from nothing.
fn wrote(stream: &std::sync::mpsc::Sender<SessionEvent>, id: &str, path: &str, line: &str) {
    stream
        .send(SessionEvent::ToolStarted {
            id: id.into(),
            name: "Write".into(),
            input: serde_json::json!({ "file_path": path }),
        })
        .unwrap();
    stream
        .send(SessionEvent::ToolCompleted {
            id: id.into(),
            output: "created".into(),
            is_error: false,
            result: ferrite_core::ToolResult::FileEdit {
                path: path.into(),
                hunks: vec![ferrite_core::Hunk {
                    old_start: 0,
                    old_lines: 0,
                    new_start: 1,
                    new_lines: 1,
                    lines: vec![format!("+{line}")],
                    section: None,
                }],
            },
        })
        .unwrap();
}

/// A turn that wrote `notes.md` and `site/index.html` and named a local
/// server, finished, in a 1600 × 900 window: its cards are the note's
/// (0), the page's (1) and the server's (2).
fn turn<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    std::path::PathBuf,
    &'a mut gpui::VisualTestContext,
) {
    let (core, fake, workspace) = bound_cockpit(name, Provider::Claude);
    std::fs::create_dir_all(workspace.join("site")).unwrap();
    std::fs::write(workspace.join("notes.md"), "# Notes\n").unwrap();
    std::fs::write(
        workspace.join("site/index.html"),
        "<html><head><title>Sales report</title></head><body>Q3</body></html>",
    )
    .unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    let streams = fake.streams.borrow();
    wrote(&streams[0], "write-notes", "notes.md", "# Notes");
    wrote(
        &streams[0],
        "write-page",
        "site/index.html",
        "<title>Sales report</title>",
    );
    streams[0]
        .send(SessionEvent::TextDelta {
            text: "Wrote the report; the dev server is at http://localhost:5173/.".into(),
        })
        .unwrap();
    streams[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    drop(streams);
    tick(cx);
    (view, workspace, cx)
}

fn menu_labels(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, _| {
        view.context_menu
            .as_ref()
            .map(|menu| {
                menu.rows
                    .iter()
                    .flatten()
                    .map(|(item, _)| item.label.to_string())
                    .collect()
            })
            .unwrap_or_default()
    })
}

#[gpui::test]
fn a_written_file_gets_a_card_that_opens_it(cx: &mut TestAppContext) {
    let (view, workspace, cx) = turn("output-file-card", cx);
    hide_panel(&view, cx);
    let card = cx
        .debug_bounds("file-card-0")
        .expect("the written file has its card under the answer");
    assert!(
        f32::from(card.size.width) <= ferrite_ui::theme::OUTPUT_CARD_W + 0.5,
        "a card is at most OUTPUT_CARD_W wide"
    );
    assert_eq!(
        f32::from(card.size.height),
        ferrite_ui::theme::OUTPUT_CARD_H
    );
    // Open in ›: Finder and the default app, from the card's own menu.
    let open_in = cx
        .debug_bounds("file-card-open-in-0")
        .expect("the card offers Open in");
    assert!(card.contains(&open_in.center()), "at the card's right");
    cx.simulate_click(open_in.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("output-menu").is_some(), "its menu opens");
    assert!(cx.debug_bounds("output-menu-row-0").is_some());
    assert!(cx.debug_bounds("output-menu-row-1").is_some());
    assert!(
        view.read_with(cx, |view, _| view.panes[0].preview.document().is_none()),
        "Open in is the menu, not the preview"
    );
    // A press on the card itself reads the file in the Panel.
    let card = cx.debug_bounds("file-card-0").unwrap();
    cx.simulate_click(
        gpui::point(card.left() + px(8.), card.center().y),
        gpui::Modifiers::none(),
    );
    tick(cx);
    assert!(cx.debug_bounds("output-menu").is_none(), "the menu goes");
    cx.simulate_click(
        gpui::point(card.left() + px(8.), card.center().y),
        gpui::Modifiers::none(),
    );
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0]
            .preview
            .document()
            .map(|document| document.path)),
        Some(workspace.join("notes.md"))
    );
}

#[gpui::test]
fn a_page_card_opens_the_page_beside_the_transcript(cx: &mut TestAppContext) {
    let (view, workspace, cx) = turn("output-page-card", cx);
    hide_panel(&view, cx);
    assert!(
        cx.debug_bounds("page-card-1").is_some(),
        "the written page has its card"
    );
    assert!(
        cx.debug_bounds("page-card-more-2").is_some(),
        "and the server it named, with its ⋯"
    );
    let open = cx
        .debug_bounds("page-card-open-1")
        .expect("a page card offers Open");
    cx.simulate_click(open.center(), gpui::Modifiers::none());
    tick(cx);
    let source = view.read_with(cx, |view, _| {
        view.panes[0]
            .preview
            .document()
            .and_then(|document| document.page().map(|page| page.source().clone()))
    });
    assert_eq!(
        source,
        Some(PageSource::File(workspace.join("site/index.html")))
    );
    let panel = cx.debug_bounds("panel").expect("the page shows the Panel");
    let title = cx
        .debug_bounds("preview-page-title")
        .expect("the head names the page by its title");
    assert!(panel.contains(&title.center()));
    // No engine in a test build: the "can't show" note, the browser a
    // click away.
    assert!(cx.debug_bounds("preview-unshown").is_some());
    assert!(cx.debug_bounds("preview-open-original").is_some());

    // Its title's menu.
    cx.simulate_click(title.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        menu_labels(&view, cx),
        ["Reload", "Open in browser", "Copy path"]
    );
    view.update(cx, |view, cx| {
        view.context_menu = None;
        cx.notify();
    });
    tick(cx);

    // Expand: the Panel takes all the board can give, and back.
    let wide = f32::from(panel.size.width);
    let expand = cx.debug_bounds("preview-expand").expect("an expand door");
    cx.simulate_click(expand.center(), gpui::Modifiers::none());
    tick(cx);
    let expanded = f32::from(cx.debug_bounds("panel").unwrap().size.width);
    use ferrite_ui::theme::{NAV_WIDTH, PANEL_PREVIEW_BOARD_MIN};
    let most = 1600. - NAV_WIDTH - PANEL_PREVIEW_BOARD_MIN;
    assert!(expanded > wide, "expanded is wider: {expanded} vs {wide}");
    assert!((expanded - most).abs() < 1., "{expanded} vs {most}");
    let expand = cx.debug_bounds("preview-expand").unwrap();
    cx.simulate_click(expand.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        f32::from(cx.debug_bounds("panel").unwrap().size.width),
        wide
    );

    // × closes it, and the hidden Panel with it.
    let close = cx.debug_bounds("preview-close").expect("a ×");
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none());
    assert!(view.read_with(cx, |view, _| !view.panel.expanded));
}

#[gpui::test]
fn a_local_server_opens_as_a_page_it_cannot_draw(cx: &mut TestAppContext) {
    let (view, _, cx) = turn("output-address-card", cx);
    hide_panel(&view, cx);
    let address = PageSource::Address("http://localhost:5173/".into());
    view.update_in(cx, |view, window, _| {
        view.panes[0].preview.open_page(address.clone(), window)
    });
    tick(cx);
    assert!(
        cx.debug_bounds("preview-unshown").is_some(),
        "an address is never drawn here"
    );
    let title = cx.debug_bounds("preview-page-title").unwrap();
    cx.simulate_click(title.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        menu_labels(&view, cx),
        ["Open in browser", "Copy address"],
        "nothing to reload"
    );
}

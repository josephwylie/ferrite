//! A finished turn's cards (`ferrite_pane::transcript::outputs`): the
//! files it changed get T3's changed-files card, whose files open the
//! Changes Surface; a page it wrote or named
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
/// server, finished, in a 1600 × 900 window: its changed-files card, then
/// the page's card (0) and the server's (1).
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

/// The files a turn changed draw T3's changed-files card above its page
/// cards: its head, then the tree, a folder shut until pressed, and a
/// press on a file opens the Changes Surface.
#[gpui::test]
fn the_files_a_turn_changed_draw_one_card_that_opens_the_diff(cx: &mut TestAppContext) {
    let (view, _, cx) = turn("output-changes-card", cx);
    hide_panel(&view, cx);
    let card = cx
        .debug_bounds("changes-card")
        .expect("the changed files have their card under the answer");
    let head = cx.debug_bounds("changes-card-head").unwrap();
    assert_eq!(
        f32::from(head.size.height),
        ferrite_ui::theme::CHANGES_HEAD_H
    );
    assert!(card.contains(&cx.debug_bounds("changes-card-expand").unwrap().center()));
    assert!(card.contains(&cx.debug_bounds("changes-card-open").unwrap().center()));
    let page = cx.debug_bounds("page-card-0").expect("then the page card");
    assert!(page.top() > card.bottom(), "the changes card comes first");

    // Folders first, shut; the file at the root shows.
    let folder = cx
        .debug_bounds("changes-folder-site")
        .expect("the page's folder is a row");
    let file = cx.debug_bounds("changes-file-notes.md").unwrap();
    assert!(folder.top() < file.top(), "folders before files");
    assert_eq!(
        f32::from(file.size.height),
        ferrite_ui::theme::CHANGES_ROW_H
    );
    assert!(cx.debug_bounds("changes-file-site/index.html").is_none());
    cx.simulate_click(folder.center(), gpui::Modifiers::none());
    tick(cx);
    let inner = cx
        .debug_bounds("changes-file-site/index.html")
        .expect("a press opens the folder");
    let file = cx.debug_bounds("changes-file-notes.md").unwrap();
    assert!(
        inner.top() >= folder.bottom() && inner.bottom() <= file.top(),
        "its file under it, above the root's"
    );

    // A file opens the Changes Surface.
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let file = cx.debug_bounds("changes-file-notes.md").unwrap();
    cx.simulate_click(file.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        panel_tabs(&view, thread, cx).1,
        Some(crate::cockpit::surfaces::SurfaceId::Changes)
    );
}

#[gpui::test]
fn a_page_card_opens_the_page_beside_the_transcript(cx: &mut TestAppContext) {
    let (view, workspace, cx) = turn("output-page-card", cx);
    hide_panel(&view, cx);
    assert!(
        cx.debug_bounds("page-card-0").is_some(),
        "the written page has its card"
    );
    assert!(
        cx.debug_bounds("page-card-more-1").is_some(),
        "and the server it named, with its ⋯"
    );
    let open = cx
        .debug_bounds("page-card-open-0")
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

    // Maximize: the Panel takes all the board can give, and back.
    let wide = f32::from(panel.size.width);
    let expand = cx.debug_bounds("panel-maximize").expect("a maximize door");
    cx.simulate_click(expand.center(), gpui::Modifiers::none());
    tick(cx);
    let expanded = f32::from(cx.debug_bounds("panel").unwrap().size.width);
    // Beside the full column the board would fall under its least, so the
    // nav folds away and the Panel takes its room too.
    use ferrite_ui::theme::DOCK_THREAD_MIN_W;
    assert!(cx.debug_bounds("nav-card").is_none(), "the nav folds");
    let most = 1600. - DOCK_THREAD_MIN_W;
    assert!(expanded > wide, "expanded is wider: {expanded} vs {wide}");
    assert!((expanded - most).abs() < 1., "{expanded} vs {most}");
    let expand = cx.debug_bounds("panel-restore").unwrap();
    cx.simulate_click(expand.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        f32::from(cx.debug_bounds("panel").unwrap().size.width),
        wide
    );

    // Closing its tab closes the page.
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let page = crate::cockpit::surfaces::SurfaceId::File(workspace.join("site/index.html"));
    assert!(panel_tabs(&view, thread, cx).0.contains(&page), "the page is a File tab");
    view.update(cx, |view, cx| view.close_surface(thread, &page, cx));
    tick(cx);
    assert!(!panel_tabs(&view, thread, cx).0.contains(&page));
    assert!(cx.debug_bounds("preview-page-title").is_none());
}

#[gpui::test]
fn a_local_server_uses_the_thread_browser_preview(cx: &mut TestAppContext) {
    let (view, _, cx) = turn("output-address-card", cx);
    hide_panel(&view, cx);
    view.update_in(cx, |view, window, _| {
        view.panes[0].preview.open_page(PageSource::Address("http://localhost:5173/".into()), window)
    });
    tick(cx);
    assert!(cx.debug_bounds("browser-preview").is_some(), "local addresses use the browser controls");
    assert!(cx.debug_bounds("preview-page").is_none(), "the sealed HTML host is reserved for files");
    assert!(cx.debug_bounds("panel-maximize").is_some(), "the Panel's one maximize");
}

/// A settled `ReportFindings` call, opened, draws its findings grouped by
/// file (HD70); a failed one draws none and stays a failure.
#[gpui::test]
fn a_settled_findings_call_draws_its_findings_by_file(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("findings-card", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let stream = fake.streams.borrow()[0].clone();
    stream
        .send(SessionEvent::ToolStarted {
            id: "review".into(),
            name: "ReportFindings".into(),
            input: serde_json::json!({ "findings": [
                { "file": "a.rs", "line": 2, "summary": "Divides by zero", "failure_scenario": "main panics", "verdict": "CONFIRMED" },
                { "file": "a.rs", "line": 9, "summary": "Indexes an empty Vec", "failure_scenario": "main panics" },
                { "file": "b.rs", "summary": "Leaks a handle", "failure_scenario": "fds run out" },
            ]}),
        })
        .unwrap();
    stream
        .send(SessionEvent::ToolCompleted {
            id: "review".into(),
            output: "3 findings reported.".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    stream.send(SessionEvent::TextDelta { text: "Done.\n\n".into() }).unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.panes[0].toggle_tool(&pane::DisclosureId::Tool("review".into()));
        cx.notify();
    });
    tick(cx);
    assert!(cx.debug_bounds("findings").is_some(), "the card is drawn");
}

/// A foreground command still running shows what it has printed so far when
/// its row is opened (TR10), and keeps filling as more arrives.
#[gpui::test]
fn a_running_command_opens_onto_its_output_so_far(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("live-output", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let stream = fake.streams.borrow()[0].clone();
    stream
        .send(SessionEvent::ToolStarted {
            id: "build".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo build" }),
        })
        .unwrap();
    stream
        .send(SessionEvent::ToolOutputDelta { id: "build".into(), text: "Compiling a\n".into() })
        .unwrap();
    tick(cx);
    open_live(cx);
    view.update(cx, |view, cx| {
        view.panes[0].toggle_tool(&pane::DisclosureId::Tool("build".into()));
        cx.notify();
    });
    tick(cx);
    let before = cx.debug_bounds("tool-output").expect("the output so far").size.height;
    stream
        .send(SessionEvent::ToolOutputDelta { id: "build".into(), text: "Compiling b\nCompiling c\n".into() })
        .unwrap();
    tick(cx);
    let after = cx.debug_bounds("tool-output").expect("still shown").size.height;
    assert!(after > before, "it fills as output arrives: {before:?} -> {after:?}");
}

/// A steered turn's changed-files card spans the whole turn, so a folder
/// the operator opened on it stays open as the transcript moves on, even
/// when its file was written before the steer.
#[gpui::test]
fn a_steered_turns_opened_folder_stays_open(cx: &mut TestAppContext) {
    let (mut core, fake, workspace) = bound_cockpit("steered-folder", Provider::Claude);
    let first = core.threads()[0];
    core.send(first, "write the site".into());
    std::fs::create_dir_all(workspace.join("site")).unwrap();
    std::fs::create_dir_all(workspace.join("docs")).unwrap();
    std::fs::write(workspace.join("site/index.html"), "<p>x</p>").unwrap();
    std::fs::write(workspace.join("docs/b.md"), "b").unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    let stream = fake.streams.borrow()[0].clone();
    wrote(&stream, "w1", "site/index.html", "<p>x</p>");
    stream
        .send(SessionEvent::Queue(ferrite_core::QueueEvent::Started {
            historical: false,
            client_id: "steer-1".into(),
            text: Some("and the docs".into()),
        }))
        .unwrap();
    wrote(&stream, "w2", "docs/b.md", "b");
    stream.send(SessionEvent::TextDelta { text: "Done.".into() }).unwrap();
    stream
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    hide_panel(&view, cx);
    let steered = view.read_with(cx, |view, _| {
        let thread = view.cockpit.threads()[0];
        view.cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .any(|block| block.steer)
    });
    assert!(steered, "premise: the turn holds a steer");
    let folder = cx.debug_bounds("changes-folder-site").expect("the site folder");
    cx.simulate_click(folder.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds("changes-file-site/index.html").is_some(),
        "premise: the press opened it"
    );
    stream.send(SessionEvent::TextDelta { text: "More.".into() }).unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("changes-file-site/index.html").is_some(),
        "the folder the operator opened stays open"
    );
}

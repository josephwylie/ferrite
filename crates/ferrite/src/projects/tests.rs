//! The Projects sheet as the window drives it — the popup's sections and
//! its project picker, on a scratch root — and the snapshot the nav reads.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use ferrite_core::ThreadId;
use ferrite_projects::keys::{KeyCode, KeyEvent, KeyModifiers};
use ferrite_projects::project;
use ferrite_projects::thread::{self, Group};
use gpui::{Entity, TestAppContext, VisualTestContext};

use super::*;

/// A projects root of its own, removed with it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("ferrite-projects-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("config")).unwrap();
        Scratch(dir)
    }

    /// The place, with a project made for each name.
    fn place(&self, names: &[&str]) -> Place {
        let root = self.0.join("root");
        for name in names {
            project::create(&root, name, "", Vec::new()).unwrap();
        }
        Place {
            root,
            config_dir: self.0.join("config"),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn press(sheet: &Entity<ProjectsSheet>, cx: &mut VisualTestContext, code: KeyCode) {
    sheet.update(cx, |sheet, cx| {
        sheet.key(KeyEvent::new(code, KeyModifiers::NONE), cx)
    });
}

fn typed(sheet: &Entity<ProjectsSheet>, cx: &mut VisualTestContext, text: &str) {
    for c in text.chars() {
        press(sheet, cx, KeyCode::Char(c));
    }
}

/// The popup on `place`, as wide as a window's sheet.
fn open<'a>(
    cx: &'a mut TestAppContext,
    place: Place,
) -> (Entity<ProjectsSheet>, &'a mut VisualTestContext) {
    let (sheet, cx) = cx.add_window_view(|_, cx| ProjectsSheet::popup(place, None, cx));
    sheet.update(cx, |sheet, _| sheet.fit(1400.0, 600.0));
    (sheet, cx)
}

/// The screen once the popup has answered every key.
fn screen(sheet: &Entity<ProjectsSheet>, cx: &mut VisualTestContext) -> Vec<String> {
    sheet.update(cx, |sheet, cx| {
        sheet.settle(cx);
        sheet.screen()
    })
}

#[gpui::test]
fn tab_and_shift_tab_walk_the_six_sections_and_wrap(cx: &mut TestAppContext) {
    let scratch = Scratch::new("sections");
    let place = scratch.place(&["Alpha"]);
    let (sheet, cx) = open(cx, place);
    let header = screen(&sheet, cx)[0].clone();
    assert!(
        header.starts_with(" Projects · all projects   [threads] · tasks · inbox"),
        "{header}"
    );
    assert!(header.ends_with("  1 project · 0 need you"), "{header}");
    for name in [
        "tasks", "inbox", "routines", "settings", "memory", "threads",
    ] {
        press(&sheet, cx, KeyCode::Tab);
        let header = screen(&sheet, cx)[0].clone();
        assert!(header.contains(&format!("[{name}]")), "{name}: {header}");
    }
    press(&sheet, cx, KeyCode::BackTab);
    assert!(
        screen(&sheet, cx)[0].contains("[memory]"),
        "shift-tab wraps back"
    );
    press(&sheet, cx, KeyCode::Right);
    assert!(screen(&sheet, cx)[0].contains("[threads]"));
    press(&sheet, cx, KeyCode::Left);
    assert!(screen(&sheet, cx)[0].contains("[memory]"));
    // Each section's keys, as upstream says them, in the footer.
    let footer = screen(&sheet, cx).last().cloned().unwrap();
    assert!(
        footer.contains("↵ read  P project  / find  tab section  esc close"),
        "{footer}"
    );
}

#[gpui::test]
fn the_picker_filters_on_name_and_slug_and_scopes_the_popup(cx: &mut TestAppContext) {
    let scratch = Scratch::new("picker");
    let place = scratch.place(&["Alpha", "Beta"]);
    let (sheet, cx) = open(cx, place);
    press(&sheet, cx, KeyCode::Char('/'));
    typed(&sheet, cx, "be");
    let lines = screen(&sheet, cx);
    assert!(
        lines[2].starts_with(" Switch to project  / be\u{258f}"),
        "{lines:?}"
    );
    assert!(lines[3].starts_with("   Beta (beta) · idle"), "{lines:?}");
    assert!(
        !lines.iter().any(|line| line.contains("Alpha")),
        "{lines:?}"
    );
    press(&sheet, cx, KeyCode::Enter);
    let header = screen(&sheet, cx)[0].clone();
    assert!(
        header.starts_with(" Projects · beta   [threads]"),
        "{header}"
    );
    // P opens on the scope, marked; esc leaves it as it was.
    press(&sheet, cx, KeyCode::Char('P'));
    let lines = screen(&sheet, cx);
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with(" • Beta (beta) · ")),
        "{lines:?}"
    );
    press(&sheet, cx, KeyCode::Esc);
    assert!(screen(&sheet, cx)[0].starts_with(" Projects · beta "));
    // Nothing matches: it says so.
    press(&sheet, cx, KeyCode::Char('/'));
    typed(&sheet, cx, "zz");
    assert!(screen(&sheet, cx)
        .iter()
        .any(|line| line.starts_with("  no projects match")));
}

#[gpui::test]
fn esc_closes_the_popup(cx: &mut TestAppContext) {
    let scratch = Scratch::new("close");
    let place = scratch.place(&["Alpha"]);
    let (sheet, cx) = open(cx, place);
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&sheet, move |_, event: &SheetEvent, _| {
            seen.borrow_mut().push(event.clone())
        })
        .detach()
    });
    screen(&sheet, cx);
    press(&sheet, cx, KeyCode::Esc);
    screen(&sheet, cx);
    assert_eq!(*events.borrow(), [SheetEvent::Close]);
}

#[test]
fn the_snapshot_lists_open_threads_by_need_with_their_sub_lines() {
    let scratch = Scratch::new("model");
    let place = scratch.place(&["Alpha"]);
    let alpha = project::Project::load(&place.root, "alpha").unwrap();
    thread::allocate(&alpha, |t| {
        t.title = "First".into();
        t.status = thread::Status::Open;
        t.last_group = "working".into();
        t.pane_id = "11".into();
        t.state_line = "working · ~40%".into();
        t.activity = "Writing tests".into();
    })
    .unwrap();
    thread::allocate(&alpha, |t| {
        t.title = "Second".into();
        t.status = thread::Status::Open;
        t.last_group = "waiting-on-you".into();
        t.pane_id = "12".into();
        t.state_line = "needs you · blocked".into();
    })
    .unwrap();
    thread::allocate(&alpha, |t| {
        t.title = "Done".into();
        t.status = thread::Status::Resolved;
        t.pane_id = "13".into();
    })
    .unwrap();
    thread::allocate(&alpha, |t| {
        t.title = "Elsewhere".into();
        t.status = thread::Status::Open;
        t.machine = "box".into();
        t.pane_id = "14".into();
    })
    .unwrap();

    let model = Model::load(&place);
    assert!(!model.ticker, "no ticker runs on a scratch root");
    assert_eq!(model.needs_you, None, "no stale count without the ticker");
    let project = &model.projects[0];
    assert_eq!(
        (project.slug.as_str(), project.name.as_str()),
        ("alpha", "Alpha")
    );
    assert_eq!(project.summary, "1 need you · 2 working");
    let rows: Vec<(&str, Option<ThreadId>, Group, &str, &str)> = project
        .threads
        .iter()
        .map(|row| {
            (
                row.id.as_str(),
                row.thread,
                row.group,
                row.display.as_str(),
                row.sub.as_str(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                "t-0002",
                Some(ThreadId::new(12)),
                Group::WaitingOnYou,
                "t-0002 · Second",
                "needs you · blocked"
            ),
            (
                "t-0001",
                Some(ThreadId::new(11)),
                Group::Working,
                "t-0001 · First",
                "~40% · Writing tests"
            ),
            ("t-0004", None, Group::Working, "t-0004 · Elsewhere", ""),
        ],
        "needs you first; another machine's pane is no Thread here"
    );
    assert_eq!(
        model.project_of(ThreadId::new(12)).map(|p| p.slug.as_str()),
        Some("alpha")
    );
    assert!(
        model.project_of(ThreadId::new(13)).is_none(),
        "a resolved thread is no one's"
    );
    assert_eq!(
        adopt_refusal(&model, ThreadId::new(11)).as_deref(),
        Some("pane 11 is already thread t-0001 of `alpha`")
    );
}

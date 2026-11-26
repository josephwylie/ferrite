//! What a provider CLI does for a Machine's work is done there: a draft's
//! `/` menu lists the commands box's Server found, box's import picker
//! lists the sessions on box, and box's Server titles box's Threads.
//! This computer's CLI and its session files never answer for box.

use super::environments::{focused_draft, registered, resolved_dir};
use super::*;
use ferrite_core::session_file::{Candidate, Found};

/// box's provider CLI, as box's Server runs it: a command, a session and
/// a title of box's own, and how often each was asked for.
#[derive(Clone, Default)]
struct BoxCli {
    commands_asked: Arc<AtomicUsize>,
    titles: Arc<Mutex<Titler>>,
}

/// box's titler: how often it was asked, and, while it holds its answers,
/// the ones it owes.
#[derive(Default)]
struct Titler {
    asked: usize,
    holding: bool,
    owed: Vec<(ThreadId, ferrite_core::doorbell::Sender<ferrite_engine::titler::Titled>)>,
}

const BOX_TITLE: &str = "Titled on box";

impl BoxCli {
    fn titles_asked(&self) -> usize {
        self.titles.lock().unwrap().asked
    }

    /// From now on its titles wait for `write_held`.
    fn hold_titles(&self) {
        self.titles.lock().unwrap().holding = true;
    }

    /// The titles it held, written.
    fn write_held(&self) {
        for (thread, replies) in self.titles.lock().unwrap().owed.drain(..) {
            let _ = replies.send(ferrite_engine::titler::Titled {
                thread,
                title: Some(BOX_TITLE.into()),
            });
        }
    }
}

impl ferrite_engine::engine::Spawner for BoxCli {
    fn spawn(
        &mut self,
        _: ferrite_engine::engine::SpawnRequest,
    ) -> std::io::Result<Box<dyn ferrite_engine::providers::Session>> {
        let (alive, events) = ferrite_core::doorbell::channel();
        Ok(Box::new(Quiet {
            _alive: alive,
            events,
        }))
    }

    fn discover_commands(
        &mut self,
        _: Provider,
        _: &std::path::Path,
    ) -> Option<ferrite_core::Discovery> {
        self.commands_asked.fetch_add(1, Ordering::SeqCst);
        let (answer, listed) = ferrite_core::doorbell::channel();
        let _ = answer.send(Ok(vec![ferrite_core::SessionCommand {
            name: "on-box".into(),
            description: "box's own command".into(),
            path: None,
            source: None,
        }]));
        Some(listed)
    }

    fn discover_sessions(
        &mut self,
        _: Vec<(Provider, std::path::PathBuf)>,
        _: usize,
    ) -> Option<mpsc::Receiver<std::io::Result<Found>>> {
        let (answer, listed) = ferrite_core::doorbell::channel();
        let _ = answer.send(Ok(Found {
            sessions: vec![Candidate {
                provider: Provider::Claude,
                path: "/on-box/sessions/box-session.jsonl".into(),
                modified: Some(std::time::SystemTime::now()),
                title: Some("Box's session".into()),
                cwd: Some("/on-box/src/app".into()),
                session_id: Some("box-session".into()),
            }],
            roots: vec!["/on-box/sessions".into()],
        }));
        Some(listed)
    }

    fn title(
        &mut self,
        job: ferrite_engine::titler::Job,
        replies: ferrite_core::doorbell::Sender<ferrite_engine::titler::Titled>,
    ) {
        let mut titler = self.titles.lock().unwrap();
        titler.asked += 1;
        if titler.holding {
            titler.owed.push((job.thread, replies));
            return;
        }
        let _ = replies.send(ferrite_engine::titler::Titled {
            thread: job.thread,
            title: Some(BOX_TITLE.into()),
        });
    }
}

/// A draft in a Project of box's, in `folder`, focused in a window over
/// this computer and box.
fn box_draft<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    folder: &std::path::Path,
    cli: &BoxCli,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext, MachineId, PaneIdentity) {
    bind_production_keys(cx);
    let (mut core, _fake, machines) = beside_serving(name, 1, box_machine(), Box::new(cli.clone()));
    let on_box = machines.held(1).unwrap();
    std::fs::create_dir_all(folder).unwrap();
    let theirs = registered(&mut core, on_box, folder);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(theirs, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    let composer = view.read_with(cx, |view, cx| view.panes[view.index_of(draft).unwrap()].read(cx).composer.clone());
    cx.update(|window, cx| window.focus(&composer.read(cx).focus_handle(cx), cx));
    tick(cx);
    (view, cx, on_box, draft)
}

/// The names the open `/` menu lists.
fn menu_rows(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, cx| {
        view.popover(cx)
            .map(|menu| menu.rows.iter().map(|row| row.name.to_string()).collect())
            .unwrap_or_default()
    })
}

/// A draft on box lists box's commands under `/`, found by box's Server in
/// box's folder. The folder is gone from this computer's disk, as another
/// computer's folder would be: this computer's CLI cannot list it.
#[gpui::test]
fn a_drafts_commands_on_a_machine_are_that_machines(cx: &mut TestAppContext) {
    let cli = BoxCli::default();
    let folder = resolved_dir("box-commands").join("on-box");
    let (view, cx, _, _) = box_draft(cx, "box-commands", &folder, &cli);
    std::fs::remove_dir_all(&folder).unwrap();
    cx.simulate_input("/");
    until(cx, |cx| !menu_rows(&view, cx).iter().any(|row| row.starts_with("Searching")));
    let rows = menu_rows(&view, cx);
    assert!(rows.iter().any(|row| row == "on-box"), "box's command is listed: {rows:?}");
    assert_eq!(cli.commands_asked.load(Ordering::SeqCst), 1, "box was asked once");
}

/// This computer's session files, under `root` alone, for its own import
/// picker.
fn this_computers_sessions(view: &Entity<CockpitView>, root: &std::path::Path, cx: &mut gpui::VisualTestContext) {
    let roots = vec![(Provider::Claude, root.to_path_buf())];
    view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_session_roots(roots))
    });
}

/// box's import picker lists the sessions box's Server found on box, not
/// the ones on this computer.
#[gpui::test]
fn a_machines_import_picker_lists_that_machines_sessions(cx: &mut TestAppContext) {
    let cli = BoxCli::default();
    let base = resolved_dir("box-sessions");
    let (view, cx, _, _) = box_draft(cx, "box-sessions", &base.join("on-box"), &cli);
    let here_root = base.join("this-computer").join("claude-projects");
    let session = here_root.join("-workspace").join("here-session.jsonl");
    std::fs::create_dir_all(session.parent().unwrap()).unwrap();
    std::fs::write(
        &session,
        concat!(
            r#"{"type":"user","sessionId":"here-session","cwd":"/workspace","message":{"role":"user","content":"This computer's session"}}"#,
            "\n",
        ),
    )
    .unwrap();
    this_computers_sessions(&view, &here_root, cx);
    cx.simulate_input("/im");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    let rows = |cx: &mut gpui::VisualTestContext| -> Option<Vec<String>> {
        view.read_with(cx, |view, cx| {
            let palette = view.floats.palette.as_ref()?.read(cx);
            if palette.scope != crate::palette::PaletteScope::Sessions || palette.listing.is_some() {
                return None;
            }
            Some(palette.survivors().iter().map(|row| row.name.to_string()).collect())
        })
    };
    until(cx, |cx| rows(cx).is_some_and(|rows| !rows.is_empty()));
    assert_eq!(rows(cx).unwrap(), ["Box's session"], "box's session, and only box's");
}

/// `text` sent from the focused draft on box: the Thread it starts there.
fn first_prompt(
    view: &Entity<CockpitView>,
    on_box: MachineId,
    draft: PaneIdentity,
    text: &str,
    cx: &mut gpui::VisualTestContext,
) -> ThreadKey {
    cx.simulate_input(text);
    cx.simulate_keystrokes("enter");
    until(cx, |cx| view.read_with(cx, |view, _| view.index_of(draft).is_none()));
    view.read_with(cx, |view, cx| {
        let threads = view.cockpit.read(cx).threads();
        threads.into_iter().find(|thread| thread.machine() == on_box).expect("box's Thread")
    })
}

/// `thread`'s title, as the window holds it.
fn title_of(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, cx| {
        view.cockpit.read(cx).thread(thread).and_then(|open| open.title().map(str::to_string))
    })
}

/// Whether box's Server says it is writing `thread`'s title.
fn titling(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) -> bool {
    view.read_with(cx, |view, cx| view.cockpit.read(cx).thread(thread).is_some_and(|open| open.titling()))
}

/// A Thread on box is titled by box's Server from its first prompt, once,
/// and every window attached to box sees the title.
#[gpui::test]
fn a_thread_on_a_machine_is_titled_by_its_server(cx: &mut TestAppContext) {
    let cli = BoxCli::default();
    let name = "box-titles";
    let (view, cx, on_box, draft) = box_draft(cx, name, &resolved_dir(name).join("on-box"), &cli);
    let mut other = another_client(name);
    let thread = first_prompt(&view, on_box, draft, "Fix the login page", cx);
    until(cx, |cx| title_of(&view, thread, cx).is_some());
    assert_eq!(title_of(&view, thread, cx).as_deref(), Some(BOX_TITLE));
    until(cx, |cx| view.read_with(cx, |view, _| view.facts.name(thread).as_ref() == BOX_TITLE));
    ferrite_core::doorbell::until("the other window never saw the title", || {
        other.pump();
        let theirs = other.threads();
        theirs.iter().any(|thread| {
            other.thread(*thread).and_then(|open| open.title().map(str::to_string)).as_deref()
                == Some(BOX_TITLE)
        })
    });
    assert_eq!(cli.titles_asked(), 1, "titled once, for both windows");
}

/// The operator's name for a Thread stands: one typed while box's titler
/// writes is the one it keeps.
#[gpui::test]
fn a_rename_while_a_machine_titles_its_thread_stands(cx: &mut TestAppContext) {
    let cli = BoxCli::default();
    cli.hold_titles();
    let name = "box-title-renamed";
    let (view, cx, on_box, draft) = box_draft(cx, name, &resolved_dir(name).join("on-box"), &cli);
    let thread = first_prompt(&view, on_box, draft, "Fix the login page", cx);
    until(cx, |cx| titling(&view, thread, cx));
    let mut renamed = view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.rename_thread(thread, "Login, my way"))
    });
    let mut answer = None;
    until(cx, |_| {
        answer = answer.take().or_else(|| renamed.answer());
        answer.is_some()
    });
    assert!(answer.unwrap().is_ok(), "renamed");
    cli.write_held();
    until(cx, |cx| !titling(&view, thread, cx));
    assert_eq!(title_of(&view, thread, cx).as_deref(), Some("Login, my way"));
    assert_eq!(cli.titles_asked(), 1);
}

/// With box's *Name threads automatically* off, box's Server asks its
/// titler nothing.
#[gpui::test]
fn a_machine_with_titles_off_titles_nothing(cx: &mut TestAppContext) {
    let cli = BoxCli::default();
    let name = "box-titles-off";
    let (view, cx, on_box, draft) = box_draft(cx, name, &resolved_dir(name).join("on-box"), &cli);
    let off = ferrite_core::settings::ServerDefaults {
        auto_title: false,
        ..Default::default()
    };
    let written = view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.server_defaults(on_box, Some(off)))
    });
    let written = written.expect("asked");
    until(cx, |_| written.try_recv().is_ok());
    let thread = first_prompt(&view, on_box, draft, "Fix the login page", cx);
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.cockpit.read(cx).thread(thread).is_some_and(|open| open.first_prompt_sent())
        })
    });
    assert_eq!(cli.titles_asked(), 0, "nothing asked");
    assert!(!titling(&view, thread, cx));
    assert_eq!(title_of(&view, thread, cx), None);
}

//! Another machine's Thread is usable in place, as this computer's is:
//! every verb of its row menu and the palette reaches it on its own
//! machine, and every surface that draws its records streams them.

use super::*;

/// Pump until `done`, failing with `what` if it never is.
#[track_caller]
fn until_said(
    cx: &mut gpui::VisualTestContext,
    what: &str,
    mut done: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) {
    ferrite_core::doorbell::until(what, || {
        leap(cx, crate::cockpit::user_commands::ENDED_POLL);
        done(cx)
    });
}

/// box's row in the nav.
fn box_row(
    cx: &mut gpui::VisualTestContext,
    machine: MachineId,
    thread: ThreadKey,
) -> gpui::Bounds<Pixels> {
    debug_bounds(cx, format!("nav-thread-{}", on(machine, thread))).expect("box's row")
}

/// Rename from box's row menu opens the field in that row, on its title,
/// and ↵ renames the Thread on box (D1): the field is drawn where the
/// keys are sent.
#[gpui::test]
fn rename_from_a_machines_row_menu_renames_it_there(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake, machines, _agents) = two_machines("row-rename", 1);
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    tick(cx);
    let row = box_row(cx, on_box, theirs);
    cx.simulate_mouse_down(row.center(), gpui::MouseButton::Right, gpui::Modifiers::none());
    tick(cx);
    cx.simulate_keystrokes("r");
    tick(cx);
    let field = cx.debug_bounds("rename-field").expect("the rename field is drawn");
    assert!(row.contains(&field.origin), "in box's row: {field:?} {row:?}");
    let editor = view.read_with(cx, |view, _| view.rename.shown().unwrap().1.clone());
    editor.update(cx, |editor, cx| editor.set("Renamed on box".into(), cx));
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(cx.debug_bounds("rename-field").is_none(), "it closed");
    // Read back by another client of box's Server: the title is box's.
    let mut other = another_client("row-rename");
    let on_other = other.key(theirs.thread());
    ferrite_core::doorbell::until("box never had the title", || {
        other.pump();
        other.thread(on_other).and_then(|open| open.title().map(str::to_string)).as_deref()
            == Some("Renamed on box")
    });
}

/// A side chat on box's Thread shows box's answer (D2): its fork is drawn
/// in the Panel, so box streams it as it streams a Pane's Thread. Only
/// what follows the Thread's history shows.
#[gpui::test]
fn a_side_chat_on_a_machines_thread_shows_its_answer(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake, machines, agents) = two_machines("side-chat", 1);
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    view.update(cx, |view, cx| view.focus_thread(theirs, cx));
    agents.say(0, SessionEvent::TextDelta { text: "history words\n\n".into() });
    agents.say(0, SessionEvent::TurnEnded { outcome: ferrite_core::TurnOutcome::Completed, cost_usd: None });
    until_said(cx, "box's Thread never answered", |cx| {
        shows(&view, theirs, "history words", cx) && !running(&view, theirs, cx)
    });

    view.update(cx, |view, cx| view.open_side_chat(theirs, cx));
    let fork = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| view.side_chat.read(cx).shown_fork(theirs))
    };
    until_said(cx, "the tab never took its fork", |cx| fork(cx).is_some());
    let fork = fork(cx).unwrap();
    assert_eq!(fork.machine(), on_box, "the fork is box's");
    until_said(cx, "box never streamed the fork's history", |cx| {
        shows(&view, fork, "history words", cx)
    });
    tick(cx);
    assert!(
        cx.debug_bounds("side-chat-empty").is_some(),
        "the tab shows none of the history it inherited"
    );
    cx.simulate_input("what was said?");
    cx.simulate_keystrokes("enter");
    until_said(cx, "box's fork never heard the question", |_| agents.sent("what was said?"));
    agents.say(1, SessionEvent::TextDelta { text: "the side answer\n\n".into() });
    until_said(cx, "the side chat never streamed box's answer", |cx| {
        shows(&view, fork, "the side answer", cx)
    });
    tick(cx);
    assert!(cx.debug_bounds("side-chat-empty").is_none(), "the tab shows the answer");
}

/// The nav's search finds box's open and parked Threads beside this
/// computer's, its Parked shelf lists box's parked Thread and a click
/// there revives it on box, and the shelf's Delete parked threads offers
/// exactly what the shelf lists.
#[gpui::test]
fn search_and_the_parked_shelf_hold_every_machines_threads(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake, machines, _agents) = two_machines("lists", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let open_there = open_on(&mut core, on_box);
    let parked_there = open_on(&mut core, on_box);
    let renamed = core.rename_thread(mine, "Needle here");
    core.wait_for(renamed).unwrap();
    let renamed = core.rename_thread(open_there, "Needle open on box");
    core.wait_for(renamed).unwrap();
    let renamed = core.rename_thread(parked_there, "Needle parked on box");
    core.wait_for(renamed).unwrap();
    // A prompt gives its parked row an age.
    let _ = core.send(parked_there, "go".into());
    let asked = core.park(parked_there);
    core.wait_for(asked).unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until_said(cx, "box's park never reached the window", |cx| {
        view.read_with(cx, |view, _| view.facts.parked().contains(&parked_there))
    });
    tick(cx);

    let field = cx.debug_bounds("nav-search").expect("the field");
    cx.simulate_click(field.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("needle");
    tick(cx);
    let found = view.read_with(cx, |view, cx| {
        let state = view.nav_state(cx);
        let mut found: Vec<ThreadKey> =
            view.nav_view.read(cx).search_matches(&state, cx).iter().map(|row| row.key).collect();
        found.sort();
        found
    });
    let mut every = vec![mine, open_there, parked_there];
    every.sort();
    assert_eq!(found, every, "the search holds every machine's Threads");
    for thread in [open_there, parked_there] {
        let result = debug_bounds(cx, format!("nav-result-{}", on(on_box, thread)))
            .expect("box's result");
        let glyph = debug_bounds(cx, format!("nav-card-machine-{}", on(on_box, thread)))
            .expect("a result wears its machine's glyph");
        assert!(result.contains(&glyph.center()), "{glyph:?} on {result:?}");
        assert_eq!(tooltip_at(cx, glyph.center()).as_deref(), Some("box"), "it names box");
    }
    assert!(
        debug_bounds(cx, format!("nav-card-machine-{}", mine.thread().get())).is_none(),
        "this computer's result draws none"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);

    view.update(cx, |view, cx| view.set_shelf_open(nav::Shelf::Parked, true, cx));
    tick(cx);
    let parked = view.read_with(cx, |view, cx| {
        view.nav_state(cx).parked.iter().map(|row| row.key).collect::<Vec<_>>()
    });
    assert_eq!(parked, [parked_there], "box's parked Thread is on the shelf");
    let delete = view.read_with(cx, |view, cx| view.parked_threads(cx));
    assert_eq!(delete, parked, "Delete parked threads takes what the shelf lists");
    let row = debug_bounds(cx, format!("nav-thread-{}", on(on_box, parked_there)))
        .expect("its row on the shelf");
    let glyph = debug_bounds(cx, format!("nav-card-machine-{}", on(on_box, parked_there)))
        .expect("the slim row wears its machine's glyph");
    assert!(row.contains(&glyph.center()), "{glyph:?} on {row:?}");
    assert_eq!(glyph.size.width, px(ferrite_ui::theme::NAV_MACHINE_GLYPH));
    let time = debug_bounds(cx, format!("nav-slim-time-{}", on(on_box, parked_there)))
        .expect("its age");
    assert_eq!(
        time.right(),
        row.right() - px(ferrite_ui::theme::NAV_SLIM_PAD_X),
        "the age ends the row's right column"
    );
    assert_eq!(
        glyph.right() + px(ferrite_ui::theme::SPACE_1),
        time.left(),
        "the glyph stands just before it"
    );
    assert_eq!(tooltip_at(cx, glyph.center()).as_deref(), Some("box"), "it names box");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    until_said(cx, "box's parked Thread never revived", |cx| {
        view.read_with(cx, |view, cx| view.cockpit.read(cx).thread(parked_there).is_some())
    });
}

/// Every verb of box's row menu, and the palette's Thread verbs, act on
/// box's Thread on box (D1), as this computer's do on its own: none
/// falls on this computer's Thread, and box's menu leaves none out but
/// what only this computer's paths can do. Rename is the test above.
#[gpui::test]
fn every_row_and_palette_verb_reaches_its_threads_machine(cx: &mut TestAppContext) {
    use crate::cockpit::context_menu::Submenu;
    use crate::palette::Command;
    bind_production_keys(cx);
    let (mut core, _fake, machines, agents) = two_machines("verbs", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    agents.say(0, SessionEvent::TextDelta { text: "box's answer\n\n".into() });
    agents.say(0, SessionEvent::TurnEnded { outcome: ferrite_core::TurnOutcome::Completed, cost_usd: None });
    let run = |verb: MenuVerb, cx: &mut gpui::VisualTestContext| {
        view.update_in(cx, |view, window, cx| {
            view.run_menu_verb(MenuTarget::Thread(theirs), verb, window, cx)
        });
        tick(cx);
    };
    let offers = |verb: MenuVerb, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.context_rows(MenuTarget::Thread(theirs), cx)
                .iter()
                .any(|row| matches!(row, Some((_, found)) if *found == verb))
        })
    };
    // Open, and snoozed or not; None while it is not open.
    let snoozed = |thread: ThreadKey, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.cockpit.read(cx).thread(thread).map(|open| open.snooze().is_some())
        })
    };
    let box_threads = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let threads = view.cockpit.read(cx).threads();
            threads.into_iter().filter(|thread| thread.machine() == on_box).count()
        })
    };
    let clipboard =
        |cx: &mut gpui::VisualTestContext| cx.read_from_clipboard().and_then(|item| item.text());
    for verb in [
        MenuVerb::Pin,
        MenuVerb::Rename,
        MenuVerb::Fork,
        MenuVerb::NewThread,
        MenuVerb::CopyPath,
        MenuVerb::CopyLink,
        MenuVerb::RefreshPr,
        MenuVerb::Submenu(Submenu::Snooze),
        MenuVerb::Park,
        MenuVerb::Delete,
    ] {
        assert!(offers(verb, cx), "box's menu offers {verb:?}");
    }

    // Pin, on box's Thread alone.
    run(MenuVerb::Pin, cx);
    view.read_with(cx, |view, cx| assert_eq!(view.pins(cx), [theirs], "box's Thread is pinned"));
    assert!(offers(MenuVerb::Unpin, cx));
    run(MenuVerb::Unpin, cx);
    view.read_with(cx, |view, cx| assert!(view.pins(cx).is_empty(), "and unpinned"));

    // Mark as unread, while it is off screen and its Notice is read:
    // box's Notice.
    let attention = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.cockpit.read(cx).notifications(on_box).attention(theirs.thread())
        })
    };
    until_said(cx, "box never noticed its answer", attention);
    view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| cockpit.mark_notices_read()));
    until_said(cx, "box never read its Notice", |cx| !attention(cx));
    assert!(offers(MenuVerb::MarkUnread, cx), "box's read Notice can be unread");
    run(MenuVerb::MarkUnread, cx);
    until_said(cx, "box's Notice never came back", attention);
    view.read_with(cx, |view, cx| {
        let cockpit = view.cockpit.read(cx);
        let here = cockpit.first_machine();
        assert!(!cockpit.notifications(here).attention(mine.thread()), "mine stays read");
    });

    // Open focuses it, and box streams what it shows.
    assert!(offers(MenuVerb::Focus, cx));
    run(MenuVerb::Focus, cx);
    view.read_with(cx, |view, cx| assert_eq!(view.focused_thread(cx), Some(theirs)));
    until_said(cx, "box never streamed its answer", |cx| shows(&view, theirs, "box's answer", cx));

    // Fork makes a Thread on box.
    let before = box_threads(cx);
    run(MenuVerb::Fork, cx);
    until_said(cx, "box never forked", |cx| box_threads(cx) == before + 1);

    // New thread in this project drafts in box's Project.
    view.update(cx, |view, cx| view.focus_thread(mine, cx));
    run(MenuVerb::NewThread, cx);
    let drafted = view.read_with(cx, |view, cx| {
        view.panes[view.focused(cx)].read(cx).draft().map(|draft| draft.binding.project())
    });
    let project = view.read_with(cx, |view, cx| view.cockpit.read(cx).project_of(theirs));
    assert!(project.is_some_and(|project| project.machine() == on_box));
    assert_eq!(drafted, project, "the draft is in box's Project");

    // Its path and its link are box's.
    run(MenuVerb::CopyPath, cx);
    let path = view.read_with(cx, |view, cx| view.thread_path(theirs, cx)).expect("box's path");
    assert_eq!(clipboard(cx), Some(path.to_string_lossy().to_string()));
    run(MenuVerb::CopyLink, cx);
    assert_eq!(
        clipboard(cx),
        Some(format!("ferrite --machine box thread focus {}", theirs.thread().get()))
    );

    // Refresh PR status asks of box's Thread, once its first ask is
    // answered.
    view.update(cx, |view, _| {
        view.facts.pr_answered(theirs, None);
        view.pr_reads.asked.clear();
    });
    run(MenuVerb::RefreshPr, cx);
    view.read_with(cx, |view, _| assert_eq!(view.pr_reads.asked, [(theirs, true)]));

    // Snooze and Wake, on box.
    let preset = ferrite_core::snooze::Preset::available(ferrite_core::clock::system_time())[0].0;
    run(MenuVerb::Snooze(preset), cx);
    until_said(cx, "box never snoozed it", |cx| snoozed(theirs, cx) == Some(true));
    assert_eq!(snoozed(mine, cx), Some(false), "mine is awake");
    assert!(offers(MenuVerb::Wake, cx));
    run(MenuVerb::Wake, cx);
    until_said(cx, "box never woke it", |cx| snoozed(theirs, cx) == Some(false));
    run(MenuVerb::SnoozeCustom, cx);
    view.read_with(cx, |view, cx| {
        let editing = view.nav_view.read(cx).snooze.as_ref().map(|editor| editor.thread);
        assert_eq!(editing, Some(theirs), "Custom… snoozes box's Thread");
    });
    cx.simulate_keystrokes("escape");
    tick(cx);

    // Park, and Open resumes it on box.
    run(MenuVerb::Park, cx);
    until_said(cx, "box never parked it", |cx| {
        view.read_with(cx, |view, _| view.facts.parked().contains(&theirs))
    });
    assert_eq!(snoozed(theirs, cx), None, "parked on box");
    run(MenuVerb::Focus, cx);
    until_said(cx, "box never resumed it", |cx| snoozed(theirs, cx).is_some());

    // The palette's Thread verbs act on the focused Thread, box's.
    view.update(cx, |view, cx| view.focus_thread(theirs, cx));
    tick(cx);
    let before = box_threads(cx);
    view.update_in(cx, |view, window, cx| view.run_command(Command::Fork, window, cx));
    until_said(cx, "the palette's Fork never forked on box", |cx| box_threads(cx) == before + 1);
    view.update(cx, |view, cx| view.focus_thread(theirs, cx));
    tick(cx);
    view.update_in(cx, |view, window, cx| view.run_command(Command::ParkThread, window, cx));
    until_said(cx, "the palette's Park never parked on box", |cx| {
        view.read_with(cx, |view, _| view.facts.parked().contains(&theirs))
    });

    // Delete, last: box forgets it.
    run(MenuVerb::Delete, cx);
    until_said(cx, "box never deleted it", |cx| {
        view.read_with(cx, |view, cx| !view.cockpit.read(cx).known_threads().contains(&theirs))
    });
    assert!(snoozed(mine, cx).is_some(), "mine stays");
}

/// Turning box off and on again keeps its Thread's Panel as it was (D4):
/// the terminal tab, its shell ended with box and a Try again in its
/// place, and the side chat tab, on the same fork, which is still no Pane
/// or row of its own.
#[gpui::test]
fn turning_a_machine_off_and_on_keeps_its_panels_tabs(cx: &mut TestAppContext) {
    use crate::cockpit::surfaces::SurfaceId;
    bind_production_keys(cx);
    let (mut core, _fake, machines, agents) = two_machines("off-on", 1);
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    view.update(cx, |view, cx| view.focus_thread(theirs, cx));
    agents.say(0, SessionEvent::TextDelta { text: "history words\n\n".into() });
    agents.say(0, SessionEvent::TurnEnded { outcome: ferrite_core::TurnOutcome::Completed, cost_usd: None });
    until_said(cx, "box's Thread never answered", |cx| {
        shows(&view, theirs, "history words", cx) && !running(&view, theirs, cx)
    });
    view.update(cx, |view, cx| view.new_terminal_surface(theirs, cx));
    view.update(cx, |view, cx| view.open_side_chat(theirs, cx));
    let fork_of = |owner: ThreadKey, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| view.side_chat.read(cx).shown_fork(owner))
    };
    until_said(cx, "the tab never took its fork", |cx| fork_of(theirs, cx).is_some());
    let fork = fork_of(theirs, cx).unwrap();
    let tabs = |thread: ThreadKey, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.panel.surfaces.get(thread).map(|panel| panel.tabs.clone()).unwrap_or_default()
        })
    };
    let before = tabs(theirs, cx);
    assert!(
        matches!(before[..], [SurfaceId::Overview, SurfaceId::Terminal(_), SurfaceId::SideChat(_)]),
        "{before:?}"
    );
    let turn = |enabled: bool, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| {
            view.prefs.settings.machines = vec![Machine { enabled, ..box_machine() }];
            view.machines_changed(cx);
        });
        tick(cx);
    };

    turn(false, cx);
    view.read_with(cx, |view, _| assert!(view.pane_for(theirs).is_none(), "box went"));
    turn(true, cx);
    let again = |thread: ThreadKey, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let saved = SavedThread { machine: on_box_key(), thread: thread.thread() };
            view.cockpit.read(cx).resolve(&saved)
        })
    };
    until_said(cx, "box's Thread never came back", |cx| {
        again(fork, cx).is_some_and(|fork| {
            view.read_with(cx, |view, cx| view.cockpit.read(cx).thread(fork).is_some())
        }) && again(theirs, cx).is_some_and(|theirs| {
            view.read_with(cx, |view, _| view.pane_for(theirs).is_some())
        })
    });
    let (theirs, fork) = (again(theirs, cx).unwrap(), again(fork, cx).unwrap());
    tick(cx);
    assert_eq!(tabs(theirs, cx), before, "box's Panel keeps its tabs");
    assert_eq!(fork_of(theirs, cx), Some(fork), "the side chat is on its fork still");
    view.read_with(cx, |view, cx| {
        assert!(view.pane_for(fork).is_none(), "the fork has no Pane");
        assert!(
            view.nav_state(cx).open_rows().all(|row| row.key != fork),
            "nor a row"
        );
        let SurfaceId::Terminal(number) = before[1] else { unreachable!() };
        assert_eq!(view.terminals.read(cx).numbers(theirs), [number], "the terminal tab has its shell's place");
    });
    view.update(cx, |view, cx| {
        view.focus_thread(theirs, cx);
        view.activate_surface(theirs, &before[1], cx);
    });
    tick(cx);
    assert!(cx.debug_bounds("terminal-retry").is_some(), "the ended shell offers Try again");
}

/// The focused Pane's draft, as its band aims it.
fn drafted(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<ProjectKey> {
    view.read_with(cx, |view, cx| {
        view.panes.get(view.focused(cx))?.read(cx).draft().map(|draft| draft.binding.project())
    })
}

/// With box turned off, a new Thread starts on this computer (the
/// checker's no-Project start): a draft aimed at box's Project goes with
/// box, its words kept, and nothing the window guesses with names box's
/// Projects, so ⌘N drafts here and its send starts here, where the draft
/// left standing was refused ("re-pick it"). Back on, box's draft is back,
/// its words as they were.
#[gpui::test]
fn with_a_machine_off_a_new_thread_starts_on_this_computer(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, fake, machines, _agents) = two_machines("no-project", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let asked = core.park(mine);
    core.wait_for(asked).unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    let boxes = view.read_with(cx, |view, cx| view.cockpit.read(cx).project_of(theirs)).unwrap();
    view.update(cx, |view, cx| view.open_draft_in_project(boxes, cx));
    tick(cx);
    assert_eq!(drafted(&view, cx), Some(boxes));
    cx.simulate_input("for box");
    tick(cx);
    let turn = |enabled: bool, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| {
            view.prefs.settings.machines = vec![Machine { enabled, ..box_machine() }];
            view.machines_changed(cx);
        });
        tick(cx);
    };

    turn(false, cx);
    let here = view.read_with(cx, |view, cx| view.cockpit.read(cx).this_computer()).unwrap();
    view.read_with(cx, |view, cx| {
        assert!(
            view.panes.iter().all(|pane| pane.read(cx).draft().is_none_or(|draft| draft.binding.project().machine() != on_box)),
            "box's draft went with box"
        );
    });
    cx.simulate_keystrokes("cmd-n");
    tick(cx);
    assert_eq!(drafted(&view, cx).map(ProjectKey::machine), Some(here), "⌘N drafts on this computer");
    cx.simulate_input("hello here");
    cx.simulate_keystrokes("enter");
    until_said(cx, "the draft never started here", |_| {
        fake.sent.borrow().iter().any(|sent| sent.contains("hello here"))
    });

    turn(true, cx);
    until_said(cx, "box's draft never came back", |cx| {
        view.read_with(cx, |view, cx| {
            view.panes.iter().any(|pane| {
                pane.read(cx).draft().is_some_and(|draft| {
                    draft.binding.project().machine() != here
                        && pane.read(cx).composer.read(cx).text() == "for box"
                })
            })
        })
    });
}

/// Open on box's Connections row has Settings edit box, and the page says
/// so atop it, as T3 Code's settings say the environment they apply to.
#[gpui::test]
fn open_in_connections_names_the_machine_settings_edit(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, _fake, machines, _agents) = two_machines("settings-scope", 1);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.hide_sheet(cx));
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    let sheet = view.read_with(cx, |view, cx| view.settings_sheet(cx).expect("Settings"));
    sheet.update(cx, |sheet, cx| sheet.show(settings::PageKey::Connections, cx));
    tick(cx);
    let scope = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, _| window.debug_text("settings-scope"))
            .map(|lines| lines.join(" "))
    };
    assert_eq!(scope(cx).as_deref(), Some("Applying settings on this computer"));
    let open = connection_act(&view, "connection-open-1", cx);
    cx.update(|window, cx| open(window, cx));
    tick(cx);
    assert_eq!(scope(cx).as_deref(), Some("Applying settings on box"));
}

/// Copy conversation on box's Thread, once its view dropped its earliest
/// Blocks, copies the whole conversation, first prompt included: box's
/// Server replays the log, as this computer's does (ADR 0049).
#[gpui::test]
fn copy_conversation_on_a_machine_copies_all_of_it(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake, machines, agents) = two_machines("copy-whole", 0);
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    view.update(cx, |view, cx| view.focus_thread(theirs, cx));
    prompt(&view, theirs, "the very first question", cx);
    until_said(cx, "box never took the prompt", |_| agents.sent("the very first question"));
    // Past the Blocks a transcript keeps.
    let turns = ferrite_core::activity::ActivityLimits::default().blocks_per_subject + 10;
    for turn in 0..turns {
        agents.say(0, SessionEvent::TextDelta { text: format!("turn {turn}\n\n") });
        agents.say(0, SessionEvent::TurnEnded { outcome: ferrite_core::TurnOutcome::Completed, cost_usd: None });
    }
    let last = format!("turn {}", turns - 1);
    until_said(cx, "box's turns never arrived", |cx| shows(&view, theirs, &last, cx));
    let dropped = view.read_with(cx, |view, cx| {
        view.cockpit.read(cx).thread(theirs).unwrap().transcript().earlier_dropped()
    });
    assert!(dropped, "the window keeps only the latest");

    view.update_in(cx, |view, window, cx| {
        view.run_menu_verb(MenuTarget::Pane(theirs), MenuVerb::CopyTranscript, window, cx)
    });
    let clipboard =
        |cx: &mut gpui::VisualTestContext| cx.read_from_clipboard().and_then(|item| item.text());
    until_said(cx, "nothing was copied", |cx| clipboard(cx).is_some());
    let copied = clipboard(cx).unwrap();
    assert!(copied.contains("> the very first question"), "{}", &copied[..copied.len().min(300)]);
    assert!(copied.contains(&last), "and the newest turn too");
}

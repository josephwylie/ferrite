//! The operator's own palette commands and keys (ADR 0024).
use super::*;
use crate::palette::{Act, Command};
use ferrite_core::user_commands::{Output, Run, UserCommand};

/// The palette's rows for the focused Pane, by name.
fn command_rows(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Vec<(String, Act)> {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.palette_command_rows(window, cx)
                .into_iter()
                .map(|row| (row.name.to_string(), row.act))
                .collect()
        })
    })
}

/// Tick until `done`: a command runs on a thread of its own, outside the
/// test's executor.
fn tick_until(
    cx: &mut gpui::VisualTestContext,
    mut done: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) {
    for _ in 0..500 {
        tick(cx);
        if done(cx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the command never finished");
}

fn run(view: &Entity<CockpitView>, index: usize, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_act(Act::Command(Command::User(index)), false, window, cx)
        })
    });
}

/// A command is a palette row named by the operator, its line after it;
/// it runs in the focused Thread's directory, told the Thread's id, and a
/// `reader` command's output opens beside the Pane.
#[cfg(unix)]
#[gpui::test]
fn a_user_command_runs_for_the_focused_thread_into_the_reader(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("user-command", 1);
    let thread = core.threads()[0];
    let mut prefs = Preferences::ephemeral();
    prefs.settings.commands = vec![UserCommand {
        name: "where".into(),
        run: Run::Shell("pwd; echo \"$FERRITE_THREAD_ID\"".into()),
        output: Output::Reader,
    }];
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    tick(cx);
    let rows = command_rows(&view, cx);
    let (name, act) = rows.last().expect("the operator's command is listed");
    assert_eq!(name, "where");
    assert_eq!(*act, Act::Command(Command::User(0)));
    run(&view, 0, cx);
    tick_until(cx, |cx| {
        view.read_with(cx, |view, _| view.panes[0].preview.document().is_some())
    });
    let (document, cwd) = view.read_with(cx, |view, _| {
        (
            view.panes[0].preview.document().expect("the output opened"),
            view.thread_path(thread).unwrap(),
        )
    });
    assert_eq!(document.name.as_deref(), Some("where"));
    assert!(document.path.ends_with("where.log"), "it reads as a log");
    let pwd = std::fs::canonicalize(&cwd).unwrap();
    assert_eq!(
        &*document.text().unwrap().source,
        format!("{}\n{thread}", pwd.display())
    );
}

/// `reset keys` shows only while the operator has keys of their own, and
/// forgets them.
#[gpui::test]
fn reset_keys_forgets_the_operators_keys(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("reset-keys", 1);
    let mut prefs = Preferences::ephemeral();
    prefs
        .settings
        .keys
        .insert("palette::Toggle".into(), Some("cmd-p".into()));
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    tick(cx);
    let reset = Act::Command(Command::ResetKeys);
    assert!(command_rows(&view, cx).iter().any(|(_, act)| *act == reset));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_act(reset.clone(), false, window, cx)
        })
    });
    tick(cx);
    view.read_with(cx, |view, _| assert!(view.prefs.settings.keys.is_empty()));
    assert!(!command_rows(&view, cx).iter().any(|(_, act)| *act == reset));
    assert_eq!(
        crate::keymap::active(),
        crate::keymap::bindings(crate::keymap::PLATFORM)
    );
}

/// A running command's row stops it, and the row comes back when it ends.
#[cfg(unix)]
#[gpui::test]
fn a_running_commands_row_stops_it(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("user-command-stop", 1);
    let mut prefs = Preferences::ephemeral();
    prefs.settings.commands = vec![UserCommand {
        name: "watch".into(),
        run: Run::Shell("sleep 60".into()),
        output: Output::Toast,
    }];
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    tick(cx);
    run(&view, 0, cx);
    tick(cx);
    let names = |cx: &mut gpui::VisualTestContext| -> Vec<String> {
        command_rows(&view, cx)
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    };
    assert_eq!(names(cx).last().map(String::as_str), Some("stop watch"));
    run(&view, 0, cx);
    tick_until(cx, |cx| {
        view.read_with(cx, |view, _| view.user_commands.running.is_empty())
    });
    assert_eq!(names(cx).last().map(String::as_str), Some("watch"));
}

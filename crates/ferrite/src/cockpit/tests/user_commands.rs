//! The operator's own palette commands and keys (ADR 0022).
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
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_act(Act::Command(Command::User(0)), false, window, cx)
        })
    });
    tick(cx);
    let (document, cwd) = view.read_with(cx, |view, _| {
        (
            view.panes[0].preview.document().expect("the output opened"),
            view.thread_path(thread).unwrap(),
        )
    });
    assert_eq!(document.title, "where");
    assert_eq!(document.kind(), "LOG");
    let pwd = std::fs::canonicalize(&cwd).unwrap();
    assert_eq!(document.source, format!("{}\n{thread}", pwd.display()));
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

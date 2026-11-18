//! The one sheet host (ADR 0042): Settings and the Project card.
#[allow(unused_imports)]
use super::*;

fn up(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> (bool, bool) {
    view.read_with(cx, |view, cx| (view.settings_open(cx), view.project_editor_open(cx)))
}

/// Opening one sheet over the other leaves exactly one up, and only that
/// one is drawn.
#[gpui::test]
fn opening_one_sheet_over_the_other_leaves_one_up(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sheets-one-up", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    assert_eq!(up(&view, cx), (true, false));
    view.update(cx, |view, cx| view.open_project_creator(cx));
    tick(cx);
    assert_eq!(up(&view, cx), (false, true), "the card replaces Settings");
    assert!(cx.debug_bounds("settings-card").is_none(), "with no exit");
    assert!(cx.debug_bounds("project-editor-card").is_some());

    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    assert_eq!(up(&view, cx), (true, false), "Settings replaces the card");
    assert!(cx.debug_bounds("project-editor-card").is_none());
    assert!(cx.debug_bounds("settings-card").is_some());
}

/// Escape, the sheet's own close button and a press on the veil each
/// close either sheet.
#[gpui::test]
fn escape_the_close_button_and_the_veil_each_close_a_sheet(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sheets-close", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let outside = gpui::point(px(20.), px(650.));

    for (open, close) in [
        (true, "settings-close"),
        (false, "project-editor-close"),
    ] {
        let show = |cx: &mut gpui::VisualTestContext| {
            view.update(cx, |view, cx| {
                if open {
                    view.toggle_settings(cx)
                } else {
                    view.open_project_creator(cx)
                }
            });
            cx.run_until_parked();
            assert_ne!(up(&view, cx), (false, false), "{close}: the sheet is up");
        };

        show(cx);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(up(&view, cx), (false, false), "{close}: escape closes it");

        show(cx);
        let button = cx.debug_bounds(close).expect(close).center();
        cx.simulate_click(button, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(up(&view, cx), (false, false), "{close}: its close button closes it");

        show(cx);
        cx.simulate_click(outside, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(up(&view, cx), (false, false), "{close}: a press on the veil closes it");
    }
}

/// Closing Settings, by any way out, drops the connection being edited.
#[gpui::test]
fn closing_settings_drops_the_connection_editor(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sheets-connection-editor", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let editing = |cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| {
            if !view.settings_open(cx) {
                view.toggle_settings(cx);
            }
        });
        tick(cx);
        view.update_in(cx, |view, window, cx| view.edit_connection(None, window, cx));
        tick(cx);
        assert!(view.read_with(cx, |view, _| view.connections.editor.is_some()));
    };
    let dropped = |cx: &mut gpui::VisualTestContext, how: &str| {
        view.read_with(cx, |view, cx| {
            assert!(!view.settings_open(cx), "{how} closes Settings");
            assert!(view.connections.editor.is_none(), "{how} drops the editor");
        });
    };

    editing(cx);
    view.update(cx, |view, cx| view.toggle_settings(cx));
    tick(cx);
    dropped(cx, "cmd-,");

    editing(cx);
    view.update(cx, |view, cx| {
        view.settings_sheet(cx)
            .unwrap()
            .update(cx, |_, cx| cx.emit(settings_view::SettingsEvent::Closed))
    });
    tick(cx);
    dropped(cx, "the close button");

    editing(cx);
    view.update(cx, |view, cx| {
        view.sheets.update(cx, |_, cx| cx.emit(crate::cockpit::sheets::Dismissed))
    });
    tick(cx);
    dropped(cx, "the veil");

    editing(cx);
    view.update(cx, |view, cx| view.open_project_creator(cx));
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.project_editor_open(cx));
        assert!(view.connections.editor.is_none(), "the card drops the editor");
    });
}

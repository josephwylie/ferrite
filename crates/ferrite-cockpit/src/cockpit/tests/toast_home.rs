//! Where the kit's toasts stand (ADR 0048 §2): over the Composer of the
//! board's lower right Pane, their right edge on its box's and their foot
//! over its stack, or over the row the operator answers at the transcript's
//! foot; drawn over any sheet and under every menu (`ferrite_ui::layer`).
use super::*;
use gpui::component::notification::Notification;
use gpui::component::WindowExt as _;
use ferrite_ui::theme::TOAST_TOP;

/// A window of one Thread with the app's keys, 1440 by 900, its toasts
/// at rest, and the Fake its agent speaks through.
fn board<'a>(name: &str, cx: &'a mut TestAppContext) -> (Entity<CockpitView>, Fake, &'a mut gpui::VisualTestContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (core, fake) = cockpit(name, 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    (view, fake, cx)
}

/// A toast that stays until closed, at rest; it counts the presses on it.
fn toast(title: &'static str, cx: &mut gpui::VisualTestContext) -> std::rc::Rc<std::cell::Cell<usize>> {
    let pressed = std::rc::Rc::new(std::cell::Cell::new(0));
    cx.update(|window, cx| {
        let pressed = pressed.clone();
        let toast = Notification::new()
            .title(title)
            .autohide(false)
            .on_click(move |_, _, _| pressed.set(pressed.get() + 1));
        window.push_notification(toast, cx);
    });
    settles(cx);
    pressed
}

type Rect = gpui::Bounds<gpui::Pixels>;

/// The toast and the Composer's box, the toast at rest and the window
/// drawn as it is.
fn toast_and_box(cx: &mut gpui::VisualTestContext) -> (Rect, Rect) {
    settles(cx);
    (bounds(cx, "notification-card".into()), bounds(cx, "composer-block".into()))
}

/// The toast stands with its right edge on the Composer box's and its foot
/// `TOAST_TOP` over the box, the top of a Thread's Composer stack.
#[track_caller]
fn on_the_composer(card: Rect, field: Rect, layout: &str) {
    assert!(
        (card.right() - field.right()).abs() <= px(1.)
            && (card.bottom() - (field.top() - px(TOAST_TOP))).abs() <= px(1.),
        "{layout}: the toast at {card:?} is not over the Composer at {field:?}"
    );
}

/// The toast's home is the Composer's, whatever stands beside it: its
/// right edge on the box's, its foot just over it, with the panel open,
/// the Details card up, or the panel shut.
#[gpui::test]
fn a_toast_stands_on_the_composers_right_edge(cx: &mut TestAppContext) {
    let (view, fake, cx) = board("toast-home", cx);
    spoken(&fake, 0);
    tick(cx);
    toast("Copying app to box", cx);
    let (card, field) = toast_and_box(cx);
    assert!(debug_bounds(cx, "panel".into()).is_some(), "the panel is open");
    on_the_composer(card, field, "the panel open");

    view.update_in(cx, |view, window, cx| view.toggle_details_now(window, cx));
    let (card, field) = toast_and_box(cx);
    assert!(debug_bounds(cx, "details-card".into()).is_some(), "the Details card is up");
    on_the_composer(card, field, "the Details card up");
    view.update_in(cx, |view, window, cx| view.toggle_details_now(window, cx));

    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    let (card, field) = toast_and_box(cx);
    assert!(debug_bounds(cx, "panel".into()).is_none(), "the panel is shut");
    on_the_composer(card, field, "the panel shut");
}

/// A Decision waiting at the transcript's foot, right over the Composer,
/// is never under a toast: the toast stands over it, still on the
/// Composer's right edge, and Allow and Deny are the operator's to press.
#[gpui::test]
fn a_toast_stands_clear_of_a_waiting_decision(cx: &mut TestAppContext) {
    let (_view, fake, cx) = board("toast-over-decision", cx);
    let said: String = (0..60).map(|line| format!("Line {line} of what the agent read.\n\n")).collect();
    fake.streams.borrow()[0].send(SessionEvent::TextDelta { text: said }).unwrap();
    fake.streams.borrow()[0].send(decision("perm_01")).unwrap();
    tick(cx);
    let pressed = toast("Copying app to box", cx);
    let (card, field) = toast_and_box(cx);
    let island = bounds(cx, "question-island".into());
    let actions = bounds(cx, "decision-actions".into());
    assert!(
        island.bottom() > field.top() - px(TOAST_TOP) - card.size.height,
        "the Decision at {island:?} waits where a toast over the Composer at {field:?} would stand"
    );
    assert!(!card.intersects(&island), "the toast at {card:?} covers the Decision at {island:?}");
    assert!(!card.intersects(&actions), "the toast at {card:?} covers Allow and Deny at {actions:?}");
    assert!(card.bottom() <= island.top(), "over the Decision: {card:?} {island:?}");
    assert!((card.right() - field.right()).abs() <= px(1.), "on the Composer's right edge: {card:?} {field:?}");
    cx.simulate_click(actions.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(pressed.get(), 0, "a press on Allow or Deny is the Decision's");
}

/// A sheet up holds the pointer and the keys, so the Decision under it is
/// not answered there: a toast over the sheet stands at its home on the
/// Composer, not risen into the sheet's card.
#[gpui::test]
fn a_toast_over_a_sheet_does_not_rise_for_the_decision_under_it(cx: &mut TestAppContext) {
    let (_view, fake, cx) = board("toast-sheet-decision", cx);
    let said: String = (0..60).map(|line| format!("Line {line} of what the agent read.\n\n")).collect();
    fake.streams.borrow()[0].send(SessionEvent::TextDelta { text: said }).unwrap();
    fake.streams.borrow()[0].send(decision("perm_01")).unwrap();
    tick(cx);
    toast("Copy of app to box stopped", cx);
    let (risen, field) = toast_and_box(cx);
    assert!(
        risen.bottom() < field.top() - px(TOAST_TOP),
        "the premise: the toast at {risen:?} rose over the Decision"
    );
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    let (card, field) = toast_and_box(cx);
    on_the_composer(card, field, "Settings up over a waiting Decision");
}

/// A toast stands over Settings where it stands without it, over the
/// Composer under the sheet: it draws over the sheet's veil, clear of the
/// widest page's close button, Add button and Local switch, and a press on
/// it is the toast's, not the veil's, which would close Settings.
#[gpui::test]
fn a_toast_stands_over_the_settings_sheet(cx: &mut TestAppContext) {
    let (view, fake, cx) = board("toast-over-settings", cx);
    spoken(&fake, 0);
    tick(cx);
    let pressed = toast("Over Settings", cx);
    let (home, field) = toast_and_box(cx);
    on_the_composer(home, field, "the board");
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    // Connections is the widest and tallest page: its controls come
    // nearest the toasts.
    view.update(cx, |view, cx| {
        let sheet = view.settings_sheet(cx).expect("Settings is up");
        sheet.update(cx, |sheet, cx| sheet.show(settings::PageKey::Connections, cx));
        cx.notify();
    });
    settles(cx);
    let card = bounds(cx, "notification-card".into());
    assert_eq!(card, home, "a sheet moves no toast");
    for control in ["settings-close", "connection-add", "connection-toggle-local"] {
        let at = bounds(cx, control.into());
        assert!(!card.intersects(&at), "the toast at {card:?} covers {control} at {at:?}");
    }
    cx.simulate_click(card.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(pressed.get(), 1, "the press reached the toast");
    view.read_with(cx, |view, cx| assert!(view.settings_sheet(cx).is_some(), "Settings stays up"));
}

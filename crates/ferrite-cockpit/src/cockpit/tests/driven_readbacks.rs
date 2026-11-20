//! What the headless driver (ADR 0038) reads back from the last frame: the
//! text an element shows (`window.text`) and what holds the keyboard
//! (`window.focus`), the two things a checker without screenshots cannot
//! otherwise learn.
use super::*;
use crate::cockpit::panel::PanelControl;
use crate::cockpit::surfaces::SurfaceId;
use crate::headless::Driver;
use gpui::Modifiers;

fn driver(name: &str) -> Driver {
    let (core, _fake, _checkout) = bound_cockpit(name, Provider::Claude);
    let cx = crate::headless::app(ferrite_ui::keymap::bindings(ferrite_ui::keymap::PLATFORM));
    let mut driver = Driver::open(cx, |cx| CockpitView::new(core, cx));
    driver.tick(16);
    driver
}

#[test]
fn text_reads_what_an_element_shows_and_refuses_one_not_drawn() {
    let mut driver = driver("driven-text");
    assert!(driver.click("composer-block", MouseButton::Left, 1, Modifiers::default()).is_some());
    driver.type_text("Reply with MANGO").unwrap();

    let lines = driver.text("composer-block").expect("the Composer is drawn");
    assert!(
        lines.iter().any(|line| line == "Reply with MANGO"),
        "the typed draft is among {lines:?}"
    );
    let nav = driver.text("nav-list").expect("the nav list is drawn");
    assert!(
        !nav.iter().any(|line| line.contains("MANGO")),
        "another element's text is not the Composer's: {nav:?}"
    );
    assert_eq!(driver.text("no-such-selector"), None);
    driver.close();
}

#[test]
fn focus_names_the_composer_then_the_panel_stop_f6_and_an_arrow_reach() {
    let mut driver = driver("driven-focus");
    driver.update(|view, _, cx| {
        let thread = view.panes[0].thread().unwrap();
        view.prefs.settings.panel_shown = true;
        view.upsert_surface(thread, SurfaceId::Files, cx);
    });
    driver.tick(16);
    assert!(driver.click("composer-block", MouseButton::Left, 1, Modifiers::default()).is_some());

    let composer = driver.focus();
    let innermost = composer.contexts.last().and_then(|context| context.split(' ').next());
    assert_eq!(innermost, Some("Composer"), "in {:?}", composer.contexts);
    assert_eq!(composer.selector.as_deref(), Some("focused-prompt-editor"));
    assert_eq!(composer.label, None, "the Composer is no Panel stop");

    let named = |driver: &mut Driver, control: PanelControl| {
        driver.read(|view, _| {
            view.panel
                .focus
                .drawn()
                .into_iter()
                .find(|(drawn, _)| *drawn == control)
                .map(|(_, semantics)| semantics.name.to_string())
        })
    };
    assert_eq!(driver.keys(&["f6", "home"]), Ok(vec![true, true]));
    let overview = driver.focus();
    assert_eq!(
        overview.contexts.last().map(String::as_str),
        Some(ferrite_ui::keymap::PANEL_CONTROL)
    );
    assert_eq!(
        overview.label,
        named(&mut driver, PanelControl::Tab(SurfaceId::Overview)),
        "named as the row names it"
    );
    assert_eq!(overview.selector.as_deref(), Some("panel-tab-Overview"));
    let tab_text = driver.text("panel-tab-Overview").unwrap();
    assert!(
        overview.label.as_ref().is_some_and(|label| tab_text.contains(label)),
        "the tab shows its name: {tab_text:?}"
    );

    assert_eq!(driver.keys(&["right"]), Ok(vec![true]));
    let next = driver.focus();
    let stop = driver
        .update(|view, window, _| view.panel.focus.focused(window))
        .expect("the keyboard stays in the row");
    assert_ne!(stop, PanelControl::Tab(SurfaceId::Overview));
    assert_eq!(next.label, named(&mut driver, stop));
    assert_ne!(next.label, overview.label, "the keyboard moved along the row");
    driver.close();
}

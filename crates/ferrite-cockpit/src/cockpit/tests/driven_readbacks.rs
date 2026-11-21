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

/// A shot is the frame as drawn, words included. Windows's headless platform
/// once shaped text with no glyphs, so a shot showed every surface and icon
/// but no word. Linux has no off-screen renderer yet.
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn a_shot_draws_the_words_an_element_shows() {
    let mut driver = driver("driven-shot");
    assert!(driver.click("composer-block", MouseButton::Left, 1, Modifiers::default()).is_some());
    driver.type_text("Reply with MANGO and nothing else").unwrap();
    let composer = driver.bounds("composer-block").expect("the Composer is drawn");
    let dir = scratch("driven-shot");
    std::fs::create_dir_all(&dir).unwrap();
    let saved = driver.shot(&dir.join("shot.png")).expect("a shot");
    let image = image::open(dir.join("shot.png")).unwrap().to_rgba8();
    assert_eq!((image.width(), image.height()), (saved.width, saved.height));

    // The draft's glyphs are light on the Composer's dark field; its icon
    // and caret alone light only a few hundred pixels.
    let px = |v: gpui::Pixels| (f32::from(v) * saved.scale) as u32;
    let (x0, y0) = (px(composer.origin.x), px(composer.origin.y));
    let (x1, y1) = (px(composer.right()).min(image.width()), px(composer.bottom()).min(image.height()));
    let lit = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let [r, g, b, _] = image.get_pixel(x, y).0;
            (u32::from(r) + u32::from(g) + u32::from(b)) / 3 > 160
        })
        .count();
    assert!(lit > 1500, "the typed draft lit {lit} pixels of the Composer");
    driver.close();
}

/// A File tab half scrolled out of the Panel's strip, its middle under the
/// strip's right chevron: a click by selector is refused, not pressed on
/// the chevron in its place, and a pointer on the tab's title where it
/// shows picks it with the first press.
#[test]
fn a_half_hidden_tab_is_refused_by_selector_and_picked_where_it_shows() {
    use ferrite_ui::theme::{SURFACE_TAB_CLOSE, SURFACE_TAB_GAP, SURFACE_TAB_PAD_L};
    let mut driver = driver("driven-half-hidden-tab");
    let dir = scratch("driven-half-hidden-tab-file");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("outside.txt");
    std::fs::write(&file, "outside the checkout\n").unwrap();
    // A tab ahead of the File tab, so the File tab starts far enough along
    // the strip that widening the Panel can bring its edge inside the tab.
    driver.update(|view, _, cx| {
        let thread = view.panes[0].thread().unwrap();
        view.upsert_surface(thread, SurfaceId::Changes, cx);
    });
    driver.update(|view, window, cx| view.open_preview(file.clone(), None, window, cx));
    for _ in 0..30 {
        driver.tick(16);
    }
    assert!(driver.click("panel-tab-Overview", MouseButton::Left, 1, Modifiers::default()).is_some());
    for _ in 0..60 {
        driver.tick(16);
    }
    let shown = |driver: &mut Driver| {
        driver.read(|view, _| {
            let thread = view.panes[0].thread().unwrap();
            view.active_surface(thread)
        })
    };
    let title_at = |tab: gpui::Bounds<gpui::Pixels>| {
        tab.left() + px(SURFACE_TAB_PAD_L + SURFACE_TAB_CLOSE + SURFACE_TAB_GAP)
    };
    // Where the tabs end depends on how wide their words draw, which differs
    // between platforms' fonts. Drag the Panel wider, as the operator does,
    // until the strip's fixed right edge falls between the File tab's title
    // and its middle.
    let strip = driver.bounds("panel-tabs").expect("the strip is drawn");
    let tab = driver.bounds("panel-tab-File-outside.txt").expect("its tab stays");
    let widen = (title_at(tab) + tab.center().x) / 2. - strip.right();
    assert!(
        widen > px(0.),
        "the premise: only a narrower Panel would clip the tab {tab:?} in {strip:?}"
    );
    let edge = driver.bounds("panel-edge").expect("the Panel's edge").center();
    for event in crate::hands::drag(edge, gpui::point(edge.x - widen, edge.y)) {
        driver.mouse(event);
    }
    for _ in 0..30 {
        driver.tick(16);
    }
    let strip = driver.bounds("panel-tabs").expect("the strip is drawn");
    let tab = driver.bounds("panel-tab-File-outside.txt").expect("its tab stays");
    let title = title_at(tab);
    assert!(
        title < strip.right() && tab.center().x > strip.right(),
        "the premise: the tab {tab:?} shows its title's start, not its middle, in {strip:?}"
    );

    let refused = driver.aim("panel-tab-File-outside.txt").unwrap_err();
    assert!(refused.contains("out of sight"), "{refused}");
    assert_eq!(
        driver.click("panel-tab-File-outside.txt", MouseButton::Left, 1, Modifiers::default()),
        None
    );
    assert_eq!(
        driver.bounds("panel-tab-File-outside.txt"),
        Some(tab),
        "nothing was pressed: the strip did not scroll"
    );
    assert_eq!(shown(&mut driver), Some(SurfaceId::Overview));

    let at = gpui::point((title + strip.right()) / 2., tab.center().y);
    driver.click_at(at, MouseButton::Left, 1, Modifiers::default());
    assert_eq!(shown(&mut driver), Some(SurfaceId::File(file)), "the first press picks the tab");
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

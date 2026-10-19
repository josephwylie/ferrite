//! The first run: one screen with both choices (the board's key, herdr's sidebar), answered with single keys; a key
//! herdr or your config uses refused as it is typed; esc asks again next time. And a board that loads its new version
//! by itself once nothing is half typed.
use crate::common;
use common::board::open;
use common::*;
use sw::board::state::Setup;
use sw::board::update::{update, Effect, Event};

fn flat(t: &str) -> String { regex::Regex::new(r"\s+").unwrap().replace_all(t, " ").into_owned() }
fn start(w: &World, key: Option<(&str, &str)>, sidebar: bool) -> common::board::Board {
    let mut b = open(&w.root, 30);
    b.cols = 100;
    let file = sw::extras::herdr_config().display().to_string();
    b.s.setup = Some(Setup { file, typing: false, key: key.map(|(k, n)| (true, k.to_string(), n.to_string())), sidebar: sidebar.then_some(true) });
    b
}

#[test]
fn the_first_run_is_one_screen_with_both_choices_answered_with_single_keys() {
    let w = world();
    let mut s = start(&w, Some(("prefix+a", "")), true);
    let t = s.text();
    assert!(t.contains("herdr-projects · first run") && !t.contains("question 1 of"), "{t}");
    assert!(t.contains("1  [x] Open this board with prefix+a, from anywhere in herdr") && t.contains("2  [x] Show each task's stage"), "{t}");
    assert!(t.contains("1 2 on or off · enter save · esc ask me next time"), "{t}");
    s.press(&["2"]); assert!(s.text().contains("2  [ ] Show each task's stage"));
    // a key herdr uses is refused as it is typed, with what it does today
    s.press(&["k"]); s.typed("prefix+w"); s.press(&["enter"]);
    assert!(flat(&s.text()).contains("herdr itself uses prefix+w"), "{}", s.text());
    assert!(s.s.setup.as_ref().unwrap().typing && s.text().contains("Open this board with prefix+a"));
    s.typed("prefix+y"); s.press(&["enter"]);
    assert!(s.text().contains("Open this board with prefix+y"), "{}", s.text());
    s.press(&["2", "enter"]);
    assert!(s.s.setup.is_none(), "{}", s.text());
    let config = read(&sw::extras::herdr_config());
    assert!(config.contains("prefix+y"), "{config}");
    assert!(flat(&s.text()).contains("prefix+y opens the board; herdr's sidebar shows each task's stage"), "{}", s.text());
    // esc changes nothing and asks again next time
    let mut s = start(&w, Some(("prefix+b", "")), true);
    s.press(&["esc"]);
    assert!(s.s.setup.is_none() && s.text().contains("Asked again next time. Nothing was changed."));
}

#[test]
fn after_an_update_the_board_loads_its_new_version_by_itself_once_nothing_is_half_typed() {
    let w = world();
    let mut s = open(&w.root, 34);
    s.s.ctx.can_reload = true;
    s.typed("a draft");
    let fx = update(&mut s.s, Event::Stamp("new".into())); assert!(!fx.contains(&Effect::Reload));
    let fx = update(&mut s.s, Event::Stamp("new".into())); assert!(!fx.contains(&Effect::Reload), "never under a draft");
    assert!(s.text().contains("herdr-projects was updated: the new version loads when nothing is half typed."));
    s.press(&["esc"]);
    let fx = update(&mut s.s, Event::Stamp("new".into()));
    assert!(fx.contains(&Effect::Reload));
}

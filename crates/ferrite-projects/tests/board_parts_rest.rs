//! What the old board-setup and board-tab tests checked that tests/board_setup.rs and tests/board_parts.rs do not: the
//! first run offers the next free key when prefix+a is taken and says why, and saves only what it asked; and the popup
//! (no pane of its own) or a board outside herdr has no tab to name.
use crate::common;
use common::board::open;
use common::*;
use sw::board::run::place::board_pane;
use sw::board::state::Setup;

fn flat(t: &str) -> String { regex::Regex::new(r"\s+").unwrap().replace_all(t, " ").into_owned() }

#[test]
fn prefix_a_taken_the_first_run_offers_the_next_free_key_and_says_why() {
    let w = world();
    let mut s = open(&w.root, 30); s.cols = 100;
    let note = "prefix+a is already bound in your config (lazygit), so this is the next free key.";
    s.s.setup = Some(Setup { file: sw::extras::herdr_config().display().to_string(), typing: false, key: Some((true, "prefix+y".into(), note.into())), sidebar: None });
    assert!(flat(&s.text()).contains(&format!("Open this board with prefix+y, from anywhere in herdr {note}")), "{}", s.text());
    assert!(!s.text().contains("Show each task's stage"), "only what is asked");
    s.press(&["enter"]);
    assert!(s.s.setup.is_none(), "{}", s.text());
    let config = std::fs::read_to_string(sw::extras::herdr_config()).unwrap_or_default();
    assert!(config.contains("prefix+y"), "{config}");
    assert!(!flat(&s.text()).contains("sidebar shows"), "{}", s.text());
}

#[test]
fn the_popup_and_a_board_outside_herdr_have_no_tab_to_name() {
    let _w = world();
    let set = |pairs: &[(&str, &str)]| { for k in ["HERDR_ENV", "HERDR_PANE_ID", "HERDR_PLUGIN_ID", "TMUX"] { unsafe { std::env::remove_var(k) } } for (k, v) in pairs { unsafe { std::env::set_var(k, v) } } };
    set(&[("HERDR_ENV", "1"), ("HERDR_PLUGIN_ID", "herdr-projects")]); assert_eq!(board_pane(), "", "the popup: no pane");
    set(&[]); assert_eq!(board_pane(), "", "outside herdr");
    set(&[("HERDR_ENV", "1"), ("HERDR_PANE_ID", "w1:p1")]); assert_eq!(board_pane(), "w1:p1");
    set(&[]);
}

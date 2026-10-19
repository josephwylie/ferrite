//! Every fixture row and its panel, drawn at the widths from 20 to 200: nothing runs over the width it was given, and
//! what says which task it is never vanishes (tests/common/fixtures.rs holds the rows: a new state is one new row there).
use crate::common;
use common::fixtures::fixture_rows;
use common::*;
use sw::board::state::{Rec, State};
use sw::board::theme::Theme;
use sw::json::*;
use sw::text::width;

#[test]
fn every_row_at_every_width_from_20_to_200_fits_and_keeps_its_mark_and_name() {
    let w = world();
    let id = w.add("Max length for task titles");
    let rows = fixture_rows(&task(&id), sw::clock::now());
    let all: Vec<Rec> = rows.iter().map(|(_, r)| std::sync::Arc::new(r.clone())).collect();
    let t = Theme::default_dark(true);
    let mut s = State::new(Default::default(), Default::default());
    for wd in 20..=200 {
        let lay = sw::board::view::row::row_layout(&mut s, &t, &all, wd, &[all.clone()], wd);
        for ((name, _), r) in rows.iter().zip(all.iter()) {
            let at = format!("{name}, {wd} columns");
            let lines: Vec<String> = sw::board::view::row::row_lines(&s, &t, r, wd, &lay).iter().map(|l| l.text()).collect();
            assert!(!lines.is_empty() && lines.len() <= 12, "{at}: {} lines", lines.len());
            for l in &lines { assert!(width(l) <= wd, "{at}: a line is {} wide: {l:?}", width(l)); }
            let title: String = r.s("title").chars().take(3).collect();
            assert!(lines[0].contains(&title), "{at}: the start of its name is gone: {:?}", lines[0]);
        }
    }
}

#[test]
fn every_rows_panel_beside_the_list_and_full_screen_fits_and_says_its_state() {
    let w = world();
    let id = w.add("Max length for task titles");
    let rows = fixture_rows(&task(&id), sw::clock::now());
    let t = Theme::default_dark(true);
    let mut s = State::new(Default::default(), Default::default());
    let beside: Vec<usize> = (20..=100).filter(|n| *n <= 60 || n % 2 == 0).collect();
    let full: Vec<usize> = (20..=200).filter(|n| *n <= 100 || n % 5 == 0).collect();
    for (name, r) in &rows {
        for (whole, widths) in [(false, &beside), (true, &full)] {
            for wd in widths.iter().copied() {
                let at = format!("{name}, {wd} columns{}", if whole { ", full screen" } else { "" });
                let lines: Vec<String> = sw::board::view::panel::panel(&mut s, &t, r, wd, whole, 0).iter().map(|l| l.text()).collect();
                for l in &lines { assert!(width(l) <= wd, "{at}: a line is {} wide: {l:?}", width(l)); }
                assert!(lines.iter().any(|l| l.starts_with(" State")), "{at}: the State line is gone");
                assert!(lines.len() >= 3, "{at}: the panel is empty");
            }
        }
    }
}

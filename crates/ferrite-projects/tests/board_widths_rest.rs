//! What the old board-widths test checked that tests/board_widths.rs does not: at every width from 20 to 200 every
//! fixture row's first line starts with its mark, in the gutter's cell after the bar's.
use crate::common;
use common::fixtures::fixture_rows;
use common::*;
use sw::board::state::{Rec, State};
use sw::board::theme::Theme;

#[test]
fn every_rows_mark_is_there_at_every_width_from_20_to_200() {
    let w = world();
    let id = w.add("Max length for task titles");
    let rows = fixture_rows(&task(&id), sw::clock::now());
    let all: Vec<Rec> = rows.iter().map(|(_, r)| std::sync::Arc::new(r.clone())).collect();
    let t = Theme::default_dark(true);
    let mut s = State::new(Default::default(), Default::default());
    let mark = regex::Regex::new(r"^[ ▌]\S").unwrap();
    for wd in 20..=200 {
        let lay = sw::board::view::row::row_layout(&mut s, &t, &all, wd, &[all.clone()], wd);
        for ((name, _), r) in rows.iter().zip(all.iter()) {
            let first = sw::board::view::row::row_lines(&s, &t, r, wd, &lay)[0].text();
            assert!(mark.is_match(&first), "{name}, {wd} columns: the mark: {first:?}");
        }
    }
}

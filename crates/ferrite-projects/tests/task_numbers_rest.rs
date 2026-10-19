//! A task's number on its row (the rest of task_numbers.rs): one row laid out on its own, in a list of wider numbers,
//! takes the list's column (`#9` then the room of `#134`), so its title starts where every other title starts.
use regex::Regex;
use serde_json::{json, Value};
use std::sync::Arc;
use sw::board::state::{Rec, State};
use sw::board::theme::Theme;
use sw::board::update::list::fold_rows;
use sw::board::view::row::{row_layout, row_lines};
use sw::json::*;
use sw::shown::states::{group_of, GROUPS};

const NOW: i64 = 1_800_000_000_000;
const AT: i64 = NOW - 2 * 3600000;
fn rec(base: Value, more: Value) -> Rec { let mut o = obj(base); for (k, v) in more.as_object().unwrap() { o.insert(k.clone(), v.clone()); } Arc::new(o) }

#[test]
fn one_row_laid_out_on_its_own_takes_the_number_column_of_its_list() {
    sw::clock::set(Some(NOW));
    let list = vec![
        rec(json!({ "state": "blocked", "kind": "permission", "stage": "approval", "agent": "claude", "at": AT }), json!({ "id": "p/134", "title": "Add an undo", "pr": 98 })),
        rec(json!({ "state": "working", "stage": "working", "agent": "claude", "at": AT }), json!({ "id": "p/34", "title": "Tidy the questions", "msg": "Running the tests", "progress": 40, "eta": 12 })),
        rec(json!({ "state": "idle", "stage": "todo", "at": AT }), json!({ "id": "p/9", "title": "Rename the hint line" })),
    ];
    let mut s = State::new(Default::default(), Default::default());
    s.grouped = GROUPS.iter().map(|g| (g.to_string(), list.iter().filter(|r| group_of(r) == *g).cloned().collect())).collect();
    s.rows = fold_rows(&mut s);
    let t = Theme::default_dark(false);
    let one = std::slice::from_ref(&list[2]);
    let lay = row_layout(&mut s, &t, one, 60, &[one.to_vec()], 200);
    let first = row_lines(&s, &t, &list[2], 60, &lay)[0].text();
    assert!(Regex::new(r"^[ ▌]○ #9 {3}Rename").unwrap().is_match(&first), "on a list, the list's column: {first:?}");
}

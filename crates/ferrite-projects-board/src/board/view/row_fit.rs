//! What a section of the list shows when space is short: its rows give the same things up together, in one order.
//! First to go to last: what a row says, the bar, the word "ago", the part of a name past NAME_OWED cells, the percent
//! with the minutes left, the review tag with the pull request, the time, the task's number (for the whole list at
//! once: tags.rs), and then the rest of the name.

/// A name is whole up to this many cells; what is past them is cut before a column goes.
pub const NAME_OWED: usize = 40;
/// The minutes left are decided at this width whatever they read now: "9m" turning "10m" must not hide a bar.
pub const ETA_STEADY: usize = 9;
/// The percent the same way: "99%" turning "100%" must not either.
pub const PERCENT_STEADY: usize = 4;
/// The percent's column is drawn this wide at least ("45%").
pub const PERCENT_MIN: usize = 3;
/// Columns given up in one step, never one without the other.
const TOGETHER: [(&str, &str); 3] = [("percent", "eta"), ("review", "pr"), ("lead", "time")];

/// A row as the fit sees it: the cells right of its mark and number, its name's width, which columns it has.
#[derive(Clone, Debug)]
pub struct Fit { pub room: usize, pub name: usize, pub has: Vec<&'static str> }
/// One column of the list, left to right, at the width it is decided with.
#[derive(Clone, Copy, Debug)]
pub struct Cell { pub key: &'static str, pub w: usize }
/// The columns from `from` to the right edge: a cell between each, two before them.
pub fn wide(cells: &[Cell], from: usize) -> usize { cells[from.min(cells.len())..].iter().map(|c| c.w + 1).sum::<usize>() + 1 }
/// A row's first own column at or right of `from`.
pub fn own_from(row: &Fit, cells: &[Cell], mut from: usize) -> usize { while from < cells.len() && !row.has.contains(&cells[from].key) { from += 1; } from }
/// What is left for a row's name and what it says, beside the columns it shows from `from`.
pub fn label_room(row: &Fit, cells: &[Cell], from: usize) -> i64 { let at = own_from(row, cells, from); if at < cells.len() { row.room as i64 - wide(cells, at) as i64 } else { row.room as i64 } }
/// Where a section's columns start (the same for every row of it), and what the section went without: the columns
/// right of the bar it does not show, then the cells cut off its names.
pub fn shown_from(rows: &[Fit], cells: &[Cell]) -> (usize, (usize, usize)) {
    let fits = |from: usize, owed: usize| rows.iter().all(|row| own_from(row, cells, from) == cells.len() || label_room(row, cells, from) >= row.room.min(row.name).min(owed) as i64);
    let stops: Vec<usize> = (0..=cells.len()).filter(|n| !TOGETHER.iter().any(|(a, b)| cells.get(*n).map(|c| c.key == *b).unwrap_or(false) && *n > 0 && cells[n - 1].key == *a)).collect();
    // A bar is never worth a cut name: names are cut only for the columns right of it.
    let a = stops.iter().copied().find(|n| fits(*n, usize::MAX)).unwrap_or(cells.len());
    let b = stops.iter().copied().find(|n| cells.get(*n).map(|c| c.key != "bar").unwrap_or(true) && fits(*n, NAME_OWED)).unwrap_or(cells.len());
    let from = a.min(b);
    let cut: usize = rows.iter().map(|row| (row.room.min(row.name) as i64 - label_room(row, cells, from)).max(0) as usize).sum();
    (from, (from.max(if cells.first().map(|c| c.key == "bar").unwrap_or(false) { 1 } else { 0 }), if from < cells.len() { cut } else { 0 }))
}

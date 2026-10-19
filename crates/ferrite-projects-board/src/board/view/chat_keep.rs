//! Where an opened chat stays while its conversation changes under it: on the lines you were reading, found again
//! wherever they moved (the oldest messages drop off as new ones come), or on its end when you were there.
use crate::board::state::ChatAt;
use std::collections::BTreeSet;

/// How many lines from the top of what showed are looked for again.
const LOOK: usize = 24;

/// The first line to show of `lines` (`last`: the last one that can be first), for a chat opened at `at`.
pub fn kept(at: &ChatAt, lines: &[String], last: usize) -> usize {
    let (top, was) = (at.top.unwrap_or(0), &at.lines);
    if at.end { return last; }
    if was.is_empty() || was.as_slice() == lines { return top.min(last); }
    // How the conversation moved: of the shifts that bring a line you were reading to a place it stands now, the one
    // that lines up the most of the rest (the smallest on a tie), so that a line said twice is found where it was.
    let (n, m) = (lines.len() as i64, was.len() as i64);
    let same = |d: i64| (0.max(-d)..n.min(m - d)).filter(|&k| lines[k as usize] == was[(k + d) as usize]).count();
    let shifts: BTreeSet<i64> = (top..was.len().min(top + LOOK)).filter(|&i| !was[i].trim().is_empty())
        .flat_map(|i| lines.iter().enumerate().filter(move |(_, l)| **l == was[i]).map(move |(k, _)| i as i64 - k as i64)).chain([0]).collect();
    let d = shifts.into_iter().map(|d| (same(d), -d.abs(), d)).max().map(|x| x.2).unwrap_or(0);
    (top as i64 - d).clamp(0, last as i64) as usize
}

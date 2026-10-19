//! A live task's third line on a narrow screen: the bar, the percent, the minutes left, the review tag, the pull
//! request and the time, which have no room beside the name there. Below TALL_BELOW columns every live row has it.
use super::fields::{fields, mark, F};
use super::row::Layout;
use super::row_fit::PERCENT_MIN;
use crate::board::state::State;
use crate::board::theme::Theme;
use crate::json::*;
use crate::shown::ink::{Ink, Inked};
use crate::shown::states::status_ink;
use crate::shown::words::bar;

/// Below this many columns every live row has the third line; from it on, none has.
pub const TALL_BELOW: usize = 80;
/// A bar shorter than this says nothing: the percent beside it does.
const BAR_MIN: usize = 4;
/// The minutes left are this wide at least ("10m left").
const ETA_MIN: usize = 8;
pub fn tall_at(screen: usize) -> bool { screen < TALL_BELOW }
#[derive(Clone, Debug, Default)]
pub struct Third { pub percent: usize, pub eta: usize, pub review: usize, pub pr: usize, pub elapsed: usize, pub ago: bool, pub right: usize }
fn groups(w: &Third) -> Vec<Vec<(&'static str, usize)>> {
    [vec![("percent", w.percent), ("eta", w.eta)], vec![("review", w.review), ("pr", w.pr)], vec![("time", w.elapsed + if w.ago { 4 } else { 0 })]]
        .into_iter().map(|g| g.into_iter().filter(|(_, c)| *c > 0).collect::<Vec<_>>()).filter(|g| !g.is_empty()).collect()
}
fn progress(g: &[(&str, usize)]) -> bool { ["percent", "eta"].contains(&g[0].0) }
fn before(n: usize, all: &[Vec<(&str, usize)>]) -> usize { if Some(n) == all.iter().position(|g| !progress(g)) { 2 } else { 1 } }
fn right(w: &Third) -> usize { let g = groups(w); g.iter().enumerate().map(|(n, x)| before(n, &g) + x.iter().map(|(_, c)| c + 1).sum::<usize>() - 1).sum() }
/// The third line's columns, once for the whole list. In a width too small for all of them "ago" goes first, then
/// the tags, then the percent and the minutes; the time always stays.
pub fn tall_layout(f: &[F], width: usize, lead: usize) -> Third {
    let max = |k: &str| f.iter().map(|x| if k == "elapsed" { crate::text::width(&x.elapsed) } else { x.get(k).width() }).max().unwrap_or(0);
    let elapsed = max("elapsed").max(3);
    let mut w = Third { percent: if max("percent") > 0 { max("percent").max(PERCENT_MIN) } else { 0 }, eta: if max("eta") > 0 { max("eta").max(ETA_MIN) } else { 0 }, review: max("review"), pr: max("pr").min(width.saturating_sub(3 + elapsed + 3)), elapsed, ago: f.iter().any(|x| x.ago), right: 0 };
    for drop in [&["ago"][..], &["review", "pr"], &["percent", "eta"]] {
        if lead + right(&w) > width { for k in drop { match *k { "ago" => w.ago = false, "review" => w.review = 0, "pr" => w.pr = 0, "percent" => w.percent = 0, _ => w.eta = 0 } } }
    }
    // The bar takes what the columns leave: too short to read, it gives its room to the word "ago" before it goes.
    if w.ago && f.iter().any(|x| !x.bar.is_empty()) && (width as i64 - lead as i64 - right(&w) as i64) < BAR_MIN as i64 { w.ago = false; }
    w.right = right(&w);
    w
}
fn fill(x: &Inked, n: usize, left: bool) -> Inked { let gap = Inked::spaces(n.saturating_sub(x.width())); if left { x.clone().add(gap) } else { gap.add(x.clone()) } }
/// The line itself: right of the mark and the numbers' column to the right edge. A row with no progress, no review
/// and no pull request yet still has its time there, so every live row is the same height.
pub fn tall_line(s: &State, t: &Theme, r: &Obj, width: usize, lay: &Layout) -> Inked {
    let w = lay.third.clone().unwrap_or_default();
    let f = fields(s, t, r, lay.now);
    let text = |k: &str| -> Inked { match k { "pr" => f.pr.cut(w.pr), "time" => Inked::of(format!("{}{}", f.elapsed, if w.ago && f.ago { " ago" } else { "" }), Ink::Muted), k => f.get(k) } };
    let g = groups(&w);
    let mut rt = Inked::new();
    for (n, grp) in g.iter().enumerate() {
        rt = rt.add(Inked::spaces(before(n, &g)));
        for (m, (k, c)) in grp.iter().enumerate() { if m > 0 { rt.push(" ", Ink::Plain); } rt = rt.add(fill(&text(k), *c, *k == "pr")); }
    }
    let lead = (mark(t, r).width() + 2 + if lay.num > 0 { lay.num + 1 } else { 0 }).min(width);
    let room = width as i64 - lead as i64 - w.right as i64;
    let drawn = if !f.bar.is_empty() && room >= BAR_MIN as i64 { Inked::of(bar(r.f("progress"), room as usize), status_ink(r)) } else { Inked::new() };
    Inked::spaces(lead).add(fill(&drawn, room.max(0) as usize, true)).add(rt).cut(width)
}

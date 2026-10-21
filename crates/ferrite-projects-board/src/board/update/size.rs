//! The board's size, decided: what herdr shows of its pane against the terminal's size (the board draws inside what is
//! shown), a viewer that holds the terminal at its own size (taken back after two looks agree, twice at most, then it
//! says what frees it), a remote view somebody looks through (the board is its guest and keeps away), and a size that
//! changes for a moment (not laid out for until it has lasted `settle_ms`). Pure: what herdr said comes in as an event,
//! taking the terminal goes out as an effect; run/size.rs asks herdr. docs/board-size.md says why.
use super::Effect;
use crate::board::state::State;
use crate::shown::ink::Ink;

/// How herdr shows the pane: its outer rectangle, and whether other panes are beside it (then a frame is inside it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown { pub cols: u16, pub rows: u16, pub split: bool }
/// What the board may draw on, and whether the terminal and what is shown disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit { pub cols: usize, pub rows: usize, pub held: bool }
/// A remote view opened this many seconds ago or less, or back this soon after it was disconnected, has somebody
/// looking through it (herdr's web UI reattaches by itself within a few seconds).
const FRESH_S: i64 = 20;
const BACK_MS: i64 = 20000;
#[derive(Clone, Debug, Default)]
pub struct Size {
    /// The terminal's size at the last draw ("COLSxROWS"), and the one herdr's answer is waited for after a change.
    pub term: String, pub drawn_for: String,
    /// herdr's answer: the one drawn for and the size it was for, the last one heard and since when, a confirmed change.
    pub shown: Option<Shown>, pub shown_for: String, pub shown_seen: String, pub shown_since: i64, pub sure_for: String,
    /// Looks in a row that disagreed, takes made, the size taken at, when one worked, why the last failed.
    pub off_seen: u32, pub takes: u32, pub took_at: String, pub took_back_at: i64, pub take_failed: String,
    /// When a remote view was last asked about, and the views taken for ones somebody looks through.
    pub live_at: i64, pub hosts: Option<Vec<i64>>,
    /// The size the screen stands at while a new one has not lasted yet, and since when it waits.
    pub still: Option<(usize, usize)>, pub moved_at: i64,
    /// The held size the last look saw: said only when two looks agree.
    pub held_seen: String,
    /// herdr is to be asked what it shows (the runtime asks once the frame is drawn).
    pub ask: bool,
}
impl Size { pub fn gave_up(&self) -> bool { self.takes >= 2 } }
pub fn fit_shown(cols: usize, rows: usize, shown: Option<Shown>) -> Fit {
    let Some(s) = shown else { return Fit { cols, rows, held: false } };
    let border = if s.split { 2 } else { 0 };
    let (most_c, most_r) = (s.cols as i64 - border, s.rows as i64 - border);
    // Whether one more column goes to herdr's own edge is not told: a terminal up to that wide fits, one held is drawn a column narrower.
    let (in_c, in_r) = (most_c - 1, most_r);
    let (c, r) = (cols as i64, rows as i64);
    let held = c > most_c || r > most_r || c < in_c || r < in_r;
    Fit { cols: if c > most_c { in_c.max(1) as usize } else { cols }, rows: if r > most_r { in_r.max(1) as usize } else { rows }, held }
}
fn size_key(c: usize, r: usize) -> String { format!("{c}x{r}") }
/// What herdr shows, for the terminal's size now (`settle_ms` 0: what it said last). After a change it is asked at
/// once, and the board draws on its whole terminal until it has answered for this size.
pub fn shown_now(s: &mut State, cols: usize, rows: usize) -> Option<Shown> {
    let size = size_key(cols, rows);
    s.size.term = size.clone();
    if s.ctx.settle_ms <= 0 { return s.size.shown; }
    if size != s.size.drawn_for { s.size.drawn_for = size.clone(); if !s.ctx.pane.is_empty() { s.size.ask = true; } }
    if s.size.shown_for == size { s.size.shown } else { None }
}
/// herdr's answer about the pane, for a terminal of `term`, with the remote views on its terminal where it disagrees
/// (None: not known). One that disagrees with the terminal is drawn for once herdr said the same again after
/// `settle_ms`; seen twice in a row, the terminal is taken back, unless somebody looks through the view that holds it.
pub fn see(s: &mut State, term: (usize, usize), shown: Option<Shown>, by: Option<Vec<(i64, i64)>>, now: i64, fx: &mut Vec<Effect>) {
    let tsize = size_key(term.0, term.1);
    let held = fit_shown(term.0, term.1, shown).held;
    let ms = s.ctx.settle_ms.max(0);
    let key = format!("{shown:?}");
    if key != s.size.shown_seen { s.size.shown_seen = key.clone(); s.size.shown_since = now; }
    let sure = !held || ms == 0 || now - s.size.shown_since >= ms;
    let changed = sure && key != format!("{:?}", s.size.shown);
    if sure { s.size.shown = shown; s.size.shown_for = tsize.clone(); }
    if changed && ms > 0 { s.size.sure_for = tsize; } // it has waited already: drawn for at once
    s.size.off_seen = if held { s.size.off_seen + 1 } else { 0 };
    if held && s.msg.starts_with("Another viewer held ") { s.msg.clear(); } // it is back: that is no longer true
    if !held && s.size.takes > 0 { if s.size.gave_up() { s.size.takes = 0; } else { took_back(s, now); } }
    if !held { if shown.is_some() { s.guest = false; } return; }
    if !(sure && s.size.off_seen >= 2 && !s.guest) { return; }
    // Is somebody looking through it? Asked before each take, and every few seconds after giving up.
    let ask = !s.size.gave_up() || now - s.size.live_at >= 3000;
    if ask { s.size.live_at = now; }
    if ask && live_view(s, by, now) { s.guest = true; s.size.takes = 0; }
    else if !s.size.gave_up() {
        let f = fit_shown(term.0, term.1, s.size.shown);
        s.size.off_seen = 0; s.size.takes += 1; s.size.took_at = format!("{}×{}", term.0, term.1);
        fx.push(Effect::Take { cols: f.cols as u16, rows: f.rows as u16 });
    }
}
/// What taking the terminal answered: a failure is said, and tried again at the next look.
pub fn took(s: &mut State, r: Result<(), String>) {
    match r {
        Ok(()) => s.size.take_failed.clear(),
        Err(e) => { s.size.take_failed = crate::shown::words::clean(if e.is_empty() { "herdr refused" } else { &e }); s.size.takes = s.size.takes.saturating_sub(1); s.size.off_seen = s.size.off_seen.max(2); }
    }
}
fn took_back(s: &mut State, now: i64) {
    s.size.takes = 0; s.size.took_back_at = now;
    let at = s.size.took_at.clone();
    s.flash(format!("Another viewer held this terminal at {at}: it was disconnected, and your window has it again."), Ink::Muted);
}
/// A view somebody looks through: just opened, back right after being disconnected, or one taken for such before.
fn live_view(s: &mut State, by: Option<Vec<(i64, i64)>>, now: i64) -> bool {
    let Some(by) = by else { return s.size.hosts.as_ref().map(|h| !h.is_empty()).unwrap_or(false) }; // herdr did not answer
    if by.is_empty() { return false; } // no such view: a second herdr window, handled by the takes
    let known = s.size.hosts.clone().unwrap_or_default();
    let live = by.iter().any(|v| known.contains(&v.0)) || now - s.size.took_back_at < BACK_MS || by.iter().map(|v| v.1).min().unwrap_or(i64::MAX) < FRESH_S;
    if live { s.size.hosts = Some(by.iter().map(|v| v.0).collect()); }
    live
}
/// A size that changes and is back within a moment is not laid out for: the size the screen stands at, which follows
/// `want` once that has lasted `settle_ms`. Every change waits, also one right after another.
pub fn steady(s: &mut State, want: (usize, usize), now: i64) -> (usize, usize) {
    let ms = s.ctx.settle_ms;
    let sure = !s.size.sure_for.is_empty() && s.size.sure_for == s.size.term;
    if sure { s.size.sure_for.clear(); }
    match s.size.still {
        Some(still) if ms > 0 && !sure && still != want => {
            if s.size.moved_at == 0 { s.size.moved_at = now; }
            if now - s.size.moved_at < ms { return still; }
        }
        _ => {}
    }
    s.size.still = Some(want); s.size.moved_at = 0;
    want
}
/// The note above the hints while herdr shows the pane at another size than the terminal has (the line at the top
/// says the same with more: while that shows, this is not needed).
pub fn off_note(s: &State, cols: usize, rows: usize, shown: Option<Shown>, held: bool) -> String {
    let Some(sh) = shown.filter(|_| held && s.held.is_none()) else { return String::new() };
    let border = if sh.split { 2 } else { 0 };
    let then = if !s.size.take_failed.is_empty() { format!("Taking it back failed ({}): trying again.", s.size.take_failed) }
        else if s.size.gave_up() { "A key typed in this window takes it back: another herdr window (a second device, a browser tab) has this session at its own size.".into() }
        else { "The board takes it back.".into() };
    format!("Another viewer holds this terminal at {cols}×{rows}; herdr shows it at {}×{}. {then}", sh.cols as i64 - border, sh.rows as i64 - border)
}

//! Is the board's terminal sized by something other than herdr's own window? A `herdr terminal attach` (herdr's web
//! UI starts one per pane it shows) resizes the pane's terminal to its own size and holds it there; herdr's window then
//! shows a cut-out. herdr says where the pane sits in its window (`pane layout`); a terminal of another size than that
//! place (less its frame) is held by another view. Checked on herdr 0.9.3. Also herdr asked what it shows of the pane,
//! and the terminal taken back, off the loop, for what update/size.rs decides.
use crate::board::state::Held;
use crate::herdr::Herdr;
use crate::json::*;

const SLACK: (i64, i64) = (4, 3);
pub fn held_size(cols: u16, rows: u16, layout: &Value, pane: &str) -> Option<(u16, u16, (u16, u16))> {
    let rect = layout.arr("panes").iter().find(|p| p.s("pane_id") == pane).map(|p| p.at("rect").clone())?;
    let (c, r) = (cols as i64, rows as i64);
    let fits = |x: &Value| x.is_object() && c <= x.i("width") && c >= x.i("width") - SLACK.0 && r <= x.i("height") && r >= x.i("height") - SLACK.1;
    // Zooming a pane focuses it: the zoomed pane has the tab, not its old place.
    let room = if layout.b("zoomed") && layout.s("focused_pane_id") == pane && layout.at("area").is_object() { layout.at("area").clone() } else { rect };
    if fits(&room) { return None; }
    Some((cols, rows, (room.i("width") as u16, room.i("height") as u16)))
}
/// The attach processes on this machine that hold a terminal, oldest first: (pid, seconds).
pub fn holders(terminal: &str) -> Vec<(i64, i64)> {
    if terminal.is_empty() { return vec![]; }
    // `etime`, not `etimes`: macOS's ps has no `etimes` and fails outright, and then no view was ever seen.
    let out = std::process::Command::new("ps").args(["-eo", "pid=,etime=,args="]).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    holders_in(terminal, &out)
}
/// The same, from what `ps -eo pid=,etime=,args=` printed.
pub fn holders_in(terminal: &str, out: &str) -> Vec<(i64, i64)> {
    if terminal.is_empty() { return vec![]; }
    let valued = ["--session", "--machine", "--remote", "--remote-keybindings"];
    let mut v: Vec<(i64, i64)> = out.lines().filter_map(|l| {
        let mut it = l.split_whitespace();
        let (pid, secs) = (it.next()?.parse::<i64>().ok()?, elapsed(it.next()?)?);
        let a: Vec<&str> = it.collect();
        let first = a.first()?;
        if !(*first == "herdr" || first.ends_with("/herdr")) { return None; }
        let mut plain = vec![];
        let mut n = 1;
        while n < a.len() { if valued.contains(&a[n]) { n += 1; } else if !a[n].starts_with('-') { plain.push(a[n]); } n += 1; }
        (plain.len() == 3 && plain[0] == "terminal" && plain[1] == "attach" && plain[2] == terminal).then_some((pid, secs))
    }).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}
/// `ps`'s elapsed time in seconds: `[[days-]hours:]minutes:seconds`, or plain seconds.
fn elapsed(t: &str) -> Option<i64> {
    let (days, rest) = match t.split_once('-') { Some((d, r)) => (d.parse::<i64>().ok()?, r), None => (0, t) };
    rest.split(':').try_fold(0i64, |n, p| p.parse::<i64>().ok().map(|v| n * 60 + v)).map(|s| days * 86400 + s)
}
/// One look, for the pane this process runs in. None when nothing holds the size, or it cannot be known.
pub fn look(cols: u16, rows: u16) -> Option<Held> {
    let pane = crate::util::env_val("HERDR_PANE_ID")?;
    if std::env::var("HERDR_ENV").as_deref() != Ok("1") || crate::util::env_set("TMUX") { return None; }
    let h = Herdr::new(None);
    let (c, r, room) = held_size(cols, rows, &h.pane_layout(&pane)?, &pane)?;
    let id = h.pane_get(&pane).map(|p| p.t("terminal_id")).unwrap_or_default();
    Some(Held { columns: c, rows: r, room, by: holders(&id), known: !id.is_empty() })
}
/// Is a remote view attached to this pane's terminal at all (a browser showing the board)?
pub fn viewed() -> Option<bool> {
    let pane = crate::util::env_val("HERDR_PANE_ID")?;
    if std::env::var("HERDR_ENV").as_deref() != Ok("1") || crate::util::env_set("TMUX") { return None; }
    let id = Herdr::new(None).pane_get(&pane)?.t("terminal_id");
    if id.is_empty() { return None; }
    Some(!holders(&id).is_empty())
}
/// herdr asked what it shows of the pane, off the loop: the answer is for the terminal's size when it was asked (asked
/// again when the size changed meanwhile), with the remote views on its terminal where the two disagree.
pub fn ask_shown(tx: &std::sync::mpsc::Sender<super::Msg>, pane: &str, busy: &std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use crate::board::update::size::{fit_shown, Shown};
    use std::sync::atomic::Ordering::SeqCst;
    if pane.is_empty() || busy.swap(true, SeqCst) { return; }
    let (tx, pane, busy) = (tx.clone(), pane.to_string(), busy.clone());
    std::thread::spawn(move || for n in 0..5 {
        let at = crossterm::terminal::size().unwrap_or((0, 0));
        let h = Herdr::new(None);
        let shown = h.pane_layout(&pane).and_then(|l| crate::herdr::api::shown_of(&l, &pane)).map(|(cols, rows, split)| Shown { cols, rows, split });
        if crossterm::terminal::size().unwrap_or((0, 0)) != at && n < 4 { continue; }
        let by = if fit_shown(at.0 as usize, at.1 as usize, shown).held { h.pane_get(&pane).map(|p| p.t("terminal_id")).filter(|id| !id.is_empty()).map(|id| holders(&id)) } else { None };
        busy.store(false, SeqCst);
        let _ = tx.send(super::Msg::Ev(crate::board::update::Event::Shown { term: at, shown, by }));
        return;
    });
}
/// The terminal taken back from the viewer that holds it, at this size.
pub fn take(tx: &std::sync::mpsc::Sender<super::Msg>, pane: &str, cols: u16, rows: u16) {
    let (tx, pane) = (tx.clone(), pane.to_string());
    std::thread::spawn(move || { let r = crate::herdr::api::take_terminal(&Herdr::new(None), &pane, cols, rows).map_err(|e| e.message); let _ = tx.send(super::Msg::Ev(crate::board::update::Event::Took(r))); });
}
/// A frame drawn again after `ms`: a new size that has lasted by then is laid out for.
pub fn wake(tx: &std::sync::mpsc::Sender<super::Msg>, ms: u64) { let tx = tx.clone(); std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_millis(ms)); let _ = tx.send(super::Msg::Resize); }); }

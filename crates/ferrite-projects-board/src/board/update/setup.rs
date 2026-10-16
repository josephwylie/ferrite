//! Where this board is, and what follows from it: the popup's own key closes it; the first run's one screen; a draft
//! brings the board back to the repo it was typed in.
use super::Effect;
use crate::board::job::Job;
use crate::board::keys::{is, key_name, norm, Press, K};
use crate::board::state::{Away, State};
use crate::shown::ink::Ink;

/// How long the board key's second press is waited for after herdr's prefix key.
const PREFIX_MS: i64 = 3000;
fn pressed(p: &Press, chord: &str) -> bool { norm(&p.chord) == norm(chord) || p.ch.map(|c| c.to_string() == chord).unwrap_or(false) }
/// herdr hands its popup every key, its own too: the board knows the key that opened it and closes on it, which makes
/// it the toggle it is everywhere else. herdr's prefix key does nothing by itself here.
pub fn own_key(s: &mut State, p: &Press, fx: &mut Vec<Effect>) -> bool {
    let Some((first, key)) = s.ctx.own_key.clone().filter(|_| s.ctx.popup) else { return false };
    let armed = s.prefixed > 0 && s.now - s.prefixed < PREFIX_MS;
    s.prefixed = 0;
    if if first.is_empty() { pressed(p, &key) } else { armed && pressed(p, &key) } { fx.push(Effect::Save); fx.push(Effect::Close); return true; }
    if first.iter().any(|f| pressed(p, f)) { s.prefixed = s.now; return true; }
    false
}
/// The first run: 1 and 2 switch a choice on or off, k types another key, enter saves, esc asks again next time.
pub fn key(s: &mut State, p: &Press, fx: &mut Vec<Effect>) {
    let st = s.setup.as_mut().unwrap();
    let (enter, esc) = (is(p, K::Enter), is(p, K::Back) || is(p, K::Close));
    if st.typing {
        if esc { st.typing = false; s.input.set_text(""); return; }
        if !enter { s.input.handle(p); return; }
        let v = s.input.text().trim().to_lowercase();
        // A key herdr or the config already uses is refused here, with what it does today: it is never kept.
        if !v.is_empty() { if let Err(m) = crate::extras::check_key(&v) { s.input.set_text(""); return s.flash(m, Ink::Red); } }
        let st = s.setup.as_mut().unwrap();
        st.typing = false;
        if let Some(k) = st.key.as_mut() { if !v.is_empty() { *k = (true, v, String::new()); } }
        s.input.set_text(""); s.msg.clear();
        return;
    }
    if esc { s.setup = None; return s.flash("Asked again next time. Nothing was changed.", Ink::Muted); }
    match p.ch {
        Some('1') => if let Some(k) = st.key.as_mut() { if k.1.is_empty() { st.typing = true; } else { k.0 = !k.0; } },
        Some('2') => if let Some(b) = st.sidebar.as_mut() { *b = !*b; },
        Some('k') | Some('K') => if st.key.is_some() { st.typing = true; s.input.set_text(""); },
        _ if enter => {
            let key = st.key.as_ref().map(|k| if k.0 { k.1.clone() } else { String::new() });
            fx.push(Effect::Run(Job::Setup { key, sidebar: st.sidebar, file: st.file.clone() }));
        }
        _ => {}
    }
}
/// Words put away and brought back: the board goes to the repo they were typed for.
pub fn away_home(s: &mut State, d: &Away) { if d.to.is_empty() { to_repo_of(s, &d.cwd); } }
/// The popup opens wherever you are, and what you type is sent from the repo of that place. Words typed for another
/// repo keep the board there while they are in the box, and it says so.
pub fn to_repo_of(s: &mut State, was: &str) {
    if !s.ctx.popup || was.is_empty() || was == s.ctx.cwd { return; }
    let base = |p: &str| std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !std::path::Path::new(was).exists() { let now = base(&s.ctx.cwd); return s.flash(format!("What you typed was for {}, a folder that is gone. Sent now, it goes from {now}; {} puts it away.", base(was), key_name("back")), Ink::Yellow); }
    // Another folder of the same repo is no move: nothing is said.
    let repo = |p: &str| crate::store::repos::repo_of(std::path::Path::new(p)).map(|r| r.0).ok();
    let same = repo(was).is_some() && repo(was) == repo(&s.ctx.cwd);
    s.ctx.cwd = was.into();
    if same { return; }
    s.flash(format!("What you typed was for {}, so the board is there now. Once it is sent or put away, it opens where you are.", base(was)), Ink::Yellow);
}

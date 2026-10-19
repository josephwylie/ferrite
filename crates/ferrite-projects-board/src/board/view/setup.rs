//! The first run, drawn: one screen asks what the plugin may add to herdr's settings. Only inside herdr, only what is
//! missing; nothing is changed without a yes.
use crate::board::keys::{key_name, name, K};
use crate::board::state::State;
use crate::shown::ink::{Ink, Inked};

pub fn setup_lines(s: &State, width: usize, rows: usize) -> Vec<Inked> {
    let st = s.setup.as_ref().unwrap();
    let wrap = |x: Inked, indent: &str| -> Vec<Inked> { x.wrap(width.saturating_sub(2 + indent.len()).max(10)).into_iter().map(|l| Inked::plain(indent).add(l)).collect() };
    let check = |on: bool| if on { Inked::of("[x]", Ink::Green) } else { Inked::of("[ ]", Ink::Muted) };
    let mut l = vec![Inked::new(), Inked::plain("  ").add(Inked::plain("herdr-projects · first run").bold()), Inked::new()];
    l.extend(wrap(Inked::of("Two things the board can add to herdr's settings. Both are optional.", Ink::Soft), "  "));
    if let Some((on, value, note)) = &st.key {
        l.push(Inked::new());
        let what = if value.is_empty() { Inked::plain("Open this board with a key of your own (1 or k: type it)") } else { Inked::plain("Open this board with ").add(Inked::plain(value.clone()).bold()).add(Inked::plain(", from anywhere in herdr")) };
        l.extend(wrap(Inked::of("1", Ink::Accent).add(Inked::plain("  ")).add(check(*on)).add(Inked::plain(" ")).add(what), "  "));
        if !note.is_empty() { l.extend(wrap(Inked::of(note.clone(), Ink::Yellow), "         ")); }
        if !value.is_empty() { l.extend(wrap(Inked::of("k: use another key", Ink::Muted), "         ")); }
    }
    if let Some(on) = st.sidebar { l.push(Inked::new()); l.extend(wrap(Inked::of("2", Ink::Accent).add(Inked::plain("  ")).add(check(on)).add(Inked::plain(" Show each task's stage, and what it is doing, in herdr's sidebar")), "  ")); }
    l.push(Inked::new());
    l.extend(wrap(Inked::of(format!("Saved in {} between two marker lines. The file is backed up first, and herdr-projects remove-extras takes it all out again.", crate::store::marks::tilde(&st.file)), Ink::Muted), "  "));
    l.push(Inked::new());
    if st.typing {
        let v = s.value();
        l.push(Inked::plain("  ").add(Inked::of("the key ❯", Ink::Accent)).add(Inked::plain(" ")).add(if v.is_empty() { Inked::of("for example prefix+a", Ink::Muted) } else { Inked::plain(v) }));
        l.push(Inked::new());
        l.extend(wrap(Inked::of(format!("{} keeps it · {} back", name(K::Enter), name(K::Back)), Ink::Muted), "  "));
    } else {
        l.extend(wrap(Inked::of("1 2", Ink::Text).add(Inked::of(" on or off · ", Ink::Muted)).add(Inked::of(key_name("enter"), Ink::Text)).add(Inked::of(" save · ", Ink::Muted)).add(Inked::of(key_name("back"), Ink::Text)).add(Inked::of(" ask me next time", Ink::Muted)), "  "));
    }
    if !s.msg.is_empty() { l.push(Inked::new()); l.extend(wrap(Inked::of(s.msg.clone(), s.msg_kind), "  ")); }
    l.resize(rows.max(l.len()), Inked::new());
    l.truncate(rows);
    l
}

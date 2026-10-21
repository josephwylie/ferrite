//! A key as crossterm reads it, said as the board's keys name chords ("ctrl+l", "shift+enter", "up"). Old terminals
//! send alt as esc and a letter; crossterm reads that as alt too, so the alt keys work in every view, the web UI's
//! terminal included.
use crate::board::keys::Press;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

pub fn press_of(k: &KeyEvent) -> Option<Press> {
    if k.kind == KeyEventKind::Release { return None; }
    let m = k.modifiers;
    let mut mods = vec![];
    if m.contains(KeyModifiers::CONTROL) { mods.push("ctrl"); }
    if m.contains(KeyModifiers::ALT) { mods.push("alt"); }
    let shift = m.contains(KeyModifiers::SHIFT);
    let (name, ch): (String, Option<char>) = match k.code {
        KeyCode::Char(c) => {
            let typed = mods.is_empty() && !c.is_control();
            // ctrl+letter arrives as the letter with ctrl; a shifted letter as its capital.
            let base = if c.is_ascii_uppercase() && !mods.is_empty() { c.to_ascii_lowercase() } else { c };
            if typed { return Some(Press::char(c)); }
            (if base == ' ' { "space".to_string() } else { base.to_string() }, None)
        }
        KeyCode::Enter => ("enter".into(), None), KeyCode::Tab => ("tab".into(), None), KeyCode::BackTab => ("tab".into(), None),
        KeyCode::Backspace => ("backspace".into(), None), KeyCode::Delete => ("delete".into(), None), KeyCode::Esc => ("escape".into(), None),
        KeyCode::Up => ("up".into(), None), KeyCode::Down => ("down".into(), None), KeyCode::Left => ("left".into(), None), KeyCode::Right => ("right".into(), None),
        KeyCode::Home => ("home".into(), None), KeyCode::End => ("end".into(), None), KeyCode::PageUp => ("pageup".into(), None), KeyCode::PageDown => ("pagedown".into(), None),
        KeyCode::F(n) => (format!("f{n}"), None),
        _ => return None,
    };
    let shifted = shift && (k.code == KeyCode::Enter || k.code == KeyCode::Tab || k.code == KeyCode::BackTab || matches!(k.code, KeyCode::Char(c) if c.is_ascii_alphabetic()) || !matches!(k.code, KeyCode::Char(_)));
    if shifted { mods.push("shift"); }
    if k.code == KeyCode::BackTab && !mods.contains(&"shift") { mods.push("shift"); }
    mods.push(&name);
    Some(Press { chord: mods.join("+"), ch })
}

//! A GPUI keystroke said as the board names chords (`ctrl+l`,
//! `shift+enter`, `up`), exactly as upstream's terminal runtime reads a
//! crossterm key (`board/run/input.rs`): a typed character is itself (a
//! capital is `shift+` its letter, so `Y` and `N` are told apart), ctrl and
//! alt name their letter, shift names every other key it holds.
//!
//! On macOS alt is Option, and Option+letter reaches the board as
//! `alt+<letter>`, never as the character Option composes (`π` for
//! Option+P): the board's alt keys are its own. There is no terminal byte
//! stream, so no ESC-prefixed letter to read as alt. ⌘ stays the window's
//! (`None`), but for ⌘V, which pastes into the board as one paste.

use gpui::Keystroke;
use sw_board::board::keys::Press;

/// The keystroke as a board key press, or `None` for one the window keeps.
pub(crate) fn press_of(keystroke: &Keystroke) -> Option<Press> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform {
        return None;
    }
    let mut mods = Vec::new();
    if modifiers.control {
        mods.push("ctrl");
    }
    if modifiers.alt {
        mods.push("alt");
    }
    let key = keystroke.key.as_str();
    let named = match key {
        "enter" | "tab" | "backspace" | "delete" | "escape" | "up" | "down" | "left" | "right"
        | "home" | "end" | "pageup" | "pagedown" => Some(key.to_string()),
        f if f.len() > 1 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => Some(f.into()),
        _ => None,
    };
    let name = match named {
        Some(name) => {
            if modifiers.shift {
                mods.push("shift");
            }
            name
        }
        None => {
            let base = if key == "space" { ' ' } else { single(key)? };
            if mods.is_empty() {
                // Typed: the character the key makes, shift and all.
                let shifted = if modifiers.shift {
                    base.to_ascii_uppercase()
                } else {
                    base
                };
                let typed = keystroke
                    .key_char
                    .as_deref()
                    .and_then(single)
                    .unwrap_or(shifted);
                return (!typed.is_control()).then(|| Press::char(typed));
            }
            let base = base.to_ascii_lowercase();
            if modifiers.shift && base.is_ascii_alphabetic() {
                mods.push("shift");
            }
            if base == ' ' {
                "space".into()
            } else {
                base.to_string()
            }
        }
    };
    mods.push(&name);
    Some(Press {
        chord: mods.join("+"),
        ch: None,
    })
}

/// ⌘V (ctrl-V on Windows): the clipboard goes in as one paste.
pub(crate) fn is_paste(keystroke: &Keystroke) -> bool {
    let primary = match crate::keymap::PLATFORM {
        crate::keymap::Platform::Mac => keystroke.modifiers.platform,
        crate::keymap::Platform::Windows => keystroke.modifiers.control,
    };
    primary && keystroke.key == "v"
}

fn single(text: &str) -> Option<char> {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    }
}

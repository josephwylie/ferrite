//! What a keystroke writes to the PTY: text as typed, control letters,
//! the named keys as xterm sends them (arrows honour the application
//! cursor mode), and alt as an escape prefix. A command-key chord is the
//! window's, not the shell's.

use alacritty_terminal::term::TermMode;
use gpui::Keystroke;

/// The bytes `keystroke` writes, or `None` where it is no input for the shell.
pub(crate) fn encode(keystroke: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    let m = &keystroke.modifiers;
    if m.platform || m.function {
        return None;
    }
    // xterm's modifier parameter: 1 + shift 1, alt 2, control 4.
    let param = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.control);
    let cursor = |final_byte: char| -> Vec<u8> {
        match (param, mode.contains(TermMode::APP_CURSOR)) {
            (1, true) => format!("\x1bO{final_byte}"),
            (1, false) => format!("\x1b[{final_byte}"),
            (param, _) => format!("\x1b[1;{param}{final_byte}"),
        }
        .into_bytes()
    };
    let tilde = |number: u8| -> Vec<u8> {
        match param {
            1 => format!("\x1b[{number}~"),
            param => format!("\x1b[{number};{param}~"),
        }
        .into_bytes()
    };
    let named: Option<Vec<u8>> = match keystroke.key.as_str() {
        "enter" => Some(b"\r".to_vec()),
        "tab" if m.shift => Some(b"\x1b[Z".to_vec()),
        "tab" => Some(b"\t".to_vec()),
        "escape" => Some(b"\x1b".to_vec()),
        "backspace" if m.alt => Some(b"\x1b\x7f".to_vec()),
        "backspace" => Some(b"\x7f".to_vec()),
        "delete" => Some(tilde(3)),
        "up" => Some(cursor('A')),
        "down" => Some(cursor('B')),
        "right" => Some(cursor('C')),
        "left" => Some(cursor('D')),
        "home" => Some(cursor('H')),
        "end" => Some(cursor('F')),
        "pageup" => Some(tilde(5)),
        "pagedown" => Some(tilde(6)),
        "space" if m.control => Some(vec![0]),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    if m.control {
        let mut chars = keystroke.key.chars();
        let letter = chars.next().filter(|_| chars.next().is_none())?;
        let byte = match letter {
            'a'..='z' => letter as u8 - b'a' + 1,
            '[' => 0x1b,
            '\\' => 0x1c,
            ']' => 0x1d,
            '^' => 0x1e,
            '_' | '/' => 0x1f,
            _ => return None,
        };
        return Some(if m.alt { vec![0x1b, byte] } else { vec![byte] });
    }
    let text = keystroke.key_char.as_deref().filter(|text| !text.is_empty())?;
    let mut bytes = Vec::new();
    if m.alt {
        bytes.push(0x1b);
    }
    bytes.extend_from_slice(text.as_bytes());
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(source: &str) -> Keystroke {
        Keystroke::parse(source).unwrap()
    }

    fn typed(text: &str) -> Keystroke {
        Keystroke {
            key_char: Some(text.into()),
            ..keys(text)
        }
    }

    fn bytes(keystroke: &Keystroke) -> Option<Vec<u8>> {
        encode(keystroke, TermMode::NONE)
    }

    #[test]
    fn text_is_what_was_typed_and_alt_prefixes_an_escape() {
        assert_eq!(bytes(&typed("a")), Some(b"a".to_vec()));
        assert_eq!(bytes(&typed("é")), Some("é".as_bytes().to_vec()));
        let alt = Keystroke { key_char: Some("b".into()), ..keys("alt-b") };
        assert_eq!(bytes(&alt), Some(b"\x1bb".to_vec()));
    }

    #[test]
    fn control_letters_are_their_control_bytes() {
        assert_eq!(bytes(&keys("ctrl-c")), Some(vec![3]));
        assert_eq!(bytes(&keys("ctrl-a")), Some(vec![1]));
        assert_eq!(bytes(&keys("ctrl-[")), Some(vec![0x1b]));
        assert_eq!(bytes(&keys("ctrl-space")), Some(vec![0]));
    }

    #[test]
    fn named_keys_are_xterms_and_arrows_follow_the_cursor_mode() {
        assert_eq!(bytes(&keys("enter")), Some(b"\r".to_vec()));
        assert_eq!(bytes(&keys("backspace")), Some(vec![0x7f]));
        assert_eq!(bytes(&keys("shift-tab")), Some(b"\x1b[Z".to_vec()));
        assert_eq!(bytes(&keys("up")), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode(&keys("up"), TermMode::APP_CURSOR), Some(b"\x1bOA".to_vec()));
        assert_eq!(bytes(&keys("ctrl-left")), Some(b"\x1b[1;5D".to_vec()));
        assert_eq!(bytes(&keys("delete")), Some(b"\x1b[3~".to_vec()));
        assert_eq!(bytes(&keys("pageup")), Some(b"\x1b[5~".to_vec()));
    }

    #[test]
    fn a_command_chord_is_the_windows() {
        assert_eq!(bytes(&keys("cmd-c")), None);
        assert_eq!(bytes(&keys("cmd-k")), None);
    }
}

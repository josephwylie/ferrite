//! Translating the seam's [`Input`](crate::web::Input) pieces into CEF events,
//! and CEF cursors back into [`Cursor`].

use cef::{CursorType, KeyEvent, KeyEventType, MouseButtonType, MouseEvent};

use crate::web::{Cursor, Modifiers, MouseButton};

// cef_event_flags_t bits.
const SHIFT: u32 = 1 << 1;
const CONTROL: u32 = 1 << 2;
const ALT: u32 = 1 << 3;
const LEFT_BUTTON: u32 = 1 << 4;
const MIDDLE_BUTTON: u32 = 1 << 5;
const RIGHT_BUTTON: u32 = 1 << 6;
const COMMAND: u32 = 1 << 7;

pub(super) fn modifier_flags(modifiers: Modifiers) -> u32 {
    let mut flags = 0;
    if modifiers.shift {
        flags |= SHIFT;
    }
    if modifiers.control {
        flags |= CONTROL;
    }
    if modifiers.alt {
        flags |= ALT;
    }
    if modifiers.command {
        flags |= COMMAND;
    }
    flags
}

/// The "button is held" flag CEF expects on moves during a drag.
pub(super) fn button_flag(button: MouseButton) -> u32 {
    match button {
        MouseButton::Left => LEFT_BUTTON,
        MouseButton::Middle => MIDDLE_BUTTON,
        MouseButton::Right => RIGHT_BUTTON,
    }
}

pub(super) fn button_type(button: MouseButton) -> MouseButtonType {
    match button {
        MouseButton::Left => MouseButtonType::LEFT,
        MouseButton::Middle => MouseButtonType::MIDDLE,
        MouseButton::Right => MouseButtonType::RIGHT,
    }
}

/// A mouse event at view position (`x`, `y`) in logical pixels (CEF's DIPs).
pub(super) fn mouse_event(x: f32, y: f32, flags: u32) -> MouseEvent {
    MouseEvent { x: x.round() as i32, y: y.round() as i32, modifiers: flags }
}

/// The CEF events for one key press or release.
///
/// Down: a `RAWKEYDOWN` for the physical key, then one `CHAR` per UTF-16 unit
/// of `text` (a windowless browser doesn't derive text from raw keys). Up: a
/// `KEYUP`.
pub(super) fn key_events(key: &str, text: Option<&str>, down: bool, modifiers: Modifiers) -> Vec<KeyEvent> {
    let flags = modifier_flags(modifiers);
    let (windows_key_code, native_key_code) = key_codes(key);
    let unmodified = first_utf16(key);
    let character = text.map(first_utf16).unwrap_or(unmodified);
    let physical = KeyEvent {
        type_: if down { KeyEventType::RAWKEYDOWN } else { KeyEventType::KEYUP },
        modifiers: flags,
        windows_key_code,
        native_key_code,
        character,
        unmodified_character: unmodified,
        ..Default::default()
    };
    let mut events = vec![physical];
    if down {
        for unit in text.unwrap_or_default().encode_utf16() {
            // AppKit reports Return as LF; Chromium's CHAR path wants CR.
            let unit = if unit == b'\n' as u16 { b'\r' as u16 } else { unit };
            events.push(KeyEvent {
                type_: KeyEventType::CHAR,
                modifiers: flags,
                windows_key_code: unit as i32,
                native_key_code,
                character: unit,
                unmodified_character: unit,
                ..Default::default()
            });
        }
    }
    events
}

fn first_utf16(text: &str) -> u16 {
    if text.chars().count() != 1 {
        return 0;
    }
    text.encode_utf16().next().unwrap_or(0)
}

/// GPUI key name → (Windows virtual key code, macOS virtual key code). CEF
/// speaks VK_* codes on every platform; on macOS it also wants the native one.
fn key_codes(key: &str) -> (i32, i32) {
    let named = match key {
        "backspace" => Some((0x08, 51)),
        "tab" => Some((0x09, 48)),
        "enter" => Some((0x0D, 36)),
        "shift" => Some((0x10, 56)),
        "control" | "ctrl" => Some((0x11, 59)),
        "alt" => Some((0x12, 58)),
        "capslock" => Some((0x14, 57)),
        "escape" => Some((0x1B, 53)),
        "space" | " " => Some((0x20, 49)),
        "pageup" => Some((0x21, 116)),
        "pagedown" => Some((0x22, 121)),
        "end" => Some((0x23, 119)),
        "home" => Some((0x24, 115)),
        "left" => Some((0x25, 123)),
        "up" => Some((0x26, 126)),
        "right" => Some((0x27, 124)),
        "down" => Some((0x28, 125)),
        "insert" => Some((0x2D, 114)),
        "delete" => Some((0x2E, 117)),
        "platform" | "cmd" => Some((0x5B, 55)),
        ";" => Some((0xBA, 41)),
        "=" => Some((0xBB, 24)),
        "," => Some((0xBC, 43)),
        "-" => Some((0xBD, 27)),
        "." => Some((0xBE, 47)),
        "/" => Some((0xBF, 44)),
        "`" => Some((0xC0, 50)),
        "[" => Some((0xDB, 33)),
        "\\" => Some((0xDC, 42)),
        "]" => Some((0xDD, 30)),
        "'" => Some((0xDE, 39)),
        _ => None,
    };
    if let Some(codes) = named {
        return codes;
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<i32>().ok()) {
        if (1..=12).contains(&n) {
            const MAC_F: [i32; 12] = [122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111];
            return (0x70 + n - 1, MAC_F[(n - 1) as usize]);
        }
    }
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        let c = c.to_ascii_lowercase();
        const MAC_LETTERS: [i32; 26] = [
            0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7,
            16, 6,
        ];
        const MAC_DIGITS: [i32; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
        if c.is_ascii_lowercase() {
            return (c.to_ascii_uppercase() as i32, MAC_LETTERS[(c as u8 - b'a') as usize]);
        }
        if c.is_ascii_digit() {
            return (c as i32, MAC_DIGITS[(c as u8 - b'0') as usize]);
        }
    }
    (0, 0)
}

pub(super) fn cursor(cursor: CursorType) -> Cursor {
    match cursor {
        CursorType::HAND => Cursor::Pointer,
        CursorType::IBEAM | CursorType::VERTICALTEXT => Cursor::Text,
        CursorType::CROSS | CursorType::CELL => Cursor::Crosshair,
        CursorType::GRAB => Cursor::Grab,
        CursorType::GRABBING | CursorType::MOVE => Cursor::Grabbing,
        CursorType::NOTALLOWED | CursorType::NODROP => Cursor::NotAllowed,
        _ => Cursor::Arrow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_named_keys_map() {
        assert_eq!(key_codes("a"), (0x41, 0));
        assert_eq!(key_codes("Z"), (0x5A, 6));
        assert_eq!(key_codes("7"), (0x37, 26));
        assert_eq!(key_codes("enter"), (0x0D, 36));
        assert_eq!(key_codes("f5"), (0x74, 96));
        assert_eq!(key_codes("nonsense"), (0, 0));
    }

    #[test]
    fn key_down_with_text_sends_char_events() {
        let events = key_events("a", Some("a"), true, Modifiers::default());
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].type_, KeyEventType::RAWKEYDOWN);
        assert_eq!(events[1].type_, KeyEventType::CHAR);
        assert_eq!(events[1].character, 'a' as u16);
        let up = key_events("a", None, false, Modifiers::default());
        assert_eq!(up.len(), 1);
        assert_eq!(up[0].type_, KeyEventType::KEYUP);
    }
}

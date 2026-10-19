//! What each key does in the text box: the editing keys pi-tui's editor had, by the same chords (docs/keys.md names
//! the text box's own). Answers whether the box took the key.
use super::TextBox;
use crate::board::keys::{norm, Press};

impl TextBox {
    pub fn handle(&mut self, p: &Press) -> bool {
        let chord = norm(&p.chord);
        // A character typed: anything printable without ctrl or alt.
        if let Some(c) = p.ch { if !chord.starts_with("ctrl+") && !chord.starts_with("alt+") { self.killed_last = false; self.type_char(c); return true; } }
        let was_kill = self.killed_last;
        self.killed_last = false;
        match chord.as_str() {
            "shift+enter" | "ctrl+j" | "alt+enter" => self.newline(),
            "backspace" | "shift+backspace" | "ctrl+h" => self.backspace(),
            "delete" | "ctrl+d" => self.delete(),
            "left" | "ctrl+b" => self.left(),
            "right" | "ctrl+f" => self.right(),
            "alt+left" | "ctrl+left" | "alt+b" => self.word_left(),
            "alt+right" | "ctrl+right" | "alt+f" => self.word_right(),
            "home" | "ctrl+a" => self.home(),
            "end" | "ctrl+e" => self.end(),
            "up" => { self.vertical(-1); }
            "down" => { self.vertical(1); }
            "pageup" | "ctrl+pageup" => { for _ in 0..5 { if !self.vertical(-1) { break; } } }
            "pagedown" | "ctrl+pagedown" => { for _ in 0..5 { if !self.vertical(1) { break; } } }
            "ctrl+w" | "alt+backspace" => { self.killed_last = was_kill; self.delete_word_back(); }
            "alt+d" | "alt+delete" => { self.killed_last = was_kill; self.delete_word_forward(); }
            "ctrl+u" => { self.killed_last = was_kill; self.delete_to_start(); }
            "ctrl+k" => { self.killed_last = was_kill; self.delete_to_end(); }
            "ctrl+y" => self.yank(),
            "alt+y" => self.yank_pop(),
            "ctrl+-" | "ctrl+_" | "ctrl+/" => self.undo(),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn typed(b: &mut TextBox, s: &str) { for c in s.chars() { b.handle(&Press::char(c)); } }
    #[test]
    fn edits_as_before() {
        let mut b = TextBox::default();
        typed(&mut b, "hello big world");
        b.handle(&Press::chord("ctrl+w"));
        assert_eq!(b.text(), "hello big ");
        b.handle(&Press::chord("ctrl+y"));
        assert_eq!(b.text(), "hello big world");
        b.handle(&Press::chord("ctrl+-"));
        assert_eq!(b.text(), "hello big ");
        b.handle(&Press::chord("shift+enter"));
        typed(&mut b, "日本👍");
        b.handle(&Press::chord("backspace"));
        assert_eq!(b.text(), "hello big \n日本");
        b.handle(&Press::chord("ctrl+a"));
        b.handle(&Press::chord("ctrl+k"));
        assert_eq!(b.text(), "hello big \n");
        b.insert("pasted\ttext\r\nline");
        assert_eq!(b.text(), "hello big \npasted    text\nline");
    }
    #[test]
    fn moves_through_wrapped_lines() {
        let mut b = TextBox::default();
        b.layout_width = 10;
        typed(&mut b, "one two three four five");
        assert!(b.vertical(-1));
        assert!(b.cursor().1 < 20);
        b.handle(&Press::chord("ctrl+e"));
        assert_eq!(b.cursor().1, b.text().len());
    }
}

//! The Composer's editing state: one text line, no window attached.

use std::ops::Range;

pub struct Line {
    content: String,
    selection: Range<usize>,
    marked: Option<Range<usize>>,
}

impl Default for Line {
    fn default() -> Self {
        Self {
            content: String::new(),
            selection: 0..0,
            marked: None,
        }
    }
}

impl Line {
    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn cursor(&self) -> usize {
        self.selection.end
    }

    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    /// Put a line back, ready to edit at its end — a prompt recalled from the
    /// queue, or history.
    pub fn set(&mut self, text: String) {
        let at = text.len();
        self.content = text;
        self.selection = at..at;
        self.marked = None;
    }

    /// Hand the line over and clear it — what Enter does.
    pub fn take(&mut self) -> String {
        self.selection = 0..0;
        std::mem::take(&mut self.content)
    }

    /// Remove the selection, or the character before the cursor.
    pub fn backspace(&mut self) {
        if self.selection.is_empty() {
            self.selection.start = self.previous_boundary(self.selection.start);
        }
        self.replace(None, "");
    }

    /// Remove the selection, or the character after the cursor.
    pub fn delete(&mut self) {
        if self.selection.is_empty() {
            self.selection.end = self.next_boundary(self.selection.end);
        }
        self.replace(None, "");
    }

    pub fn move_home(&mut self) {
        self.selection = 0..0;
    }

    pub fn move_end(&mut self) {
        let at = self.content.len();
        self.selection = at..at;
    }

    pub fn move_left(&mut self) {
        let at = if self.selection.is_empty() {
            self.previous_boundary(self.selection.start)
        } else {
            self.selection.start
        };
        self.selection = at..at;
    }

    pub fn move_right(&mut self) {
        let at = if self.selection.is_empty() {
            self.next_boundary(self.selection.end)
        } else {
            self.selection.end
        };
        self.selection = at..at;
    }

    /// Byte offset for a UTF-16 offset, as the platform input handler speaks.
    pub fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for ch in self.content.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    /// UTF-16 offset for a byte offset.
    pub fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for ch in self.content.chars() {
            if utf8 >= offset {
                break;
            }
            utf8 += ch.len_utf8();
            utf16 += ch.len_utf16();
        }
        utf16
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content[offset..]
            .chars()
            .next()
            .map(|c| offset + c.len_utf8())
            .unwrap_or(offset)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content[..offset]
            .chars()
            .next_back()
            .map(|c| offset - c.len_utf8())
            .unwrap_or(0)
    }

    /// The one edit primitive: replace `range`, defaulting to the text an IME
    /// has marked, else the selection. Committing clears the mark.
    pub fn replace(&mut self, range: Option<Range<usize>>, text: &str) {
        let range = self.target(range);
        self.content.replace_range(range.clone(), text);
        let at = range.start + text.len();
        self.selection = at..at;
        self.marked = None;
    }

    /// The same edit, but the new text stays marked as in-composition.
    pub fn replace_and_mark(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
    ) {
        let range = self.target(range);
        self.content.replace_range(range.clone(), text);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        // Both ends of an IME selection are relative to where the text landed.
        self.selection = selection
            .map(|s| range.start + s.start..range.start + s.end)
            .unwrap_or_else(|| {
                let at = range.start + text.len();
                at..at
            });
    }

    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub fn unmark(&mut self) {
        self.marked = None;
    }

    fn target(&self, range: Option<Range<usize>>) -> Range<usize> {
        range
            .or_else(|| self.marked.clone())
            .unwrap_or(self.selection.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_inserts_at_the_cursor() {
        let mut line = Line::default();
        line.replace(None, "ferrite");
        assert_eq!(line.text(), "ferrite");
        assert_eq!(line.cursor(), 7);
    }

    #[test]
    fn backspace_removes_the_character_before_the_cursor() {
        let mut line = Line::default();
        line.replace(None, "cat");
        line.backspace();
        assert_eq!(line.text(), "ca");
        assert_eq!(line.cursor(), 2);

        line.backspace();
        line.backspace();
        line.backspace(); // start of line: nothing left to remove
        assert_eq!(line.text(), "");
        assert_eq!(line.cursor(), 0);
    }

    #[test]
    fn the_cursor_steps_over_whole_characters_not_bytes() {
        let mut line = Line::default();
        line.replace(None, "héllo 🌒");
        assert_eq!(line.cursor(), 11); // 'é' is 2 bytes, '🌒' is 4

        line.backspace();
        assert_eq!(line.text(), "héllo ");

        for _ in 0..5 {
            line.move_left();
        }
        assert_eq!(line.cursor(), 1); // ' ', 'o', 'l', 'l', then over the 2-byte 'é'

        line.move_right();
        assert_eq!(line.cursor(), 3);
    }

    #[test]
    fn take_hands_over_the_line_and_clears_it() {
        let mut line = Line::default();
        line.replace(None, "ship it");

        assert_eq!(line.take(), "ship it");
        assert_eq!(line.text(), "");
        assert_eq!(line.cursor(), 0);
        assert_eq!(line.take(), ""); // nothing left to send
    }

    #[test]
    fn a_line_can_be_handed_back_with_the_cursor_at_the_end() {
        let mut line = Line::default();
        line.replace(None, "half typed");

        line.set("a prompt taken back off the queue".into());

        assert_eq!(line.text(), "a prompt taken back off the queue");
        assert_eq!(line.cursor(), line.text().len());
        assert_eq!(line.marked(), None);
    }

    #[test]
    fn home_and_end_jump_to_the_ends_of_the_line() {
        let mut line = Line::default();
        line.replace(None, "prompt");

        line.move_home();
        assert_eq!(line.cursor(), 0);
        line.replace(None, ">"); // typing lands where the cursor is
        assert_eq!(line.text(), ">prompt");

        line.move_end();
        assert_eq!(line.cursor(), 7);
    }

    #[test]
    fn delete_removes_the_character_after_the_cursor() {
        let mut line = Line::default();
        line.replace(None, "cat");
        line.move_home();

        line.delete();
        assert_eq!(line.text(), "at");
        assert_eq!(line.cursor(), 0);

        line.move_end();
        line.delete(); // end of line: nothing to remove
        assert_eq!(line.text(), "at");
    }

    // U+1F312 is 4 bytes of UTF-8 and a 2-unit surrogate pair in UTF-16, which
    // is the arithmetic the macOS input handler hands us ranges in.
    #[test]
    fn utf16_offsets_round_trip_through_astral_characters() {
        let mut line = Line::default();
        line.replace(None, "a🌒b");

        assert_eq!(line.offset_to_utf16(6), 4);
        assert_eq!(line.offset_from_utf16(4), 6);
        assert_eq!(line.offset_to_utf16(1), 1);
        assert_eq!(line.offset_from_utf16(3), 5); // just past the surrogate pair
    }

    #[test]
    fn an_ime_composition_replaces_its_marked_range_until_it_is_committed() {
        let mut line = Line::default();

        line.replace_and_mark(None, "に", None);
        assert_eq!(line.text(), "に");
        assert_eq!(line.marked(), Some(0..3));

        // The next keystroke of the same composition replaces the marked text.
        line.replace_and_mark(None, "にほ", None);
        assert_eq!(line.text(), "にほ");
        assert_eq!(line.marked(), Some(0..6));

        // Committing clears the mark and leaves the cursor after the text.
        line.replace(None, "日本");
        assert_eq!(line.text(), "日本");
        assert_eq!(line.marked(), None);
        assert_eq!(line.cursor(), 6);
    }
}

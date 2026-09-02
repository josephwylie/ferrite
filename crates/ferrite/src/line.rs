//! The Composer's editing state: one text line, no window attached.
//!
//! The selection is a byte range plus which end the caret sits on. Every
//! motion has a plain form that collapses the selection and a `select_`
//! form that extends it from the caret — the shift-arrow grammar of every
//! native text field. Word motions read the line the way macOS does:
//! whitespace, then one run of word characters or one run of punctuation.

use std::ops::Range;

pub struct Line {
    content: String,
    /// The selected bytes, always `start <= end`; empty means a bare caret.
    selection: Range<usize>,
    /// The caret sits at `selection.start` when true, else at `end`.
    reversed: bool,
    marked: Option<Range<usize>>,
}

impl Default for Line {
    fn default() -> Self {
        Self {
            content: String::new(),
            selection: 0..0,
            reversed: false,
            marked: None,
        }
    }
}

/// What a word motion steps over.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Kind {
    Space,
    Word,
    Punctuation,
}

fn kind(c: char) -> Kind {
    if c.is_whitespace() {
        Kind::Space
    } else if c.is_alphanumeric() || c == '_' {
        Kind::Word
    } else {
        Kind::Punctuation
    }
}

impl Line {
    pub fn text(&self) -> &str {
        &self.content
    }

    /// The caret: the moving end of the selection.
    pub fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    /// The fixed end of the selection — the caret itself when nothing is
    /// selected.
    fn anchor(&self) -> usize {
        if self.reversed {
            self.selection.end
        } else {
            self.selection.start
        }
    }

    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    /// The selected text, if any.
    pub fn selected_text(&self) -> Option<&str> {
        (!self.selection.is_empty()).then(|| &self.content[self.selection.clone()])
    }

    /// Put a line back, ready to edit at its end — a prompt recalled from the
    /// queue, or history.
    pub fn set(&mut self, text: String) {
        let at = text.len();
        self.content = text;
        self.place(at);
        self.marked = None;
    }

    /// Hand the line over and clear it — what Enter does.
    pub fn take(&mut self) -> String {
        self.place(0);
        std::mem::take(&mut self.content)
    }

    /// Remove the selection, or the character before the cursor.
    pub fn backspace(&mut self) {
        if self.selection.is_empty() {
            let from = self.previous_boundary(self.selection.start);
            self.selection = from..self.selection.end;
        }
        self.replace(None, "");
    }

    /// Remove the selection, or the character after the cursor.
    pub fn delete(&mut self) {
        if self.selection.is_empty() {
            let to = self.next_boundary(self.selection.end);
            self.selection = self.selection.start..to;
        }
        self.replace(None, "");
    }

    /// Remove the selection, or the word before the cursor (alt-backspace).
    pub fn delete_word_left(&mut self) {
        if self.selection.is_empty() {
            let from = self.word_start(self.selection.start);
            self.selection = from..self.selection.end;
        }
        self.replace(None, "");
    }

    /// Remove the selection, or the word after the cursor (alt-delete).
    pub fn delete_word_right(&mut self) {
        if self.selection.is_empty() {
            let to = self.word_end(self.selection.end);
            self.selection = self.selection.start..to;
        }
        self.replace(None, "");
    }

    /// Remove the selection, or everything before the cursor (cmd-backspace).
    pub fn delete_to_start(&mut self) {
        if self.selection.is_empty() {
            self.selection = 0..self.selection.end;
        }
        self.replace(None, "");
    }

    /// Remove the selection, or everything after the cursor (cmd-delete).
    pub fn delete_to_end(&mut self) {
        if self.selection.is_empty() {
            self.selection = self.selection.start..self.content.len();
        }
        self.replace(None, "");
    }

    pub fn move_home(&mut self) {
        self.place(0);
    }

    pub fn move_end(&mut self) {
        self.place(self.content.len());
    }

    pub fn move_left(&mut self) {
        let at = if self.selection.is_empty() {
            self.previous_boundary(self.selection.start)
        } else {
            self.selection.start
        };
        self.place(at);
    }

    pub fn move_right(&mut self) {
        let at = if self.selection.is_empty() {
            self.next_boundary(self.selection.end)
        } else {
            self.selection.end
        };
        self.place(at);
    }

    pub fn move_word_left(&mut self) {
        let at = self.word_start(self.cursor());
        self.place(at);
    }

    pub fn move_word_right(&mut self) {
        let at = self.word_end(self.cursor());
        self.place(at);
    }

    pub fn select_left(&mut self) {
        let at = self.previous_boundary(self.cursor());
        self.extend(at);
    }

    pub fn select_right(&mut self) {
        let at = self.next_boundary(self.cursor());
        self.extend(at);
    }

    pub fn select_word_left(&mut self) {
        let at = self.word_start(self.cursor());
        self.extend(at);
    }

    pub fn select_word_right(&mut self) {
        let at = self.word_end(self.cursor());
        self.extend(at);
    }

    pub fn select_home(&mut self) {
        self.extend(0);
    }

    pub fn select_end(&mut self) {
        self.extend(self.content.len());
    }

    pub fn select_all(&mut self) {
        self.selection = 0..self.content.len();
        self.reversed = false;
    }

    /// Select the word under `offset` — a double-click.
    pub fn select_word_at(&mut self, offset: usize) {
        let offset = offset.min(self.content.len());
        let at = self.char_boundary_at_or_before(offset);
        let Some(c) = self.content[at..].chars().next() else {
            // Past the end: the last word, if the line ends in one.
            let start = self.word_start(at);
            self.selection = start..at;
            self.reversed = false;
            return;
        };
        let here = kind(c);
        let start = self.run_start(at, here);
        let end = self.run_end(at, here);
        self.selection = start..end;
        self.reversed = false;
    }

    /// Whether the caret sits at the selection's start — what the platform
    /// input handler asks when it reports the selection back.
    pub fn reversed(&self) -> bool {
        self.reversed && !self.selection.is_empty()
    }

    /// A click: the caret lands at `offset`, snapped onto a character.
    pub fn place_caret(&mut self, offset: usize) {
        let at = self.char_boundary_at_or_before(offset);
        self.place(at);
    }

    /// A shift-click or a drag: the selection grows from its anchor to
    /// `offset`, snapped onto a character.
    pub fn select_to(&mut self, offset: usize) {
        let at = self.char_boundary_at_or_before(offset);
        self.extend(at);
    }

    /// Collapse the selection to a caret at `at`.
    fn place(&mut self, at: usize) {
        let at = at.min(self.content.len());
        self.selection = at..at;
        self.reversed = false;
    }

    /// Move the caret to `at`, keeping the anchor where it is.
    fn extend(&mut self, at: usize) {
        let at = at.min(self.content.len());
        let anchor = self.anchor();
        if at < anchor {
            self.selection = at..anchor;
            self.reversed = true;
        } else {
            self.selection = anchor..at;
            self.reversed = false;
        }
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

    fn char_boundary_at_or_before(&self, offset: usize) -> usize {
        let mut at = offset.min(self.content.len());
        while !self.content.is_char_boundary(at) {
            at -= 1;
        }
        at
    }

    /// The start of the word before `offset`: back over whitespace, then
    /// back over one run of the kind found there.
    fn word_start(&self, offset: usize) -> usize {
        let mut at = offset;
        while let Some(c) = self.content[..at].chars().next_back() {
            if kind(c) != Kind::Space {
                break;
            }
            at -= c.len_utf8();
        }
        let Some(c) = self.content[..at].chars().next_back() else {
            return at;
        };
        self.run_start(at - c.len_utf8(), kind(c))
    }

    /// The end of the word after `offset`: over whitespace, then over one
    /// run of the kind found there.
    fn word_end(&self, offset: usize) -> usize {
        let mut at = offset;
        while let Some(c) = self.content[at..].chars().next() {
            if kind(c) != Kind::Space {
                break;
            }
            at += c.len_utf8();
        }
        let Some(c) = self.content[at..].chars().next() else {
            return at;
        };
        self.run_end(at, kind(c))
    }

    /// Where the run of `of` containing the character at `at` begins.
    fn run_start(&self, at: usize, of: Kind) -> usize {
        let mut start = at;
        while let Some(c) = self.content[..start].chars().next_back() {
            if kind(c) != of {
                break;
            }
            start -= c.len_utf8();
        }
        start
    }

    /// Where the run of `of` containing the character at `at` ends.
    fn run_end(&self, at: usize, of: Kind) -> usize {
        let mut end = at;
        while let Some(c) = self.content[end..].chars().next() {
            if kind(c) != of {
                break;
            }
            end += c.len_utf8();
        }
        end
    }

    /// The one edit primitive: replace `range`, defaulting to the text an IME
    /// has marked, else the selection. Committing clears the mark.
    pub fn replace(&mut self, range: Option<Range<usize>>, text: &str) {
        let range = self.target(range);
        self.content.replace_range(range.clone(), text);
        self.place(range.start + text.len());
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
        self.reversed = false;
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

    fn of(text: &str) -> Line {
        let mut line = Line::default();
        line.replace(None, text);
        line
    }

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

    /// alt-backspace: whitespace, then one word — and punctuation is its
    /// own run, so `foo.bar` loses `bar`, then `.`, then `foo`.
    #[test]
    fn delete_word_left_removes_one_word_at_a_time() {
        let mut line = of("fix the   tests  ");
        line.delete_word_left();
        assert_eq!(line.text(), "fix the   ");
        line.delete_word_left();
        assert_eq!(line.text(), "fix ");
        line.delete_word_left();
        assert_eq!(line.text(), "");
        line.delete_word_left(); // nothing left: no panic, no change
        assert_eq!(line.text(), "");

        let mut line = of("read foo.bar");
        line.delete_word_left();
        assert_eq!(line.text(), "read foo.");
        line.delete_word_left();
        assert_eq!(line.text(), "read foo");

        let mut line = of("héllo wörld");
        line.delete_word_left();
        assert_eq!(line.text(), "héllo ");
    }

    #[test]
    fn delete_word_right_mirrors_it() {
        let mut line = of("  fix the tests");
        line.move_home();
        line.delete_word_right();
        assert_eq!(line.text(), " the tests");
        assert_eq!(line.cursor(), 0);
        line.move_end();
        line.delete_word_right(); // end of line: nothing
        assert_eq!(line.text(), " the tests");
    }

    #[test]
    fn delete_to_start_and_end_clear_either_side_of_the_caret() {
        let mut line = of("keep this, drop that");
        for _ in 0..10 {
            line.move_left();
        }
        line.delete_to_end();
        assert_eq!(line.text(), "keep this,");
        line.move_left();
        line.delete_to_start();
        assert_eq!(line.text(), ",");
        assert_eq!(line.cursor(), 0);
    }

    #[test]
    fn word_motions_step_the_caret_by_words() {
        let mut line = of("one two  three");
        line.move_word_left();
        assert_eq!(line.cursor(), 9);
        line.move_word_left();
        assert_eq!(line.cursor(), 4);
        line.move_word_left();
        assert_eq!(line.cursor(), 0);
        line.move_word_left();
        assert_eq!(line.cursor(), 0);
        line.move_word_right();
        assert_eq!(line.cursor(), 3);
        line.move_word_right();
        assert_eq!(line.cursor(), 7);
        line.move_word_right();
        assert_eq!(line.cursor(), 14);
    }

    /// Shift-arrows grow a selection from the caret; the anchor stays put
    /// whichever way the caret then goes, and a plain arrow collapses it.
    #[test]
    fn shift_motions_extend_a_selection_from_the_anchor() {
        let mut line = of("abc def");
        line.select_left();
        line.select_left();
        assert_eq!(line.selection(), 5..7);
        assert_eq!(line.cursor(), 5);
        assert_eq!(line.selected_text(), Some("ef"));
        line.select_word_left();
        assert_eq!(line.selection(), 4..7);
        line.select_home();
        assert_eq!(line.selection(), 0..7);
        // Back the other way past the anchor: the selection flips.
        line.select_end();
        assert_eq!(line.selection(), 7..7);
        line.move_home();
        line.select_right();
        line.select_word_right();
        assert_eq!(line.selection(), 0..3);
        assert_eq!(line.cursor(), 3);
        line.move_left(); // collapses to the selection's start
        assert_eq!(line.selection(), 0..0);
        // Typing over a selection replaces it.
        line.select_all();
        assert_eq!(line.selected_text(), Some("abc def"));
        line.replace(None, "x");
        assert_eq!(line.text(), "x");
        assert_eq!(line.cursor(), 1);
    }

    #[test]
    fn a_word_is_selected_under_a_double_click() {
        let mut line = of("open crates/ferrite now");
        line.select_word_at(7);
        assert_eq!(line.selected_text(), Some("crates"));
        line.select_word_at(11); // on the '/'
        assert_eq!(line.selected_text(), Some("/"));
        line.select_word_at(4); // the space
        assert_eq!(line.selected_text(), Some(" "));
        line.select_word_at(23); // past the end
        assert_eq!(line.selected_text(), Some("now"));
        line.backspace();
        assert_eq!(line.text(), "open crates/ferrite ");
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

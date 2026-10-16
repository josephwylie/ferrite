//! A title's one length limit, and how a longer one is shortened without losing what it said. Every title the board
//! saves (typed, renamed, or given by an agent) goes through here.
use crate::text::{clean, head, width};

/// 60 cells of the screen (60 letters; a wide character, as in Chinese or an emoji, takes two): what a row of the list
/// has for a title in a 120-column window with the task panel open, and what herdr's sidebar is given of it.
pub const TITLE_MAX: usize = 60;
pub const TITLE_RULE: &str = "at most 60 characters";

/// A title of at most `max` cells. One that fits is kept as it is. A longer one loses the full stop that ends it when
/// that is all it takes; otherwise it is cut at the end of a word and ends in "…".
pub fn short_title(text: &str, max: usize) -> String {
    let full = clean(text);
    if width(&full) <= max { return full; }
    let bare = full.trim_end_matches(['.', '!']);
    if width(bare) <= max { return bare.to_string(); }
    let h = head(&full, max.saturating_sub(1));
    let space = h.rfind(char::is_whitespace);
    let next_is_space = full[h.len()..].starts_with(char::is_whitespace);
    // A word boundary, unless the cut would then throw away over half the room (one very long word).
    let kept = match space {
        Some(sp) if !next_is_space && width(&h[..sp]) * 2 >= max => &h[..sp],
        _ => h,
    };
    format!("{}…", kept.trim_end_matches(|c: char| c.is_whitespace() || ",;:.!?([{\"'`–—-".contains(c)))
}
fn flat(s: &str) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase() }

pub struct Titled { pub title: String, pub full: String, pub cut: bool, pub description: String }
/// A title and a description as they are saved. When the title had to be cut, its full text is the description's
/// first paragraph (unless the description says it already): nothing that was written is lost.
pub fn titled(title: &str, description: &str) -> Titled {
    let full = clean(title);
    let short = short_title(&full, TITLE_MAX);
    let text = description.trim().to_string();
    let cut = short.ends_with('…') && short != full;
    let description = if cut && !flat(&text).contains(&flat(&full)) {
        [full.clone(), text].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n\n")
    } else { text };
    Titled { title: short, full, cut, description }
}
/// What whoever gave a too long title is told, so that they can give a better one. "" when it fitted.
pub fn title_note(given: &str) -> String {
    let t = titled(given, "");
    if t.cut { format!(" The title was over {TITLE_MAX} characters, so it was shortened to \"{}\" and the full text was kept in the description. Rename it if a better short title exists.", t.title) } else { String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortens_at_a_word() {
        let long = "Rewrite the board in Rust on Ratatui and crossterm as one small native program for everyone";
        let s = short_title(long, TITLE_MAX);
        assert!(width(&s) <= TITLE_MAX && s.ends_with('…'), "{s}");
        assert_eq!(s, "Rewrite the board in Rust on Ratatui and crossterm as one…");
        assert_eq!(short_title("Fits.", 4), "Fits");
        let t = titled(long, "more");
        assert!(t.cut && t.description.starts_with(long));
    }
}

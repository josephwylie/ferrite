//! Text as the screen measures it: how many cells a text takes, cutting it to a width with `…`, and wrapping it.
//! One measure for everything (titles, rows, the chat, the web page's columns): each written character (a letter and
//! its marks, an emoji made of several, a flag) takes the cells the terminal gives it.
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The cells one written character takes.
pub fn grapheme_width(g: &str) -> usize {
    if g.is_ascii() { return usize::from(!g.is_empty()); }
    let w = g.width();
    // An emoji shown as one (followed by U+FE0F, or made of several joined) takes two, whatever its parts say.
    if g.contains('\u{fe0f}') || g.contains('\u{200d}') { return w.clamp(2, 2); }
    w.min(2)
}
pub fn width(s: &str) -> usize {
    if s.bytes().all(|b| (0x20..0x7f).contains(&b)) { return s.len(); }
    s.graphemes(true).map(grapheme_width).sum()
}
/// The start of a text that takes at most `max` cells, never cut inside one written character.
pub fn head(s: &str, max: usize) -> &str {
    let mut w = 0;
    for (i, g) in s.grapheme_indices(true) {
        let gw = grapheme_width(g);
        if w + gw > max { return &s[..i]; }
        w += gw;
    }
    s
}
/// A text of at most `max` cells: cut with `…` when it is longer.
pub fn cut(s: &str, max: usize) -> String {
    if width(s) <= max { return s.to_string(); }
    if max == 0 { return String::new(); }
    format!("{}…", head(s, max - 1))
}
/// The end of a text that takes at most `max` cells.
pub fn tail(s: &str, max: usize) -> &str {
    let mut w = 0;
    let gs: Vec<(usize, &str)> = s.grapheme_indices(true).collect();
    for (i, g) in gs.iter().rev() {
        let gw = grapheme_width(g);
        if w + gw > max { return &s[i + g.len()..]; }
        w += gw;
    }
    s
}
/// Padded with spaces on the right to `n` cells (never cut).
pub fn pad(s: &str, n: usize) -> String { let w = width(s); if w >= n { s.to_string() } else { format!("{s}{}", " ".repeat(n - w)) } }
/// Padded with spaces on the left to `n` cells.
pub fn pad_left(s: &str, n: usize) -> String { let w = width(s); if w >= n { s.to_string() } else { format!("{}{s}", " ".repeat(n - w)) } }

/// Words wrapped to lines of at most `max` cells. A word longer than a line is broken where it must be. Line breaks
/// in the text are kept; a blank line stays blank.
pub fn wrap(s: &str, max: usize) -> Vec<String> {
    let max = max.max(1);
    let mut out = vec![];
    for para in s.split('\n') {
        let mut line = String::new();
        let mut lw = 0;
        for word in para.split(' ') {
            let ww = width(word);
            if lw > 0 && lw + 1 + ww <= max { line.push(' '); line.push_str(word); lw += 1 + ww; continue; }
            if lw > 0 { out.push(std::mem::take(&mut line)); }
            let mut rest = word;
            while width(rest) > max { let h = head(rest, max); let h = if h.is_empty() { &rest[..rest.graphemes(true).next().map(|g| g.len()).unwrap_or(rest.len())] } else { h }; out.push(h.to_string()); rest = &rest[h.len()..]; }
            line.push_str(rest);
            lw = width(rest);
        }
        out.push(line);
    }
    out
}
/// Control characters out, runs of them a space, the ends trimmed: a text as one line.
pub fn clean(s: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in s.chars() {
        if (c as u32) < 0x20 || (0x7f..=0x9f).contains(&(c as u32)) { if !gap { out.push(' '); gap = true; } } else { out.push(c); gap = false; }
    }
    out.trim().to_string()
}
/// Runs of white space as one space, trimmed.
pub fn squash(s: &str) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ") }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn widths() {
        assert_eq!(width("abc"), 3);
        assert_eq!(width("日本"), 4);
        assert_eq!(width("👩‍👩‍👧"), 2);
        assert_eq!(width("🇸🇪"), 2);
        assert_eq!(cut("abcdef", 4), "abc…");
        assert_eq!(cut("日本語", 4), "日…");
        assert_eq!(wrap("one two three", 7), vec!["one two", "three"]);
    }
}

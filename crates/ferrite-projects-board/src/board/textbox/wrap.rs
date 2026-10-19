//! How the text box wraps a line of what you type: at the last space before a word (several spaces join; the break is
//! after the last one), or between two characters of which one is CJK; a word longer than the line is broken where it
//! must be. Each chunk keeps its place in the line, so the cursor can be put in it. The rule pi-tui's editor had.
use crate::text::grapheme_width;
use unicode_segmentation::UnicodeSegmentation;

/// One visual line of a logical line: its text and where it starts and ends (byte offsets).
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk { pub start: usize, pub end: usize }
fn is_ws(g: &str) -> bool { g.chars().all(char::is_whitespace) }
fn is_cjk(g: &str) -> bool {
    g.chars().next().map(|c| matches!(c as u32, 0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)).unwrap_or(false)
}
pub fn word_wrap_line(line: &str, max: usize) -> Vec<Chunk> {
    if line.is_empty() || max == 0 { return vec![Chunk { start: 0, end: 0 }]; }
    if crate::text::width(line) <= max { return vec![Chunk { start: 0, end: line.len() }]; }
    let segs: Vec<(usize, &str)> = line.grapheme_indices(true).collect();
    let mut chunks = vec![];
    let (mut cw, mut start) = (0usize, 0usize);
    let (mut opp, mut opp_w): (Option<usize>, usize) = (None, 0);
    for (i, &(at, g)) in segs.iter().enumerate() {
        let gw = grapheme_width(g);
        let ws = is_ws(g);
        if cw + gw > max {
            match opp {
                Some(o) if cw - opp_w + gw <= max => { chunks.push(Chunk { start, end: o }); start = o; cw -= opp_w; }
                _ if start < at => { chunks.push(Chunk { start, end: at }); start = at; cw = 0; }
                _ => {}
            }
            opp = None;
        }
        cw += gw;
        if let Some(&(next_at, next)) = segs.get(i + 1) {
            if ws && !is_ws(next) { opp = Some(next_at); opp_w = cw; }
            else if !ws && !is_ws(next) && (is_cjk(g) || is_cjk(next)) { opp = Some(next_at); opp_w = cw; }
        }
    }
    chunks.push(Chunk { start, end: line.len() });
    chunks
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wraps_at_words() {
        let l = "hello big world";
        let c: Vec<&str> = word_wrap_line(l, 10).iter().map(|c| &l[c.start..c.end]).collect();
        assert_eq!(c, vec!["hello big ", "world"]);
        let l = "abcdefghijkl";
        let c: Vec<&str> = word_wrap_line(l, 5).iter().map(|c| &l[c.start..c.end]).collect();
        assert_eq!(c, vec!["abcde", "fghij", "kl"]);
    }
}

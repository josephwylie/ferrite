//! Words in inks wrapped to a width, as every text of the board wraps: a line breaks at spaces; a word longer than a
//! line is broken where it must be; spaces at a line's end go; a line break in the words is a line break. The same
//! rule pi-tui's wrapTextWithAnsi had, so a text wraps where it always did.
use super::ink::{Inked, Piece};
use crate::text::grapheme_width;
use unicode_segmentation::UnicodeSegmentation;

/// One written character with the piece it came from.
struct G<'a> { g: &'a str, w: usize, p: usize }
fn rebuild(gs: &[G], pieces: &[Piece]) -> Inked {
    let mut out = Inked::new();
    for g in gs {
        match out.0.last_mut() { Some(last) if pieces[g.p].t.is_empty() == false && same(last, &pieces[g.p]) => last.t.push_str(g.g), _ => out.0.push(Piece { t: g.g.to_string(), ..pieces[g.p].clone() }) }
    }
    out
}
fn same(a: &Piece, b: &Piece) -> bool { a.role == b.role && a.c == b.c && a.bold == b.bold && a.dim == b.dim && a.quiet == b.quiet && a.under == b.under && a.inv == b.inv && a.bg == b.bg && a.link == b.link }
fn trimmed<'a, 'b>(line: &'b [G<'a>]) -> &'b [G<'a>] { let mut n = line.len(); while n > 0 && line[n - 1].g.trim().is_empty() { n -= 1; } &line[..n] }

pub fn wrap(text: &Inked, max: usize) -> Vec<Inked> {
    let max = max.max(1);
    let mut out = vec![];
    for line in text.lines() {
        let gs: Vec<G> = line.0.iter().enumerate().flat_map(|(p, piece)| piece.t.graphemes(true).map(move |g| G { g, w: grapheme_width(g), p })).collect();
        let total: usize = gs.iter().map(|g| g.w).sum();
        if total <= max { out.push(line.clone()); continue; }
        // Tokens: runs of spaces and runs of anything else.
        let mut tokens: Vec<(usize, usize)> = vec![]; // [start, end) into gs
        let mut at = 0;
        while at < gs.len() {
            let ws = gs[at].g.trim().is_empty();
            let mut end = at + 1;
            while end < gs.len() && gs[end].g.trim().is_empty() == ws { end += 1; }
            tokens.push((at, end));
            at = end;
        }
        let mut lines: Vec<Vec<usize>> = vec![]; // indices into gs
        let mut cur: Vec<usize> = vec![];
        let mut cw = 0;
        for (s, e) in tokens {
            let tw: usize = gs[s..e].iter().map(|g| g.w).sum();
            let ws = gs[s].g.trim().is_empty();
            if tw > max && !ws {
                if !cur.is_empty() { lines.push(std::mem::take(&mut cur)); cw = 0; }
                for i in s..e {
                    if cw + gs[i].w > max && !cur.is_empty() { lines.push(std::mem::take(&mut cur)); cw = 0; }
                    cur.push(i); cw += gs[i].w;
                }
                continue;
            }
            if cw + tw > max && cw > 0 {
                lines.push(std::mem::take(&mut cur));
                cw = 0;
                if ws { continue; } // a line never starts with the spaces it broke at
            }
            cur.extend(s..e); cw += tw;
        }
        if !cur.is_empty() { lines.push(cur); }
        for l in lines {
            let picked: Vec<G> = l.iter().map(|&i| G { g: gs[i].g, w: gs[i].w, p: gs[i].p }).collect();
            out.push(rebuild(trimmed(&picked), &line.0));
        }
    }
    if out.is_empty() { out.push(Inked::new()); }
    out
}

#[cfg(test)]
mod tests {
    use super::super::ink::{Ink, Inked};
    #[test]
    fn wraps_like_the_board() {
        let t = Inked::of("one two ", Ink::Red).add(Inked::of("three four", Ink::Green));
        let w: Vec<String> = t.wrap(9).iter().map(|l| l.text()).collect();
        assert_eq!(w, vec!["one two", "three", "four"]);
        assert_eq!(t.wrap(9)[1].0[0].c, Ink::Green);
        let w: Vec<String> = Inked::plain("abcdefghij k").wrap(4).iter().map(|l| l.text()).collect();
        assert_eq!(w, vec!["abcd", "efgh", "ij k"]);
        assert_eq!(Inked::plain("a\n\nb").wrap(5).len(), 3);
        assert_eq!(Inked::plain("日本語の文").wrap(4).iter().map(|l| l.text()).collect::<Vec<_>>(), vec!["日本", "語の", "文"]);
    }
}

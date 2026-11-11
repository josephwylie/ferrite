//! A Project's badge (T3 Code's `ProjectMonogram`): a 16px tile holding a
//! two-letter monogram of the Project's name, in one of 18 hues hashed
//! from that name. Ferrite has no Project favicons, so the monogram is the
//! whole badge. The nav card leads with it; the header breadcrumb reuses
//! it, so the same Project always reads as the same tile.

use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, Div, FontWeight};

use crate::theme::*;

/// A Project name's monogram and hue, the badge's two facts: up to two
/// uppercase glyphs, and an index into `PROJECT_HUES`.
pub fn project_monogram(name: &str) -> (String, usize) {
    (monogram(name), hue(name))
}

/// T3's `projectMonogram`: the first glyph of the first word, then the
/// first digit after it in that word, else the first glyph of the last
/// word, else the first word's last glyph. A word is a run of letters and
/// digits; a name with none reads `PR`.
fn monogram(name: &str) -> String {
    let words: Vec<Vec<char>> = name
        .trim()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.chars().collect())
        .collect();
    let (Some(first), Some(last)) = (words.first(), words.last()) else {
        return "PR".to_string();
    };
    let second = first[1..]
        .iter()
        .find(|ch| ch.is_numeric())
        .or_else(|| (words.len() > 1).then(|| &last[0]))
        .or_else(|| (first.len() > 1).then(|| &first[first.len() - 1]));
    std::iter::once(&first[0])
        .chain(second)
        .flat_map(|ch| ch.to_uppercase())
        .take(2)
        .collect()
}

/// T3's `projectColor`: over the trimmed, lowercased name (`project` when
/// empty), `index = (index * 31 + codepoint) % 18`.
fn hue(name: &str) -> usize {
    let seed = name.trim().to_lowercase();
    let seed = if seed.is_empty() { "project" } else { &seed };
    seed.chars().fold(0, |index, ch| {
        (index * 31 + ch as usize) % PROJECT_HUES.len()
    })
}

/// The badge: a `NAV_BADGE` square at a quarter radius, its ground the
/// hue at 14% and its monogram in the hue, mono 700 at
/// `FS_PROJECT_BADGE`. T3 stretches the monogram to exactly 12px (6px for
/// one glyph); gpui has no `textLength`, so each glyph takes a fixed
/// `PROJECT_BADGE_GLYPH_W` cell, which spaces any two letters alike.
pub fn project_badge(name: &str) -> Div {
    let (letters, hue) = project_monogram(name);
    let ink = PROJECT_HUES[hue];
    div()
        .debug_selector(|| "project-badge".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(NAV_BADGE))
        .rounded(px(NAV_BADGE / 4.0))
        .bg(rgba((ink << 8) | PROJECT_BADGE_GROUND_ALPHA))
        .font_family(FONT_CODE)
        .text_size(px(FS_PROJECT_BADGE))
        .line_height(px(NAV_BADGE))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(ink))
        .children(letters.chars().map(|letter| {
            div()
                .flex()
                .justify_center()
                .w(px(PROJECT_BADGE_GLYPH_W))
                .child(letter.to_string())
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T3's own examples, and the empty name's stand-in.
    #[test]
    fn a_monogram_takes_a_digit_then_the_last_word_then_the_last_letter() {
        assert_eq!(monogram("relay"), "RY");
        assert_eq!(monogram("react-explorer"), "RE");
        assert_eq!(monogram("web2"), "W2");
        assert_eq!(monogram("ferrite"), "FE");
        assert_eq!(monogram("  my big app "), "MA");
        assert_eq!(monogram("x"), "X");
        assert_eq!(monogram(""), "PR");
        assert_eq!(monogram("--"), "PR");
    }

    /// The hue is the `(i*31+cp)%18` hash of the lowercased name, so one
    /// Project always wears one hue (values worked by hand from the rule).
    #[test]
    fn a_hue_is_a_stable_hash_of_the_name() {
        assert_eq!(PROJECT_HUES.len(), 18);
        assert_eq!(hue("ferrite"), 15);
        assert_eq!(hue("relay"), 13);
        assert_eq!(hue("web2"), 8);
        assert_eq!(hue("Ferrite "), hue("ferrite"), "trimmed and lowercased");
        assert_eq!(hue(""), hue("project"));
        assert_eq!(project_monogram("relay"), ("RY".to_string(), 13));
    }

    /// The tile: 16px, radius 4, the hue at 14% under the hue's letters.
    #[test]
    fn the_badge_is_a_tinted_sixteen_pixel_tile() {
        let mut badge = project_badge("relay");
        let style = badge.style();
        assert_eq!(style.size.width, Some(px(16.0).into()));
        assert_eq!(style.corner_radii.top_left, Some(px(4.0).into()));
        let indigo = PROJECT_HUES[13];
        assert_eq!(
            style.background,
            Some(gpui::Hsla::from(rgba((indigo << 8) | 0x24)).into()),
            "14% of the hue"
        );
        assert_eq!(FS_PROJECT_BADGE, 8.25);
        assert_eq!(2.0 * PROJECT_BADGE_GLYPH_W, 12.0, "two glyphs fill 12px");
    }
}

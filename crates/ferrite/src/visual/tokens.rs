//! The `--ferrite-*` CSS custom properties a visual is styled with: Ferrite's
//! own theme (`theme.rs`), handed to the page so an agent-authored visual
//! looks like the transcript it sits in.
//!
//! The token **names** (and what each means) are the tool description's
//! contract with the agent, so they live once in core beside the tool
//! definition ([`TOKENS`]). The **values** are the app's: [`value`] maps every
//! name to a `theme.rs` colour or metric, and a test holds the two to exactly
//! the same set, in both schemes.
//!
//! [`theme_css`] is the whole block a page gets:
//!
//! - `:root` carries the host scheme plus the face and the reading size;
//! - `[data-ferrite-scheme=dark]` and `[data-ferrite-scheme=light]` carry each
//!   palette whatever the host's, so a page can pin a subtree to one (a UI
//!   mock previewing the other theme).
//!
//! Ferrite itself is dark only (theme.rs: "Dark only"). The light palette is
//! the same ladder inverted for a white plane, for pinned subtrees and the
//! day Ferrite grows a light theme; nothing else changes then but the scheme
//! passed in here.

use ferrite_core::settings::ReadingSize;

use crate::theme;

/// The token names and meanings: core's, beside the tool description that
/// lists them to the agent.
pub(crate) use ferrite_core::visual::TOKENS;

/// Which palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Scheme {
    Dark,
    Light,
}

impl Scheme {
    /// The app's scheme: Ferrite is dark only (theme.rs).
    pub(crate) const APP: Scheme = Scheme::Dark;

    fn word(self) -> &'static str {
        match self {
            Scheme::Dark => "dark",
            Scheme::Light => "light",
        }
    }
}

/// `#rrggbb` of an opaque theme colour.
fn hex(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0xff_ff_ff)
}

/// A colour token's value in `scheme`, or `None` for a name that isn't a
/// colour (or isn't a token).
fn colour(name: &str, scheme: Scheme) -> Option<String> {
    use Scheme::*;
    let rgb = match (name, scheme) {
        // The opaque ladder (theme rule 3): pages paint the reading plane.
        ("bg", Dark) => theme::PLANE,
        ("band", Dark) => theme::CHROME,
        ("band2", Dark) => theme::BAND,
        ("fg", Dark) => theme::TEXT,
        ("strong", Dark) => theme::TEXT_STRONG,
        ("muted", Dark) => theme::TEXT_MUTED,
        ("faint", Dark) => theme::TEXT_FAINT,
        ("border", Dark) => theme::LINE,
        ("border2", Dark) => theme::LINE2,
        ("accent", Dark) => theme::ACCENT,
        ("on-accent", Dark) => theme::ON_ACCENT,
        ("float", Dark) => theme::FLOAT,
        ("red", Dark) => theme::RED,
        ("green", Dark) => theme::GREEN,
        ("yellow", Dark) => theme::YELLOW,
        ("blue", Dark) => theme::BLUE,
        ("magenta", Dark) => theme::MAGENTA,
        ("cyan", Dark) => theme::CYAN,
        ("orange", Dark) => theme::ORANGE,
        // State words read in the state inks (theme rule 6).
        ("ok", Dark) => theme::RUNNING,
        ("warn", Dark) => theme::ATTENTION,
        ("danger", Dark) => theme::BLOCKED,
        ("info", Dark) => theme::BLUE,
        // Between the plane and its rule: gridlines recede behind marks.
        ("grid", Dark) => 0x272729,
        // A categorical series palette (the dataviz reference hues), checked
        // for lightness band, chroma floor and colour-vision separation of
        // adjacent pairs against `bg` (spikes/inline-cef NOTES-host.md).
        // Ferrite's pastel terminal hues fail those checks, so they stay
        // text and accent colours.
        ("chart-1", Dark) => 0x3987e5,
        ("chart-2", Dark) => 0xd95926,
        ("chart-3", Dark) => 0x199e70,
        ("chart-4", Dark) => 0xc98500,
        ("chart-5", Dark) => 0xd55181,
        ("chart-6", Dark) => 0x9085e9,

        ("bg", Light) => 0xfbfbfa,
        ("band", Light) => 0xf1f1f0,
        ("band2", Light) => 0xe8e8e7,
        ("fg", Light) => 0x2b2b2e,
        ("strong", Light) => 0x111113,
        ("muted", Light) => 0x6c6c72,
        ("faint", Light) => 0xa3a3a8,
        ("border", Light) => 0xe3e3e2,
        ("border2", Light) => 0xd2d2d1,
        ("accent", Light) => 0x2f63c8,
        ("on-accent", Light) => 0xffffff,
        ("float", Light) => 0xffffff,
        ("red", Light) => 0xc2453a,
        ("green", Light) => 0x3b8a36,
        ("yellow", Light) => 0x9a6f0c,
        ("blue", Light) => 0x2f6bcc,
        ("magenta", Light) => 0x8247c4,
        ("cyan", Light) => 0x1d8288,
        ("orange", Light) => 0xbb5f24,
        ("ok", Light) => 0x2f7d2b,
        ("warn", Light) => 0x8f6508,
        ("danger", Light) => 0xc2453a,
        ("info", Light) => 0x2f6bcc,
        ("grid", Light) => 0xededec,
        ("chart-1", Light) => 0x2a78d6,
        ("chart-2", Light) => 0xeb6834,
        ("chart-3", Light) => 0x1baf7a,
        ("chart-4", Light) => 0xeda100,
        ("chart-5", Light) => 0xe87ba4,
        ("chart-6", Light) => 0x4a3aa7,
        _ => return None,
    };
    Some(hex(rgb))
}

/// Every token's value in `scheme` at reading size `reading`, or `None` for
/// a name that isn't a token.
pub(crate) fn value(name: &str, scheme: Scheme, reading: ReadingSize) -> Option<String> {
    if let Some(colour) = colour(name, scheme) {
        return Some(colour);
    }
    Some(match name {
        "shadow" => match scheme {
            Scheme::Dark => "rgba(0,0,0,.5)".into(),
            Scheme::Light => "rgba(0,0,0,.12)".into(),
        },
        "font-mono" => {
            format!("\"{}\",ui-monospace,\"SF Mono\",Menlo,monospace", theme::FONT_UI)
        }
        "font-size" => format!("{}px", theme::answer_text_size(reading)),
        "line" => format!("{}px", theme::answer_line_height(reading)),
        "radius" => "0px".into(),
        "scheme" => scheme.word().into(),
        _ => return None,
    })
}

/// The declarations of every token in `scheme`.
fn declarations(scheme: Scheme, reading: ReadingSize) -> String {
    let mut css = String::new();
    for token in TOKENS {
        let name = token.name;
        if let Some(value) = value(name, scheme, reading) {
            css.push_str(&format!("--ferrite-{name}:{value};"));
        }
    }
    css
}

/// The theme block a page gets (see the module docs). Changing it restyles a
/// live page (`WebView::set_theme`).
pub(crate) fn theme_css(scheme: Scheme, reading: ReadingSize) -> String {
    format!(
        ":root{{{root}color-scheme:{word};}}\
         [data-ferrite-scheme=dark]{{{dark}color-scheme:dark;}}\
         [data-ferrite-scheme=light]{{{light}color-scheme:light;}}",
        root = declarations(scheme, reading),
        word = scheme.word(),
        dark = declarations(Scheme::Dark, reading),
        light = declarations(Scheme::Light, reading),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_token_has_a_value_in_both_schemes() {
        for scheme in [Scheme::Dark, Scheme::Light] {
            for token in TOKENS {
                let name = token.name;
                assert!(
                    value(name, scheme, ReadingSize::default()).is_some(),
                    "--ferrite-{name} has no {scheme:?} value"
                );
            }
        }
        assert_eq!(value("nope", Scheme::Dark, ReadingSize::default()), None);
    }

    #[test]
    fn the_dark_palette_is_the_app_theme() {
        let reading = ReadingSize::default();
        assert_eq!(value("bg", Scheme::Dark, reading).unwrap(), "#1b1b1c");
        assert_eq!(value("accent", Scheme::Dark, reading).unwrap(), hex(theme::ACCENT));
        assert_eq!(value("fg", Scheme::Dark, reading).unwrap(), hex(theme::TEXT));
        assert_eq!(
            value("font-size", Scheme::Dark, reading).unwrap(),
            format!("{}px", theme::answer_text_size(reading))
        );
    }

    #[test]
    fn the_block_declares_every_name_once_per_scope() {
        let css = theme_css(Scheme::Dark, ReadingSize::default());
        for token in TOKENS {
            let name = token.name;
            let decl = format!("--ferrite-{name}:");
            assert_eq!(css.matches(&decl).count(), 3, "{name}: root, dark, light");
        }
        assert!(css.starts_with(":root{--ferrite-bg:#1b1b1c;"));
        assert!(!css.contains("url("), "the theme fetches nothing");
    }

    #[test]
    fn reading_size_restyles_the_text_tokens_only() {
        let small = theme_css(Scheme::Dark, ReadingSize::SMALLEST);
        let large = theme_css(Scheme::Dark, ReadingSize::LARGEST);
        assert!(small.contains("--ferrite-font-size:12px;--ferrite-line:18px;"), "{small}");
        assert!(large.contains("--ferrite-font-size:24px;--ferrite-line:36px;"), "{large}");
        fn strip(css: &str) -> Vec<&str> {
            css.split(';').filter(|d| !d.contains("font-size") && !d.contains("-line:")).collect()
        }
        assert_eq!(strip(&small), strip(&large));
    }
}

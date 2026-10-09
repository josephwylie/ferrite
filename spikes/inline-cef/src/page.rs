//! The demo pages: what an agent might hand back as an "interactive answer",
//! one per kind of visual output Ferrite has to show inline.
//!
//! Each page is self-contained (inline data and script, no network, no fonts
//! to fetch) and takes every colour and font from the host's `--ferrite-*`
//! custom properties, so [`theme_css`] restyles it live. The HTML lives in
//! `src/pages/*.html` so it can be opened and edited as HTML.

use crate::web::Page;

/// One demo page. [`Demo::ALL`] lists them in transcript order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Demo {
    /// A squarified treemap of a repo: hover, click to zoom, two colourings.
    Treemap,
    /// "Show me options": three labelled UI mock variants side by side.
    Mockups,
    /// KPI tiles, a line chart with a crosshair, stacked bars, a sortable table.
    Stats,
    /// A layered dependency diagram; hovering a node highlights its edges.
    Diagram,
    /// A CI run as a waterfall, with expandable failing tests (the page grows).
    Timeline,
}

impl Demo {
    pub const ALL: [Demo; 5] =
        [Demo::Treemap, Demo::Mockups, Demo::Stats, Demo::Diagram, Demo::Timeline];

    pub fn title(self) -> &'static str {
        match self {
            Demo::Treemap => "treemap",
            Demo::Mockups => "mockups",
            Demo::Stats => "stats",
            Demo::Diagram => "diagram",
            Demo::Timeline => "timeline",
        }
    }

    pub fn html(self) -> &'static str {
        match self {
            Demo::Treemap => include_str!("pages/treemap.html"),
            Demo::Mockups => include_str!("pages/mockups.html"),
            Demo::Stats => include_str!("pages/stats.html"),
            Demo::Diagram => include_str!("pages/diagram.html"),
            Demo::Timeline => include_str!("pages/timeline.html"),
        }
    }

    /// The page in the given host theme.
    pub fn page(self, dark: bool) -> Page {
        Page { html: self.html().to_owned(), theme_css: theme_css(dark) }
    }

    pub fn from_title(title: &str) -> Option<Demo> {
        Demo::ALL.into_iter().find(|d| d.title() == title)
    }
}

/// The treemap, kept for callers of the original single-page spike.
pub const TREEMAP_HTML: &str = include_str!("pages/treemap.html");

/// Dark tokens. The base palette is Ferrite's real opaque palette
/// (`crates/ferrite/src/theme.rs`); the rest are spike additions.
const DARK: &[(&str, &str)] = &[
    ("bg", "#1b1b1c"),
    ("band", "#242425"),
    ("band2", "#2e2e30"),
    ("fg", "#d7d7d9"),
    ("strong", "#f4f4f5"),
    ("muted", "#98989d"),
    ("faint", "#5e5e63"),
    ("border", "#2f2f31"),
    ("border2", "#3c3c3f"),
    ("accent", "#8eb1f0"),
    ("on-accent", "#0b0d12"),
    ("float", "#2a2a2c"),
    ("red", "#ef8a80"),
    ("green", "#93cf8c"),
    ("yellow", "#e6c47c"),
    ("blue", "#82b1f2"),
    ("magenta", "#c59df0"),
    ("cyan", "#78ccd0"),
    ("orange", "#eda879"),
    // Semantic state, readable as text on `bg` and `band`.
    ("ok", "#93cf8c"),
    ("warn", "#e6c47c"),
    ("danger", "#ef8a80"),
    ("info", "#82b1f2"),
    // Chart gridlines: quieter than `border`.
    ("grid", "#272729"),
    // Categorical series, in this order (validated for colour-vision
    // deficiency on adjacent pairs, and >= 3:1 against `bg`). Marks only: text
    // stays in text tokens.
    ("chart-1", "#3987e5"),
    ("chart-2", "#d95926"),
    ("chart-3", "#199e70"),
    ("chart-4", "#c98500"),
    ("chart-5", "#d55181"),
    ("chart-6", "#9085e9"),
];

/// Light tokens: a spike stand-in with the same names.
const LIGHT: &[(&str, &str)] = &[
    ("bg", "#fbfbfa"),
    ("band", "#f1f1f0"),
    ("band2", "#e8e8e7"),
    ("fg", "#2b2b2e"),
    ("strong", "#111113"),
    ("muted", "#6c6c72"),
    ("faint", "#a3a3a8"),
    ("border", "#e3e3e2"),
    ("border2", "#d2d2d1"),
    ("accent", "#2f63c8"),
    ("on-accent", "#ffffff"),
    ("float", "#ffffff"),
    ("red", "#c2453a"),
    ("green", "#3b8a36"),
    ("yellow", "#9a6f0c"),
    ("blue", "#2f6bcc"),
    ("magenta", "#8247c4"),
    ("cyan", "#1d8288"),
    ("orange", "#bb5f24"),
    ("ok", "#2f7d2b"),
    ("warn", "#8f6508"),
    ("danger", "#c2453a"),
    ("info", "#2f6bcc"),
    ("grid", "#ededec"),
    // Three of these are under 3:1 on `bg`; charts using them label directly
    // or offer a table.
    ("chart-1", "#2a78d6"),
    ("chart-2", "#eb6834"),
    ("chart-3", "#1baf7a"),
    ("chart-4", "#eda100"),
    ("chart-5", "#e87ba4"),
    ("chart-6", "#4a3aa7"),
];

fn declarations(dark: bool) -> String {
    let mut css = String::new();
    for (name, value) in if dark { DARK } else { LIGHT } {
        css.push_str(&format!("--ferrite-{name}:{value};"));
    }
    css.push_str(&format!(
        "--ferrite-scheme:{};--ferrite-shadow:{};",
        if dark { "dark" } else { "light" },
        if dark { "rgba(0,0,0,.4)" } else { "rgba(0,0,0,.12)" },
    ));
    css
}

/// Ferrite's tokens for the host theme, as CSS.
///
/// - `:root` carries the host theme (`dark` or light) plus the fonts.
/// - `[data-ferrite-scheme=dark]` and `[data-ferrite-scheme=light]` carry each
///   palette whatever the host theme, so a page can pin a subtree to one (a UI
///   mock that previews the other theme). They are the same in both outputs.
pub fn theme_css(dark: bool) -> String {
    format!(
        ":root{{{}--ferrite-font-mono:\"Geist Mono\",ui-monospace,\"SF Mono\",Menlo,monospace;--ferrite-font-size:12px;--ferrite-line:20px;}}\
         [data-ferrite-scheme=dark]{{{}color-scheme:dark;}}\
         [data-ferrite-scheme=light]{{{}color-scheme:light;}}",
        declarations(dark),
        declarations(true),
        declarations(false),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_names(css: &str) -> Vec<String> {
        let root = &css[":root{".len()..css.find('}').unwrap()];
        root.split(';').filter_map(|d| d.split(':').next()).map(str::to_owned).collect()
    }

    #[test]
    fn every_page_is_self_contained_and_styled_only_by_tokens() {
        for demo in Demo::ALL {
            // The SVG namespace is an identifier and `url(#id)` a same-document
            // reference (an SVG marker); neither fetches.
            let html = demo.html().replace("http://www.w3.org/2000/svg", "").replace("url(#", "");
            let name = demo.title();
            for banned in ["http://", "https://", "src=", "@import", "url(", "<link"] {
                assert!(!html.contains(banned), "{name} must not fetch anything: {banned}");
            }
            // Every colour comes from a token: none in the stylesheet, and none
            // in inline styles or script either (both are page-authored CSS).
            let hex_colour = html.split('#').skip(1).any(|rest| {
                let word: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                let boundary = rest[word.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric());
                matches!(word.len(), 3 | 6 | 8)
                    && word.chars().all(|c| c.is_ascii_hexdigit())
                    && word.chars().any(|c| c.is_ascii_digit())
                    && boundary
            });
            assert!(!hex_colour, "{name}: no literal colours");
            for f in ["rgb(", "rgba(", "hsl(", "oklch("] {
                assert!(!html.contains(f), "{name}: no literal colours ({f})");
            }
            assert!(html.contains("--ferrite-bg"), "{name} paints the theme background");
        }
    }

    #[test]
    fn titles_round_trip() {
        for demo in Demo::ALL {
            assert_eq!(Demo::from_title(demo.title()), Some(demo));
        }
        assert_eq!(TREEMAP_HTML, Demo::Treemap.html());
    }

    #[test]
    fn both_themes_define_the_same_tokens() {
        assert_eq!(root_names(&theme_css(true)), root_names(&theme_css(false)));
        assert!(theme_css(true).starts_with(":root{--ferrite-bg:#1b1b1c"));
        assert!(theme_css(false).starts_with(":root{--ferrite-bg:#fbfbfa"));
        let names = |t: &[(&'static str, &'static str)]| t.iter().map(|(n, _)| *n).collect::<Vec<_>>();
        assert_eq!(names(DARK), names(LIGHT));
    }

    #[test]
    fn scheme_scopes_are_independent_of_the_host_theme() {
        let scoped = |css: String| css[css.find("[data-ferrite-scheme").unwrap()..].to_owned();
        assert_eq!(scoped(theme_css(true)), scoped(theme_css(false)));
    }

    /// Not a test: writes each page as the CEF adapter would load it, for
    /// previewing in a desktop browser.
    /// `INLINE_CEF_PREVIEW=/tmp/dir cargo test dump_previews -- --ignored`
    #[test]
    #[ignore]
    fn dump_previews() {
        let dir = std::env::var("INLINE_CEF_PREVIEW").expect("set INLINE_CEF_PREVIEW to a directory");
        std::fs::create_dir_all(&dir).unwrap();
        for demo in Demo::ALL {
            for dark in [true, false] {
                let page = demo.page(dark);
                let doc = format!(
                    "<!doctype html><html><head><meta charset=\"utf-8\"><style>html,body{{overflow:hidden}}</style><style id=\"__ferrite_theme\">{}</style>\n{}",
                    page.theme_css, page.html
                );
                let path = format!("{dir}/{}-{}.html", demo.title(), if dark { "dark" } else { "light" });
                std::fs::write(path, doc).unwrap();
            }
        }
        std::fs::write(format!("{dir}/theme-dark.css"), theme_css(true)).unwrap();
        std::fs::write(format!("{dir}/theme-light.css"), theme_css(false)).unwrap();
    }
}

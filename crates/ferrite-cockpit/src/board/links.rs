//! The board's links (ADR 0031, Seam 3). Upstream's view marks words that
//! open an address (`Piece::link`: a pull request, `↗ Open review`, `Open
//! report`, `Open plan`, a piece of evidence) and its terminal runtime writes
//! them as OSC 8 links the terminal opens. Here the same words are clickable:
//! a click on one opens its address. Evidence and report pages stay on
//! upstream's `serve` (127.0.0.1, shared with `tailscale serve` where it is
//! installed), so their addresses are opened as they are.

use sw_board::shown::ink::Inked;

/// A link on the screen: its words' cells on one line, and where it goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub y: usize,
    pub x: usize,
    pub width: usize,
    pub url: String,
}

/// The links of a drawn screen, at the cells the drawer gives their words:
/// neighbouring pieces with the same address are one link.
pub fn links(lines: &[Inked]) -> Vec<Link> {
    let mut out: Vec<Link> = vec![];
    for (y, line) in lines.iter().enumerate() {
        let mut x = 0;
        for piece in &line.0 {
            let width = sw_board::text::width(&piece.t);
            if let Some(url) = piece.link.as_ref().filter(|_| width > 0) {
                match out.last_mut() {
                    Some(last) if last.y == y && last.x + last.width == x && last.url == *url => {
                        last.width += width
                    }
                    _ => out.push(Link {
                        y,
                        x,
                        width,
                        url: url.clone(),
                    }),
                }
            }
            x += width;
        }
    }
    out
}

/// The link under a cell, if any.
pub fn at(links: &[Link], x: usize, y: usize) -> Option<&Link> {
    links
        .iter()
        .find(|l| l.y == y && (l.x..l.x + l.width).contains(&x))
}

/// What a link opens: a web or file address as it is, a bare absolute path
/// as a file address. Anything else (a relative path, a word) opens nothing.
pub fn target(url: &str) -> Option<String> {
    let url = url.trim();
    if ["http://", "https://", "file://"]
        .iter()
        .any(|scheme| url.starts_with(scheme))
    {
        return Some(url.to_string());
    }
    url::Url::from_file_path(url).ok().map(Into::into)
}

/// Open a link's address with the system's handler.
pub fn open(url: &str, cx: &mut gpui::App) {
    if let Some(target) = target(url) {
        cx.open_url(&target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sw_board::shown::ink::Ink;

    #[test]
    fn a_link_is_found_at_its_words_cells_after_wide_characters() {
        let line = Inked::plain(" 日本 ")
            .add(Inked::of("↗ ", Ink::Accent))
            .add(
                Inked::plain("Open review")
                    .link("http://127.0.0.1:4/r/1")
                    .under(),
            )
            .add(Inked::plain(" · "))
            .add(Inked::of("PR#60", Ink::Green).link("https://github.com/a/b/pull/60"));
        let found = links(&[Inked::new(), line]);
        assert_eq!(
            found,
            vec![
                Link {
                    y: 1,
                    x: 8,
                    width: 11,
                    url: "http://127.0.0.1:4/r/1".into()
                },
                Link {
                    y: 1,
                    x: 22,
                    width: 5,
                    url: "https://github.com/a/b/pull/60".into()
                },
            ]
        );
        assert_eq!(at(&found, 8, 1).map(|l| l.x), Some(8));
        assert_eq!(at(&found, 18, 1).map(|l| l.x), Some(8));
        assert!(
            at(&found, 19, 1).is_none(),
            "the gap between links is no link"
        );
        assert!(at(&found, 8, 0).is_none());
        assert_eq!(
            at(&found, 26, 1).map(|l| l.url.as_str()),
            Some("https://github.com/a/b/pull/60")
        );
    }

    #[test]
    fn a_link_wrapped_over_pieces_of_one_line_is_one_link() {
        let url = "http://127.0.0.1:4/e/1";
        let line = Inked::plain("  ")
            .add(Inked::plain("the screenshot").link(url))
            .add(Inked::of(" of it", Ink::Muted).link(url));
        assert_eq!(
            links(&[line]),
            vec![Link {
                y: 0,
                x: 2,
                width: 20,
                url: url.into()
            }]
        );
    }

    #[test]
    fn web_file_and_absolute_path_links_open_and_nothing_else_does() {
        assert_eq!(
            target("https://github.com/a/b/pull/60").as_deref(),
            Some("https://github.com/a/b/pull/60")
        );
        assert_eq!(
            target("http://127.0.0.1:4/r/1").as_deref(),
            Some("http://127.0.0.1:4/r/1")
        );
        assert_eq!(
            target("/tmp/report.md").as_deref(),
            Some("file:///tmp/report.md")
        );
        assert_eq!(
            target("file:///tmp/plan.md").as_deref(),
            Some("file:///tmp/plan.md")
        );
        assert_eq!(target("plan.md"), None);
    }
}

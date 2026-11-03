//! A page beside the transcript (Claude's preview pane): an `.html` file a
//! turn wrote or named, drawn live by the visuals' engine
//! (`visual::element::WebView`) in the Panel's Preview tab, or a local
//! address. It opens from the transcript's page card
//! (`transcript/outputs.rs`) through `Preview::open_page`.
//!
//! The engine draws the agent's own HTML, sandboxed as a visual is: the
//! file's markup with nothing fetched past it. It loads no addresses, so a
//! local address — and a file in a build without Chromium — shows the
//! Panel's "can't show" note ([`Shown::Unshown`]) with the page a click
//! away in the browser.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use gpui::{App, Entity, SharedString};

use crate::visual::element::WebView;
use crate::visual::engine::{Availability, Visuals};

/// Where a page lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageSource {
    File(PathBuf),
    /// A local address (`http://localhost:5173/`).
    Address(String),
}

impl PageSource {
    /// What the browser opens: the file's URL, or the address.
    pub fn url(&self) -> String {
        match self {
            PageSource::File(path) => url::Url::from_file_path(path)
                .map(Into::into)
                .unwrap_or_else(|_| format!("file://{}", path.display())),
            PageSource::Address(address) => address.clone(),
        }
    }

    /// What a head or a card names it by: the file's `<title>`, else its
    /// name; the address without its scheme.
    pub fn title(&self) -> String {
        match self {
            PageSource::File(path) => page_title(path).unwrap_or_else(|| {
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                )
            }),
            PageSource::Address(address) => address
                .split_once("://")
                .map_or(address.as_str(), |(_, rest)| rest)
                .trim_end_matches('/')
                .to_owned(),
        }
    }

    /// What it reads as under its card's title.
    pub fn kind(&self) -> &'static str {
        match self {
            PageSource::File(_) => "HTML page",
            PageSource::Address(_) => "Local server",
        }
    }
}

/// A file's `<title>`, if it has a readable one.
fn page_title(path: &Path) -> Option<String> {
    let html = std::fs::read_to_string(path).ok()?;
    title_of(&html)
}

/// The text of the first `<title>` in `html`, its whitespace folded.
fn title_of(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    let title = html[start..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!title.is_empty()).then_some(title)
}

/// What the Preview tab can draw of a page this frame.
pub enum Shown {
    Live(Entity<WebView>),
    /// Chromium is starting; the window draws again when it is up.
    Starting,
    /// It can't show here, and why: the panel's "can't show" note, the page
    /// a click away in the browser.
    Unshown(SharedString),
}

/// A page open in a preview: where it lives, and its live view once the
/// engine has drawn it.
pub struct PageView {
    source: PageSource,
    web: OnceLock<Entity<WebView>>,
}

impl PageView {
    pub fn new(source: PageSource) -> Arc<Self> {
        Arc::new(Self {
            source,
            web: OnceLock::new(),
        })
    }

    pub fn source(&self) -> &PageSource {
        &self.source
    }

    /// Read the file again (its title menu's `Reload`): the live page
    /// takes its new markup in the same view.
    pub fn reload(&self, cx: &mut App) {
        if let (PageSource::File(path), Some(web)) = (&self.source, self.web.get()) {
            if let Ok(html) = std::fs::read_to_string(path) {
                web.update(cx, |web, cx| web.set_html(html, cx));
            }
        }
    }

    /// What can be drawn of it now; the first ask with the engine up opens
    /// the live page.
    pub fn shown(&self, cx: &mut App) -> Shown {
        if let Some(web) = self.web.get() {
            return Shown::Live(web.clone());
        }
        let PageSource::File(path) = &self.source else {
            return Shown::Unshown(ADDRESS_UNSHOWN.into());
        };
        let engine = match Visuals::engine(cx) {
            Availability::Ready(engine) => engine,
            Availability::Starting => return Shown::Starting,
            Availability::Unavailable(why) => return Shown::Unshown(why),
        };
        let html = match std::fs::read_to_string(path) {
            Ok(html) => html,
            Err(error) => return Shown::Unshown(error.to_string().into()),
        };
        let page = crate::visual::web::Page {
            html,
            theme_css: crate::visual::tokens::theme_css(
                crate::visual::tokens::Scheme::APP,
                ferrite_core::settings::ReadingSize::STANDARD,
            ),
        };
        Shown::Live(
            self.web
                .get_or_init(|| WebView::new(engine, page, cx))
                .clone(),
        )
    }
}

/// Why a local address shows no page here: the engine draws the agent's
/// files, never a server's.
const ADDRESS_UNSHOWN: &str =
    "Ferrite draws pages from files; a local server opens in your browser";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_named_by_its_title_or_its_place() {
        assert_eq!(
            title_of("<html><head><TITLE>\n  Sales  report </title>").as_deref(),
            Some("Sales report")
        );
        assert_eq!(title_of("<title></title>"), None);
        assert_eq!(title_of("<p>no title</p>"), None);
        let address = PageSource::Address("http://localhost:5173/".into());
        assert_eq!(address.title(), "localhost:5173");
        assert_eq!(address.url(), "http://localhost:5173/");
        let file = PageSource::File(PathBuf::from("/nowhere/chart.html"));
        assert_eq!(file.title(), "chart.html", "an unreadable file, by name");
        assert_eq!(file.url(), "file:///nowhere/chart.html");
    }
}

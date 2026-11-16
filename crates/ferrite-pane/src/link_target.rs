//! Where a link goes, decided once. A press, its context menu, Copy and a
//! keyboard activation all resolve the same address to one `LinkTarget`,
//! and the label a link shows never takes part: it is not an address.
//!
//! The renderer's own schemes (`issue:`, `ferrite-path:`, `ferrite-thread:`,
//! the chip and skill ones) end here. None reaches the OS opener: they
//! become a real address, an app destination, or an `Unavailable` that says
//! why.

use std::path::Path;

use ferrite_core::ThreadId;
use gpui::{App, SharedString, Window};

use crate::file_links::{issue_url, origin_base, FileLink};
use crate::preview::Preview;
use crate::rich::{CHIP_SCHEME, ISSUE_SCHEME, PATH_SCHEME, SKILL_SCHEME, THREAD_SCHEME};

#[derive(Clone, Debug, PartialEq)]
pub enum LinkTarget {
    /// A Thread's pull request, which the Thread's checkout names: `url`
    /// is the forge's address, absent while the forge has not given one.
    PullRequest {
        thread: ThreadId,
        number: u64,
        url: Option<String>,
    },
    /// A pull request of the repository the link's checkout belongs to,
    /// named by `#N` or by its forge address: the app opens its native
    /// detail, and `url` is where a browser would go.
    RepoPullRequest {
        number: u64,
        url: String,
    },
    Thread(ThreadId),
    /// A file, and the line or range inside it (`FileLink::location`).
    File(FileLink),
    /// A heading inside the document that holds the link.
    Fragment(String),
    /// A web address, or another scheme the OS knows (`mailto:`).
    External(String),
    Unavailable(SharedString),
}

/// Which door the operator chose: the one the press names, the integrated
/// browser, or the system's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Open {
    Primary,
    Integrated,
    System,
}

impl Open {
    /// A press opens the primary destination; the secondary modifier
    /// (⌘, Ctrl) picks the system's browser. A key press is a click with
    /// no modifier.
    pub fn of(event: &gpui::ClickEvent) -> Self {
        Self::of_modifiers(event.modifiers())
    }

    pub fn of_modifiers(modifiers: gpui::Modifiers) -> Self {
        if modifiers.secondary() {
            Open::System
        } else {
            Open::Primary
        }
    }
}

impl LinkTarget {
    /// The destination a link's `raw` address names, where `cwd` is the
    /// checkout its relative paths and issue numbers belong to.
    pub fn resolve(raw: &str, cwd: Option<&Path>) -> Self {
        let raw = raw.strip_prefix(PATH_SCHEME).unwrap_or(raw);
        let base = cwd.and_then(origin_base);
        if let Some(pr) = base.as_deref().and_then(|base| repo_pull_request(raw, base)) {
            return pr;
        }
        if let Some(number) = raw.strip_prefix(ISSUE_SCHEME) {
            return match cwd.and_then(|cwd| issue_url(cwd, number)) {
                Some(url) => Self::External(url),
                None => Self::unavailable(format!(
                    "Issue #{number} has no address: this checkout has no origin remote"
                )),
            };
        }
        if let Some(id) = raw.strip_prefix(THREAD_SCHEME) {
            return match id.parse() {
                Ok(id) => Self::Thread(ThreadId::new(id)),
                Err(_) => Self::unavailable("This Thread link names no Thread"),
            };
        }
        if raw.starts_with(CHIP_SCHEME) || raw.starts_with(SKILL_SCHEME) {
            return Self::unavailable("This is a part of the prompt, not an address");
        }
        if raw.starts_with('#') {
            return Self::Fragment(raw.to_owned());
        }
        if let Some(file) = FileLink::resolve(raw, cwd) {
            return Self::File(file);
        }
        if url::Url::parse(raw).is_ok() {
            return Self::External(raw.to_owned());
        }
        Self::unavailable(format!("{raw} names no file here and is not an address"))
    }

    fn unavailable(why: impl Into<SharedString>) -> Self {
        Self::Unavailable(why.into())
    }

    /// The text Copy puts on the clipboard, or why there is none.
    pub fn address(&self) -> Result<String, SharedString> {
        match self {
            Self::PullRequest { url: Some(url), .. }
            | Self::RepoPullRequest { url, .. }
            | Self::External(url) => Ok(url.clone()),
            Self::PullRequest { number, .. } => {
                Err(format!("PR #{number} has no address yet").into())
            }
            Self::Thread(thread) => Ok(thread_link(*thread)),
            Self::File(file) => Ok(file.address()),
            Self::Fragment(fragment) => Ok(fragment.clone()),
            Self::Unavailable(why) => Err(why.clone()),
        }
    }

    /// Open what the pane can open itself; false for what only the app can
    /// (a PR's native detail, a Thread). A web address goes to the
    /// integrated browser of the Thread `preview` belongs to, unless the
    /// system's was asked for. What cannot open says why, on screen.
    pub fn open(
        &self,
        how: Open,
        preview: Option<&Preview>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        match self {
            Self::RepoPullRequest { .. } if how == Open::Primary => false,
            Self::External(url) | Self::RepoPullRequest { url, .. } => {
                let inside = how != Open::System
                    && preview.is_some_and(|preview| preview.open_url(url, window));
                if !inside {
                    cx.open_url(url);
                }
                true
            }
            Self::File(file) => {
                file.open(window, cx);
                true
            }
            Self::Fragment(_) => {
                notify_unavailable("Section links do not scroll the answer yet", window, cx);
                true
            }
            Self::Unavailable(why) => {
                notify_unavailable(why, window, cx);
                true
            }
            Self::PullRequest { .. } | Self::RepoPullRequest { .. } | Self::Thread(_) => false,
        }
    }
}

/// `raw` as a pull request of the repository at `base`: `issue:N` (the
/// renderer's `#N`), or the forge's `<base>/pull/N` with or without a page
/// after it.
/// Only GitHub's numbers read natively yet.
fn repo_pull_request(raw: &str, base: &str) -> Option<LinkTarget> {
    if !base.starts_with("https://github.com/") {
        return None;
    }
    // GitHub sends `/issues/N` on to `/pull/N` for a PR, so a bare `#N`
    // keeps an address that is right whichever it names.
    let (number, url) = match raw.strip_prefix(ISSUE_SCHEME) {
        Some(number) => (number, format!("{base}/issues/{number}")),
        None => {
            let rest = raw.strip_prefix(base)?.strip_prefix("/pull/")?;
            let end = rest.find(['/', '#', '?']).unwrap_or(rest.len());
            (&rest[..end], raw.to_string())
        }
    };
    let number: u64 = number.parse().ok()?;
    Some(LinkTarget::RepoPullRequest { number, url })
}

pub fn notify_unavailable(why: &str, window: &mut Window, cx: &mut App) {
    use gpui::component::{notification::Notification, WindowExt};
    window.push_notification(Notification::error(why.to_owned()), cx);
}

/// A Thread's link, in the form the CLI takes: there is no `ferrite://`
/// scheme, so the command that lands every window on it (`thread.focus`,
/// ADR 0027).
pub fn thread_link(thread: ThreadId) -> String {
    format!("ferrite thread focus {}", thread.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_renderers_schemes_never_become_an_address_for_the_os() {
        for raw in ["issue:212", "ferrite-chip:/a", "ferrite-skill:x", "ferrite-thread:x"] {
            assert!(
                matches!(LinkTarget::resolve(raw, None), LinkTarget::Unavailable(_)),
                "{raw}"
            );
        }
        assert_eq!(
            LinkTarget::resolve("ferrite-thread:12", None),
            LinkTarget::Thread(ThreadId::new(12))
        );
    }

    #[test]
    fn an_address_is_what_the_link_names_not_what_it_shows() {
        assert_eq!(
            LinkTarget::resolve("https://example.com/a?b=c#d", None).address().as_deref(),
            Ok("https://example.com/a?b=c#d")
        );
        assert_eq!(
            LinkTarget::resolve("#details", None),
            LinkTarget::Fragment("#details".into())
        );
        let file = LinkTarget::resolve("src/main.rs:42", Some(Path::new("/repo")));
        assert_eq!(file.address().as_deref(), Ok("/repo/src/main.rs:42"));
        assert!(LinkTarget::resolve("src/main.rs", None).address().is_err());
    }

    #[test]
    fn a_pr_without_an_address_says_so() {
        let pr = LinkTarget::PullRequest {
            thread: ThreadId::new(1),
            number: 7,
            url: None,
        };
        assert_eq!(pr.address(), Err("PR #7 has no address yet".into()));
    }
}

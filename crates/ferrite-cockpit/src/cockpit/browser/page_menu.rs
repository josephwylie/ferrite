//! The menu a right-click on a web page opens.
use super::*;
use crate::cockpit::context_menu::MenuVerb;
use ferrite_ui::menu;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::cockpit) enum PageVerb {
    OpenLink,
    CopyLink,
    CopySelection,
    Back,
    Forward,
    Reload,
    CopyAddress,
    OpenExternal,
}

impl CockpitView {
    pub(in crate::cockpit) fn browser_page_rows(
        &self,
        thread: ThreadKey,
        cx: &gpui::App,
    ) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let Some(browser) = self.browsers.threads.get(&thread) else {
            return Vec::new();
        };
        let tab = browser.shown_tab(self.browser_chrome.read(cx).new_tab(thread));
        let context = browser.context.as_ref();
        let row = |label: &str, verb: PageVerb, enabled: bool| {
            Some((menu::Item::new(label).disabled(!enabled), MenuVerb::Browser(verb)))
        };
        let link = context.is_some_and(|context| context.link.is_some());
        let selection = context.is_some_and(|context| context.selection.is_some());
        let mut rows = Vec::new();
        if link {
            rows.push(row("Open link in new tab", PageVerb::OpenLink, true));
            rows.push(row("Copy link address", PageVerb::CopyLink, true));
            rows.push(None);
        }
        if selection {
            rows.push(row("Copy", PageVerb::CopySelection, true));
            rows.push(None);
        }
        rows.push(row(
            "Back",
            PageVerb::Back,
            tab.is_some_and(|tab| tab.can_back),
        ));
        rows.push(row(
            "Forward",
            PageVerb::Forward,
            tab.is_some_and(|tab| tab.can_forward),
        ));
        rows.push(row("Reload", PageVerb::Reload, tab.is_some()));
        rows.push(None);
        rows.push(row("Copy page address", PageVerb::CopyAddress, tab.is_some()));
        rows.push(row(
            "Open in system browser",
            PageVerb::OpenExternal,
            tab.is_some(),
        ));
        rows
    }

    pub(in crate::cockpit) fn run_browser_page_verb(
        &mut self,
        thread: ThreadKey,
        verb: PageVerb,
        cx: &mut Context<Self>,
    ) {
        let new_tab = self.browser_chrome.read(cx).new_tab(thread);
        let Some(browser) = self.browsers.threads.get(&thread) else {
            return;
        };
        let tab = browser.shown_tab(new_tab);
        let document = tab.map(|tab| tab.document());
        let address = tab.map(|tab| tab.url.clone());
        let context = browser.context.clone().unwrap_or_default();
        match verb {
            PageVerb::OpenLink => {
                if let Some(url) = context.link {
                    self.browser_action(thread, Action::Open { url }, cx);
                }
            }
            PageVerb::CopyLink => copy(context.link, cx),
            PageVerb::CopySelection => copy(context.selection, cx),
            PageVerb::CopyAddress => copy(address, cx),
            PageVerb::OpenExternal => {
                if let Some(url) = address {
                    cx.open_url(&url);
                }
            }
            PageVerb::Back | PageVerb::Forward | PageVerb::Reload => {
                let Some(document) = document else { return };
                let action = match verb {
                    PageVerb::Back => Action::Back { document },
                    PageVerb::Forward => Action::Forward { document },
                    _ => Action::Reload { document },
                };
                self.browser_action(thread, action, cx);
            }
        }
    }
}

fn copy(text: Option<String>, cx: &mut Context<CockpitView>) {
    if let Some(text) = text {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
    }
}

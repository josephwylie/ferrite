//! The Browser's chrome (ADR 0041): the tab strip and the toolbar above the
//! page, and the address field each Thread's Browser keeps, with whether a
//! new tab is being entered. It has no render of its own: the root has it
//! draw the strip and the toolbar (`browser_body`), handing it the Thread's
//! tabs and what its page allows with `browser_chrome_inputs`. What a press
//! asks of the pages it says as a `BrowserChromeEvent`. The pages, the
//! Server's answers and the engine are the root's.
use super::*;

/// What the address field acts on when submitted. A new tab acts on no
/// existing page, so while one is pending no page's controls apply.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Entry {
    Page,
    NewTab,
}

/// One Thread's chrome.
struct Chrome {
    address: Entity<Composer>,
    /// The page address the field last took from the service. An edit is any
    /// text that differs from it, and survives the page's own navigation.
    address_shown: Option<(TabId, String)>,
    entry: Entry,
}

/// The chrome of every Thread's Browser.
#[derive(Default)]
pub(in crate::cockpit) struct BrowserChromeView {
    threads: BTreeMap<ThreadKey, Chrome>,
}

/// What a press in the chrome asks of the window.
pub(in crate::cockpit) enum BrowserChromeEvent {
    /// A tab, Back, Forward, Reload, Go or a tab's close: an action on the
    /// Thread's pages.
    Act(ThreadKey, Action),
    /// The annotate button: begin an annotation, or close the one open.
    Annotate { thread: ThreadKey, begin: bool },
    /// New tab: a capture under way stops.
    NewTab(ThreadKey),
}

impl EventEmitter<BrowserChromeEvent> for BrowserChromeView {}

/// What the chrome draws of one Thread's Browser.
pub(in crate::cockpit) struct BrowserChromeInputs {
    tabs: Vec<Tab>,
    active: Option<TabId>,
    /// A live page stands for the active tab.
    realized: bool,
    annotating: bool,
}

/// The chrome as drawn: the strip, while there is a choice of tabs, and the
/// toolbar.
pub(in crate::cockpit) struct ChromeParts {
    pub(super) strip: Option<AnyElement>,
    pub(super) toolbar: AnyElement,
}

impl BrowserChromeView {
    /// `thread`'s chrome, made the first time its Browser is.
    pub(super) fn make(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        self.threads.entry(thread).or_insert_with(|| Chrome {
            address: cx.new(|cx| {
                let mut field = Composer::new(cx);
                field.set_role(Role::Field(Some("BrowserAddress")), cx);
                field.set_visible_row_limit(1, cx);
                field
            }),
            address_shown: None,
            entry: Entry::Page,
        });
    }

    /// Every Thread `keep` refuses takes its chrome with it.
    pub(in crate::cockpit) fn retain_threads(&mut self, keep: impl Fn(ThreadKey) -> bool) {
        self.threads.retain(|key, _| keep(*key));
    }

    #[cfg(test)]
    pub(in crate::cockpit) fn held_for_test(&self, thread: ThreadKey) -> usize {
        self.threads.keys().filter(|key| **key == thread).count()
    }

    #[cfg(test)]
    pub(in crate::cockpit) fn address_for_test(&self, thread: ThreadKey, cx: &gpui::App) -> String {
        self.threads[&thread].address.read(cx).text().to_string()
    }

    /// A new tab is being entered: no page's controls apply.
    pub(super) fn new_tab(&self, thread: ThreadKey) -> bool {
        self.threads
            .get(&thread)
            .is_some_and(|chrome| chrome.entry == Entry::NewTab)
    }

    /// The Server took an action: the field acts on the page again, and
    /// with no page left it empties.
    pub(super) fn landed(&mut self, thread: ThreadKey, page: bool, cx: &mut Context<Self>) {
        let Some(chrome) = self.threads.get_mut(&thread) else {
            return;
        };
        chrome.entry = Entry::Page;
        if !page {
            chrome.address_shown = None;
            chrome
                .address
                .update(cx, |field, cx| field.set(String::new(), cx));
        }
    }

    /// The active tab shows `url`: the field takes it, unless it holds an
    /// edit of this tab's address or a new tab is being entered.
    pub(super) fn follow(&mut self, thread: ThreadKey, tab: TabId, url: &str, cx: &mut Context<Self>) {
        let Some(chrome) = self.threads.get_mut(&thread) else {
            return;
        };
        let shown = (tab, url.to_owned());
        if chrome.entry != Entry::Page || chrome.address_shown.as_ref() == Some(&shown) {
            return;
        }
        let edited = chrome
            .address_shown
            .as_ref()
            .is_some_and(|(id, url)| *id == tab && chrome.address.read(cx).text() != url);
        if !edited {
            chrome
                .address
                .update(cx, |field, cx| field.set(shown.1.clone(), cx));
        }
        chrome.address_shown = Some(shown);
    }

    /// Leave a pending new tab, or put back what an edit replaced of the
    /// active page's address, `active`. False when the address held nothing
    /// to cancel.
    fn cancel_address(&mut self, thread: ThreadKey, active: Option<&str>, cx: &mut Context<Self>) -> bool {
        let Some(chrome) = self.threads.get_mut(&thread) else {
            return false;
        };
        let edited = active.is_some_and(|url| chrome.address.read(cx).text() != url);
        if chrome.entry == Entry::Page && !edited {
            return false;
        }
        chrome.entry = Entry::Page;
        chrome.address_shown = None;
        cx.notify();
        true
    }

    /// `thread`'s tab strip and toolbar.
    pub(super) fn bar(
        &mut self,
        thread: ThreadKey,
        inputs: BrowserChromeInputs,
        cx: &mut Context<Self>,
    ) -> ChromeParts {
        use ferrite_ui::{
            components::{icon_button, icon_button_as, IconSize, IconState, Tip as _},
            icons,
            theme::*,
        };
        // The root makes it with the Thread's Browser (`ensure_browser`).
        let chrome = &self.threads[&thread];
        let entry = chrome.entry;
        let BrowserChromeInputs {
            tabs,
            active,
            realized,
            annotating,
        } = inputs;
        let active_tab = tabs.iter().find(|tab| Some(tab.id) == active);
        let shown = active_tab.filter(|_| entry == Entry::Page);
        let strip = (tabs.len() > 1 || (entry == Entry::NewTab && !tabs.is_empty())).then(|| {
            let chips = tabs.iter().map(|tab| {
                let id = tab.id;
                let title = url::Url::parse(&tab.url)
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_owned))
                    .unwrap_or_else(|| tab.url.clone());
                div()
                    .id(("browser-tab", id.0))
                    .debug_selector(move || format!("browser-tab-{}", id.0))
                    .flex()
                    .items_center()
                    .min_w_0()
                    .flex_shrink_0()
                    .max_w(px(190.))
                    .rounded(px(R_CONTROL))
                    .text_size(px(FS_LABEL))
                    .font_weight(W_LABEL)
                    .px(px(8.))
                    .py(px(4.))
                    .map(|tab| {
                        let key = format!("browser-tab-{}", id.0);
                        if Some(id) == active && entry == Entry::Page {
                            tab.hover_carried(key).text_color(rgb(INK_STRONG))
                        } else {
                            tab.hover_raised(key)
                                .press_raised()
                                .text_color(rgb(INK_SUBTLE))
                        }
                    })
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(title),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        // Choosing a page leaves a new tab being entered at
                        // once; the Server's selection follows.
                        if let Some(chrome) = this.threads.get_mut(&thread) {
                            chrome.entry = Entry::Page;
                            cx.notify();
                        }
                        cx.emit(BrowserChromeEvent::Act(thread, Action::Select { tab: id }))
                    }))
                    .child(
                        icon_button(
                            format!("browser-close-tab-{}", id.0),
                            icons::CLOSE,
                            IconSize::Standard,
                            "Close tab",
                            cx,
                        )
                        .tip("Close tab")
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.stop_propagation();
                            cx.emit(BrowserChromeEvent::Act(thread, Action::Close { tab: id }));
                        })),
                    )
            });
            div()
                .id("browser-tabs")
                .debug_selector(|| "browser-tabs".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .overflow_x_scroll()
                .children(chips.collect::<Vec<_>>())
                .when(entry == Entry::NewTab, |strip| {
                    strip
                        .child(
                            div()
                                .flex_shrink_0()
                                .px(px(8.))
                                .py(px(4.))
                                .text_size(px(FS_LABEL))
                                .font_weight(W_LABEL)
                                .text_color(rgb(INK_STRONG))
                                .child("New tab"),
                        )
                        .child(
                            icon_button(
                                "browser-cancel-new-tab",
                                icons::CLOSE,
                                IconSize::Standard,
                                "Cancel new tab",
                                cx,
                            )
                            .tip("Cancel new tab")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.cancel_address(thread, None, cx);
                            })),
                        )
                })
                .into_any_element()
        });
        let mut toolbar = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .h(px(40.))
            .px(px(8.));
        let document = shown.map(|tab| tab.document());
        for (id, glyph, label, enabled, kind) in [
            (
                "browser-back",
                icons::BACK,
                "Back",
                shown.is_some_and(|tab| tab.can_back),
                0,
            ),
            (
                "browser-forward",
                icons::FORWARD,
                "Forward",
                shown.is_some_and(|tab| tab.can_forward),
                1,
            ),
            (
                "browser-reload",
                icons::REFRESH,
                "Reload",
                shown.is_some(),
                2,
            ),
        ] {
            toolbar = toolbar.child(
                icon_button_as(id, glyph, IconSize::Standard, label, IconState::live(enabled), cx)
                    .tip(label)
                    .on_click(cx.listener(move |_, _, _, cx| {
                        let Some(document) = document else { return };
                        cx.emit(BrowserChromeEvent::Act(
                            thread,
                            match kind {
                                0 => Action::Back { document },
                                1 => Action::Forward { document },
                                _ => Action::Reload { document },
                            },
                        ));
                    })),
            );
        }
        let loading = shown.is_some_and(|tab| tab.loading);
        let annotatable = !loading && shown.is_some() && realized;
        let address_empty = chrome.address.read(cx).text().is_empty();
        let annotate_why = if annotatable || annotating {
            None
        } else if loading {
            Some("Available when the page finishes loading")
        } else {
            Some("Open a page to annotate it")
        };
        let annotate = match annotate_why {
            Some(why) => IconState::Disabled(Some(why.into())),
            None if annotating => IconState::Lit,
            None => IconState::Plain,
        };
        // What a press acts on is read as it lands: a new tab entered since
        // this frame acts on no page.
        let active_document = active_tab.map(Tab::document);
        let active_url = active_tab.map(|tab| tab.url.clone());
        let cancel_url = active_url.clone();
        toolbar = toolbar
            .child(
                div()
                    .debug_selector(|| "browser-address".into())
                    .relative()
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w_0()
                    .h(px(ICON_BUTTON))
                    .px(px(SPACE_2))
                    .rounded(px(R_CONTROL))
                    .border_1()
                    .border_color(paint::FIELD_RING)
                    .bg(paint::FIELD)
                    .child(div().flex_1().min_w_0().child(chrome.address.clone()))
                    .when(address_empty, |field| {
                        field.child(
                            div()
                                .absolute()
                                .left(px(SPACE_2))
                                .text_color(rgb(PLACEHOLDER_INK))
                                .child("Search or enter address"),
                        )
                    })
                    .on_action(cx.listener(move |this, _: &Go, _, cx| {
                        this.go(thread, active_document, cx)
                    }))
                    .on_action(cx.listener(move |this, _: &CancelAddress, _, cx| {
                        if !this.cancel_address(thread, cancel_url.as_deref(), cx) {
                            cx.propagate();
                        }
                    })),
            )
            .child(
                icon_button_as(
                    "browser-annotate",
                    icons::SQUARE_PEN,
                    IconSize::Standard,
                    "Annotate preview",
                    annotate,
                    cx,
                )
                .when(annotate_why.is_none(), |button| {
                    button.tip(if annotating {
                        "Cancel annotation (Esc)"
                    } else {
                        "Annotate preview"
                    })
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(BrowserChromeEvent::Annotate {
                        thread,
                        begin: !annotating,
                    })
                })),
            )
            .child(
                icon_button("browser-go", icons::FORWARD, IconSize::Standard, "Go to address", cx)
                    .tip("Go to address")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.go(thread, active_document, cx)
                    })),
            )
            .child(
                icon_button("browser-new-tab", icons::PLUS, IconSize::Standard, "New tab", cx)
                    .tip("New tab")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.emit(BrowserChromeEvent::NewTab(thread));
                        if let Some(chrome) = this.threads.get_mut(&thread) {
                            chrome.entry = Entry::NewTab;
                            chrome
                                .address
                                .update(cx, |field, cx| field.set(String::new(), cx));
                            let focus = chrome.address.read(cx).focus_target(window, cx);
                            window.focus(&focus, cx);
                            cx.notify();
                        }
                    })),
            )
            .child(
                icon_button_as(
                    "browser-open-external",
                    icons::EXTERNAL,
                    IconSize::Standard,
                    "Open in system browser",
                    IconState::live(shown.is_some()),
                    cx,
                )
                .tip("Open in system browser")
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(url) = active_url.as_ref().filter(|_| !this.new_tab(thread)) {
                        cx.open_url(url);
                    }
                })),
            )
            .child(
                icon_button_as(
                    "browser-close-page",
                    icons::CLOSE,
                    IconSize::Standard,
                    "Close tab",
                    IconState::live(shown.is_some()),
                    cx,
                )
                .tip("Close tab")
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(document) = active_document.filter(|_| !this.new_tab(thread)) {
                        let tab = document.tab;
                        cx.emit(BrowserChromeEvent::Act(thread, Action::Close { tab }));
                    }
                })),
            );
        // The progress line sits on the toolbar's lower edge, so it moves no
        // control.
        let toolbar = div()
            .relative()
            .flex_shrink_0()
            .child(toolbar)
            .when(loading, |bar| {
                bar.child(
                    div()
                        .debug_selector(|| "browser-progress".into())
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(px(2.))
                        .bg(rgb(LINK_INK)),
                )
            })
            .into_any_element();
        ChromeParts { strip, toolbar }
    }

    /// Go: the active page navigates to the field's address, or while a
    /// new tab is entered, or with no page, a tab opens on it.
    fn go(&mut self, thread: ThreadKey, active: Option<Document>, cx: &mut Context<Self>) {
        let Some(chrome) = self.threads.get(&thread) else {
            return;
        };
        let url = chrome.address.read(cx).text().trim().to_owned();
        cx.emit(BrowserChromeEvent::Act(
            thread,
            match active.filter(|_| chrome.entry == Entry::Page) {
                Some(document) => Action::Navigate { document, url },
                None => Action::Open { url },
            },
        ));
    }
}

impl CockpitView {
    /// What the chrome draws of `thread`'s Browser: its tabs, the active
    /// one, whether a live page stands for it, and whether an annotation or
    /// its review is open.
    pub(super) fn browser_chrome_inputs(&self, thread: ThreadKey, cx: &gpui::App) -> Option<BrowserChromeInputs> {
        let browser = self.browsers.threads.get(&thread)?;
        Some(BrowserChromeInputs {
            tabs: browser.state.tabs.clone(),
            active: browser.state.active,
            realized: browser
                .state
                .active
                .is_some_and(|tab| browser.pages.contains_key(&tab)),
            annotating: browser.annotation.is_some() || self.browser_review.read(cx).open(thread),
        })
    }

    /// What a press in the chrome asked: an action on the pages, an
    /// annotation begun or closed, a capture stopped for a new tab.
    pub(in crate::cockpit) fn on_browser_chrome_event(
        &mut self,
        _: &Entity<BrowserChromeView>,
        event: &BrowserChromeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            BrowserChromeEvent::Act(thread, action) => {
                self.browser_action(*thread, action.clone(), cx)
            }
            BrowserChromeEvent::Annotate { thread, begin: true } => {
                self.begin_browser_annotation(*thread, window, cx)
            }
            BrowserChromeEvent::Annotate { thread, begin: false } => {
                self.close_browser_annotation(*thread, cx)
            }
            BrowserChromeEvent::NewTab(thread) => self.cancel_browser_capture(*thread, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn a_browsers_chrome_keeps_two_machines_threads_apart(cx: &mut gpui::TestAppContext) {
        let here = ThreadKey::fixture(0, 1);
        let there = ThreadKey::fixture(1, 1);
        let chrome = cx.new(|_| BrowserChromeView::default());
        chrome.update(cx, |chrome, cx| {
            chrome.make(here, cx);
            chrome.make(there, cx);
            chrome.follow(here, TabId(1), "http://here.test/", cx);
            chrome.follow(there, TabId(1), "http://there.test/", cx);
            // As its New tab press does.
            chrome.threads.get_mut(&there).unwrap().entry = Entry::NewTab;
        });
        chrome.read_with(cx, |chrome, cx| {
            assert_eq!(chrome.address_for_test(here, cx), "http://here.test/");
            assert_eq!(chrome.address_for_test(there, cx), "http://there.test/");
            assert!(!chrome.new_tab(here) && chrome.new_tab(there));
        });
    }
}

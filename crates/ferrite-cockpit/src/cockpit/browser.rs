//! Window realization of the Server's Thread-owned browser tabs.
//! Native pages belong here, independently of whichever Preview is visible.
use super::*;
use ferrite_core::browser::{Action, Document, Owner, PageState, State, TabId};
use ferrite_pane::composer::{Composer, Role};
use ferrite_pane::visual::{
    element::WebView,
    engine::{Availability, Visuals},
    web::{BrowserState, BrowserView, BrowserViewport, Input},
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

gpui::actions!(browser, [Go]);

#[derive(Default)]
pub(super) struct Browsers {
    threads: BTreeMap<ThreadId, Browser>,
    events: Rc<RefCell<Vec<Event>>>,
}
struct Browser {
    state: State,
    owner: Option<Owner>,
    pages: BTreeMap<TabId, Page>,
    address: Entity<Composer>,
    address_shown: Option<(TabId, String)>,
    error: Option<String>,
    new_tab: bool,
    failed: BTreeMap<TabId, Document>,
}
struct Page {
    web: Entity<WebView>,
    native: Rc<dyn BrowserView>,
    document: Rc<Cell<Document>>,
    suppress_navigation: Rc<Cell<bool>>,
}
enum Event {
    Input(ThreadId, Owner, Document, Option<u64>, Input),
    Changed(ThreadId, Owner, PageState),
}

impl CockpitView {
    pub(super) fn ensure_browser(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.browsers
            .threads
            .entry(thread)
            .or_insert_with(|| Browser {
                state: State::default(),
                owner: None,
                pages: BTreeMap::new(),
                address: cx.new(|cx| {
                    let mut field = Composer::new(cx);
                    field.set_role(Role::Field(Some("BrowserAddress")), cx);
                    field.set_visible_row_limit(1, cx);
                    field
                }),
                address_shown: None,
                error: None,
                new_tab: false,
                failed: BTreeMap::new(),
            });
    }

    pub(super) fn open_browser(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.ensure_browser(thread, cx);
        match self.cockpit.browser_attach(thread) {
            Ok(state) => {
                let active = state.active;
                let browser = self.browsers.threads.get_mut(&thread).unwrap();
                browser.owner = state.owner;
                browser.state = state;
                browser.error = None;
                if let Some(tab) = active {
                    self.browser_action(thread, Action::Select { tab }, cx);
                }
            }
            Err(error) => {
                self.browsers.threads.get_mut(&thread).unwrap().error = Some(error.to_string())
            }
        }
        if let Some(index) = self.pane_for(thread) {
            self.panes[index].preview.show_browser();
        }
        cx.notify();
    }

    pub(super) fn browser_action(
        &mut self,
        thread: ThreadId,
        action: Action,
        cx: &mut Context<Self>,
    ) {
        self.ensure_browser(thread, cx);
        if matches!(action, Action::Open { .. }) {
            if let Some(index) = self.pane_for(thread) {
                self.panes[index].preview.show_browser();
            }
        }
        if self.browsers.threads[&thread].owner.is_none() {
            match self.cockpit.browser_attach(thread) {
                Ok(state) => {
                    let browser = self.browsers.threads.get_mut(&thread).unwrap();
                    browser.owner = state.owner;
                    browser.state = state;
                }
                Err(error) => {
                    self.browsers.threads.get_mut(&thread).unwrap().error = Some(error.to_string());
                    cx.notify();
                    return;
                }
            }
        }
        match self.cockpit.browser(thread, action) {
            Ok(state) => {
                let browser = self.browsers.threads.get_mut(&thread).unwrap();
                browser.state = state;
                browser.error = None;
                browser.new_tab = false;
                browser.sync_documents(cx, false);
            }
            Err(error) => {
                self.browsers.threads.get_mut(&thread).unwrap().error = Some(error.to_string())
            }
        }
        cx.notify();
    }

    pub(super) fn pump_browsers(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.cockpit.server(),
            Some(ferrite_core::cockpit::ServerState::Detached { .. })
        ) {
            for browser in self.browsers.threads.values_mut() {
                browser.pages.clear();
                browser.owner = None;
            }
        }
        let requests: Vec<_> = self
            .panes
            .iter()
            .flat_map(|pane| {
                let thread = pane.thread();
                pane.preview
                    .take_browser_urls()
                    .into_iter()
                    .map(move |url| (thread, url))
            })
            .collect();
        for (thread, url) in requests {
            if let Some(thread) = thread {
                self.browser_action(thread, Action::Open { url }, cx);
            }
        }
        for message in self.cockpit.take_browser_messages() {
            let ferrite_core::protocol::ServerMessage::Browser {
                thread,
                owner,
                state,
                action,
            } = message
            else {
                continue;
            };
            self.ensure_browser(thread, cx);
            if matches!(action, Some(Action::Open { .. } | Action::Select { .. })) {
                if let Some(index) = self.pane_for(thread) {
                    self.panes[index].preview.show_browser();
                }
            }
            let browser = self.browsers.threads.get_mut(&thread).unwrap();
            browser.state = state;
            browser.sync_documents(cx, action.is_none());
            if browser.owner != owner {
                browser.pages.clear();
                continue;
            }
            browser.pages.retain(|id, _| {
                browser
                    .state
                    .tabs
                    .iter()
                    .any(|tab| tab.id == *id && tab.live)
            });
            // Navigation commands operate on the exact existing view. The Server
            // has already validated the Thread, owner and previous generation.
            if let Some(action) = action {
                let command = match action {
                    Action::Navigate { document, url } => Some((document.tab, Some(url), 0)),
                    Action::Back { document } => Some((document.tab, None, 1)),
                    Action::Forward { document } => Some((document.tab, None, 2)),
                    Action::Reload { document } => Some((document.tab, None, 3)),
                    _ => None,
                };
                if let Some((id, url, kind)) = command {
                    if let Some(page) = browser.pages.get(&id) {
                        if let Some(tab) = browser.state.tabs.iter().find(|tab| tab.id == id) {
                            page.document.set(tab.document());
                        }
                        page.suppress_navigation.set(true);
                        let result = match kind {
                            0 => page.native.navigate(url.as_deref().unwrap()),
                            1 => page.native.back(),
                            2 => page.native.forward(),
                            _ => page.native.reload(),
                        };
                        if let Err(error) = result {
                            browser.error = Some(error);
                        }
                    }
                }
            }
            cx.notify();
        }
        let events = std::mem::take(&mut *self.browsers.events.borrow_mut());
        for event in events {
            match event {
                Event::Input(thread, owner, document, generation, input) => {
                    let Some(browser) = self
                        .browsers
                        .threads
                        .get(&thread)
                        .filter(|browser| browser.owner == Some(owner))
                    else {
                        continue;
                    };
                    let Some(page) = browser.pages.get(&document.tab).filter(|page| {
                        page.document.get() == document
                            && generation == Some(page.native.state().document_generation)
                    }) else {
                        continue;
                    };
                    match self.cockpit.browser_input(thread, owner) {
                        Ok(_) => page.native.input(input),
                        Err(error) => {
                            self.browsers.threads.get_mut(&thread).unwrap().error =
                                Some(error.to_string());
                            cx.notify();
                        }
                    }
                }
                Event::Changed(thread, owner, page) => {
                    self.cockpit.browser_page(thread, owner, page)
                }
            }
        }
        // Only the selected live tab is realized. Restored metadata does not
        // start Chromium until an explicit selection or open marks it live.
        let events = self.browsers.events.clone();
        for (thread, browser) in &mut self.browsers.threads {
            let Some(owner) = browser
                .owner
                .filter(|owner| Some(*owner) == browser.state.owner)
            else {
                continue;
            };
            let Some(tab) = browser
                .state
                .tabs
                .iter()
                .find(|tab| Some(tab.id) == browser.state.active)
            else {
                continue;
            };
            let shown = (tab.id, tab.url.clone());
            if !browser.new_tab && browser.address_shown.as_ref() != Some(&shown) {
                browser
                    .address
                    .update(cx, |field, cx| field.set(tab.url.clone(), cx));
                browser.address_shown = Some(shown);
            }
            if !tab.live
                || browser.pages.contains_key(&tab.id)
                || browser.failed.get(&tab.id) == Some(&tab.document())
            {
                continue;
            }
            let engine = match Visuals::engine(cx) {
                Availability::Ready(engine) => engine,
                Availability::Starting => continue,
                Availability::Unavailable(why) => {
                    let error = why.to_string();
                    browser.failed.insert(tab.id, tab.document());
                    events.borrow_mut().push(Event::Changed(
                        *thread,
                        owner,
                        failed_page(tab, error.clone()),
                    ));
                    browser.error = Some(error);
                    continue;
                }
            };
            let document = Rc::new(Cell::new(tab.document()));
            let thread = *thread;
            let id = tab.id;
            let input_events = events.clone();
            let changed_events = events.clone();
            let changed_document = document.clone();
            let last = RefCell::new(None);
            let native_generation = Rc::new(Cell::new(None));
            let input_generation = native_generation.clone();
            let input_document = document.clone();
            let suppress_navigation = Rc::new(Cell::new(false));
            let suppress = suppress_navigation.clone();
            let web = WebView::browser(
                engine,
                &tab.url,
                BrowserViewport {
                    width: 800,
                    height: 600,
                    scale: 1.,
                },
                Rc::new(move |input| {
                    input_events.borrow_mut().push(Event::Input(
                        thread,
                        owner,
                        input_document.get(),
                        input_generation.get(),
                        input,
                    ))
                }),
                Rc::new(move |state: &BrowserState| {
                    let changed_generation = native_generation
                        .replace(Some(state.document_generation))
                        .is_some_and(|generation| generation != state.document_generation);
                    let navigation = changed_generation && !suppress.replace(false);
                    let page = PageState {
                        navigation,
                        document: changed_document.get(),
                        url: state.url.clone(),
                        loading: state.loading,
                        can_back: state.can_back,
                        can_forward: state.can_forward,
                        error: state.error.clone(),
                        dirty: match state.dirty {
                            ferrite_pane::visual::web::BrowserDirty::Clean => {
                                ferrite_core::browser::Dirty::Clean
                            }
                            ferrite_pane::visual::web::BrowserDirty::Dirty => {
                                ferrite_core::browser::Dirty::Dirty
                            }
                            ferrite_pane::visual::web::BrowserDirty::Unknown => {
                                ferrite_core::browser::Dirty::Unknown
                            }
                        },
                    };
                    if last.borrow().as_ref() != Some(&page) {
                        *last.borrow_mut() = Some(page.clone());
                        changed_events
                            .borrow_mut()
                            .push(Event::Changed(thread, owner, page));
                    }
                }),
                cx,
            );
            match web {
                Ok(web) => {
                    let native = web
                        .read(cx)
                        .browser_handle()
                        .expect("browser host has its native page");
                    browser.pages.insert(
                        id,
                        Page {
                            web,
                            native,
                            document,
                            suppress_navigation,
                        },
                    );
                    browser.error = None;
                }
                Err(error) => {
                    browser.failed.insert(tab.id, tab.document());
                    events.borrow_mut().push(Event::Changed(
                        thread,
                        owner,
                        failed_page(tab, error.clone()),
                    ));
                    browser.error = Some(error);
                }
            }
        }
    }

    fn browser_go(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(browser) = self.browsers.threads.get(&thread) else {
            return;
        };
        let url = browser.address.read(cx).text().trim().to_owned();
        let document = browser
            .state
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == browser.state.active)
            .filter(|_| !browser.new_tab)
            .map(|tab| tab.document());
        self.browser_action(
            thread,
            match document {
                Some(document) => Action::Navigate { document, url },
                None => Action::Open { url },
            },
            cx,
        );
    }

    pub(super) fn browser_body(
        &self,
        thread: Option<ThreadId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::{icons, theme::*};
        let Some(thread) = thread else {
            return div()
                .p(px(16.))
                .child("Send a prompt to create a Thread before opening its browser.")
                .into_any_element();
        };
        let Some(browser) = self.browsers.threads.get(&thread) else {
            return div()
                .p(px(16.))
                .child("Enter a website address to browse here.")
                .into_any_element();
        };
        let active = browser
            .state
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == browser.state.active);
        let tabs = browser.state.tabs.iter().map(|tab| {
            let id = tab.id;
            let title = url::Url::parse(&tab.url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_else(|| tab.url.clone());
            div()
                .id(("browser-tab", id.0))
                .flex()
                .items_center()
                .min_w_0()
                .max_w(px(190.))
                .px(px(8.))
                .py(px(4.))
                .when(Some(id) == browser.state.active, |tab| tab.bg(paint::CARD))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(title),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.browser_action(thread, Action::Select { tab: id }, cx)
                }))
                .child(
                    ferrite_pane::preview::head_door(
                        "browser-close-tab",
                        format!("close-tab-{}", id.0),
                        icons::CLOSE,
                    )
                    .tooltip(ferrite_ui::menu::tooltip("Close tab"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.browser_action(thread, Action::Close { tab: id }, cx);
                    })),
                )
        });
        let mut toolbar = div().flex().items_center().gap(px(4.)).p(px(4.));
        for (id, glyph, label, enabled, kind) in [
            (
                "browser-back",
                icons::BACK,
                "Back",
                active.is_some_and(|tab| tab.can_back),
                0,
            ),
            (
                "browser-forward",
                icons::FORWARD,
                "Forward",
                active.is_some_and(|tab| tab.can_forward),
                1,
            ),
            (
                "browser-reload",
                icons::REFRESH,
                "Reload",
                active.is_some(),
                2,
            ),
        ] {
            let document = active.map(|tab| tab.document());
            toolbar = toolbar.child(
                ferrite_pane::preview::head_door(id, id.into(), glyph)
                    .opacity(if enabled { 1. } else { 0.35 })
                    .tooltip(ferrite_ui::menu::tooltip(label))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !enabled {
                            return;
                        }
                        let Some(document) = document else { return };
                        this.browser_action(
                            thread,
                            match kind {
                                0 => Action::Back { document },
                                1 => Action::Forward { document },
                                _ => Action::Reload { document },
                            },
                            cx,
                        );
                    })),
            );
        }
        toolbar = toolbar
            .child(div().px(px(4.)).child("Address"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(browser.address.clone())
                    .on_action(cx.listener(move |this, _: &Go, _, cx| this.browser_go(thread, cx))),
            )
            .child(
                ferrite_pane::preview::head_door("browser-go", "browser-go".into(), icons::FORWARD)
                    .tooltip(ferrite_ui::menu::tooltip("Go to address"))
                    .on_click(cx.listener(move |this, _, _, cx| this.browser_go(thread, cx))),
            );
        let body = active
            .filter(|_| !browser.new_tab)
            .and_then(|tab| browser.pages.get(&tab.id))
            .map(|page| page.web.clone());
        div()
            .debug_selector(|| "browser-preview".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .id("browser-tabs")
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .overflow_x_scroll()
                    .children(tabs)
                    .child(
                        ferrite_pane::preview::head_door(
                            "browser-new-tab",
                            "browser-new-tab".into(),
                            icons::PLUS,
                        )
                        .tooltip(ferrite_ui::menu::tooltip("New tab"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                if let Some(browser) = this.browsers.threads.get_mut(&thread) {
                                    browser.new_tab = true;
                                    browser
                                        .address
                                        .update(cx, |field, cx| field.set(String::new(), cx));
                                    let focus = browser.address.read(cx).focus_target(window, cx);
                                    window.focus(&focus, cx);
                                    cx.notify();
                                }
                            },
                        )),
                    ),
            )
            .child(toolbar)
            .when(active.is_some_and(|tab| tab.loading), |body| {
                body.child(
                    div()
                        .px(px(8.))
                        .text_color(rgb(INK_SUBTLE))
                        .child("Loading…"),
                )
            })
            .when_some(
                browser
                    .error
                    .as_ref()
                    .or_else(|| active.and_then(|tab| tab.error.as_ref())),
                |body, error| {
                    body.child(div().px(px(8.)).py(px(4.)).child(format!(
                        "{error} · Edit the address or reload to try again."
                    )))
                },
            )
            .when_some(body, |root, body| {
                root.child(div().flex_1().min_h_0().child(body))
            })
            .into_any_element()
    }
}

impl Browser {
    fn sync_documents(&self, cx: &mut Context<CockpitView>, resample: bool) {
        for tab in &self.state.tabs {
            if let Some(page) = self.pages.get(&tab.id) {
                if page.document.replace(tab.document()) != tab.document() && resample {
                    page.web.update(cx, |web, cx| web.refresh_browser_state(cx));
                }
            }
        }
    }
}

fn failed_page(tab: &ferrite_core::browser::Tab, error: String) -> PageState {
    PageState {
        document: tab.document(),
        navigation: false,
        url: tab.url.clone(),
        loading: false,
        can_back: tab.can_back,
        can_forward: tab.can_forward,
        error: Some(error),
        dirty: ferrite_core::browser::Dirty::Unknown,
    }
}

//! Window realization of the Server's Thread-owned browser tabs.
//! Native pages belong here, independently of whichever Preview is visible.
mod annotations;
mod chrome;
pub(super) use chrome::BrowserChromeView;
mod controls;
mod dialog;
mod live_annotation;
use live_annotation::LiveAnnotation;
pub(super) use dialog::BrowserDialogView;
use controls::ControlRequest;
pub(super) use controls::Controls;
mod manual;
mod page_menu;
mod review;
pub(super) use review::BrowserReviewView;

pub(super) use page_menu::PageVerb;

use super::*;
use ferrite_core::browser::{Action, Document, Owner, PageState, State, Tab, TabId};
use ferrite_pane::composer::{Composer, Role};
use ferrite_pane::visual::{
    element::WebView,
    engine::{Availability, Visuals},
    web::{
        BrowserState, BrowserView, BrowserViewport, DialogAnswer, DialogKind, DialogRequest,
        Input, PageContext,
    },
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

gpui::actions!(
    browser,
    [Go, CancelAnnotation, CancelAddress, AcceptDialog, DismissDialog]
);

#[derive(Default)]
pub(super) struct Browsers {
    pub(super) threads: BTreeMap<ThreadId, Browser>,
    events: Rc<RefCell<Vec<Event>>>,
    /// Threads whose browser asked to be shown, with their Panel's
    /// operator revision when it asked: an agent's page may reveal the
    /// Browser tab, not take the Panel from a tab chosen since.
    reveal: BTreeMap<ThreadId, u64>,
    /// The revision stamped on each of this window's own Opens and Selects,
    /// oldest first, until the Server's echo of it arrives: the echo reveals
    /// with the stamp of its action, not of the frame it is read in, or it
    /// would outrank a tab the operator chose in between.
    sent: BTreeMap<ThreadId, std::collections::VecDeque<u64>>,
}

pub(super) struct Browser {
    state: State,
    owner: Option<Owner>,
    pages: BTreeMap<TabId, Page>,
    error: Option<String>,
    /// The focus pump's: its dialog's card (`BrowserDialogView`) was in the
    /// last frame drawn, so it can take focus.
    dialog_drawn: Cell<bool>,
    /// `dialog_drawn` as of the last frame: only a dialog on screen holds
    /// the keyboard.
    dialog_on_screen: bool,
    /// The right-click the page reported, and where the pointer was. The
    /// menu is the popup slot's, so these stay the root's (ADR 0041, What
    /// does not get a view).
    context: Option<PageContext>,
    pointer: Rc<Cell<gpui::Point<gpui::Pixels>>>,
    failed: BTreeMap<TabId, Document>,
    annotation: Option<LiveAnnotation>,
    native_peers: Rc<RefCell<manual::Peers>>,
}
struct Page {
    web: Entity<WebView>,
    native: Rc<dyn BrowserView>,
    document: Rc<Cell<Document>>,
    acknowledged: Cell<Option<(Document, u64)>>,
    pending_navigation: RefCell<Option<PendingNavigation>>,
    manual: Rc<RefCell<manual::Manual>>,
}
impl Browsers {
    /// Whether any Browser has a live tab or a page open: the pump keeps
    /// its beat for it, since a page's events, the engine's start and a
    /// control's answer do not ring (ADR 0041).
    pub(super) fn live(&self) -> bool {
        self.threads.values().any(|browser| {
            !browser.pages.is_empty() || browser.state.tabs.iter().any(|tab| tab.live)
        })
    }

    /// Every Thread `keep` refuses takes its browser with it: its pages
    /// close here, as when the Server detaches; the Server deleted its side
    /// with the Thread.
    pub(super) fn retain_threads(&mut self, keep: impl Fn(ThreadId) -> bool) {
        self.threads.retain(|thread, _| keep(*thread));
        self.reveal.retain(|thread, _| keep(*thread));
        self.sent.retain(|thread, _| keep(*thread));
    }

    #[cfg(test)]
    pub(super) fn held_for_test(&self, thread: ThreadId) -> usize {
        usize::from(self.threads.contains_key(&thread))
            + usize::from(self.reveal.contains_key(&thread))
            + usize::from(self.sent.contains_key(&thread))
    }

    /// Rechecked at queue dispatch and after native attachment becomes Ready.
    /// The latter additionally checks the exact native document and epoch.
    fn control_page(&self, request: &ControlRequest) -> Option<&Page> {
        if !request.control.supported() {
            return None;
        }
        let browser = self.threads.get(&request.thread)?;
        if browser.owner != Some(request.owner)
            || !browser.pages.values().all(|page| {
                let manual = page.manual.borrow();
                manual.revision <= request.revision
                    && (request.control.observation() || !manual.active)
            })
        {
            return None;
        }
        browser.pages.get(&request.document.tab).filter(|page| {
            let manual = page.manual.borrow();
            page.document.get() == request.document
                && page.pending_navigation.borrow().is_none()
                && (!matches!(request.control, ferrite_core::browser::Control::Screenshot)
                    || !page.native.state().loading)
                && manual.revision <= request.revision
                && (request.control.observation() || !manual.active)
        })
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        // Running work retains its local protection handle. Fence the native
        // attachment before that handle can outlive the owning page/window.
        self.manual.borrow_mut().cancel();
        self.manual.borrow_mut().capture_pause(false);
        self.native.set_agent_paused(true);
    }
}
struct PendingNavigation {
    generation: u64,
    previous_url: String,
    destination: Option<String>,
    history: bool,
}
impl PendingNavigation {
    fn settled(&self, state: &BrowserState) -> bool {
        if state.document_generation != self.generation || state.error.is_some() {
            return true;
        }
        if state.loading {
            return false;
        }
        // Fragment/history navigation can settle without a new document. A
        // different earlier intent settling must not complete this one.
        match &self.destination {
            Some(destination) => url::Url::parse(destination).is_ok_and(|destination| {
                url::Url::parse(&state.url).is_ok_and(|url| url == destination)
            }),
            None => self.history && state.url != self.previous_url,
        }
    }
}

enum Event {
    Restore(ThreadId, TabId),
    Input(ThreadId, Owner, Document, Option<u64>, u64, Input),
    SettledInput(ThreadId, Owner, Document, u64),
    Changed(ThreadId, Owner, PageState, Option<u64>),
}

impl CockpitView {
    pub(super) fn ensure_browser(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.browser_chrome
            .update(cx, |chrome, cx| chrome.make(thread, cx));
        self.browser_dialog
            .update(cx, |dialog, cx| dialog.make(thread, cx));
        self.browsers
            .threads
            .entry(thread)
            .or_insert_with(|| Browser {
                state: State::default(),
                owner: None,
                pages: BTreeMap::new(),
                error: None,
                dialog_drawn: Cell::new(false),
                dialog_on_screen: false,
                context: None,
                pointer: Default::default(),
                failed: BTreeMap::new(),
                annotation: None,
                native_peers: Default::default(),
            });
    }

    pub(super) fn open_browser(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.ensure_browser(thread, cx);
        match self.cockpit.update(cx, |cockpit, _| cockpit.browser_attach(thread)) {
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
        self.upsert_surface(thread, super::surfaces::SurfaceId::Browser, cx);
    }

    pub(super) fn browser_action(
        &mut self,
        thread: ThreadId,
        action: Action,
        cx: &mut Context<Self>,
    ) {
        if self.defer_annotation_action(thread, action.clone(), cx) { return; }
        self.cancel_browser_capture(thread, cx);
        self.ensure_browser(thread, cx);
        let seen = self.surface_revision(thread);
        // Only an action the Server accepts is echoed back to claim this revision.
        let echoed = matches!(action, Action::Open { .. } | Action::Select { .. });
        if matches!(action, Action::Open { .. }) {
            self.browsers.reveal.entry(thread).or_insert(seen);
        }
        if self.browsers.threads[&thread].owner.is_none() {
            match self.cockpit.update(cx, |cockpit, _| cockpit.browser_attach(thread)) {
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
        let recovery_tab = match &action {
            Action::Close { tab } => Some(*tab),
            Action::Reload { document } => Some(document.tab),
            _ => None,
        };
        let recovery = recovery_tab.and_then(|tab| {
            self.browsers
                .threads
                .get(&thread)
                .and_then(|browser| browser.pages.get(&tab))
                .map(|page| page.manual.clone())
        });
        if let Some(manual) = &recovery {
            let settled = manual.borrow_mut().retire_for_action();
            if let (Some((document, revision)), Some(owner)) =
                (settled, self.browsers.threads[&thread].owner)
            {
                let _ = self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.browser_settled(thread, owner, document, revision));
            }
        }
        let result = self.cockpit.update(cx, |cockpit, _| cockpit.browser(thread, action));
        if let Some(manual) = &recovery {
            manual.borrow().restore_pause();
        }
        match result {
            Ok(state) => {
                if echoed {
                    self.browsers.sent.entry(thread).or_default().push_back(seen);
                }
                let page = state.active.is_some();
                let browser = self.browsers.threads.get_mut(&thread).unwrap();
                browser.state = state;
                browser.error = None;
                browser.sync_documents(cx, false);
                self.browser_chrome
                    .update(cx, |chrome, cx| chrome.landed(thread, page, cx));
            }
            Err(error) => {
                self.browsers.threads.get_mut(&thread).unwrap().error = Some(error.to_string())
            }
        }
        cx.notify();
    }

    pub(super) fn pump_browsers(&mut self, cx: &mut Context<Self>) {
        self.pump_browser_annotations(cx);
        if matches!(
            self.cockpit.read(cx).server(),
            Some(ferrite_core::cockpit::ServerState::Detached { .. })
        ) {
            for browser in self.browsers.threads.values_mut() {
                browser.pages.clear();
                browser.owner = None;
                browser.state.owner = None;
                for tab in &mut browser.state.tabs {
                    tab.live = false;
                    tab.loading = false;
                }
                browser.error =
                    Some("Browser disconnected. Reconnect Ferrite to resume this page.".into());
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
        self.apply_browser_messages(cx);
        self.drain_browser_events(cx);
        for (thread, seen) in std::mem::take(&mut self.browsers.reveal) {
            match self.pane_for(thread) {
                Some(index) => {
                    if self.reveal_surface(thread, super::surfaces::SurfaceId::Browser, seen) {
                        self.panes[index].preview.show_browser();
                    }
                }
                None => {
                    self.browsers.reveal.insert(thread, seen);
                }
            }
        }
        self.serve_page_requests(cx);
        self.realize_browser_pages(cx);
        self.tend_browser_pages(cx);
        self.cockpit.update(cx, |cockpit, cx| {
            self.browser_controls.pump(&self.browsers, cockpit, cx)
        });
    }

    /// The Server's word since the last frame: a control a provider asks
    /// for, and each Browser's tabs and owner as they now stand, with the
    /// navigation it asked of a page.
    fn apply_browser_messages(&mut self, cx: &mut Context<Self>) {
        for message in self.cockpit.update(cx, |cockpit, _| cockpit.take_browser_messages()) {
            let message = match message {
                ferrite_core::protocol::ServerMessage::BrowserRequest {
                    request,
                    thread,
                    owner,
                    document,
                    revision,
                    control,
                    dispatch,
                } => {
                    self.browser_controls.push(ControlRequest {
                        request,
                        thread,
                        owner,
                        document,
                        revision,
                        control,
                        dispatch,
                    });
                    continue;
                }
                other => other,
            };
            let ferrite_core::protocol::ServerMessage::Browser {
                thread,
                owner,
                state,
                action,
                owned,
            } = message
            else {
                continue;
            };
            self.ensure_browser(thread, cx);
            if matches!(action, Some(Action::Open { .. } | Action::Select { .. })) {
                let seen = self
                    .browsers
                    .sent
                    .get_mut(&thread)
                    .and_then(std::collections::VecDeque::pop_front)
                    .unwrap_or_else(|| self.surface_revision(thread));
                self.browsers.reveal.entry(thread).or_insert(seen);
            }
            let browser = self.browsers.threads.get_mut(&thread).unwrap();
            browser.state = state;
            if !owned
                || browser
                    .owner
                    .is_some_and(|previous| Some(previous) != owner)
            {
                browser.pages.clear();
            }
            browser.owner = if owned { owner } else { None };
            if !owned {
                continue;
            }
            browser.sync_documents(cx, action.is_none());
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
                let document = match &action {
                    Action::Navigate { document, .. }
                    | Action::Back { document }
                    | Action::Forward { document }
                    | Action::Reload { document } => Some(*document),
                    _ => None,
                };
                if let Some(document) = document {
                    if let Some(page) = browser.pages.get(&document.tab) {
                        let before = page.native.state();
                        *page.pending_navigation.borrow_mut() = Some(PendingNavigation {
                            generation: before.document_generation,
                            previous_url: before.url,
                            destination: match &action {
                                Action::Navigate { url, .. } => Some(url.clone()),
                                _ => None,
                            },
                            history: matches!(action, Action::Back { .. } | Action::Forward { .. }),
                        });
                        let result = match action {
                            Action::Navigate { url, .. } => page.native.navigate(&url),
                            Action::Back { .. } => page.native.back(),
                            Action::Forward { .. } => page.native.forward(),
                            Action::Reload { .. } => page.native.reload(),
                            _ => unreachable!("navigation document checked above"),
                        };
                        if result.is_ok() {
                            browser.error = None;
                        }
                        if let Err(error) = result {
                            page.pending_navigation.borrow_mut().take();
                            browser.error = Some(error.clone());
                            if let (Some(owner), Some(tab)) = (
                                browser.owner,
                                browser.state.tabs.iter().find(|tab| tab.id == document.tab),
                            ) {
                                self.browsers.events.borrow_mut().push(Event::Changed(
                                    thread,
                                    owner,
                                    failed_page(tab, error),
                                    Some(page.native.state().document_generation),
                                ));
                            }
                        }
                    }
                }
            }
            cx.notify();
        }
    }

    /// What the pages said since the last frame (`Event`): a restored tab
    /// to select, the operator's input, a page's state for the Server.
    fn drain_browser_events(&mut self, cx: &mut Context<Self>) {
        let mut events = std::mem::take(&mut *self.browsers.events.borrow_mut())
            .into_iter()
            .peekable();
        while let Some(event) = events.next() {
            match event {
                Event::Restore(thread, tab) => {
                    if !matches!(
                        self.cockpit.read(cx).server(),
                        Some(ferrite_core::cockpit::ServerState::Attached { .. })
                    ) {
                        continue;
                    }
                    if self.browsers.threads.get(&thread).is_some_and(|browser| {
                        browser
                            .state
                            .tabs
                            .iter()
                            .any(|page| page.id == tab && !page.live)
                    }) {
                        self.browser_action(thread, Action::Select { tab }, cx);
                    }
                }
                Event::SettledInput(thread, owner, document, revision) => {
                    let _ = self
                        .cockpit
                        .update(cx, |cockpit, _| cockpit.browser_settled(thread, owner, document, revision));
                }
                Event::Input(thread, owner, document, generation, sequence, input) => {
                    let Some(browser) = self
                        .browsers
                        .threads
                        .get(&thread)
                        .filter(|browser| browser.owner == Some(owner))
                    else {
                        continue;
                    };
                    let Some(page) = browser.pages.get(&document.tab) else {
                        continue;
                    };
                    if page.document.get() != document
                        || generation != Some(page.native.state().document_generation)
                        || !page.manual.borrow().valid(sequence)
                    {
                        let settled = page.manual.borrow_mut().discard(sequence, manual::Finish::Stale);
                        if let Some((document, revision)) = settled {
                            let _ = self
                                .cockpit
                                .update(cx, |cockpit, _| cockpit.browser_settled(thread, owner, document, revision));
                        }
                        continue;
                    }
                    match self.cockpit.update(cx, |cockpit, _| cockpit.browser_input(thread, owner, document)) {
                        Ok(revision) => {
                            page.manual
                                .borrow_mut()
                                .acknowledge(sequence, document, revision);
                            page.native.input(input);
                        }
                        Err(error) => {
                            let settled = page.manual.borrow_mut().discard(sequence, manual::Finish::Refused);
                            if let Some((document, revision)) = settled {
                                let _ = self
                                    .cockpit
                                    .update(cx, |cockpit, _| cockpit.browser_settled(thread, owner, document, revision));
                            }
                            self.browsers.threads.get_mut(&thread).unwrap().error =
                                Some(error.to_string());
                            cx.notify();
                        }
                    }
                }
                Event::Changed(thread, owner, mut page, mut generation) => {
                    while let Some(Event::Changed(next_thread, next_owner, next_page, _)) =
                        events.peek()
                    {
                        if *next_thread != thread
                            || *next_owner != owner
                            || next_page.document != page.document
                        {
                            break;
                        }
                        let Some(Event::Changed(_, _, mut next, next_generation)) = events.next()
                        else {
                            unreachable!()
                        };
                        next.navigation |= page.navigation;
                        page = next;
                        generation = next_generation;
                    }
                    let Some(browser) = self
                        .browsers
                        .threads
                        .get(&thread)
                        .filter(|browser| browser.owner == Some(owner))
                    else {
                        continue;
                    };
                    if !browser
                        .state
                        .tabs
                        .iter()
                        .any(|tab| tab.document() == page.document)
                    {
                        continue;
                    }
                    if let Some(native) = browser.pages.get(&page.document.tab) {
                        let current = native.native.state();
                        if generation != Some(current.document_generation) {
                            native
                                .web
                                .update(cx, |web, cx| web.refresh_browser_state(cx));
                            continue;
                        }
                        if native
                            .pending_navigation
                            .borrow()
                            .as_ref()
                            .is_some_and(|pending| !pending.settled(&current))
                        {
                            // Retain the service's loading/Unknown state while
                            // the old native document is still on screen.
                            continue;
                        }
                        if native.pending_navigation.borrow().is_some() {
                            page.url = current.url;
                            page.loading = current.loading;
                            page.can_back = current.can_back;
                            page.can_forward = current.can_forward;
                            page.error = current.error;
                            page.dirty = match current.dirty {
                                ferrite_pane::visual::web::BrowserDirty::Clean => {
                                    ferrite_core::browser::Dirty::Clean
                                }
                                ferrite_pane::visual::web::BrowserDirty::Dirty => {
                                    ferrite_core::browser::Dirty::Dirty
                                }
                                ferrite_pane::visual::web::BrowserDirty::Unknown => {
                                    ferrite_core::browser::Dirty::Unknown
                                }
                            };
                            native.pending_navigation.borrow_mut().take();
                        }
                    }
                    page.visible = browser
                        .pages
                        .get(&page.document.tab)
                        .is_some_and(|page| page.web.read(cx).browser_visible());
                    let tab_id = page.document.tab;
                    match self.cockpit.update(cx, |cockpit, _| cockpit.browser_page(thread, owner, page)) {
                        Ok(state) => {
                            let browser = self.browsers.threads.get_mut(&thread).unwrap();
                            browser.state = state;
                            if let (Some(generation), Some(page), Some(tab)) = (
                                generation,
                                browser.pages.get(&tab_id),
                                browser.state.tabs.iter().find(|tab| tab.id == tab_id),
                            ) {
                                page.acknowledged.set(Some((tab.document(), generation)));
                            }
                            browser.sync_documents(cx, true);
                        }
                        Err(error) => {
                            self.browsers.threads.get_mut(&thread).unwrap().error =
                                Some(error.to_string())
                        }
                    }
                }
            }
        }
    }

    /// Only the selected live tab is realized. Restored metadata does not
    /// start Chromium until an explicit selection or open marks it live.
    /// The chrome's address field follows each active page as it goes.
    fn realize_browser_pages(&mut self, cx: &mut Context<Self>) {
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
            self.browser_chrome
                .update(cx, |chrome, cx| chrome.follow(*thread, tab.id, &tab.url, cx));
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
                        None,
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
            let manual = Rc::new(RefCell::new(manual::Manual::default()));
            let input_manual = manual.clone();
            let web = WebView::browser(
                engine,
                &tab.url,
                BrowserViewport {
                    width: 800,
                    height: 600,
                    scale: 1.,
                },
                Rc::new(move |input| {
                    if matches!(input, Input::Leave) {
                        // Lifecycle Leave cancels, never begins, intervention.
                        let settled = input_manual.borrow_mut().finish(manual::Finish::Leave);
                        if let Some((document, revision)) = settled {
                            input_events
                                .borrow_mut()
                                .push(Event::SettledInput(thread, owner, document, revision));
                        }
                    } else if let Some(sequence) = input_manual.borrow_mut().capture(&input) {
                        input_events.borrow_mut().push(Event::Input(
                            thread,
                            owner,
                            input_document.get(),
                            input_generation.get(),
                            sequence,
                            input,
                        ));
                    }
                }),
                Rc::new(move |state: &BrowserState| {
                    let changed_generation = native_generation
                        .replace(Some(state.document_generation))
                        .is_some_and(|generation| generation != state.document_generation);
                    // Every observed native commit invalidates prior page
                    // references, including a commit after a service intent.
                    // Extra generations are safer than hiding a link navigation.
                    let navigation = changed_generation;
                    let page = PageState {
                        visible: false,
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
                        changed_events.borrow_mut().push(Event::Changed(
                            thread,
                            owner,
                            page,
                            Some(state.document_generation),
                        ));
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
                    manual
                        .borrow_mut()
                        .join(browser.native_peers.clone(), id.0, native.clone());
                    manual.borrow_mut().server_protection(tab.protected);
                    let pending_navigation =
                        (native.state().document_generation == 0).then(|| PendingNavigation {
                            generation: 0,
                            previous_url: String::new(),
                            destination: Some(tab.url.clone()),
                            history: false,
                        });
                    browser.pages.insert(
                        id,
                        Page {
                            web,
                            native,
                            document,
                            acknowledged: Cell::new(None),
                            pending_navigation: RefCell::new(pending_navigation),
                            manual,
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
                        None,
                    ));
                    browser.error = Some(error);
                }
            }
        }
    }

    /// What websites asked of the window since the last frame: pages to open,
    /// a right-click to answer, a script dialog to show.
    fn serve_page_requests(&mut self, cx: &mut Context<Self>) {
        let mut windows = Vec::new();
        let mut menus = Vec::new();
        for (thread, browser) in &mut self.browsers.threads {
            let new_tab = self.browser_chrome.read(cx).new_tab(*thread);
            let shown = browser.shown_tab(new_tab).map(|tab| tab.id);
            for (id, page) in &browser.pages {
                windows.extend(
                    page.native
                        .take_new_windows()
                        .into_iter()
                        .map(|url| (*thread, url)),
                );
                let context = page.native.take_context();
                if Some(*id) != shown {
                    continue;
                }
                if let Some(context) = context {
                    menus.push((*thread, browser.pointer.get()));
                    browser.context = Some(context);
                }
            }
        }
        let threads: Vec<_> = self.browsers.threads.keys().copied().collect();
        for thread in threads {
            let asked = self.browser_dialog_inputs(thread, cx);
            self.browser_dialog
                .update(cx, |dialog, cx| dialog.sync(thread, asked, cx));
        }
        for (thread, url) in windows {
            let new_tab = self.browser_chrome.read(cx).new_tab(thread);
            self.browser_action(thread, Action::Open { url }, cx);
            if new_tab {
                self.browser_chrome
                    .update(cx, |chrome, _| chrome.resume_new_tab(thread));
            }
        }
        for (thread, at) in menus {
            self.open_context_menu(MenuTarget::BrowserPage(thread), at, cx);
        }
    }

    fn finish_browser_interaction(
        &mut self,
        thread: ThreadId,
        owner: Owner,
        tab: TabId,
        reason: manual::Finish,
        cx: &mut gpui::App,
    ) {
        let Some(browser) = self
            .browsers
            .threads
            .get(&thread)
            .filter(|browser| browser.owner == Some(owner))
        else {
            return;
        };
        let Some(page) = browser.pages.get(&tab) else {
            return;
        };
        let settled = page.manual.borrow_mut().finish(reason);
        if let Some((document, revision)) = settled {
            let _ = self
                .cockpit
                .update(cx, |cockpit, _| cockpit.browser_settled(thread, owner, document, revision));
        }
    }

    fn tend_browser_pages(&mut self, cx: &mut Context<Self>) {
        let mut settled = Vec::new();
        for (thread, browser) in &self.browsers.threads {
            let Some(owner) = browser.owner else {
                continue;
            };
            for (tab, page) in &browser.pages {
                let manual = page.manual.borrow();
                if !manual.active {
                    continue;
                }
                let reason = if !self.cockpit.read(cx).window_active() {
                    manual::Finish::WindowInactive
                } else if !page.web.read(cx).browser_visible() {
                    manual::Finish::Hidden
                } else if manual.due() {
                    manual::Finish::Quiet
                } else {
                    continue;
                };
                settled.push((*thread, owner, *tab, reason));
            }
        }
        for (thread, owner, tab, reason) in settled {
            self.finish_browser_interaction(thread, owner, tab, reason, cx);
        }
        // Visibility comes from the actual host, including selected tabs in
        // Threads that are no longer on screen. Selection is not visibility.
        let mut visibility = Vec::new();
        for (thread, browser) in &self.browsers.threads {
            let Some(owner) = browser.owner else { continue };
            for tab in &browser.state.tabs {
                let Some(page) = browser.pages.get(&tab.id) else {
                    continue;
                };
                let visible = page.web.read(cx).browser_visible();
                if tab.visible != visible {
                    visibility.push((
                        *thread,
                        owner,
                        PageState {
                            document: tab.document(),
                            navigation: false,
                            visible,
                            url: tab.url.clone(),
                            loading: tab.loading,
                            can_back: tab.can_back,
                            can_forward: tab.can_forward,
                            error: tab.error.clone(),
                            dirty: tab.dirty,
                        },
                    ));
                }
            }
        }
        for (thread, owner, page) in visibility {
            match self.cockpit.update(cx, |cockpit, _| cockpit.browser_page(thread, owner, page)) {
                Ok(state) => {
                    let browser = self.browsers.threads.get_mut(&thread).unwrap();
                    browser.state = state;
                    browser.sync_documents(cx, true);
                }
                Err(error) => {
                    self.browsers.threads.get_mut(&thread).unwrap().error = Some(error.to_string())
                }
            }
        }
        let focused = self.focused_thread(cx);
        let mut candidates = Vec::new();
        for (thread, browser) in &self.browsers.threads {
            let Some(owner) = browser.owner else { continue };
            for tab in &browser.state.tabs {
                let Some(page) = browser.pages.get(&tab.id) else {
                    continue;
                };
                if tab.live
                    && !tab.visible
                    && !tab.loading
                    && !tab.protected
                    && tab.dirty == ferrite_core::browser::Dirty::Clean
                    && !page.web.read(cx).browser_visible()
                    && page.native.state().dirty == ferrite_pane::visual::web::BrowserDirty::Clean
                    && !(Some(*thread) == focused && Some(tab.id) == browser.state.active)
                {
                    if let Some((document, generation)) =
                        page.acknowledged.get().filter(|(document, generation)| {
                            *document == tab.document()
                                && *generation == page.native.state().document_generation
                        })
                    {
                        candidates.push((
                            *thread,
                            owner,
                            document,
                            generation,
                            page.native.clone(),
                            page.web.clone(),
                        ));
                    }
                }
            }
        }
        // Immediate state-based release is provisional; no unmeasured timer or
        // memory budget is imposed. The Server reserves before native release.
        for (thread, owner, document, generation, native, web) in candidates {
            let lease = match self.cockpit.update(cx, |cockpit, _| cockpit.browser_reclaim(thread, owner, document)) {
                Ok(Some(lease)) => lease,
                Ok(None) => continue,
                Err(error) => {
                    self.browsers.threads.get_mut(&thread).unwrap().error = Some(error.to_string());
                    continue;
                }
            };
            let released = if native.state().document_generation == generation {
                match native.try_release() {
                    Ok(released) => released,
                    Err(error) => {
                        self.browsers.threads.get_mut(&thread).unwrap().error = Some(error);
                        false
                    }
                }
            } else {
                web.update(cx, |web, cx| web.refresh_browser_state(cx));
                false
            };
            match self
                .cockpit
                .update(cx, |cockpit, _| cockpit.browser_reclaimed(thread, owner, lease, released))
            {
                Ok(state) => {
                    let browser = self.browsers.threads.get_mut(&thread).unwrap();
                    browser.state = state;
                    browser.pages.retain(|id, _| {
                        browser
                            .state
                            .tabs
                            .iter()
                            .any(|tab| tab.id == *id && tab.live)
                    });
                    browser.sync_documents(cx, false);
                }
                Err(error) => {
                    self.browsers.threads.get_mut(&thread).unwrap().error = Some(error.to_string())
                }
            }
            cx.notify();
        }
    }

    pub(super) fn browser_body(
        &self,
        thread: Option<ThreadId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let Some(thread) = thread else {
            return div()
                .p(px(16.))
                .child("Send a prompt to create a Thread before opening its browser.")
                .into_any_element();
        };
        let (Some(browser), Some(inputs)) = (
            self.browsers.threads.get(&thread),
            self.browser_chrome_inputs(thread, cx),
        ) else {
            return div()
                .p(px(16.))
                .child("Enter a website address to browse here.")
                .into_any_element();
        };
        if let Some(tab) = browser
            .state
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == browser.state.active && !tab.live)
        {
            self.browsers
                .events
                .borrow_mut()
                .push(Event::Restore(thread, tab.id));
        }
        let chrome = self
            .browser_chrome
            .update(cx, |chrome, cx| chrome.bar(thread, inputs, cx));
        let active = browser.shown_tab(self.browser_chrome.read(cx).new_tab(thread));
        let failure = browser
            .error
            .as_ref()
            .or_else(|| active.and_then(|tab| tab.error.as_ref()));
        let body = active
            .and_then(|tab| browser.pages.get(&tab.id))
            .map(|page| page.web.clone());
        let retry = failure.is_some().then(|| active.map(|tab| tab.document())).flatten();
        let notice = match failure {
            _ if browser.annotation.is_some() || self.browser_review.read(cx).open(thread) => None,
            Some(error) if retry.is_some() => Some(format!("{error}\nEdit the address or retry.")),
            Some(error) => Some(format!("{error}\nEdit the address.")),
            None if body.is_none() => Some("Enter a website address above to browse.".to_owned()),
            None => None,
        };
        let dialog = self
            .browser_dialog
            .update(cx, |dialog, cx| dialog.card(thread, active.is_some(), cx));
        browser.dialog_drawn.set(dialog.is_some());
        let pointer = browser.pointer.clone();
        div()
            .debug_selector(|| "browser-preview".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .children(chrome.strip)
            .child(chrome.toolbar)
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .capture_any_mouse_down(move |event, _, _| {
                        if event.button == gpui::MouseButton::Right {
                            pointer.set(event.position);
                        }
                    })
                    .child(self.annotation_browser_body(thread, body, cx))
                    .children(notice.map(|notice| {
                        div()
                            .debug_selector(|| "browser-notice".into())
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .flex()
                            .flex_col()
                            .gap(px(SPACE_2))
                            .items_center()
                            .justify_center()
                            .p(px(SPACE_4))
                            .bg(paint::CARD)
                            .text_color(rgb(INK_SUBTLE))
                            .child(div().max_w_full().text_center().child(notice))
                            .children(retry.map(|document| {
                                ferrite_ui::components::button("browser-retry")
                                    .debug_selector(|| "browser-retry".into())
                                    .tab_stop(true)
                                    .child("Retry")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.browser_action(thread, Action::Reload { document }, cx)
                                    }))
                            }))
                    }))
                    .children(dialog),
            )
            .into_any_element()
    }
}

impl Browser {
    /// The page the controls act on: none while a new tab is being entered
    /// (the chrome's `new_tab`).
    fn shown_tab(&self, new_tab: bool) -> Option<&Tab> {
        self.state
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == self.state.active)
            .filter(|_| !new_tab)
    }

    fn sync_documents(&self, cx: &mut Context<CockpitView>, resample: bool) {
        for tab in &self.state.tabs {
            if let Some(page) = self.pages.get(&tab.id) {
                let changed = page.document.replace(tab.document()) != tab.document();
                if changed {
                    let _ = page.manual.borrow_mut().finish(manual::Finish::Navigated);
                }
                page.manual.borrow_mut().server_protection(tab.protected);
                if changed && resample {
                    page.web.update(cx, |web, cx| web.refresh_browser_state(cx));
                }
            }
        }
    }
}

fn failed_page(tab: &ferrite_core::browser::Tab, error: String) -> PageState {
    PageState {
        document: tab.document(),
        visible: false,
        navigation: false,
        url: tab.url.clone(),
        loading: false,
        can_back: tab.can_back,
        can_forward: tab.can_forward,
        error: Some(error),
        dirty: ferrite_core::browser::Dirty::Unknown,
    }
}

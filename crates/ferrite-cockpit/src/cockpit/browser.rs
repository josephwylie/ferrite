//! Window realization of the Server's Thread-owned browser tabs.
//! Native pages belong here, independently of whichever Preview is visible.
mod manual;

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
    reveal: std::collections::BTreeSet<ThreadId>,
    control_queue: std::collections::VecDeque<ControlRequest>,
    #[cfg(feature = "cef")]
    pub(super) controller:
        Option<std::sync::Arc<ferrite_pane::visual::browser_control::native::Worker>>,
    #[cfg(feature = "cef")]
    running_control: Option<RunningControl>,
    #[cfg(feature = "cef")]
    capture_queue: std::collections::VecDeque<CaptureRequest>,
    #[cfg(feature = "cef")]
    running_capture: Option<RunningCapture>,
}

struct ControlRequest {
    request: u64,
    thread: ThreadId,
    owner: Owner,
    document: Document,
    revision: u64,
    control: ferrite_core::browser::Control,
}
#[cfg(feature = "cef")]
struct RunningControl {
    request: ControlRequest,
    native_generation: u64,
    epoch: u64,
    manual: Rc<RefCell<manual::Manual>>,
    answer: std::sync::mpsc::Receiver<ferrite_pane::visual::browser_control::native::Event>,
}

#[cfg(feature = "cef")]
struct CaptureRequest {
    thread: ThreadId,
    owner: Owner,
    document: Document,
    reply: std::sync::mpsc::SyncSender<
        Result<ferrite_pane::visual::browser_control::CapturedPage, String>,
    >,
    queued: std::time::Instant,
    point: Option<ferrite_core::browser_annotations::Point>,
}
#[cfg(feature = "cef")]
struct RunningCapture {
    request: CaptureRequest,
    native_generation: u64,
    epoch: u64,
    viewport: BrowserViewport,
    url: String,
    manual: Rc<RefCell<manual::Manual>>,
    answer: std::sync::mpsc::Receiver<ferrite_pane::visual::browser_control::native::Event>,
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
                native_peers: Default::default(),
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
            self.browsers.reveal.insert(thread);
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
                    .browser_settled(thread, owner, document, revision);
            }
        }
        let result = self.cockpit.browser(thread, action);
        if let Some(manual) = &recovery {
            manual.borrow().restore_pause();
        }
        match result {
            Ok(state) => {
                let browser = self.browsers.threads.get_mut(&thread).unwrap();
                browser.state = state;
                browser.error = None;
                browser.new_tab = false;
                browser.address_shown = None;
                if browser.state.active.is_none() {
                    browser
                        .address
                        .update(cx, |field, cx| field.set(String::new(), cx));
                }
                browser.sync_documents(cx, false);
            }
            Err(error) => {
                self.browsers.threads.get_mut(&thread).unwrap().error = Some(error.to_string())
            }
        }
        cx.notify();
    }

    /// Capture callers retain their Annotation lease through Server retention.
    /// Results never borrow the live View or use its last OnPaint frame.
    pub(super) fn capture_browser_page(
        &mut self,
        thread: ThreadId,
        owner: Owner,
        document: Document,
        _lease: ferrite_core::browser::Lease,
        point: Option<ferrite_core::browser_annotations::Point>,
        _cx: &mut Context<Self>,
    ) -> std::sync::mpsc::Receiver<
        Result<ferrite_pane::visual::browser_control::CapturedPage, String>,
    > {
        let (reply, answer) = std::sync::mpsc::sync_channel(1);
        #[cfg(feature = "cef")]
        self.browsers.capture_queue.push_back(CaptureRequest {
            thread,
            owner,
            document,
            reply,
            queued: std::time::Instant::now(),
            point,
        });
        #[cfg(not(feature = "cef"))]
        {
            let _ = (thread, owner, document, point);
            let _ = reply.send(Err(
                "Conversation browser capture is not available in this build".into(),
            ));
        }
        answer
    }

    pub(super) fn pump_browsers(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.cockpit.server(),
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
        for message in self.cockpit.take_browser_messages() {
            let message = match message {
                ferrite_core::protocol::ServerMessage::BrowserRequest {
                    request,
                    thread,
                    owner,
                    document,
                    revision,
                    control,
                } => {
                    self.browsers.control_queue.push_back(ControlRequest {
                        request,
                        thread,
                        owner,
                        document,
                        revision,
                        control,
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
                self.browsers.reveal.insert(thread);
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
        let mut events = std::mem::take(&mut *self.browsers.events.borrow_mut())
            .into_iter()
            .peekable();
        while let Some(event) = events.next() {
            match event {
                Event::Restore(thread, tab) => {
                    if !matches!(
                        self.cockpit.server(),
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
                        .browser_settled(thread, owner, document, revision);
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
                        let settled = page.manual.borrow_mut().discard(sequence);
                        if let Some((document, revision)) = settled {
                            let _ = self
                                .cockpit
                                .browser_settled(thread, owner, document, revision);
                        }
                        continue;
                    }
                    match self.cockpit.browser_input(thread, owner, document) {
                        Ok(revision) => {
                            page.manual
                                .borrow_mut()
                                .acknowledge(sequence, document, revision);
                            page.native.input(input);
                        }
                        Err(error) => {
                            let settled = page.manual.borrow_mut().discard(sequence);
                            if let Some((document, revision)) = settled {
                                let _ = self
                                    .cockpit
                                    .browser_settled(thread, owner, document, revision);
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
                    match self.cockpit.browser_page(thread, owner, page) {
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
        for thread in std::mem::take(&mut self.browsers.reveal) {
            if let Some(index) = self.pane_for(thread) {
                self.panes[index].preview.show_browser();
            } else {
                self.browsers.reveal.insert(thread);
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
                        let settled = input_manual.borrow_mut().finish();
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
        self.tend_browser_pages(cx);
        #[cfg(feature = "cef")]
        self.pump_browser_captures(cx);
        self.pump_browser_controls(cx);
    }

    #[cfg(feature = "cef")]
    fn pump_browser_captures(&mut self, cx: &mut Context<Self>) {
        use ferrite_core::browser::{Control, Outcome};
        use ferrite_pane::visual::browser_control::{
            native::{screenshot_bytes, Event as NativeEvent, Worker},
            CapturedPage,
        };
        if let Some(running) = self.browsers.running_capture.take() {
            let unchanged = || {
                self.browsers
                    .threads
                    .get(&running.request.thread)
                    .filter(|browser| {
                        browser.owner == Some(running.request.owner)
                            && browser.state.active == Some(running.request.document.tab)
                    })
                    .and_then(|browser| browser.pages.get(&running.request.document.tab))
                    .is_some_and(|page| {
                        page.document.get() == running.request.document
                            && page.pending_navigation.borrow().is_none()
                            && page.native.state().document_generation == running.native_generation
                            && page.native.browser_viewport() == Some(running.viewport)
                            && page.native.control_epoch() == running.epoch
                    })
            };
            let result = loop {
                match running.answer.try_recv() {
                    Ok(NativeEvent::Ready(authorize)) => {
                        let _ = authorize.send(unchanged());
                    }
                    Ok(NativeEvent::Finished(Outcome::Success { value })) => {
                        break Some(if unchanged() {
                            screenshot_bytes(&value).map(|png| CapturedPage {
                                document: running.request.document,
                                native_generation: running.native_generation,
                                url: running.url.clone(),
                                viewport: ferrite_core::browser_annotations::Viewport {
                                    width_css: running.viewport.width as f64,
                                    height_css: running.viewport.height as f64,
                                    device_scale: running.viewport.scale as f64,
                                },
                                png,
                                // Without exact hit-tested DOM evidence the
                                // editor offers region feedback explicitly.
                                element: value
                                    .get("element")
                                    .cloned()
                                    .and_then(|value| serde_json::from_value(value).ok()),
                            })
                        } else {
                            Err(
                                "The page or viewport changed during capture; capture it again"
                                    .into(),
                            )
                        });
                    }
                    Ok(NativeEvent::Finished(
                        Outcome::Failed { error } | Outcome::Uncertain { error },
                    )) => break Some(Err(error)),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        break Some(Err("Browser capture worker disconnected".into()))
                    }
                }
            };
            if let Some(result) = result {
                running.manual.borrow_mut().release_local();
                let _ = running.request.reply.send(result);
            } else {
                self.browsers.running_capture = Some(running);
                return;
            }
        }
        let Some(request) = self.browsers.capture_queue.pop_front() else {
            return;
        };
        let page = self
            .browsers
            .threads
            .get(&request.thread)
            .filter(|browser| {
                browser.owner == Some(request.owner)
                    && browser.state.active == Some(request.document.tab)
            })
            .and_then(|browser| browser.pages.get(&request.document.tab))
            .filter(|page| {
                page.document.get() == request.document
                    && page.pending_navigation.borrow().is_none()
            });
        let Some(page) =
            page.filter(|_| request.queued.elapsed() < std::time::Duration::from_secs(35))
        else {
            let _ = request.reply.send(Err(
                "The selected browser document is no longer available; capture it again".into(),
            ));
            return;
        };
        let Some(viewport) = page.native.browser_viewport() else {
            let _ = request
                .reply
                .send(Err("The exact browser viewport is unavailable".into()));
            return;
        };
        let Some(target) = page.native.target_id() else {
            let _ = request
                .reply
                .send(Err("The live browser target is unavailable".into()));
            return;
        };
        let endpoint = match Visuals::engine(cx) {
            Availability::Ready(engine) => engine.controller(),
            _ => Err("The owning browser engine is unavailable".into()),
        };
        let endpoint = match endpoint {
            Ok(endpoint) => endpoint,
            Err(error) => {
                let _ = request.reply.send(Err(error));
                return;
            }
        };
        let worker = self.browsers.controller.get_or_insert_with(Worker::shared);
        let state = page.native.state();
        let epoch = page.native.control_epoch();
        page.manual.borrow_mut().protect_local();
        let answer = worker.try_start(
            endpoint,
            target,
            request.document,
            page.manual.borrow().revision,
            epoch,
            Control::Screenshot,
            request.point.clone(),
        );
        if let Some(answer) = answer {
            self.browsers.running_capture = Some(RunningCapture {
                request,
                native_generation: state.document_generation,
                epoch,
                viewport,
                url: state.url,
                manual: page.manual.clone(),
                answer,
            });
        } else {
            page.manual.borrow_mut().release_local();
            self.browsers.capture_queue.push_front(request);
        }
    }

    fn reply_browser_control(
        &mut self,
        request: ControlRequest,
        outcome: ferrite_core::browser::Outcome,
    ) {
        self.cockpit.browser_reply(
            request.request,
            request.thread,
            request.owner,
            request.document,
            request.revision,
            outcome,
        );
    }

    fn pump_browser_controls(&mut self, cx: &mut Context<Self>) {
        use ferrite_core::browser::Outcome;
        #[cfg(feature = "cef")]
        if let Some(running) = self.browsers.running_control.take() {
            let result = loop {
                match running.answer.try_recv() {
                    Ok(ferrite_pane::visual::browser_control::native::Event::Ready(authorize)) => {
                        let thread_idle = self
                            .browsers
                            .threads
                            .get(&running.request.thread)
                            .is_some_and(|browser| {
                                browser.pages.values().all(|page| {
                                    page.manual.borrow().revision <= running.request.revision
                                        && (running.request.control.observation()
                                            || !page.manual.borrow().active)
                                })
                            });
                        let valid = thread_idle
                            && self
                                .browsers
                                .threads
                                .get(&running.request.thread)
                                .filter(|browser| browser.owner == Some(running.request.owner))
                                .and_then(|browser| {
                                    browser.pages.get(&running.request.document.tab)
                                })
                                .is_some_and(|page| {
                                    page.document.get() == running.request.document
                                        && page.pending_navigation.borrow().is_none()
                                        && page.native.state().document_generation
                                            == running.native_generation
                                        && page.native.control_epoch() == running.epoch
                                        && page.manual.borrow().revision <= running.request.revision
                                        && (running.request.control.observation()
                                            || !page.manual.borrow().active)
                                });
                        let _ = authorize.send(valid);
                    }
                    Ok(ferrite_pane::visual::browser_control::native::Event::Finished(outcome)) => {
                        break Some(outcome)
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        break Some(Outcome::Uncertain {
                            error:
                                "Browser worker disconnected; observe the page before continuing"
                                    .into(),
                        })
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break None,
                }
            };
            if let Some(mut outcome) = result {
                let current = self
                    .browsers
                    .threads
                    .get(&running.request.thread)
                    .filter(|browser| browser.owner == Some(running.request.owner))
                    .and_then(|browser| browser.pages.get(&running.request.document.tab));
                let unchanged = current.is_some_and(|page| {
                    page.document.get() == running.request.document
                        && page.native.state().document_generation == running.native_generation
                        && page.native.control_epoch() == running.epoch
                });
                if !unchanged {
                    outcome = if running.request.control.observation() {
                        Outcome::Failed { error: "The page or operator state changed during observation; observe again".into() }
                    } else {
                        Outcome::Uncertain { error: "The page or operator state changed while input was in flight; observe the result and do not replay the write".into() }
                    };
                }
                running.manual.borrow_mut().release_local();
                self.reply_browser_control(running.request, outcome);
            } else {
                self.browsers.running_control = Some(running);
                return;
            }
        }
        loop {
            let Some(request) = self.browsers.control_queue.pop_front() else {
                return;
            };
            let page = self
                .browsers
                .threads
                .get(&request.thread)
                .filter(|browser| browser.owner == Some(request.owner))
                .and_then(|browser| browser.pages.get(&request.document.tab));
            let thread_idle = self
                .browsers
                .threads
                .get(&request.thread)
                .is_some_and(|browser| {
                    browser.pages.values().all(|page| {
                        page.manual.borrow().revision <= request.revision
                            && (request.control.observation() || !page.manual.borrow().active)
                    })
                });
            let valid = thread_idle
                && page.is_some_and(|page| {
                    page.document.get() == request.document
                        && page.pending_navigation.borrow().is_none()
                        && page.manual.borrow().revision <= request.revision
                        && (request.control.observation() || !page.manual.borrow().active)
                });
            if !valid {
                self.reply_browser_control(request, Outcome::Failed { error: "The owned browser document or operator revision changed before dispatch; obtain fresh State and Observe".into() });
                continue;
            }
            #[cfg(not(feature = "cef"))]
            {
                let _ = cx;
                self.reply_browser_control(
                    request,
                    Outcome::Failed {
                        error: "Browser agent control is not available in this build".into(),
                    },
                );
            }
            #[cfg(feature = "cef")]
            {
                let endpoint = match Visuals::engine(cx) {
                    Availability::Ready(engine) => engine.controller(),
                    _ => Err("The owning browser engine is unavailable".into()),
                };
                let endpoint = match endpoint {
                    Ok(endpoint) => endpoint,
                    Err(error) => {
                        self.reply_browser_control(request, Outcome::Failed { error });
                        continue;
                    }
                };
                let page = page.unwrap();
                let Some(target) = page.native.target_id() else {
                    self.reply_browser_control(
                        request,
                        Outcome::Failed {
                            error: "The exact live browser target is not ready; observe again"
                                .into(),
                        },
                    );
                    continue;
                };
                let worker = self.browsers.controller.get_or_insert_with(
                    ferrite_pane::visual::browser_control::native::Worker::shared,
                );
                let generation = page.native.state().document_generation;
                let epoch = page.native.control_epoch();
                // This direct edge precedes dispatch and cannot disappear in a
                // coalesced Server state update.
                page.manual.borrow_mut().protect_local();
                let answer = worker.try_start(
                    endpoint,
                    target,
                    request.document,
                    request.revision,
                    epoch,
                    request.control.clone(),
                    None,
                );
                if let Some(answer) = answer {
                    self.browsers.running_control = Some(RunningControl {
                        request,
                        native_generation: generation,
                        epoch,
                        manual: page.manual.clone(),
                        answer,
                    });
                } else {
                    page.manual.borrow_mut().release_local();
                    self.browsers.control_queue.push_front(request);
                }
                return;
            }
        }
    }

    fn finish_browser_interaction(&mut self, thread: ThreadId, owner: Owner, tab: TabId) {
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
        let settled = page.manual.borrow_mut().finish();
        if let Some((document, revision)) = settled {
            let _ = self
                .cockpit
                .browser_settled(thread, owner, document, revision);
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
                if manual.active
                    && (manual.due()
                        || !page.web.read(cx).browser_visible()
                        || !self.cockpit.window_active())
                {
                    settled.push((*thread, owner, *tab));
                }
            }
        }
        for (thread, owner, tab) in settled {
            self.finish_browser_interaction(thread, owner, tab);
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
            match self.cockpit.browser_page(thread, owner, page) {
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
        let focused = self.focused_thread();
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
            let lease = match self.cockpit.browser_reclaim(thread, owner, document) {
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
                .browser_reclaimed(thread, owner, lease, released)
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
        if let Some(tab) = active.filter(|tab| !tab.live) {
            self.browsers
                .events
                .borrow_mut()
                .push(Event::Restore(thread, tab.id));
        }
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
                .flex_shrink_0()
                .max_w(px(190.))
                .rounded(px(R_CONTROL))
                .text_size(px(FS_LABEL))
                .font_weight(W_LABEL)
                .px(px(8.))
                .py(px(4.))
                .map(|tab| {
                    let key = format!("browser-tab-{}", id.0);
                    if Some(id) == browser.state.active && !browser.new_tab {
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
            .when(
                active.is_some_and(|tab| tab.loading) && !browser.new_tab,
                |body| {
                    body.child(
                        div()
                            .px(px(8.))
                            .text_color(rgb(INK_SUBTLE))
                            .child("Loading…"),
                    )
                },
            )
            .when_some(
                browser.error.as_ref().or_else(|| {
                    active
                        .filter(|_| !browser.new_tab)
                        .and_then(|tab| tab.error.as_ref())
                }),
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
                let changed = page.document.replace(tab.document()) != tab.document();
                if changed {
                    let _ = page.manual.borrow_mut().finish();
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

//! A correlated DevTools response is the only authority for guest submissions.
//! The page cannot access the isolated world or its closed editor shadow root.
use super::ViewState;
use crate::visual::live_annotation::{self, Event};
use cef::*;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) const FIRST_ID: i32 = 1_500_000_000;
static NEXT_ID: AtomicI32 = AtomicI32::new(FIRST_ID);
const LIMIT: usize = 1024 * 1024 + 65536;
const GUEST: &str = include_str!("../../live_annotation/guest.js");
const SOURCE: &str = include_str!("../../live_annotation/source.js");
const KEYBOARD_GUARD: &str = include_str!("../../live_annotation/keyboard-guard.js");
const WORLD: &str = "ferrite-live-annotation";

fn next_id() -> i32 {
    NEXT_ID
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |id| {
            Some(if id >= i32::MAX - 1 { FIRST_ID } else { id + 1 })
        })
        .unwrap()
}

#[derive(Clone, Copy)]
enum Preparation {
    Waiting(Instant),
    Ready(bool),
    Started(bool),
}

/// Install only inert keyboard capture before the first website navigation.
/// The full editor remains lazy, and a timeout does not disable browsing.
pub(super) struct Bootstrap {
    _registration: Option<Registration>,
    inbox: Arc<Mutex<Inbox>>,
    expected: Arc<AtomicI32>,
    preparation: Preparation,
}

impl Bootstrap {
    pub(super) fn new(browser: &Browser) -> Self {
        let inbox = Arc::new(Mutex::new(Inbox {
            messages: VecDeque::new(),
            overflow: false,
        }));
        let id = next_id();
        let expected = Arc::new(AtomicI32::new(id));
        let mut observer = AnnotationObserver::new(inbox.clone(), expected.clone());
        let host = browser.host();
        let registration = host
            .as_ref()
            .and_then(|host| host.add_dev_tools_message_observer(Some(&mut observer)));
        let command =
            json!({ "id": id, "method": "Page.addScriptToEvaluateOnNewDocument", "params": {
                "source": KEYBOARD_GUARD, "worldName": WORLD,
            }})
            .to_string();
        let sent = registration.is_some()
            && host.is_some_and(|host| host.send_dev_tools_message(Some(command.as_bytes())) != 0);
        Self {
            _registration: registration,
            inbox,
            expected,
            preparation: if sent {
                Preparation::Waiting(Instant::now())
            } else {
                Preparation::Ready(false)
            },
        }
    }

    /// Returns true once, when the queued initial navigation may begin.
    pub(super) fn advance(&mut self) -> bool {
        if let Preparation::Waiting(since) = self.preparation {
            let mut inbox = self.inbox.lock().unwrap();
            if let Some(response) = inbox.messages.pop_front() {
                self.preparation = Preparation::Ready(
                    response.get("error").is_none()
                        && response["result"]["identifier"].as_str().is_some(),
                );
            } else if inbox.overflow || since.elapsed() >= Duration::from_secs(5) {
                self.preparation = Preparation::Ready(false);
            }
        }
        if let Preparation::Ready(available) = self.preparation {
            self.expected.store(0, Ordering::SeqCst);
            self._registration.take();
            self.preparation = Preparation::Started(available);
            true
        } else {
            false
        }
    }

    pub(super) fn started(&self) -> bool {
        matches!(self.preparation, Preparation::Started(_))
    }

    pub(super) fn available(&self) -> bool {
        matches!(self.preparation, Preparation::Started(true))
    }
}

#[derive(Clone, Copy)]
enum Step {
    Tree,
    World,
    Install,
    Wait,
    Targets,
    Properties,
    Describe,
    Resolve,
    Source,
    Release,
    Stop,
    Disposed,
}
struct Enrichment {
    event: Value,
    objects: Vec<String>,
    index: usize,
    since: Instant,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Cleanup {
    #[default]
    Idle,
    Pending(u8),
    Failed,
    Closed,
}
impl Cleanup {
    fn attempt(&mut self) -> bool {
        match *self {
            Self::Idle => {
                *self = Self::Pending(1);
                true
            }
            Self::Pending(1) => {
                *self = Self::Pending(2);
                true
            }
            Self::Pending(_) => {
                *self = Self::Failed;
                false
            }
            Self::Failed | Self::Closed => false,
        }
    }
}
struct Pending {
    id: i32,
    step: Step,
    since: Instant,
}
struct Inbox {
    messages: VecDeque<Value>,
    overflow: bool,
}

pub(super) struct AnnotationSession {
    _registration: Registration,
    inbox: Arc<Mutex<Inbox>>,
    expected: Arc<AtomicI32>,
    generation: u64,
    context: Option<i64>,
    pending: Option<Pending>,
    events: VecDeque<Event>,
    cleanup: Cleanup,
    enrichment: Option<Enrichment>,
    object_group: String,
    send_enabled: bool,
}

impl AnnotationSession {
    pub(super) fn new(
        browser: &Browser,
        generation: u64,
        send_enabled: bool,
    ) -> Result<Self, String> {
        let inbox = Arc::new(Mutex::new(Inbox {
            messages: VecDeque::new(),
            overflow: false,
        }));
        let expected = Arc::new(AtomicI32::new(0));
        let mut observer = AnnotationObserver::new(inbox.clone(), expected.clone());
        let registration = browser
            .host()
            .and_then(|host| host.add_dev_tools_message_observer(Some(&mut observer)))
            .ok_or("The browser annotation bridge could not be opened")?;
        let mut session = Self {
            _registration: registration,
            inbox,
            expected,
            generation,
            context: None,
            pending: None,
            events: VecDeque::new(),
            cleanup: Cleanup::Idle,
            send_enabled,
            enrichment: None,
            object_group: format!(
                "ferrite-annotation-source-{}",
                NEXT_ID.load(Ordering::SeqCst)
            ),
        };
        session.send(browser, Step::Tree, "Page.getFrameTree", json!({}));
        Ok(session)
    }

    fn send(&mut self, browser: &Browser, step: Step, method: &str, params: Value) {
        let id = next_id();
        self.expected.store(id, Ordering::SeqCst);
        let message = json!({ "id": id, "method": method, "params": params }).to_string();
        self.pending = Some(Pending {
            id,
            step,
            since: Instant::now(),
        });
        if !browser
            .host()
            .is_some_and(|host| host.send_dev_tools_message(Some(message.as_bytes())) != 0)
        {
            self.pending = None;
            match step {
                Step::Stop => self.stop_failed(browser),
                Step::Disposed => {
                    self.cleanup = Cleanup::Closed;
                    self.events.push_back(Event::Closed);
                }
                Step::Targets
                | Step::Properties
                | Step::Describe
                | Step::Resolve
                | Step::Source
                | Step::Release => self.sources_ready(),
                _ => self.events.push_back(Event::Failed(
                    "The browser annotation command could not be delivered".into(),
                )),
            }
        }
    }

    fn evaluate(&mut self, browser: &Browser, step: Step, expression: &str, await_promise: bool) {
        self.send(browser, step, "Runtime.evaluate", json!({
            "expression": expression, "contextId": self.context,
            "returnByValue": true, "awaitPromise": await_promise, "timeout": if await_promise { 35000 } else { 1000 },
        }));
    }

    fn wait(&mut self, browser: &Browser) {
        self.evaluate(
            browser,
            Step::Wait,
            "globalThis.__ferriteAnnotation.next()",
            true,
        );
    }

    pub(super) fn take_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }
    pub(super) fn closed(&self) -> bool {
        self.cleanup == Cleanup::Closed
    }

    pub(super) fn set_send_enabled(&mut self, browser: &Browser, enabled: bool) {
        if self.send_enabled == enabled {
            return;
        }
        self.send_enabled = enabled;
        if self.context.is_none() || self.cleanup != Cleanup::Idle {
            return;
        }
        let id = next_id();
        let message = json!({ "id": id, "method": "Runtime.evaluate", "params": {
            "expression": format!("globalThis.__ferriteAnnotation?.setSendEnabled({enabled})"),
            "contextId": self.context, "returnByValue": true, "timeout": 250,
        }})
        .to_string();
        if let Some(host) = browser.host() {
            host.send_dev_tools_message(Some(message.as_bytes()));
        }
    }

    pub(super) fn finish(&mut self, browser: &Browser) {
        if self.cleanup != Cleanup::Idle {
            return;
        }
        self.stop(browser);
    }

    fn stop(&mut self, browser: &Browser) {
        self.enrichment = None;
        if self.context.is_some() {
            if !self.cleanup.attempt() {
                self.events.push_back(Event::RestoreFailed("The browser could not restore the annotation preview. Reload or close this page to release annotation protection.".into()));
                return;
            }
            self.evaluate(
                browser,
                Step::Stop,
                "globalThis.__ferriteAnnotation?.stop(); true",
                false,
            );
        } else {
            self.pending = None;
            self.cleanup = Cleanup::Closed;
            self.events.push_back(Event::Closed);
        }
    }

    fn stop_failed(&mut self, browser: &Browser) {
        self.pending = None;
        self.stop(browser);
    }

    fn finish_sources(&mut self, browser: &Browser) {
        self.send(
            browser,
            Step::Release,
            "Runtime.releaseObjectGroup",
            json!({ "objectGroup": self.object_group }),
        );
    }

    fn next_source(&mut self, browser: &Browser) {
        let Some(source) = &mut self.enrichment else {
            return;
        };
        if source.index >= source.objects.len() || source.since.elapsed() >= Duration::from_secs(5)
        {
            self.finish_sources(browser);
        } else {
            let object = source.objects[source.index].clone();
            if object.is_empty() {
                source.index += 1;
                self.next_source(browser);
            } else {
                self.send(
                    browser,
                    Step::Describe,
                    "DOM.describeNode",
                    json!({ "objectId": object }),
                );
            }
        }
    }

    fn sources_ready(&mut self) {
        if let Some(source) = self.enrichment.take() {
            self.events.push_back(
                live_annotation::parse_event(source.event).unwrap_or_else(Event::Failed),
            );
        }
    }

    pub(super) fn pump(&mut self, browser: &Browser, state: &ViewState) {
        if self.cleanup == Cleanup::Closed {
            return;
        }
        if state.navigation.lock().unwrap().document_generation != self.generation {
            self.pending = None;
            self.cleanup = Cleanup::Closed;
            self.events.push_back(Event::Closed);
            return;
        }
        let (messages, overflow) = {
            let mut inbox = self.inbox.lock().unwrap();
            (
                std::mem::take(&mut inbox.messages),
                std::mem::take(&mut inbox.overflow),
            )
        };
        if self
            .enrichment
            .as_ref()
            .is_some_and(|source| source.since.elapsed() > Duration::from_secs(5))
        {
            if !self
                .pending
                .as_ref()
                .is_some_and(|pending| matches!(pending.step, Step::Release))
            {
                self.finish_sources(browser);
            }
        }
        if overflow
            || self.pending.as_ref().is_some_and(|pending| {
                pending.since.elapsed()
                    > Duration::from_secs(if matches!(pending.step, Step::Wait) {
                        35
                    } else {
                        5
                    })
            })
        {
            if self
                .pending
                .as_ref()
                .is_some_and(|pending| matches!(pending.step, Step::Disposed))
            {
                self.pending = None;
                self.cleanup = Cleanup::Closed;
                self.events.push_back(Event::Closed);
            } else if matches!(self.cleanup, Cleanup::Pending(_)) {
                self.stop_failed(browser);
            } else if self.enrichment.is_some() {
                self.sources_ready();
                self.pending = None;
            } else {
                self.events
                    .push_back(Event::Failed("The live annotation bridge timed out".into()));
                self.finish(browser);
            }
            return;
        }
        for response in messages {
            if !self
                .pending
                .as_ref()
                .is_some_and(|pending| response["id"].as_i64() == Some(i64::from(pending.id)))
            {
                continue;
            }
            let pending = self.pending.take().unwrap();
            if response.get("error").is_some()
                || response["result"].get("exceptionDetails").is_some()
            {
                if matches!(pending.step, Step::Disposed) {
                    self.cleanup = Cleanup::Closed;
                    self.events.push_back(Event::Closed);
                } else if matches!(pending.step, Step::Stop) {
                    self.stop_failed(browser);
                } else if self.enrichment.is_some() {
                    if matches!(pending.step, Step::Release) {
                        self.sources_ready();
                    } else if matches!(pending.step, Step::Targets | Step::Properties) {
                        self.finish_sources(browser);
                    } else {
                        self.enrichment.as_mut().unwrap().index += 1;
                        self.next_source(browser);
                    }
                } else {
                    self.events.push_back(Event::Failed(
                        "The live annotation could not be completed".into(),
                    ));
                    self.finish(browser);
                }
                continue;
            }
            let result = &response["result"];
            match pending.step {
                Step::Tree => {
                    if let Some(frame) = result["frameTree"]["frame"]["id"].as_str() {
                        self.send(
                            browser,
                            Step::World,
                            "Page.createIsolatedWorld",
                            json!({
                                "frameId": frame, "worldName": WORLD
                            }),
                        );
                    } else {
                        self.events.push_back(Event::Failed(
                            "The annotation document is unavailable".into(),
                        ));
                        self.finish(browser);
                    }
                }
                Step::World => {
                    self.context = result["executionContextId"].as_i64();
                    if self.context.is_some() {
                        let source = format!(
                            "globalThis.__ferriteAnnotationSendEnabled = {};\n{GUEST}",
                            self.send_enabled
                        );
                        self.evaluate(browser, Step::Install, &source, false);
                    } else {
                        self.events
                            .push_back(Event::Failed("The annotation world is unavailable".into()));
                        self.finish(browser);
                    }
                }
                Step::Install => {
                    self.events.push_back(Event::Ready);
                    self.wait(browser);
                }
                Step::Wait => {
                    let event = result["result"]["value"].clone();
                    if event.is_null() {
                        self.wait(browser);
                    } else if event["kind"] == "attach" {
                        self.enrichment = Some(Enrichment {
                            event,
                            objects: vec![],
                            index: 0,
                            since: Instant::now(),
                        });
                        self.send(browser, Step::Targets, "Runtime.evaluate", json!({
                            "expression": "globalThis.__ferriteAnnotation.targets()", "contextId": self.context,
                            "objectGroup": self.object_group, "returnByValue": false, "timeout": 250,
                        }));
                    } else {
                        self.events.push_back(
                            live_annotation::parse_event(event).unwrap_or_else(Event::Failed),
                        );
                    }
                }
                Step::Targets => {
                    if let Some(object) = result["result"]["objectId"].as_str() {
                        self.send(
                            browser,
                            Step::Properties,
                            "Runtime.getProperties",
                            json!({ "objectId": object, "ownProperties": true }),
                        );
                    } else {
                        self.finish_sources(browser);
                    }
                }
                Step::Properties => {
                    let source = self.enrichment.as_mut().unwrap();
                    let count = source.event["feedback"]["elements"]
                        .as_array()
                        .map_or(0, Vec::len)
                        .min(256);
                    source.objects = vec![String::new(); count];
                    for property in result["result"].as_array().into_iter().flatten() {
                        if let Some(index) = property["name"]
                            .as_str()
                            .and_then(|name| name.parse::<usize>().ok())
                            .filter(|index| *index < count)
                        {
                            source.objects[index] = property["value"]["objectId"]
                                .as_str()
                                .unwrap_or("")
                                .to_owned();
                        }
                    }
                    self.next_source(browser);
                }
                Step::Describe => {
                    if let Some(node) = result["node"]["backendNodeId"].as_i64() {
                        self.send(
                            browser,
                            Step::Resolve,
                            "DOM.resolveNode",
                            json!({ "backendNodeId": node, "objectGroup": self.object_group }),
                        );
                    } else {
                        self.enrichment.as_mut().unwrap().index += 1;
                        self.next_source(browser);
                    }
                }
                Step::Resolve => {
                    if let Some(object) = result["object"]["objectId"].as_str() {
                        self.send(browser, Step::Source, "Runtime.callFunctionOn", json!({
                            "objectId": object, "functionDeclaration": SOURCE, "returnByValue": true,
                        }));
                    } else {
                        self.enrichment.as_mut().unwrap().index += 1;
                        self.next_source(browser);
                    }
                }
                Step::Source => {
                    let source = self.enrichment.as_mut().unwrap();
                    let context = &result["result"]["value"];
                    if context.is_object() {
                        let element =
                            &mut source.event["feedback"]["elements"][source.index]["element"];
                        element["componentName"] = context["componentName"].clone();
                        element["stack"] = context["stack"].clone();
                        element["source"] = context["stack"][0].clone();
                    }
                    source.index += 1;
                    self.next_source(browser);
                }
                Step::Release => self.sources_ready(),
                Step::Stop => self.send(
                    browser,
                    Step::Disposed,
                    "Runtime.releaseObjectGroup",
                    json!({ "objectGroup": self.object_group }),
                ),
                Step::Disposed => {
                    self.cleanup = Cleanup::Closed;
                    self.events.push_back(Event::Closed);
                }
            }
        }
    }
}

wrap_dev_tools_message_observer! {
    struct AnnotationObserver { inbox: Arc<Mutex<Inbox>>, expected: Arc<AtomicI32> }
    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> ::std::os::raw::c_int {
            if self.expected.load(Ordering::SeqCst) == 0 { return 1; }
            let Some(bytes) = message else { return 1 };
            if bytes.len() > LIMIT { return 1; }
            if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
                if value["id"].as_i64() == Some(i64::from(self.expected.load(Ordering::SeqCst))) {
                    let mut inbox = self.inbox.lock().unwrap();
                    if inbox.messages.len() < 4 { inbox.messages.push_back(value); }
                    else { inbox.overflow = true; }
                }
            }
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bootstrap(preparation: Preparation) -> Bootstrap {
        Bootstrap {
            _registration: None,
            inbox: Arc::new(Mutex::new(Inbox {
                messages: VecDeque::new(),
                overflow: false,
            })),
            expected: Arc::new(AtomicI32::new(17)),
            preparation,
        }
    }

    #[test]
    fn keyboard_bootstrap_requires_ack_then_releases_navigation_once() {
        let mut bootstrap = bootstrap(Preparation::Waiting(Instant::now()));
        assert!(!bootstrap.advance());
        assert!(!bootstrap.started());
        assert!(!bootstrap.available());
        bootstrap
            .inbox
            .lock()
            .unwrap()
            .messages
            .push_back(json!({"result": {"identifier": "guard"}}));
        assert!(bootstrap.advance());
        assert!(bootstrap.started());
        assert!(bootstrap.available());
        assert!(!bootstrap.advance());
        assert_eq!(bootstrap.expected.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn keyboard_bootstrap_timeout_preserves_browsing_without_claiming_isolation() {
        let mut bootstrap = bootstrap(Preparation::Waiting(
            Instant::now() - Duration::from_secs(6),
        ));
        assert!(bootstrap.advance());
        assert!(bootstrap.started());
        assert!(!bootstrap.available());
        bootstrap
            .inbox
            .lock()
            .unwrap()
            .messages
            .push_back(json!({"result": {"identifier": "late"}}));
        assert!(!bootstrap.advance());
        assert!(!bootstrap.available());
    }

    #[test]
    fn keyboard_bootstrap_error_preserves_browsing_without_claiming_isolation() {
        let mut bootstrap = bootstrap(Preparation::Waiting(Instant::now()));
        bootstrap
            .inbox
            .lock()
            .unwrap()
            .messages
            .push_back(json!({"error": {"code": -1}}));
        assert!(bootstrap.advance());
        assert!(bootstrap.started());
        assert!(!bootstrap.available());
    }

    #[test]
    fn failed_restore_stops_after_one_retry_without_claiming_cleanup() {
        let mut cleanup = Cleanup::Idle;
        assert!(cleanup.attempt());
        assert!(cleanup.attempt());
        assert!(!cleanup.attempt());
        assert_eq!(cleanup, Cleanup::Failed);
        for _ in 0..100 {
            assert!(!cleanup.attempt());
        }
        assert_ne!(cleanup, Cleanup::Closed);
    }
}

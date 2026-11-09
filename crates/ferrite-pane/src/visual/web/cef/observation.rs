//! Browser-private dirty observation. Page console messages are never authority.
//! Inspection runs in an isolated world and closed shadow DOM is checked via
//! Chromium's DOM snapshot, retaining the page whenever inspection is uncertain.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cef::*;
use serde_json::{json, Value};

use super::{BrowserDirty, ViewState};

pub(super) const FIRST_ID: i32 = 1_000_000_000;
const INTERVAL: Duration = Duration::from_millis(500);
const LIMIT: usize = 8 * 1024 * 1024;

const INSPECT: &str = r#"(() => {
  let s = globalThis.__ferriteWorkObservation;
  if (!s) {
    s = globalThis.__ferriteWorkObservation = { touched: false };
    for (const event of ['input', 'change'])
      addEventListener(event, () => { s.touched = true }, true);
  }
  let dirty = s.touched;
  for (const node of document.querySelectorAll('input,textarea,select')) {
    if (node instanceof HTMLInputElement) {
      dirty ||= node.type === 'checkbox' || node.type === 'radio'
        ? node.checked !== node.defaultChecked
        : node.type === 'file' ? node.files.length > 0 : node.value !== node.defaultValue;
    } else if (node instanceof HTMLTextAreaElement) dirty ||= node.value !== node.defaultValue;
    else {
      const options = Array.from(node.options);
      const defaults = options.some(option => option.defaultSelected);
      dirty ||= options.some((option, i) => option.selected !==
        (option.defaultSelected || (!node.multiple && !defaults && i === 0)));
    }
  }
  let unload = true;
  try { unload = (getEventListeners(window).beforeunload || []).length > 0 || window.onbeforeunload !== null; } catch (_) {}
  const uncertain = unload || !!document.querySelector('iframe,frame,object,embed,canvas') ||
    Array.from(document.querySelectorAll('[contenteditable]')).some(node => node.isContentEditable);
  return { dirty, uncertain };
})()"#;

#[derive(Clone, Copy)]
enum Step {
    Install,
    Tree,
    World,
    Inspect,
    Snapshot(bool),
}
struct Pending {
    id: i32,
    document: u64,
    step: Step,
    since: Instant,
}
struct Inbox {
    messages: VecDeque<Value>,
    overflow: bool,
}

pub(super) struct Observation {
    // The registration must disappear before its browser is closed.
    _registration: Registration,
    inbox: Arc<Mutex<Inbox>>,
    next: i32,
    document: u64,
    context: Option<i64>,
    installed: bool,
    clean_at: Option<Instant>,
    pending: Option<Pending>,
    last: Instant,
}

impl Observation {
    pub(super) fn new(browser: &Browser) -> Option<Self> {
        let host = browser.host()?;
        let inbox = Arc::new(Mutex::new(Inbox {
            messages: VecDeque::new(),
            overflow: false,
        }));
        let mut observer = WorkObserver::new(inbox.clone());
        let registration = host.add_dev_tools_message_observer(Some(&mut observer))?;
        let mut observation = Self {
            _registration: registration,
            inbox,
            next: FIRST_ID,
            document: 0,
            context: None,
            installed: false,
            clean_at: None,
            pending: None,
            last: Instant::now() - INTERVAL,
        };
        observation.send(browser, 0, Step::Install, "Page.addScriptToEvaluateOnNewDocument", json!({ "source": INSPECT, "worldName": "ferrite-work-observation", "runImmediately": true }));
        Some(observation)
    }

    pub(super) fn invalidate(&mut self, state: &ViewState) {
        // A protected command may have written after any in-flight inspection.
        // Its old result can never make the post-command document clean.
        self.pending = None;
        self.clean_at = None;
        self.last = Instant::now() - INTERVAL;
        Self::unknown(state);
    }

    pub(super) fn release_ready(&self) -> bool {
        self.pending.is_none() && self.clean_at.is_some_and(|time| time.elapsed() < INTERVAL)
    }

    fn unknown(state: &ViewState) {
        let mut navigation = state.navigation.lock().unwrap();
        if navigation.dirty == BrowserDirty::Clean {
            navigation.dirty = BrowserDirty::Unknown;
            state.dirty.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn send(&mut self, browser: &Browser, document: u64, step: Step, method: &str, params: Value) {
        let id = self.next;
        self.next = if id == i32::MAX { FIRST_ID } else { id + 1 };
        let message = json!({ "id": id, "method": method, "params": params }).to_string();
        if browser
            .host()
            .is_some_and(|host| host.send_dev_tools_message(Some(message.as_bytes())) != 0)
        {
            self.pending = Some(Pending {
                id,
                document,
                step,
                since: Instant::now(),
            });
        }
        self.last = Instant::now();
    }

    pub(super) fn pump(&mut self, browser: &Browser, state: &ViewState) {
        let navigation = state.navigation.lock().unwrap().clone();
        if navigation.document_generation != self.document {
            self.document = navigation.document_generation;
            self.context = None;
            if !self
                .pending
                .as_ref()
                .is_some_and(|pending| matches!(pending.step, Step::Install))
            {
                self.pending = None;
            }
            self.clean_at = None;
            self.last = Instant::now() - INTERVAL;
        }
        let (messages, overflow) = {
            let mut inbox = self.inbox.lock().unwrap();
            (
                std::mem::take(&mut inbox.messages),
                std::mem::take(&mut inbox.overflow),
            )
        };
        if overflow
            || self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.since.elapsed() > Duration::from_secs(5))
        {
            self.pending = None;
            self.context = None;
            Self::unknown(state);
        }
        for response in messages {
            if !self
                .pending
                .as_ref()
                .is_some_and(|pending| response["id"].as_i64() == Some(pending.id as i64))
            {
                continue;
            }
            let pending = self.pending.take().unwrap();
            if (!matches!(pending.step, Step::Install) && pending.document != self.document)
                || response.get("error").is_some()
            {
                Self::unknown(state);
                continue;
            }
            let result = &response["result"];
            match pending.step {
                Step::Install => {
                    self.installed = true;
                    self.last = Instant::now() - INTERVAL;
                }
                Step::Tree => {
                    if let Some(frame) = result["frameTree"]["frame"]["id"].as_str() {
                        self.send(
                            browser,
                            self.document,
                            Step::World,
                            "Page.createIsolatedWorld",
                            json!({ "frameId": frame, "worldName": "ferrite-work-observation" }),
                        );
                    } else {
                        Self::unknown(state);
                    }
                }
                Step::World => {
                    self.context = result["executionContextId"].as_i64();
                    self.last = Instant::now() - INTERVAL;
                }
                Step::Inspect => {
                    let value = &result["result"]["value"];
                    if value["dirty"].as_bool() == Some(true) {
                        state.navigation.lock().unwrap().dirty = BrowserDirty::Dirty;
                        state.dirty.store(true, std::sync::atomic::Ordering::SeqCst);
                    } else if let (Some(false), Some(uncertain)) =
                        (value["dirty"].as_bool(), value["uncertain"].as_bool())
                    {
                        self.send(
                            browser,
                            self.document,
                            Step::Snapshot(uncertain),
                            "DOMSnapshot.captureSnapshot",
                            json!({ "computedStyles": [] }),
                        );
                    } else {
                        Self::unknown(state);
                        self.context = None;
                    }
                }
                Step::Snapshot(uncertain) => {
                    let documents = result["documents"].as_array();
                    let uncertain = uncertain
                        || documents.is_none_or(|documents| {
                            documents.len() != 1
                                || documents.iter().any(|document| {
                                    document["nodes"]["shadowRootType"]["value"]
                                        .as_array()
                                        .is_some_and(|roots| !roots.is_empty())
                                })
                        });
                    let mut navigation = state.navigation.lock().unwrap();
                    if navigation.document_generation == pending.document
                        && navigation.dirty != BrowserDirty::Dirty
                    {
                        self.clean_at = (!uncertain).then(Instant::now);
                        let dirty = if uncertain {
                            BrowserDirty::Unknown
                        } else {
                            BrowserDirty::Clean
                        };
                        if navigation.dirty != dirty {
                            navigation.dirty = dirty;
                            state.dirty.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                }
            }
        }
        if !self.installed
            || navigation.loading
            || navigation.document_generation == 0
            || navigation.dirty == BrowserDirty::Dirty
            || self.pending.is_some()
            || self.last.elapsed() < INTERVAL
        {
            return;
        }
        if let Some(context) = self.context {
            self.send(
                browser,
                self.document,
                Step::Inspect,
                "Runtime.evaluate",
                json!({ "expression": INSPECT, "contextId": context, "returnByValue": true, "includeCommandLineAPI": true }),
            );
        } else {
            self.send(
                browser,
                self.document,
                Step::Tree,
                "Page.getFrameTree",
                json!({}),
            );
        }
    }
}

wrap_dev_tools_message_observer! {
    struct WorkObserver { inbox: Arc<Mutex<Inbox>> }
    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> ::std::os::raw::c_int {
            let Some(bytes) = message else { return 1 };
            let mut inbox = self.inbox.lock().unwrap();
            if bytes.len() > LIMIT { inbox.overflow = true; return 1; }
            if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
                if value["id"].as_i64().is_some_and(|id| id >= FIRST_ID as i64) {
                    if inbox.messages.len() < 16 { inbox.messages.push_back(value); }
                    else { inbox.overflow = true; }
                }
            }
            // CEF invokes each observer independently; no typed callbacks here.
            1
        }
    }
}

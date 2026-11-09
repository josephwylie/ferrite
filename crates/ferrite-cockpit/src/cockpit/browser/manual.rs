//! Immediate native pause, followed by the Server's revision-bound gesture.
//! Pointer holds end on release/cancel; typing/wheel/hover settle after quiet.
use ferrite_core::browser::Document;
use ferrite_pane::visual::web::{BrowserView, Input};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

const QUIET: Duration = Duration::from_millis(350);

#[derive(Default)]
pub(super) struct Peers {
    views: Vec<Weak<dyn BrowserView>>,
    active: BTreeSet<u64>,
    captures: BTreeSet<u64>,
}
impl Peers {
    fn paused(&self) -> bool {
        !self.active.is_empty() || !self.captures.is_empty()
    }
    fn sync(&mut self) {
        let paused = self.paused();
        self.views.retain(|view| {
            if let Some(view) = view.upgrade() {
                view.set_agent_paused(paused);
                true
            } else {
                false
            }
        });
    }
}
#[derive(Default)]
pub(super) struct Manual {
    pub native: Option<Rc<dyn BrowserView>>,
    peers: Option<(Rc<RefCell<Peers>>, u64)>,
    sequence: u64,
    canceled_through: u64,
    pub active: bool,
    pending: bool,
    held: bool,
    keys: BTreeSet<String>,
    quiet_until: Option<Instant>,
    acknowledged: Option<(Document, u64)>,
    pub revision: u64,
    server_protected: bool,
    local_protections: usize,
}
impl Manual {
    pub fn join(&mut self, peers: Rc<RefCell<Peers>>, key: u64, native: Rc<dyn BrowserView>) {
        peers.borrow_mut().views.push(Rc::downgrade(&native));
        self.native = Some(native);
        self.peers = Some((peers.clone(), key));
        peers.borrow_mut().sync();
    }
    pub fn capture_pause(&mut self, paused: bool) {
        if let Some((peers, key)) = &self.peers {
            let mut peers = peers.borrow_mut();
            if paused {
                peers.captures.insert(*key);
            } else {
                peers.captures.remove(key);
            }
            peers.sync();
        } else if let Some(native) = &self.native {
            native.set_agent_paused(paused || self.active);
        }
    }
    fn mark_active(&self, active: bool) {
        if let Some((peers, key)) = &self.peers {
            let mut peers = peers.borrow_mut();
            if active {
                peers.active.insert(*key);
            } else {
                peers.active.remove(key);
            }
            peers.sync();
        }
    }
    /// Called by the actual input callback, before queuing or any Server RPC.
    pub fn capture(&mut self, input: &Input) -> Option<u64> {
        if matches!(input, Input::Leave) {
            return None;
        }
        self.sequence += 1;
        self.active = true;
        self.mark_active(true);
        self.pending = true;
        match input {
            Input::Down { .. } => self.held = true,
            Input::Up { .. } => self.held = false,
            Input::Key {
                key, down: true, ..
            } => {
                self.keys.insert(key.clone());
            }
            Input::Key {
                key, down: false, ..
            } => {
                self.keys.remove(key);
            }
            _ => {}
        }
        self.quiet_until = Some(Instant::now() + QUIET);
        if let Some(native) = &self.native {
            native.set_agent_paused(true);
            native.set_protected(true);
        }
        Some(self.sequence)
    }
    pub fn valid(&self, sequence: u64) -> bool {
        sequence > self.canceled_through
    }
    pub fn acknowledge(&mut self, sequence: u64, document: Document, revision: u64) {
        self.revision = revision;
        self.acknowledged = Some((document, revision));
        if sequence == self.sequence {
            self.pending = false;
        }
    }
    pub fn discard(&mut self, sequence: u64) -> Option<(Document, u64)> {
        if sequence == self.sequence {
            self.finish()
        } else {
            None
        }
    }
    pub fn due(&self) -> bool {
        self.active
            && !self.pending
            && !self.held
            && self.keys.is_empty()
            && self
                .quiet_until
                .is_some_and(|until| Instant::now() >= until)
    }
    pub fn cancel(&mut self) {
        self.canceled_through = self.sequence;
        self.active = false;
        self.pending = false;
        self.held = false;
        self.keys.clear();
        self.quiet_until = None;
        self.mark_active(false);
        self.restore_pause();
        self.sync_protection();
    }
    pub fn server_protection(&mut self, protected: bool) {
        self.server_protected = protected;
        self.sync_protection();
    }
    /// Local command/capture edges cannot be lost to coalesced Server state.
    pub fn protect_local(&mut self) {
        self.local_protections += 1;
        self.sync_protection();
    }
    pub fn release_local(&mut self) {
        self.local_protections = self.local_protections.saturating_sub(1);
        self.sync_protection();
    }
    fn sync_protection(&self) {
        if let Some(native) = &self.native {
            native
                .set_protected(self.server_protected || self.active || self.local_protections > 0);
        }
    }
    /// An intentional close/reload invalidates the old native attachment
    /// before its Server RPC. Queued raw input from before it is discarded.
    pub fn retire_for_action(&mut self) -> Option<(Document, u64)> {
        self.sequence += 1;
        self.canceled_through = self.sequence;
        self.active = false;
        self.pending = false;
        self.held = false;
        self.keys.clear();
        self.quiet_until = None;
        self.mark_active(false);
        if let Some(native) = &self.native {
            native.set_agent_paused(true);
        }
        self.sync_protection();
        self.acknowledged.take()
    }
    pub fn restore_pause(&self) {
        if let Some(native) = &self.native {
            native.set_agent_paused(
                self.active
                    || self
                        .peers
                        .as_ref()
                        .is_some_and(|(peers, _)| peers.borrow().paused()),
            );
        }
    }

    pub fn finish(&mut self) -> Option<(Document, u64)> {
        self.cancel();
        self.acknowledged.take()
    }
}

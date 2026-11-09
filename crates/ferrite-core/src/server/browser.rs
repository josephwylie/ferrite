//! Window routing for the one Thread browser service. Capabilities alone do
//! not grant ownership: the connection must also be attached to the Thread.
use super::*;
use crate::browser::{Action, Service};

pub(super) fn load(store: &crate::store::Store) -> Service {
    match store.load_browser().and_then(|saved| {
        saved
            .map(Service::restore)
            .transpose()
            .map_err(io::Error::other)
    }) {
        Ok(service) => service.unwrap_or_default(),
        Err(error) => {
            crate::log!("browser metadata unavailable: {error}");
            Service::default()
        }
    }
}

impl Server {
    pub(super) fn browser_window(&self, id: ClientId, thread: ThreadId) -> Result<(), WireError> {
        if self.engine.thread(thread).is_none() {
            return Err(failed("No such open Thread"));
        }
        self.clients.get(&id)
            .filter(|client| client.welcomed && client.browser_capable && client.watching.contains(&thread))
            .map(|_| ())
            .ok_or_else(|| failed("Conversation browser unavailable: open this Thread in a browser-capable Ferrite window"))
    }

    pub(super) fn attach_browser(
        &mut self,
        id: ClientId,
        thread: ThreadId,
    ) -> Result<Option<Done>, WireError> {
        self.browser_window(id, thread)?;
        self.browser.attach(thread, id).map_err(failed)?;
        self.publish_browser(thread, None);
        Ok(Some(Done::Browser {
            state: self.browser.state(thread),
        }))
    }

    pub(super) fn browser_action(
        &mut self,
        id: ClientId,
        thread: ThreadId,
        action: Action,
    ) -> Result<Option<Done>, WireError> {
        self.browser_window(id, thread)?;
        let mut next = self.browser.clone();
        let state = next.apply(thread, id, action.clone()).map_err(failed)?;
        // Metadata is durable before realizing a page. A failed disk write is
        // reported, never silently described as a successful persisted open.
        self.engine
            .browser_store()
            .save_browser(&next.save())
            .map_err(failed)?;
        self.browser = next;
        self.publish_browser(thread, Some(action));
        Ok(Some(Done::Browser { state }))
    }

    pub(super) fn save_browser(&self) -> Result<(), WireError> {
        self.engine
            .browser_store()
            .save_browser(&self.browser.save())
            .map_err(failed)
    }

    pub(super) fn publish_browser(&mut self, thread: ThreadId, action: Option<Action>) {
        let state = self.browser.state(thread);
        let owner = state.owner;
        let receivers: Vec<_> = self
            .clients
            .iter()
            .filter(|(_, client)| client.welcomed && client.watching.contains(&thread))
            .map(|(id, _)| *id)
            .collect();
        for receiver in receivers {
            self.send(
                receiver,
                ServerMessage::Browser {
                    thread,
                    owner,
                    state: state.clone(),
                    action: action
                        .clone()
                        .filter(|_| owner.is_some_and(|owner| owner.window == receiver)),
                },
            );
        }
    }
}

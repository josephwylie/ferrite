//! Window routing for the one Thread browser service. Capabilities alone do
//! not grant ownership: the connection must also be attached to the Thread.
use super::*;
use crate::browser::{Action, Service};

pub(super) fn load(dir: &std::path::Path) -> Service {
    let path = dir.join("browser-tabs.json");
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes)
            .map_err(|e| e.to_string())
            .and_then(Service::restore)
        {
            Ok(service) => service,
            Err(error) => {
                crate::log!("browser metadata unavailable: {error}");
                // Preserve the unreadable document for recovery; never replace it.
                Service::default()
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Service::default(),
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
        let state = self
            .browser
            .apply(thread, id, action.clone())
            .map_err(failed)?;
        // Metadata is durable before realizing a page. A failed disk write is
        // reported, never silently described as a successful persisted open.
        self.save_browser()?;
        self.publish_browser(thread, Some(action));
        Ok(Some(Done::Browser { state }))
    }

    pub(super) fn save_browser(&self) -> Result<(), WireError> {
        let path = self.settings_dir.join("browser-tabs.json");
        if path.exists() {
            let bytes = std::fs::read(&path).map_err(failed)?;
            let _: crate::browser::Saved = serde_json::from_slice(&bytes).map_err(failed)?;
        }
        let bytes = serde_json::to_vec(&self.browser.save()).map_err(failed)?;
        crate::store::replace_file(&path, &bytes).map_err(failed)
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

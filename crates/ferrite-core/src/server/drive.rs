//! The API's `window.*` actions (ADR 0038), for the one attached window
//! that is driven (`ferrite --headless`): each goes to it as a `drive`
//! and its caller is answered with the window's `driven`. A normal window
//! never takes one; with no driven window the caller hears so at once.

use std::collections::btree_map::{BTreeMap, Entry};

use super::Server;
use crate::api::{ApiError, CallerId, Code};
use crate::drive::{Action, Answer};
use crate::link::ClientId;
use crate::protocol::ServerMessage;

/// The actions handed to a window, and whose request each answers.
#[derive(Default)]
pub(super) struct Drives {
    owed: BTreeMap<u64, Owed>,
    next: u64,
}

struct Owed {
    window: ClientId,
    caller: CallerId,
    id: String,
}

impl Server {
    pub(super) fn drive(&mut self, caller: CallerId, id: String, action: Action) {
        let window = self
            .clients
            .iter()
            .find(|(_, client)| client.welcomed && client.driven)
            .map(|(window, _)| *window);
        let Some(window) = window else {
            let refusal = ApiError::new(
                Code::NoHeadlessWindow,
                "no headless Ferrite window is attached: start one with `ferrite --headless`",
            );
            self.api.reply(caller, id, Err(refusal));
            return;
        };
        self.drives.next += 1;
        let request = self.drives.next;
        self.drives.owed.insert(
            request,
            Owed {
                window,
                caller,
                id,
            },
        );
        self.send(window, ServerMessage::Drive { request, action });
    }

    /// `window`'s answer to the action it was handed as `request`.
    pub(super) fn driven(
        &mut self,
        window: ClientId,
        request: u64,
        ok: Option<Answer>,
        error: Option<String>,
    ) {
        let Owed { caller, id, .. } = match self.drives.owed.entry(request) {
            Entry::Occupied(owed) if owed.get().window == window => owed.remove(),
            _ => return,
        };
        let result = match (ok, error) {
            (Some(answer), None) => Ok(serde_json::to_value(answer).unwrap_or_default()),
            (_, error) => Err(ApiError::new(
                Code::Failed,
                error.unwrap_or_else(|| "the headless window answered nothing".into()),
            )),
        };
        self.api.reply(caller, id, result);
    }

    /// `window` went: what it was handed and never answered is refused.
    /// It may have been carried out.
    pub(super) fn undriven(&mut self, window: ClientId) {
        let gone: Vec<u64> = self
            .drives
            .owed
            .iter()
            .filter(|(_, owed)| owed.window == window)
            .map(|(request, _)| *request)
            .collect();
        for request in gone {
            if let Some(Owed { caller, id, .. }) = self.drives.owed.remove(&request) {
                let refusal = ApiError::new(
                    Code::Failed,
                    "the headless window went away before it answered; the action may have been carried out",
                );
                self.api.reply(caller, id, Err(refusal));
            }
        }
    }
}

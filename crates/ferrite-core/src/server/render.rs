//! Visuals without a window of the Server's own: an agent's `show_visual`
//! is drawn by an attached window that can (`render_visual`), and with
//! none attached the agent hears at once that nothing can draw it.

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

use base64::Engine as _;

use crate::link::ClientId;
use crate::protocol::{ServerMessage, WirePng};
use crate::visual::{RenderRequest, Rendered, Renderer, Reply};

/// The Sessions' renderer: hands each request to the pump.
pub(super) struct Remote(Mutex<Sender<(RenderRequest, Reply)>>);

impl Renderer for Remote {
    fn render(&self, request: RenderRequest, reply: Reply) {
        // A Server already stopping drops the reply, which answers it.
        let _ = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .send((request, reply));
    }
}

/// Requests on their way to a window, and the answers owed.
pub(super) struct Renders {
    incoming: Receiver<(RenderRequest, Reply)>,
    owed: BTreeMap<u64, (ClientId, Reply)>,
    next: u64,
}

pub(super) fn remote() -> (Remote, Renders) {
    let (send, incoming) = mpsc::channel();
    (
        Remote(Mutex::new(send)),
        Renders {
            incoming,
            owed: BTreeMap::new(),
            next: 0,
        },
    )
}

impl Renders {
    /// The requests that arrived, as messages for `drawer`. Without one,
    /// each is answered here and nothing is sent.
    pub(super) fn take(&mut self, drawer: Option<ClientId>) -> Vec<ServerMessage> {
        let mut messages = Vec::new();
        while let Ok((request, reply)) = self.incoming.try_recv() {
            let Some(drawer) = drawer else {
                reply.send(Err("no Ferrite window is open to draw it".into()));
                continue;
            };
            self.next += 1;
            self.owed.insert(self.next, (drawer, reply));
            messages.push(ServerMessage::Render {
                request: self.next,
                id: request.id,
                title: request.title,
                html: request.html,
            });
        }
        messages
    }

    pub(super) fn answer(&mut self, request: u64, png: Option<WirePng>, error: Option<String>) {
        let Some((_, reply)) = self.owed.remove(&request) else {
            return;
        };
        reply.send(match (png, error) {
            (Some(png), None) => base64::engine::general_purpose::STANDARD
                .decode(png.base64)
                .map(|bytes| Rendered {
                    png: bytes,
                    width: png.width,
                    height: png.height,
                })
                .map_err(|error| format!("the window sent an unreadable image: {error}")),
            (_, error) => Err(error.unwrap_or_else(|| "the window drew nothing".into())),
        });
    }

    /// The window drawing these closed: their agents hear so now.
    pub(super) fn client_gone(&mut self, client: ClientId) {
        let gone: Vec<u64> = self
            .owed
            .iter()
            .filter(|(_, (drawer, _))| *drawer == client)
            .map(|(request, _)| *request)
            .collect();
        for request in gone {
            if let Some((_, reply)) = self.owed.remove(&request) {
                reply.send(Err("the window drawing it closed".into()));
            }
        }
    }
}

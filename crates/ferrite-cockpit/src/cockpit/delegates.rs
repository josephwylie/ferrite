//! The operator's hand on a Thread's Delegates (ADR 0047), as T3 Code's
//! Lineage has it for a task in a linked environment: its row opens the
//! Thread there, and the row of a Thread another machine delegated opens
//! that one. Each names its Thread by the Server there and its id for it,
//! so the window finds the machine that answers as that Server. Cancelling
//! one is its Server's: the window asks, and the Thread's digest says what
//! became of it (`ThreadFacts::delegating`).

use super::*;
use crate::notifications::{Alert, RowTarget};
use ferrite_core::cockpit::ServerState;
use ferrite_core::delegate::DelegateId;
use ferrite_core::server::ServerId;
use ferrite_core::MachineKey;
use gpui::App;

/// A machine of this window whose Server answers as a given one.
pub(super) struct Serving {
    pub(super) name: String,
    /// Off screen, by its key; `None` for the machine on screen.
    pub(super) away: Option<MachineKey>,
}

impl CockpitView {
    /// The machine whose Server answers as `server` now: the one on
    /// screen, or one off screen the window has heard from.
    pub(super) fn serving(&self, server: &ServerId, cx: &App) -> Option<Serving> {
        let answers = |state: Option<ServerState>| {
            matches!(state, Some(ServerState::Attached { id: Some(id), .. }) if id == *server)
        };
        let cockpit = self.cockpit.read(cx);
        if answers(cockpit.server()) {
            let name = cockpit
                .bridge(cockpit.on_screen())
                .map_or_else(|| crate::machines::LOCAL.to_string(), |bridge| bridge.name().to_string());
            return Some(Serving { name, away: None });
        }
        self.machines
            .as_ref()?
            .read(cx)
            .off_screen()
            .find(|away| answers(Some(away.status.server())))
            .map(|away| Serving {
                name: away.name,
                away: Some(away.key),
            })
    }

    /// Open Thread `thread` of the machine whose Server is `server`: here
    /// when it is on screen, else by going to it first, as an alert's open
    /// does. A machine no longer reached opens nothing.
    pub(super) fn open_elsewhere(&mut self, server: &ServerId, thread: ThreadId, cx: &mut Context<Self>) {
        match self.serving(server, cx).map(|serving| serving.away) {
            Some(None) => self.notice_verb(Verb::OpenThread(thread), cx),
            Some(Some(away)) => self.go_to_alert(
                Alert {
                    away: Some(away),
                    target: RowTarget::Thread(thread),
                    thread,
                },
                cx,
            ),
            None => {}
        }
    }

    /// Cancel `thread`'s Delegate `delegate`, as its agent's `task_cancel`
    /// does. Its row moves when the digest says so; a refusal is said in
    /// the nav's notice.
    pub(super) fn cancel_delegate(&mut self, thread: ThreadKey, delegate: DelegateId, cx: &mut Context<Self>) {
        let answer = self
            .cockpit
            .update(cx, |cockpit, _| cockpit.cancel_delegate(thread, delegate));
        cx.spawn(async move |this, cx| {
            if let Err(error) = beside::answered(answer, cx).await {
                this.update(cx, |view, cx| {
                    view.nav_notice.problem(error.to_string());
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}

//! What the operator took off the draft, still closing its place in the
//! tray. The draft no longer holds it: the tray draws it as a ghost with no
//! controls (`motion::slip_out`) for `CHIP_OUT`, so the chips beside it close
//! up over that time and not in one jump, and the last one's going closes
//! the tray itself (`motion::slip_shut`).

use std::path::PathBuf;
use std::time::Instant;

use gpui::{App, Context, Window};

use super::Composer;
use ferrite_core::attachment::Readiness;
use ferrite_core::replies::Reply;
use ferrite_ui::motion::CHIP_OUT;

pub(super) enum Gone {
    File {
        path: PathBuf,
        readiness: Option<Readiness>,
    },
    Reply(Reply),
}

/// One thing gone from the draft: its place in what remained (`at`, an
/// index among the files or among the replies) and when it went.
pub(super) struct Departed {
    pub(super) what: Gone,
    pub(super) at: usize,
    pub(super) left: Instant,
}

/// One place in the tray: what the draft holds (by its index among its
/// kind), or what has gone and is closing.
pub(super) enum Place<'a> {
    Kept(usize),
    Gone(&'a Departed),
}

impl Composer {
    /// The ghosts still closing at `now`, oldest first.
    pub(super) fn departed(&self, now: Instant) -> impl DoubleEndedIterator<Item = &Departed> {
        self.leaving
            .iter()
            .filter(move |ghost| now.saturating_duration_since(ghost.left) < CHIP_OUT.duration())
    }

    /// The tray's places for the `kept` things the draft holds and the
    /// ghosts of `mine` among them. Each ghost's `at` counts the places
    /// there were when it went, so the newest is put back first: that
    /// undoes the removals in the order they were made, whatever order the
    /// operator made them in.
    pub(super) fn places(
        &self,
        kept: usize,
        now: Instant,
        mine: impl Fn(&Gone) -> bool,
    ) -> Vec<Place<'_>> {
        let mut places: Vec<_> = (0..kept).map(Place::Kept).collect();
        for ghost in self.departed(now).rev().filter(|ghost| mine(&ghost.what)) {
            places.insert(ghost.at.min(places.len()), Place::Gone(ghost));
        }
        places
    }

    /// Take `path` off the draft. Focus goes where the operator can keep
    /// working: on a keyboard removal from the tray, the tray while it holds
    /// more, else the line.
    pub(crate) fn remove_file(
        &mut self,
        path: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = self.files.paths().iter().position(|kept| kept == path);
        let readiness = self.files.readiness().get(path).cloned();
        let tray_held = self.files_focus.contains_focused(window, cx);
        self.files.remove(path);
        if let Some(at) = at {
            self.depart(
                Gone::File {
                    path: path.to_path_buf(),
                    readiness,
                },
                at,
                cx,
            );
        }
        self.refocus_after_removal(tray_held, window, cx);
        self.edited(cx);
    }

    /// The tray keeps the keyboard a keyboard removal had in it, while
    /// anything is left there to reach; otherwise the line takes it.
    pub(super) fn refocus_after_removal(&self, tray_held: bool, window: &mut Window, cx: &mut App) {
        if tray_held && window.last_input_was_keyboard() && self.attachment_count() > 0 {
            self.files_focus.focus(window, cx);
        } else {
            self.focus_handle.focus(window, cx);
        }
    }

    pub(super) fn depart(&mut self, what: Gone, at: usize, cx: &mut Context<Self>) {
        if ferrite_ui::motion::reduced_motion(cx) {
            return;
        }
        let now = cx.background_executor().now();
        self.leaving
            .retain(|ghost| now.saturating_duration_since(ghost.left) < CHIP_OUT.duration());
        self.leaving.push(Departed {
            what,
            at,
            left: now,
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CHIP_OUT.duration()).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }
}

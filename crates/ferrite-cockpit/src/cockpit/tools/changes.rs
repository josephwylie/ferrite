//! Changes in the window: the root's side of
//! `ferrite_tools::changes::ChangesView`, every diff the window has read,
//! which the root draws wherever it shows (`changes_pane`). The view asks
//! the root where a diff is read now through a `Source` (`changes_origin`);
//! what it asks of the window (a read, a stage, a file opened, a menu) it
//! says as a `ChangesEvent`. Git and the forge are the window's, so the root
//! asks them and hands the answers to the view.

use ferrite_tools::changes::{ChangesEvent, ChangesInputs, ChangesView, Diff, Dropdown, Origin};
use ferrite_ui::source::Source;
use gpui::{App, AppContext as _, Context, Div, Entity};

use crate::cockpit::context_menu::MenuTarget;
use crate::cockpit::menu_button::{Door, Edge};
use crate::cockpit::surfaces::SurfaceId;
use crate::cockpit::CockpitView;
use ferrite_core::ThreadKey;

impl CockpitView {
    /// The window's diffs, heard from their making. The view asks where a
    /// diff is read now as each answer lands (`changes_origin`).
    pub(in crate::cockpit) fn new_changes(cx: &mut Context<Self>) -> Entity<ChangesView> {
        let origin = Source::of(&cx.entity(), |root: &Self, diff: &Diff, cx| root.changes_origin(*diff, cx));
        let changes = cx.new(|_| ChangesView::new(origin));
        cx.subscribe(&changes, Self::on_changes_event).detach();
        changes
    }

    /// Where `diff` is read: its Thread's checkout on its Thread's machine,
    /// or the checkout or Project its pull request is read in.
    pub(in crate::cockpit) fn changes_origin(&self, diff: Diff, cx: &App) -> Option<Origin> {
        match diff {
            Diff::Changes(thread) => Some(Origin::new(
                self.thread_path(thread, cx)?,
                self.cockpit.read(cx).bridge(thread.machine()).cloned(),
            )),
            Diff::PullRequest(target) => Some(Origin::new(self.pr_source_path(target.source, cx)?, None)),
        }
    }

    /// `diff` as the Changes draw it, wherever it shows: the Panel's
    /// Changes Surface, a pull request's Code.
    pub(in crate::cockpit) fn changes_pane(&self, diff: Diff, cx: &mut Context<Self>) -> Div {
        let thread = diff.local_thread();
        let inputs = ChangesInputs {
            head: thread.and_then(|thread| self.thread_branch(thread)),
            default_branch: thread
                .and_then(|thread| self.facts.get(thread))
                .and_then(|facts| facts.default_branch.clone()),
            more: self.doors().door(MenuTarget::ChangesMore(diff)),
        };
        self.changes
            .update(cx, |changes, cx| changes.pane(diff, inputs, cx))
    }

    /// `diff`, made and read the first time it is asked for.
    pub(in crate::cockpit) fn ensure_diff(&mut self, diff: Diff, cx: &mut Context<Self>) {
        if self.changes.update(cx, |changes, cx| changes.make(diff, cx)) {
            self.read_diff(diff, false, cx);
        }
    }

    /// Ask for `diff` in its scope, and hand the answer to the Changes.
    pub(in crate::cockpit) fn read_diff(&mut self, diff: Diff, keep_shown: bool, cx: &mut Context<Self>) {
        let Some(origin) = self.changes_origin(diff, cx) else {
            return;
        };
        let Some((generation, scope)) = self
            .changes
            .update(cx, |changes, cx| changes.begin_read(diff, keep_shown, cx))
        else {
            return;
        };
        match diff {
            Diff::Changes(_) => {
                let answer = self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.changes(diff.machine(), origin.root(), scope));
                self.changes.update(cx, |changes, cx| {
                    changes.wait_local(diff, generation, origin, answer, cx)
                });
            }
            Diff::PullRequest(target) => {
                let answer = if !crate::cockpit::reads_forge(self.cockpit.read(cx), origin.root()) {
                    Err(std::io::Error::other(
                        "No published diff fixture for this pull request",
                    ))
                } else {
                    self.cockpit.update(cx, |cockpit, _| {
                        cockpit.review_pull_request_diff(diff.machine(), origin.root(), target.number)
                    })
                };
                self.changes.update(cx, |changes, cx| {
                    changes.wait_published(diff, generation, origin, answer, cx)
                });
            }
        }
    }

    /// Stage or unstage `path` in `diff`'s checkout; the Changes read the
    /// list again once git answers.
    fn stage_file(&mut self, diff: Diff, path: String, stage: bool, cx: &mut Context<Self>) {
        let Some(origin) = diff.local_thread().and_then(|_| self.changes_origin(diff, cx)) else {
            return;
        };
        let Some(generation) = self.changes.update(cx, |changes, _| changes.begin_stage(diff)) else {
            return;
        };
        let outcome = self.cockpit.update(cx, |cockpit, _| {
            cockpit.set_file_staged(diff.machine(), origin.root(), path.clone(), stage)
        });
        self.changes.update(cx, |changes, cx| {
            changes.wait_stage(diff, generation, origin, path, stage, outcome, cx)
        });
    }

    /// A Files tab's `Show changes`: the Changes column opens on that file,
    /// its diff unfolded and in view.
    pub(in crate::cockpit) fn show_changes_of(
        &mut self,
        thread: ThreadKey,
        path: &str,
        cx: &mut Context<Self>,
    ) {
        self.upsert_surface(thread, SurfaceId::Changes, cx);
        self.changes
            .update(cx, |changes, cx| changes.reveal(Diff::Changes(thread), path, cx));
    }

    pub(in crate::cockpit) fn on_changes_event(
        &mut self,
        _: Entity<ChangesView>,
        event: &ChangesEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ChangesEvent::Read { diff, keep_shown } => self.read_diff(*diff, *keep_shown, cx),
            ChangesEvent::Stage { diff, path, stage } => {
                self.stage_file(*diff, path.clone(), *stage, cx)
            }
            ChangesEvent::Open { thread, path } => {
                if let Some(root) = self.thread_path(*thread, cx) {
                    self.open_in_files(*thread, root.join(path), None, false, cx);
                }
            }
            ChangesEvent::Menu { dropdown, diff, at } => {
                let target = match dropdown {
                    Dropdown::Scope => MenuTarget::ChangesScope(*diff),
                    Dropdown::Base => MenuTarget::ChangesBase(*diff),
                };
                self.open_context_menu(target, *at, cx)
            }
            ChangesEvent::More { diff } => {
                self.toggle_door(Door::new(MenuTarget::ChangesMore(*diff), Edge::Right), cx)
            }
        }
    }
}

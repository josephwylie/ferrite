//! Files in the window: the root's side of `ferrite_tools::files::FilesView`,
//! every Thread's Files, which the Panel draws as the Files Surface's body.
//! The view reads what it draws through a `Source` the root makes over
//! itself (`files_inputs`): the Thread whose Panel shows it, whether the
//! tree fits beside the editor, its `⋯` button, its file tabs' handles in
//! the Panel's row and the diffs read. What it asks (a file opened from the
//! tree, Show changes, Attach, a menu, ⌘P, the `⋯`) it says as a
//! `FilesEvent`. Its file tabs are stops of the Panel's row: the view
//! declares them and the root registers them (`register_files_stops`).
//!
//! Every way a file opens in the right pane — a path in the conversation,
//! `Open in Files`, the tree — arrives at `open_in_files`. Making a
//! Thread's Files needs its checkout, the machine and the tabs it was left
//! with, so the root makes them and keeps the saved tabs (`WorkingSets`,
//! files.json).

use std::path::PathBuf;

use ferrite_core::ThreadKey;
use ferrite_pane::preview::Body as DocBody;
use ferrite_tools::files::{FilesEvent, FilesInputs, FilesView};
use ferrite_tools::remote_files::RemoteFiles;
use ferrite_ui::source::Source;
use ferrite_ui::theme;
use gpui::{AppContext as _, Context, Entity, Window};

use crate::cockpit::context_menu::MenuTarget;
use crate::cockpit::menu_button::{Door, Edge};
use crate::cockpit::panel::PanelControl;
use crate::cockpit::surfaces::SurfaceId;
use crate::cockpit::CockpitView;

impl CockpitView {
    /// The window's Files, reading it through a `Source` over the root.
    /// Their events are heard from the first frame (`hear_views`), since
    /// answering ⌘P needs the window.
    pub(in crate::cockpit) fn new_files(cx: &mut Context<Self>) -> Entity<FilesView> {
        let inputs = Source::of(&cx.entity(), |root: &Self, _: &(), cx| root.files_inputs(cx));
        cx.new(|_| FilesView::new(inputs))
    }

    /// Open `path` in the Files editor for `thread` (at `line`, 1-based),
    /// the Files pane shown. `keep` opens it as a tab of its own; else it
    /// is a preview tab that the next open replaces.
    pub(crate) fn open_in_files(
        &mut self,
        thread: ThreadKey,
        path: PathBuf,
        line: Option<u32>,
        keep: bool,
        cx: &mut Context<Self>,
    ) {
        self.upsert_surface(thread, SurfaceId::Files, cx);
        let beside = self.files_tree_beside(cx);
        self.files
            .update(cx, |files, cx| files.open(thread, path, line, keep, beside, cx));
    }

    /// The Files state for `thread`, made the first time it is asked for:
    /// in the Thread's checkout, on its machine, with the tabs and
    /// settings it was left with.
    pub(in crate::cockpit) fn ensure_files(&self, thread: ThreadKey, cx: &mut Context<Self>) {
        if self.files.read(cx).holds(thread) {
            return;
        }
        let Some(root) = self.thread_path(thread, cx) else {
            return;
        };
        let window = cx.weak_entity();
        let cockpit = self.cockpit.read(cx);
        let remote = cockpit.bridge(thread.machine()).cloned().map(|bridge| {
            let submit = super::remote_files::submit(window, thread.machine(), bridge);
            RemoteFiles::new(root.clone(), submit)
        });
        let stored = cockpit
            .saved(thread)
            .and_then(|saved| self.panel.working_sets.get(&saved).cloned());
        self.files
            .update(cx, |files, cx| files.make(thread, root, remote, stored, cx));
    }

    /// ⌘P: the Files Surface shows, and the tree's filter takes the
    /// keyboard.
    pub(in crate::cockpit) fn go_to_file(&mut self, thread: ThreadKey, window: &mut Window, cx: &mut Context<Self>) {
        self.upsert_surface(thread, SurfaceId::Files, cx);
        self.files.update(cx, |files, cx| files.go_to_file(thread, window, cx));
        cx.notify();
    }

    /// ⌘F with the keyboard in the shown Files surface (its tree, tab strip
    /// or file) or on the Panel's controls over it: the find bar opens over
    /// its file, source or rendered. Whether it did. A Panel showing another
    /// surface, or the keyboard in the conversation, is not Files' to answer.
    pub(in crate::cockpit) fn open_files_find(
        &mut self,
        thread: ThreadKey,
        on_controls: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.active_surface(thread) != Some(SurfaceId::Files) {
            return false;
        }
        let held = on_controls || self.panel_keys_held(window, cx);
        self.files
            .update(cx, |files, cx| files.open_find(thread, held, window, cx))
    }

    pub(in crate::cockpit) fn uses_remote_files(&self, thread: ThreadKey, cx: &gpui::App) -> bool {
        self.elsewhere(thread.machine(), cx) || self.files.read(cx).is_remote(thread)
    }

    /// Why the window cannot let `machine` go now: edited files of its
    /// Threads that letting it go would lose.
    pub(crate) fn files_leave_error(
        &self,
        machine: ferrite_core::MachineId,
        cx: &gpui::App,
    ) -> Option<&'static str> {
        self.files
            .read(cx)
            .unsaved_at_leaving(machine, cx)
            .map(|_| "Save or close edited files before turning this environment off.")
    }

    /// The Panel's stored working sets take the live Files' tabs and
    /// settings, for files.json, each as its Thread's machine's saved name
    /// says. Each machine's Thread list prunes only its own sets, and only
    /// once its Server has said which Threads it has (F-A5).
    pub(in crate::cockpit) fn sync_working_sets(&mut self, cx: &gpui::App) {
        let cockpit = self.cockpit.read(cx);
        let live = self
            .files
            .read(cx)
            .working_sets()
            .filter_map(|(thread, set)| Some((cockpit.saved(thread)?, set)));
        if !self.panel.working_sets.sync(live) {
            return;
        }
        let known = cockpit.known_threads();
        for machine in cockpit.machines().filter(|machine| cockpit.heard(*machine)) {
            let Some(saved_as) = cockpit.saved_as(machine) else {
                continue;
            };
            let threads = known
                .iter()
                .filter(|key| key.machine() == machine)
                .map(|key| key.thread())
                .collect();
            self.panel.working_sets.retain(saved_as, &threads);
        }
    }

    /// Every way a file reaches the right pane: a file opened in a Pane's
    /// preview that is plain text opens here instead (a path in the
    /// conversation, `Open in Files`, a review's line). Pages, a command's
    /// output and what is no text stay with the preview.
    pub(in crate::cockpit) fn redirect_previews_to_files(&mut self, cx: &mut Context<Self>) {
        let opened: Vec<(ThreadKey, PathBuf, Option<u32>)> = self
            .panes
            .iter()
            .filter_map(|pane| {
                let thread = pane.thread()?;
                let document = pane.preview.document()?;
                let plain = document.name.is_none()
                    && matches!(document.body, DocBody::Text { .. })
                    && self.thread_path(thread, cx).is_some_and(|root| document.path.starts_with(root));
                plain.then(|| (thread, document.path.clone(), document.line()))
            })
            .collect();
        for (thread, path, line) in opened {
            if let Some(index) = self.pane_for(thread) {
                self.panes[index].preview.close();
            }
            self.open_in_files(thread, path, line, false, cx);
        }
    }

    pub(in crate::cockpit) fn on_files_event(
        &mut self,
        _: &Entity<FilesView>,
        event: &FilesEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FilesEvent::Open { thread, path, keep } => match self.pane_for(*thread) {
                Some(index)
                    if !self.uses_remote_files(*thread, cx)
                        && ferrite_pane::image_viewer::is_image(path) =>
                {
                    self.panes[index].preview.open(path.clone(), None, &[], window, cx);
                    cx.notify();
                }
                _ => self.open_in_files(*thread, path.clone(), None, *keep, cx),
            },
            FilesEvent::ShowChanges { thread, path } => self.show_changes_of(*thread, path, cx),
            FilesEvent::Attach { thread, path, remote } => {
                if let Some(index) = self.pane_for(*thread) {
                    self.panes[index].composer.update(cx, |composer, cx| {
                        if *remote {
                            composer.insert(
                                &format!("\nPlease use the file at {} on this machine.\n", path.display()),
                                cx,
                            );
                        } else {
                            composer.add_files(&[path.clone()], cx);
                        }
                    });
                }
                cx.notify();
            }
            FilesEvent::Menu { thread, at } => {
                self.open_context_menu(MenuTarget::File(*thread), *at, cx)
            }
            FilesEvent::GoToFile { thread } => self.go_to_file(*thread, window, cx),
            FilesEvent::More { thread } => {
                self.toggle_door(Door::new(MenuTarget::FilesMore(*thread), Edge::Right), cx)
            }
            FilesEvent::Closed { thread, path, at } => self.file_tab_closed(*thread, path, *at, window, cx),
        }
    }

    /// Whether the Panel is wide enough for the tree beside a readable
    /// editor. Narrower, the tree is a page over the editor, and a file
    /// picked from it shows the editor again.
    fn files_tree_beside(&self, cx: &gpui::App) -> bool {
        self.panel_pane(cx)
            .is_some_and(|index| self.panel_rest_w(index) >= theme::FILES_TREE_W + theme::FILES_EDITOR_MIN_W)
    }

    /// Everything Files draws from: the Thread whose Panel shows it.
    fn files_inputs(&self, cx: &gpui::App) -> Option<FilesInputs> {
        let thread = self.panes[self.panel_drawn_pane(cx)?].thread()?;
        (self.active_surface(thread) == Some(SurfaceId::Files)).then(|| FilesInputs {
            thread,
            beside: self.files_tree_beside(cx),
            more: self.doors().door(MenuTarget::FilesMore(thread)),
            stops: self.panel.focus.handles(|control| match control {
                PanelControl::FileTab(path) => Some(path.clone()),
                _ => None,
            }),
            changes: self.changes.clone(),
        })
    }

    /// `thread`'s file tabs, as Files declares them, enter the Panel's row
    /// after the head's stops, before Files draws them.
    pub(in crate::cockpit) fn register_files_stops(&self, thread: ThreadKey, cx: &mut Context<Self>) {
        for (path, semantics) in self.files.read(cx).tab_stops(thread) {
            self.panel.focus.register(PanelControl::FileTab(path), semantics, cx);
        }
    }

    #[cfg(test)]
    pub(in crate::cockpit) fn remote_files_for_test(
        &mut self,
        thread: ThreadKey,
        answer: impl Fn(ferrite_core::file_operations::Request) -> std::io::Result<ferrite_core::file_operations::Reply>
            + 'static,
        cx: &mut Context<Self>,
    ) {
        self.ensure_files(thread, cx);
        self.files
            .update(cx, |files, _| files.remote_for_test(thread, answer));
        if let Some(index) = self.pane_for(thread) {
            self.panes[index].preview.set_remote_files(true);
        }
    }
}


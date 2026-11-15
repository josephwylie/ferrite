//! What a Composer's files are bound for, and whether a send may take them.
//! A file that cannot be sent stays in the draft, marked in its own place in
//! the tray (`ferrite_core::attachment`), until it is repaired or removed.

use super::*;
use ferrite_core::attachment::Destination;
use ferrite_pane::composer::Composer;
use std::path::PathBuf;

impl CockpitView {
    /// `composer`'s files are going to this window's agents: on this
    /// computer or on its machine, and kept in the app's own store.
    pub(super) fn aim_attachments(&self, composer: &Entity<Composer>, cx: &mut Context<Self>) {
        let destination = if self.on_machine() {
            Destination::Remote
        } else {
            Destination::Local
        };
        let store = self.prefs.dir.clone();
        composer.update(cx, |composer, cx| {
            composer.aim_files(destination, store, cx)
        });
    }

    /// Whether pane `index`'s send may go ahead. Its files are looked at as
    /// they are now, so one repaired since it failed passes; one that still
    /// fails stops the send and says why in the tray. The files of this
    /// computer that pass are replaced by retained copies, so what is sent is
    /// what is kept; a file inside the pane's workspace, however it was
    /// attached, is sent as its own path.
    pub(super) fn admit_files(&self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.panes.get(index) else {
            return false;
        };
        let composer = pane.composer.clone();
        if composer.update(cx, |composer, cx| composer.has_blocked_file(cx)) {
            return false;
        }
        let root = self.pane_root(index);
        composer.update(cx, |composer, cx| composer.retain_files(root.as_deref(), cx))
    }

    /// The folder pane `index`'s agent works in: its Thread's, or the one
    /// its draft is bound to.
    pub(super) fn pane_root(&self, index: usize) -> Option<PathBuf> {
        let pane = self.panes.get(index)?;
        match (pane.thread(), pane.draft()) {
            (Some(thread), _) => Some(self.cockpit.thread(thread)?.workspace()?.cwd().to_path_buf()),
            (None, Some(draft)) => Some(
                draft
                    .binding
                    .resolve(self.cockpit.registry(), !self.on_machine())
                    .ok()?
                    .source_root()
                    .to_path_buf(),
            ),
            _ => None,
        }
    }
}

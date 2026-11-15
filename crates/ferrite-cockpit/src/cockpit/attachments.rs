//! What a Composer's files are bound for, and whether a send may take them.
//! A file that cannot be sent stays in the draft, marked in its own place in
//! the tray (`ferrite_core::attachment`), until it is repaired or removed.

use super::*;
use ferrite_core::attachment::Destination;
use ferrite_pane::composer::Composer;

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

    /// Whether `composer`'s send may go ahead. Its files are looked at as
    /// they are now, so one repaired since it failed passes; one that still
    /// fails stops the send and says why in the tray. The files that pass
    /// are replaced by retained copies, so what is sent is what is kept.
    pub(super) fn admit_files(&self, composer: &Entity<Composer>, cx: &mut Context<Self>) -> bool {
        if composer.update(cx, |composer, cx| composer.has_blocked_file(cx)) {
            return false;
        }
        composer.update(cx, |composer, cx| composer.retain_files(cx));
        true
    }
}

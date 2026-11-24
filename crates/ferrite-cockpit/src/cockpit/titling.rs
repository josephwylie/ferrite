//! A Thread's title from its first prompt, written by its own Provider off
//! the UI thread (`ferrite_core::titler`).
//!
//! Only the root asks and only the answer writes which Threads are being
//! named, so it is a plain struct on the root (ADR 0041). The nav reads it
//! as it builds a Thread's row.

use super::*;
use ferrite_core::ThreadKey;

/// The Threads the titler is naming right now: their cards draw the title
/// at `NAV_CARD_RETITLING_OPACITY` until its answer lands.
#[derive(Default)]
pub(super) struct Titling(std::collections::HashSet<ThreadKey>);

impl Titling {
    /// Whether the titler is naming `thread`.
    pub(super) fn naming(&self, thread: ThreadKey) -> bool {
        self.0.contains(&thread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Thread 3 here and Thread 3 on a machine are two Threads: naming one
    /// is not naming the other.
    #[test]
    fn a_title_being_written_keeps_two_machines_threads_apart() {
        let (here, there) = (ThreadKey::fixture(0, 3), ThreadKey::fixture(1, 3));
        let mut titling = Titling::default();
        titling.0.insert(here);
        assert!(titling.naming(here));
        assert!(!titling.naming(there));
    }
}

impl CockpitView {
    /// Ask the titler for a name, off the UI thread, and adopt it when it
    /// arrives — unless the operator has named the Thread meanwhile, or
    /// turned auto-titles off. No titler configured (tests, the demo)
    /// means the prompt-derived name stands.
    pub(super) fn start_titling(&mut self, thread: ThreadKey, prompt: String, cx: &mut Context<Self>) {
        if !self.prefs.settings.auto_title || !self.prefs.titler {
            return;
        }
        let Some(provider) = self
            .cockpit
            .read(cx)
            .thread(thread)
            .filter(|open| open.title().is_none())
            .map(|open| open.provider())
        else {
            return;
        };
        // The Thread's own Provider writes its title, through the one form
        // each adapter fills: Claude's `claude -p`, Codex's `codex exec` —
        // the same copy of the CLI the Session runs.
        let program = ferrite_core::providers::discover::program(provider);
        let form = ferrite_core::titler::form_for(
            provider,
            &program,
            &ferrite_core::titler::TitleRequest {
                prompt,
                reply: None,
            },
            &self.cockpit.read(cx).model_catalog(thread.machine(), provider),
        );
        let rx = ferrite_core::titler::spawn(form);
        self.titling.0.insert(thread);
        cx.spawn(async move |this, cx| {
            let title = cx
                .background_executor()
                .spawn(async move { rx.recv().ok().flatten() })
                .await;
            this.update(cx, |view, cx| view.adopt_title(thread, title, cx))
                .ok();
        })
        .detach();
    }

    /// The titler's answer lands only on a Thread still untitled: an
    /// operator's rename in the meantime wins, and a parked Thread keeps
    /// its prompt-derived name.
    pub(super) fn adopt_title(&mut self, thread: ThreadKey, title: Option<String>, cx: &mut Context<Self>) {
        if self.titling.0.remove(&thread) {
            self.notify_part(parts::Part::Nav, cx);
        }
        let Some(title) = title else {
            return;
        };
        if self
            .cockpit
            .read(cx)
            .thread(thread)
            .is_none_or(|open| open.title().is_some())
        {
            return;
        }
        let renamed = self.cockpit.update(cx, |cockpit, _| cockpit.rename_thread(thread, &title));
        self.when_answered(
            renamed,
            move |view, renamed, cx| {
                if renamed.is_ok() {
                    view.facts.renamed(view.cockpit.read(cx), thread);
                    view.refresh_names();
                    cx.notify();
                }
            },
            cx,
        );
    }
}

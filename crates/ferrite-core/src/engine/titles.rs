//! A Thread's title from the operator's first prompt, written by its own
//! Provider on the machine the Thread runs on (`Spawner::title`). The
//! Engine asks once per Thread, however many windows watch it, and the
//! answer lands as a rename does, on every window. A Thread titled by
//! then, by the operator, the API or the Factory, keeps its title.

use super::*;
use crate::titler::{Job, Titled};

/// Whether Settings let the titler name Threads (`auto_title`), and the
/// channel its answers come back on.
pub(super) struct Titles {
    pub(super) on: bool,
    written: (Sender<Titled>, Receiver<Titled>),
}

impl Default for Titles {
    fn default() -> Self {
        Self {
            on: true,
            written: crate::doorbell::channel(),
        }
    }
}

/// Where a Thread's title from its first prompt is.
#[derive(Default)]
pub(super) enum Titling {
    #[default]
    Idle,
    /// The operator's first prompt went out: the titler is to be asked.
    Owed(String),
    /// The titler is writing it.
    Writing,
}

impl Titling {
    /// Whether a title is on its way, as the nav shows it.
    pub(super) fn writing(&self) -> bool {
        !matches!(self, Titling::Idle)
    }
}

impl Engine {
    /// Ask the titler for each Thread owed a title, then take what it
    /// wrote. Asked and answered on the same beat, a titler that writes
    /// none at once leaves the Thread as it was.
    pub(super) fn title_first_prompts(&mut self) {
        let mut jobs = Vec::new();
        for (id, thread) in self.threads.walk_mut() {
            if !matches!(thread.titling, Titling::Owed(_)) {
                continue;
            }
            let Titling::Owed(prompt) = std::mem::take(&mut thread.titling) else {
                unreachable!("matched above");
            };
            thread.state_changed = true;
            if self.titles.on && thread.title.is_none() {
                thread.titling = Titling::Writing;
                jobs.push((*id, thread.provider, prompt));
            }
        }
        for (thread, provider, prompt) in jobs {
            let job = Job {
                thread,
                provider,
                prompt,
                catalog: self.model_catalog(provider),
            };
            self.spawner.title(job, self.titles.written.0.clone());
        }
        while let Ok(titled) = self.titles.written.1.try_recv() {
            self.take_title(titled);
        }
    }

    /// The titler's answer is the Thread's title only while it is still
    /// untitled and titles are on; either way it is no longer written.
    fn take_title(&mut self, titled: Titled) {
        let Some(thread) = self
            .threads
            .get_mut_if(&titled.thread, |thread| thread.titling.writing())
        else {
            return;
        };
        thread.titling = Titling::Idle;
        let untitled = thread.title.is_none();
        let Some(title) = titled.title.filter(|_| untitled && self.titles.on) else {
            return;
        };
        if let Err(error) = self.rename_thread(titled.thread, &title) {
            crate::log!("thread {}: could not take its title: {error}", titled.thread);
        }
    }
}

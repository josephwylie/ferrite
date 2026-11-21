//! What a draft's `/` and `$` menus list before it has a Session: the
//! commands its Provider finds in its folder, asked once per Provider and
//! folder.
//!
//! Only the root asks and only the root takes the answer, in its pump and
//! as a menu is derived, so it is a plain struct on the root (ADR 0041),
//! shaped like the import picker's listing: the root asks (`listen`) and
//! takes each answer (`listed`).

use ferrite_core::providers::commands::Discovery;
use ferrite_core::SessionCommand;

use super::*;

/// One Provider's commands in one folder. Its Provider and folder keep a
/// late answer from listing one draft's commands in another's menu.
pub(super) struct Listing {
    provider: Provider,
    cwd: std::path::PathBuf,
    pending: Option<Discovery>,
    pub(super) commands: Vec<SessionCommand>,
    error: Option<String>,
}

impl Listing {
    /// Whether the ask is still out.
    pub(super) fn loading(&self) -> bool {
        self.pending.is_some()
    }

    /// Why the commands could not be listed.
    pub(super) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// The draft's listing, or none while no menu asks for one.
#[derive(Default)]
pub(super) struct DraftCommands(Option<Listing>);

impl DraftCommands {
    /// Whether the listing is `provider`'s in `cwd`.
    fn is_for(&self, provider: Provider, cwd: &std::path::Path) -> bool {
        self.0
            .as_ref()
            .is_some_and(|listing| listing.provider == provider && listing.cwd == cwd)
    }

    /// Wait for `provider`'s commands in `cwd`, in place of any listing.
    fn listen(&mut self, provider: Provider, cwd: std::path::PathBuf, reply: Option<Discovery>) {
        self.0 = Some(Listing {
            provider,
            cwd,
            pending: reply,
            commands: Vec::new(),
            error: None,
        });
    }

    /// Take the answer, if it came; whether it did.
    pub(super) fn listed(&mut self) -> bool {
        let Some(listing) = &mut self.0 else {
            return false;
        };
        let Some(rx) = &listing.pending else {
            return false;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(error) => Err(std::io::Error::other(error)),
        };
        listing.pending = None;
        match result {
            Ok(commands) => {
                listing.commands = commands;
                listing.error = None;
            }
            Err(error) => listing.error = Some(error.to_string()),
        }
        true
    }

    /// Drop the listing: the next menu asks again.
    pub(super) fn forget(&mut self) {
        self.0 = None;
    }
}

impl CockpitView {
    /// The draft's commands for `provider` in `cwd`, asked for afresh when
    /// either changed, with the answer taken if it came. Once they are
    /// listed, the draft's line `line` knows them.
    pub(super) fn draft_commands(
        &mut self,
        provider: Provider,
        cwd: std::path::PathBuf,
        line: &Entity<Composer>,
        cx: &mut Context<Self>,
    ) -> &Listing {
        if !self.draft_commands.is_for(provider, &cwd) {
            let reply = self.cockpit.update(cx, |cockpit, _| cockpit.discover_commands(provider, &cwd));
            self.draft_commands.listen(provider, cwd, reply);
        }
        self.draft_commands.listed();
        let listing = self.draft_commands.0.as_ref().expect("listened above");
        if !listing.loading() && listing.error.is_none() {
            line.update(cx, |composer, cx| composer.set_commands(&listing.commands, cx));
        }
        listing
    }
}

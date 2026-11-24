//! What a draft's `/` and `$` menus list before it has a Session: the
//! commands its Provider finds in its folder, asked of its machine's Server
//! once per machine, Provider and folder.
//!
//! Only the root asks and only the root takes the answer, in its pump and
//! as a menu is derived, so it is a plain struct on the root (ADR 0041),
//! shaped like the import picker's listing: the root asks (`listen`) and
//! takes each answer (`listed`).

use ferrite_client::cockpit::Ask;
use ferrite_core::SessionCommand;

use super::*;

/// One Provider's commands in one folder of one machine. Its machine,
/// Provider and folder keep a late answer from listing one draft's
/// commands in another's menu; `place` names the machine in a failure.
pub(super) struct Listing {
    machine: ferrite_core::MachineId,
    place: SharedString,
    provider: Provider,
    cwd: std::path::PathBuf,
    pending: Option<Ask<Vec<SessionCommand>>>,
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
    /// Whether the listing is `provider`'s in `machine`'s `cwd`.
    fn is_for(&self, machine: ferrite_core::MachineId, provider: Provider, cwd: &std::path::Path) -> bool {
        self.0.as_ref().is_some_and(|listing| {
            listing.machine == machine && listing.provider == provider && listing.cwd == cwd
        })
    }

    /// Wait for `provider`'s commands in `machine`'s `cwd`, in place of any
    /// listing.
    fn listen(
        &mut self,
        machine: ferrite_core::MachineId,
        place: SharedString,
        provider: Provider,
        cwd: std::path::PathBuf,
        reply: Ask<Vec<SessionCommand>>,
    ) {
        self.0 = Some(Listing {
            machine,
            place,
            provider,
            cwd,
            pending: Some(reply),
            commands: Vec::new(),
            error: None,
        });
    }

    /// Take the answer, if it came; whether it did.
    pub(super) fn listed(&mut self) -> bool {
        let Some(listing) = &mut self.0 else {
            return false;
        };
        let Some(result) = listing.pending.as_mut().and_then(Ask::answer) else {
            return false;
        };
        listing.pending = None;
        match result {
            Ok(commands) => {
                listing.commands = commands;
                listing.error = None;
            }
            Err(error) => listing.error = Some(super::machines::said(&error, &listing.place)),
        }
        true
    }

    /// Drop the listing: the next menu asks again.
    pub(super) fn forget(&mut self) {
        self.0 = None;
    }
}

impl CockpitView {
    /// The draft's commands for `provider` in `machine`'s `cwd`, asked for
    /// afresh when any of them changed, with the answer taken if it came.
    /// Once they are listed, the draft's line `line` knows them.
    pub(super) fn draft_commands(
        &mut self,
        machine: ferrite_core::MachineId,
        provider: Provider,
        cwd: std::path::PathBuf,
        line: &Entity<Composer>,
        cx: &mut Context<Self>,
    ) -> &Listing {
        if !self.draft_commands.is_for(machine, provider, &cwd) {
            let reply = self
                .cockpit
                .update(cx, |cockpit, _| cockpit.list_commands(machine, provider, &cwd));
            let place = self.place_of(machine, cx);
            self.draft_commands.listen(machine, place, provider, cwd, reply);
        }
        self.draft_commands.listed();
        let listing = self.draft_commands.0.as_ref().expect("listened above");
        if !listing.loading() && listing.error.is_none() {
            line.update(cx, |composer, cx| composer.set_commands(&listing.commands, cx));
        }
        listing
    }
}

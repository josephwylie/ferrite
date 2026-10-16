//! Projects (ADR 0030, ADR 0031). herdr-projects 0.2, vendored as
//! `ferrite_projects`, still draws the nav's project blocks and the bottom
//! bar's word through `data`, until it leaves; 1.0's work board is
//! `crate::board`. The cockpit's half — the palette commands, the board's
//! place, the nav blocks — is `cockpit::projects`.

mod data;
#[cfg(test)]
mod tests;

pub(crate) use data::{configure, doctor_body, Model, Place, DOCTOR_TITLE};
#[cfg(test)]
pub(crate) use data::{ProjectEntry, ThreadEntry};

gpui::actions!(
    projects,
    [
        // The work board, open or back to where you came from
        // (herdr-projects' prefix+a).
        Toggle,
    ]
);

/// The Projects commands the palette lists, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Board,
    Focus,
    Unfocus,
    Configure,
    Doctor,
}

impl Command {
    pub(crate) const ALL: [Command; 5] = [
        Command::Board,
        Command::Focus,
        Command::Unfocus,
        Command::Configure,
        Command::Doctor,
    ];

    /// The palette row's name.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Command::Board => "Projects",
            Command::Focus => "Projects: focus nav on this project",
            Command::Unfocus => "Projects: clear nav focus",
            Command::Configure => "Projects: configure",
            Command::Doctor => "Projects: doctor",
        }
    }

    /// The muted words after it.
    pub(crate) fn detail(self) -> &'static str {
        match self {
            Command::Board => "the work board",
            Command::Configure => "progress hooks and skill",
            Command::Doctor => "check setup",
            _ => "",
        }
    }
}

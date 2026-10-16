//! Projects (ADR 0030, ADR 0031). herdr-projects 0.2, vendored as
//! `ferrite_projects`, still answers the palette's configure and doctor
//! through `data`, until it leaves; 1.0's work board is `crate::board`,
//! and the nav's rows and the bottom bar's tally come from that board
//! (`crate::sidebar`). The cockpit's half — the palette commands, the
//! board's place — is `cockpit::projects`.

mod data;
#[cfg(test)]
mod tests;

pub(crate) use data::{configure, doctor_body, Place, DOCTOR_TITLE};

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
    Configure,
    Doctor,
}

impl Command {
    pub(crate) const ALL: [Command; 3] = [
        Command::Board,
        Command::Configure,
        Command::Doctor,
    ];

    /// The palette row's name.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Command::Board => "Projects",
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
        }
    }
}

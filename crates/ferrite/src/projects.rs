//! Projects (ADR 0030): herdr-projects, vendored as `ferrite_projects`,
//! drawn natively. Its popup, overview and asks are the Projects sheet
//! (`sheet`); its sidebar rows are nav rows, its tab-bar count a word in
//! the bottom bar; everything reads the crate's own data through `data`.
//! The cockpit's half — the palette commands, the sheet's place on the
//! board, the nav blocks — is `cockpit::projects`.

mod data;
mod sheet;
#[cfg(test)]
mod tests;

pub(crate) use data::{
    adoptable, configure, doctor_body, pickable, Jump, Model, Place, Verb, DOCTOR_TITLE,
};
#[cfg(test)]
pub(crate) use data::{ProjectEntry, ThreadEntry};
pub(crate) use sheet::{run_verb, Ask, Outcome, ProjectsSheet, SheetEvent};

gpui::actions!(
    projects,
    [
        // The Projects popup, open or shut (herdr-projects' prefix+a).
        Toggle,
    ]
);

/// herdr-projects' action menu, as palette commands, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Popup,
    New,
    Verb(Verb),
    Overview,
    Focus,
    Unfocus,
    Adopt,
    Configure,
    Doctor,
}

impl Command {
    pub(crate) const ALL: [Command; 11] = [
        Command::New,
        Command::Verb(Verb::Open),
        Command::Popup,
        Command::Overview,
        Command::Focus,
        Command::Unfocus,
        Command::Adopt,
        Command::Verb(Verb::Pause),
        Command::Verb(Verb::Resume),
        Command::Configure,
        Command::Doctor,
    ];

    /// The palette row's name: the action menu's title.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Command::Popup => "Projects",
            Command::New => "Projects: new project",
            Command::Verb(Verb::Open) => "Projects: open project",
            Command::Verb(Verb::Pause) => "Projects: pause project",
            Command::Verb(Verb::Resume) => "Projects: resume project",
            Command::Overview => "Projects: overview",
            Command::Focus => "Projects: focus nav on this project",
            Command::Unfocus => "Projects: clear nav focus",
            Command::Adopt => "Projects: continue this Thread as a project",
            Command::Configure => "Projects: configure",
            Command::Doctor => "Projects: doctor",
        }
    }

    /// The muted words after it.
    pub(crate) fn detail(self) -> &'static str {
        match self {
            Command::Popup => "threads, tasks, inbox, routines, settings, memory",
            Command::Configure => "progress hooks and skill",
            Command::Doctor => "check setup",
            _ => "",
        }
    }
}

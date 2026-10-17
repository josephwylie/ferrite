//! Projects (ADR 0031): herdr-projects 1.0's work board is `crate::board`,
//! and the nav's rows and the bottom bar's tally come from that board
//! (`crate::sidebar`); the palette's doctor runs the shipped
//! `ferrite-projects`. The cockpit's half — the palette commands, the
//! board's place — is `cockpit::projects`.

use std::process::{Command as Process, Output, Stdio};

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
    Doctor,
}

impl Command {
    pub(crate) const ALL: [Command; 2] = [Command::Board, Command::Doctor];

    /// The palette row's name.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Command::Board => "Projects",
            Command::Doctor => DOCTOR_TITLE,
        }
    }

    /// The muted words after it.
    pub(crate) fn detail(self) -> &'static str {
        match self {
            Command::Board => "the work board",
            Command::Doctor => "check setup",
        }
    }
}

/// The doctor action's notification title.
const DOCTOR_TITLE: &str = "Projects: doctor";

/// The doctor action: the shipped program's `doctor`, which looks at older
/// installs again and says where things stand. Blocking: run it off the UI
/// thread. Its toast: title, words, whether it failed.
pub(crate) fn doctor() -> (String, String, bool) {
    let output = Process::new(sw_board::store::files::bin())
        .arg("doctor")
        .stdin(Stdio::null())
        .output();
    doctor_said(output)
}

/// What `doctor` ended with, as its toast: what it said when it succeeded.
fn doctor_said(output: std::io::Result<Output>) -> (String, String, bool) {
    let (body, failed) = match output {
        Ok(output) if output.status.success() => (
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
            false,
        ),
        Ok(_) => (
            "Some checks FAILED. Details: ferrite-projects doctor".into(),
            true,
        ),
        Err(error) => (format!("ferrite-projects did not run: {error}"), true),
    };
    (DOCTOR_TITLE.into(), body, failed)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn ended(code: i32, stdout: &str) -> std::io::Result<Output> {
        Ok(Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.into(),
            stderr: Vec::new(),
        })
    }

    #[test]
    fn the_doctor_toasts_what_it_said_or_that_it_failed() {
        assert_eq!(
            doctor_said(ended(
                0,
                "No older install of herdr-projects or herdr-superwork on this machine.\n"
            )),
            (
                "Projects: doctor".into(),
                "No older install of herdr-projects or herdr-superwork on this machine.".into(),
                false
            )
        );
        assert_eq!(
            doctor_said(ended(1, "")),
            (
                "Projects: doctor".into(),
                "Some checks FAILED. Details: ferrite-projects doctor".into(),
                true
            )
        );
        let missing = doctor_said(Err(std::io::ErrorKind::NotFound.into()));
        assert!(
            missing.2 && missing.1.starts_with("ferrite-projects did not run"),
            "{missing:?}"
        );
    }
}

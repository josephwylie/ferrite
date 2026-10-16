//! Projects at Server start (ADR 0031): herdr-projects 1.0's take-over of
//! older installs (the 0.2 Projects set aside, its hooks and skill links
//! taken out), before the keeper starts. It is the shipped
//! `ferrite-projects take-over`, run off the pump's thread; a failure is
//! logged and never holds the Server up. The program says what it did once,
//! and nothing when there was nothing to take over; what it says is for the
//! user: besides the log, a toast once a window is there to show it.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::api::schema::{Method, NotificationShowParams};
use crate::api::Request;

/// How often, and how many times, the take-over's notes look for a window:
/// the Server starts before its first window says hello.
const TELL_EVERY: Duration = Duration::from_secs(2);
const TELL_TRIES: u32 = 150;

/// Start the Projects for the Store in `store`, in the background.
pub(super) fn start(store: &Path) {
    // Links resolved, so a `ferrite` linked onto `PATH` finds its sibling.
    let exe = std::env::current_exe().map(|exe| std::fs::canonicalize(&exe).unwrap_or(exe));
    let Ok(exe) = exe else {
        crate::log!("server: Projects are off: this executable's path is unknown");
        return;
    };
    let binary = binary(std::env::var_os("FERRITE_PROJECTS_BIN"), &exe);
    if !binary.is_file() {
        crate::log!("server: Projects are off: no {}", binary.display());
        return;
    }
    // What `ferrite-projects` reaches this Server with.
    let env = [
        ("FERRITE_BIN", exe.into_os_string()),
        (
            "FERRITE_SOCKET",
            crate::api::address(store).into_os_string(),
        ),
    ];
    let store = store.to_path_buf();
    let started = std::thread::Builder::new()
        .name("projects".into())
        .spawn(move || startup(&binary, &store, &env));
    if let Err(error) = started {
        crate::log!("server: Projects are off: {error}");
    }
}

/// The shipped `ferrite-projects`: `FERRITE_PROJECTS_BIN`, else beside
/// the running executable — as `ferrite-projects` names itself.
fn binary(given: Option<OsString>, exe: &Path) -> PathBuf {
    given
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| exe.with_file_name("ferrite-projects"))
}

fn startup(binary: &Path, store: &Path, env: &[(&str, OsString)]) {
    let notes = run(binary, env, &["take-over".as_ref()]);
    let socket = crate::api::address(store);
    tell(&notes, |body| toast(&socket, body), TELL_EVERY, TELL_TRIES);
}

/// The notes as one toast, asked for until a window shows it.
fn tell(notes: &[String], mut show: impl FnMut(&str) -> bool, every: Duration, tries: u32) {
    if notes.is_empty() {
        return;
    }
    let body = notes.join("\n");
    for _ in 0..tries {
        if show(&body) {
            return;
        }
        std::thread::sleep(every);
    }
    crate::log!("server: Projects: no window showed the take-over's notes");
}

/// A toast in every window through the Server's own API; true when a
/// window was there to show it.
fn toast(socket: &Path, body: &str) -> bool {
    let request = Request {
        id: "projects".into(),
        method: Method::NotificationShow(NotificationShowParams {
            title: "Projects took over the older version".into(),
            body: Some(body.into()),
            sound: crate::protocol::WireSound::None,
            important: false,
        }),
    };
    let mut answer = Vec::new();
    if crate::api::cli::call(&request, socket, &mut answer, &mut std::io::sink()) != 0 {
        return false;
    }
    serde_json::from_slice::<serde_json::Value>(&answer)
        .ok()
        .and_then(|answer| answer["result"]["windows"].as_u64())
        .is_some_and(|windows| windows > 0)
}

/// What `ferrite-projects` said on success, line by line; a failure is logged.
fn run(binary: &Path, env: &[(&str, OsString)], args: &[&OsStr]) -> Vec<String> {
    let output = Command::new(binary)
        .args(args)
        .envs(env.iter().map(|(key, value)| (key, value)))
        // The Server is no Thread, whatever started it.
        .env_remove("FERRITE_THREAD_ID")
        .stdin(Stdio::null())
        .output();
    let said = args.join(OsStr::new(" "));
    let said = said.to_string_lossy();
    match output {
        // What it says on success is for the user: the take-over's notes.
        Ok(output) if output.status.success() => {
            let notes: Vec<String> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::to_owned)
                .collect();
            for note in &notes {
                crate::log!("server: Projects: {note}");
            }
            return notes;
        }
        Ok(output) => crate::log!(
            "server: `ferrite-projects {said}` failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(error) => crate::log!("server: `ferrite-projects {said}` did not run: {error}"),
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh per-test scratch directory.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-projects-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_binary_is_the_override_else_beside_the_executable() {
        let exe = Path::new("/Applications/Ferrite.app/Contents/MacOS/ferrite");
        assert_eq!(
            binary(None, exe),
            Path::new("/Applications/Ferrite.app/Contents/MacOS/ferrite-projects")
        );
        assert_eq!(binary(Some("".into()), exe), binary(None, exe));
        assert_eq!(binary(Some("/opt/fp".into()), exe), Path::new("/opt/fp"));
    }

    /// A stand-in `ferrite-projects` that writes down each call: its
    /// arguments, and the variables it reached the Server with.
    #[cfg(unix)]
    fn recorder(dir: &Path, status: i32) -> (PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let calls = dir.join("calls");
        let script = dir.join("ferrite-projects");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$* | $FERRITE_BIN $FERRITE_SOCKET ${{FERRITE_THREAD_ID:-none}}\" >> '{}'\necho refused >&2\nexit {status}\n",
                calls.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        (script, calls)
    }

    #[cfg(unix)]
    #[test]
    fn the_take_over_runs_with_what_reaches_this_server() {
        let dir = scratch("take-over");
        let (script, calls) = recorder(&dir, 0);
        let env = [
            ("FERRITE_BIN", "/f".into()),
            ("FERRITE_SOCKET", "/s".into()),
        ];
        startup(&script, &dir, &env);
        assert_eq!(
            std::fs::read_to_string(&calls).unwrap(),
            "take-over | /f /s none\n"
        );
    }

    #[test]
    fn the_notes_are_one_toast_once_a_window_shows_it() {
        let mut asked = Vec::new();
        let notes = [
            "The old herdr-projects (0.2) was set aside.".to_string(),
            "Taken out of the old herdr-projects: its hooks.".to_string(),
        ];
        tell(
            &notes,
            |body| {
                asked.push(body.to_string());
                asked.len() == 3
            },
            Duration::ZERO,
            10,
        );
        assert_eq!(
            asked,
            vec![notes.join("\n"); 3],
            "asked until a window was there"
        );

        asked.clear();
        tell(
            &notes,
            |body| {
                asked.push(body.to_string());
                false
            },
            Duration::ZERO,
            4,
        );
        assert_eq!(asked.len(), 4, "then given up on");
        tell(&[], |_| unreachable!("nothing to say"), Duration::ZERO, 4);
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_take_over_is_logged_not_shown() {
        let dir = scratch("failing");
        let (script, _) = recorder(&dir, 1);
        assert!(run(&script, &[], &["take-over".as_ref()]).is_empty());
        // A binary that is not there is logged, not a panic.
        startup(&dir.join("missing"), &dir, &[]);
    }
}

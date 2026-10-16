//! Projects at Server start while the shipped `ferrite-projects` is still
//! 0.2.34's (ADR 0030), until upstream's 1.0 command line replaces it (ADR
//! 0031): the one-time migration of ADR 0028's `coordinators.json`, then
//! the Projects ticker, off the pump's thread; a failure is logged and
//! never holds the Server up. `ticker start` itself does nothing when the
//! Projects root has no project. The migration's notes are for the user:
//! besides the log, they are a toast once a window is there to show it.
//! This file leaves with the 0.2 crate.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{run, tell, toast, TELL_EVERY, TELL_TRIES};

/// What ADR 0028's coordinators were kept in, beside the Store.
const COORDINATORS: &str = "coordinators.json";

/// Start 0.2's Projects for the Store in `store` with its `program`, in
/// the background; `exe` is this executable.
pub(super) fn start(program: PathBuf, exe: &Path, store: &Path) {
    // What `ferrite-projects` reaches this Server with.
    let env = [
        ("FERRITE_BIN", exe.as_os_str().to_owned()),
        (
            "FERRITE_SOCKET",
            crate::api::address(store).into_os_string(),
        ),
    ];
    let store = store.to_path_buf();
    let started = std::thread::Builder::new()
        .name("projects".into())
        .spawn(move || startup(&program, &store, &env));
    if let Err(error) = started {
        crate::log!("server: Projects are off: {error}");
    }
}

fn startup(program: &Path, store: &Path, env: &[(&str, OsString)]) {
    let coordinators = store.join(COORDINATORS);
    let notes = if coordinators.exists() {
        run(
            program,
            env,
            &[
                "migrate-ferrite-coordinators".as_ref(),
                coordinators.as_os_str(),
            ],
        )
    } else {
        Vec::new()
    };
    run(program, env, &["ticker".as_ref(), "start".as_ref()]);
    let socket = crate::api::address(store);
    let title = "Ferrite's coordinators are now projects";
    tell(
        &notes,
        |body| toast(&socket, title, body),
        TELL_EVERY,
        TELL_TRIES,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn scratch(name: &str) -> PathBuf {
        crate::server::tests::scratch(&format!("projects-legacy-{name}"))
    }

    #[cfg(unix)]
    #[test]
    fn coordinators_are_migrated_before_the_ticker_starts() {
        let dir = scratch("order");
        let (script, calls) = recorder(&dir, 0);
        let env = [
            ("FERRITE_BIN", "/f".into()),
            ("FERRITE_SOCKET", "/s".into()),
        ];
        startup(&script, &dir, &env);
        assert_eq!(
            std::fs::read_to_string(&calls).unwrap(),
            "ticker start | /f /s none\n",
            "nothing to migrate"
        );

        std::fs::remove_file(&calls).unwrap();
        let coordinators = dir.join(COORDINATORS);
        std::fs::write(&coordinators, "{}").unwrap();
        startup(&script, &dir, &env);
        assert_eq!(
            std::fs::read_to_string(&calls).unwrap(),
            format!(
                "migrate-ferrite-coordinators {} | /f /s none\nticker start | /f /s none\n",
                coordinators.display()
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_migration_still_starts_the_ticker() {
        let dir = scratch("failing");
        let (script, calls) = recorder(&dir, 1);
        std::fs::write(dir.join(COORDINATORS), "{}").unwrap();
        startup(&script, &dir, &[]);
        let calls = std::fs::read_to_string(&calls).unwrap();
        assert_eq!(calls.lines().count(), 2, "{calls}");
        assert!(calls.lines().last().unwrap().starts_with("ticker start"));
        // A program that is not there is logged, not a panic.
        startup(&dir.join("missing"), &dir, &[]);
    }
}

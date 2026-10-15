//! Projects at Server start (ADR 0030), in place of Herdr's plugin
//! `[[startup]]` hook: the one-time migration of ADR 0028's
//! `coordinators.json`, then the Projects ticker. Both are the shipped
//! `ferrite-projects`, run off the pump's thread; a failure is logged and
//! never holds the Server up. `ticker start` itself does nothing when the
//! Projects root has no project.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What ADR 0028's coordinators were kept in, beside the Store.
const COORDINATORS: &str = "coordinators.json";

/// Start the Projects for the Store in `store`, in the background.
pub(super) fn start(store: &Path) {
    let Ok(exe) = std::env::current_exe() else {
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
        ("FERRITE_SOCKET", crate::api::address(store).into_os_string()),
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
    let coordinators = store.join(COORDINATORS);
    if coordinators.exists() {
        run(
            binary,
            env,
            &["migrate-ferrite-coordinators".as_ref(), coordinators.as_os_str()],
        );
    }
    run(binary, env, &["ticker".as_ref(), "start".as_ref()]);
}

fn run(binary: &Path, env: &[(&str, OsString)], args: &[&OsStr]) {
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
        Ok(output) if output.status.success() => {}
        Ok(output) => crate::log!(
            "server: `ferrite-projects {said}` failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(error) => crate::log!("server: `ferrite-projects {said}` did not run: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh per-test scratch directory.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-projects-{}-{name}", std::process::id()));
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
    fn coordinators_are_migrated_before_the_ticker_starts() {
        let dir = scratch("order");
        let (script, calls) = recorder(&dir, 0);
        let env = [("FERRITE_BIN", "/f".into()), ("FERRITE_SOCKET", "/s".into())];
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
        // A binary that is not there is logged, not a panic.
        startup(&dir.join("missing"), &dir, &[]);
    }
}

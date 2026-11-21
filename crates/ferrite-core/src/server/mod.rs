//! The Server (ADR 0025) as the rest of Ferrite sees it: where its Store
//! and settings live, keeping one running (`agent`), when a window asks it
//! to restart, and the machine's GitHub and provider CLIs, which a Server
//! reads for its windows and a window that runs its own agents reads
//! itself (`github`, `clis`). The process itself, `ferrite server` and
//! `ferrited`, is the `ferrite-server` crate.

use std::path::PathBuf;

use crate::protocol::ServerInfo;

pub mod agent;
pub mod clis;
pub mod github;

/// Where the Threads live: `FERRITE_STORE`, else `~/.ferrite/threads`.
pub fn store_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("FERRITE_STORE") {
        return dir.into();
    }
    // Windows spells the home directory USERPROFILE, not HOME.
    let base = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .unwrap_or_else(|| ".".into());
    PathBuf::from(base).join(".ferrite").join("threads")
}

/// `~/.ferrite`: the Store's parent, where the settings and logs live.
pub fn settings_dir() -> PathBuf {
    let store = store_dir();
    store
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or(store)
}

/// Whether a window should ask its Server to restart into a newer build:
/// only when this window is newer (equal versions never restart on their
/// own) and the Server runs this window's own executable — a Server always
/// restarts into its own path, so any other would be asked again after
/// every restart.
pub fn wants_restart(
    app_version: &str,
    app_executable: &std::path::Path,
    server: &ServerInfo,
) -> bool {
    let same = |a: &std::path::Path, b: &std::path::Path| {
        a == b
            || a.canonicalize()
                .ok()
                .zip(b.canonicalize().ok())
                .is_some_and(|(a, b)| a == b)
    };
    newer(app_version, &server.version) && same(app_executable, &server.executable)
}

/// A manual install may keep the same version. Compare the executable's
/// modification time with this Server's boot, including older servers that
/// do not advertise a build id. Never restart a different executable.
pub fn executable_replaced_since(
    app_executable: &std::path::Path,
    server: &ServerInfo,
    started: std::time::SystemTime,
) -> bool {
    let same = app_executable == server.executable
        || app_executable
            .canonicalize()
            .ok()
            .zip(server.executable.canonicalize().ok())
            .is_some_and(|(a, b)| a == b);
    same && std::fs::metadata(app_executable)
        .and_then(|m| m.modified())
        .ok()
        .is_some_and(|modified| modified > started)
}

/// Whether semantic version `a` is newer than `b`. Anything unreadable is
/// not newer.
fn newer(a: &str, b: &str) -> bool {
    let parse = |version: &str| -> Option<(u64, u64, u64)> {
        let core = version.split(['-', '+']).next()?;
        let mut parts = core.split('.').map(str::parse::<u64>);
        Some((
            parts.next()?.ok()?,
            parts.next()?.ok()?,
            parts.next()?.ok()?,
        ))
    };
    matches!((parse(a), parse(b)), (Some(a), Some(b)) if a > b)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn only_a_newer_window_running_the_same_executable_asks_for_a_restart() {
        let exe = std::env::current_exe().unwrap();
        let server = |version: &str, executable: &Path| ServerInfo {
            version: version.into(),
            executable: executable.to_owned(),
        };
        assert!(wants_restart("0.10.0", &exe, &server("0.9.3", &exe)));
        assert!(
            !wants_restart("0.9.3", &exe, &server("0.9.3", &exe)),
            "equal never"
        );
        assert!(
            !wants_restart("0.9.0", &exe, &server("0.10.0", &exe)),
            "older never"
        );
        assert!(
            !wants_restart(
                "0.10.0",
                &exe,
                &server(
                    "0.9.3",
                    Path::new("/Applications/Ferrite.app/Contents/MacOS/ferrite")
                )
            ),
            "another install's Server restarts into itself"
        );
        assert!(!wants_restart("garbage", &exe, &server("0.9.3", &exe)));
        assert!(wants_restart("1.0.0-beta.1", &exe, &server("0.9.9", &exe)));
    }

    #[test]
    fn manual_install_with_the_same_version_is_detected() {
        use std::time::{Duration, SystemTime};
        let dir = std::env::temp_dir().join(format!("ferrite-build-age-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let executable = dir.join("ferrite");
        let file = std::fs::File::create(&executable).unwrap();
        let boot = SystemTime::now() - Duration::from_secs(60);
        let server = ServerInfo {
            version: "0.4.0".into(),
            executable: executable.clone(),
        };
        file.set_modified(boot - Duration::from_secs(1)).unwrap();
        assert!(!executable_replaced_since(&executable, &server, boot));
        file.set_modified(boot + Duration::from_secs(1)).unwrap();
        assert!(executable_replaced_since(&executable, &server, boot));
        assert!(!executable_replaced_since(
            &dir.join("another"),
            &server,
            boot
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

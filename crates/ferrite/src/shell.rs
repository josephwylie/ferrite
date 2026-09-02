//! The environment a Dock launch lacks.
//!
//! A Terminal launch inherits the operator's login shell: the PATH their
//! profile builds (`~/.local/bin`, Homebrew, a node version manager) and
//! whatever else they export there. A launch from the Dock, Spotlight or
//! the Finder comes from launchd, whose PATH is the four system
//! directories — so the same binary that finds `claude` in a terminal
//! reports it missing from the app bundle. Before anything can spawn, a
//! launch that no terminal handed its environment asks the login shell
//! for its own, once, and adopts it. Windows is untouched: its apps
//! inherit the user's environment wherever they start.

/// Adopt the login shell's environment when this launch had no terminal.
/// `TERM` is the tell: every terminal sets it and launchd never does.
#[cfg(unix)]
pub fn adopt_login_environment() {
    if std::env::var_os("TERM").is_some() {
        return;
    }
    let Some(shell) = std::env::var_os("SHELL") else {
        return;
    };
    match unix::probe(std::path::Path::new(&shell)) {
        Ok(vars) => {
            for (key, value) in vars {
                std::env::set_var(key, value);
            }
        }
        Err(e) => eprintln!("ferrite: the login shell's environment could not be read: {e}"),
    }
}

#[cfg(not(unix))]
pub fn adopt_login_environment() {}

#[cfg(unix)]
mod unix {
    use std::ffi::OsString;
    use std::io;
    use std::os::unix::ffi::OsStringExt;
    use std::path::Path;
    use std::process::{Command, Stdio};

    /// Where the shell's own chatter ends and the listing begins: an rc
    /// file prints what it likes before the command runs.
    const MARKER: &str = "FERRITE_LOGIN_ENV";

    /// The probe shell's bookkeeping about itself, not the operator's
    /// environment.
    const SHELLS_OWN: [&str; 4] = ["_", "SHLVL", "PWD", "OLDPWD"];

    /// The environment an interactive login of `shell` ends up with:
    /// `-l -i` so the profile and the rc file both run, `cd $HOME` so
    /// directory hooks (direnv, mise) see the directory a new terminal
    /// opens in. The final `exit 0` keeps a failing last line of an rc
    /// file from failing the probe.
    pub fn probe(shell: &Path) -> io::Result<Vec<(OsString, OsString)>> {
        let mut script = String::new();
        if let Some(home) = std::env::var_os("HOME") {
            if let Some(home) = home.to_str() {
                script.push_str(&format!("cd '{}'; ", home.replace('\'', "'\\''")));
            }
        }
        script.push_str(&format!("printf '%s' {MARKER}; /usr/bin/env -0; exit 0"));
        let output = Command::new(shell)
            .args(["-l", "-i", "-c", &script])
            .stdin(Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "{} {}",
                shell.display(),
                output.status
            )));
        }
        Ok(parse(&output.stdout))
    }

    /// The `env -0` listing after the marker, minus the shell's own. A
    /// value keeps every byte and may itself contain `=`.
    pub fn parse(output: &[u8]) -> Vec<(OsString, OsString)> {
        let Some(at) = output
            .windows(MARKER.len())
            .position(|window| window == MARKER.as_bytes())
        else {
            return Vec::new();
        };
        output[at + MARKER.len()..]
            .split(|byte| *byte == 0)
            .filter_map(|entry| {
                let eq = entry.iter().position(|byte| *byte == b'=')?;
                let (key, value) = (&entry[..eq], &entry[eq + 1..]);
                if key.is_empty() || SHELLS_OWN.iter().any(|own| own.as_bytes() == key) {
                    return None;
                }
                Some((
                    OsString::from_vec(key.to_vec()),
                    OsString::from_vec(value.to_vec()),
                ))
            })
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_listing_after_the_marker_is_adopted_and_the_shells_own_lines_are_not() {
            let output = b"rc chatter\nFERRITE_LOGIN_ENVPATH=/a/bin:/b\0KEY=x=y\0_=/usr/bin/env\0SHLVL=1\0PWD=/home\0OLDPWD=/\0=bad\0";
            let vars = parse(output);
            assert_eq!(
                vars,
                vec![
                    ("PATH".into(), "/a/bin:/b".into()),
                    ("KEY".into(), "x=y".into())
                ]
            );
        }

        #[test]
        fn no_marker_means_nothing_is_adopted() {
            assert!(parse(b"PATH=/leaked\0").is_empty());
        }

        /// `/bin/sh` is on every unix; its login environment names a PATH.
        #[test]
        fn a_real_login_shell_reports_its_path() {
            let vars = probe(Path::new("/bin/sh")).expect("/bin/sh answers");
            assert!(vars.iter().any(|(key, _)| key == "PATH"), "{vars:?}");
        }
    }
}

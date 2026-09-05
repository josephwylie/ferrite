//! Which copy of a provider CLI to run.
//!
//! A machine collects several: Homebrew's node, nvm's, the vendor's own
//! installer under `~/.local`, volta, bun. Whichever comes first on PATH
//! wins a bare `claude` or `codex` — and PATH differs between a terminal
//! (nvm loaded by `.zshrc`) and everything else, so the operator's terminal
//! and the app disagree about "the" version. Ferrite instead looks at every
//! copy it can find, asks each its version once, and runs the newest.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use crate::store::Provider;

/// One installed copy of a CLI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    pub path: PathBuf,
    /// The version as `--version` printed it (`2.1.259`, `0.153.0`).
    pub version: String,
    pub parsed: [u64; 3],
}

/// The directories searched beyond PATH — where installers put a CLI
/// without necessarily putting it on every PATH. nvm's bins are one per
/// node version. Only directories that exist are kept.
fn well_known_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(home) = home {
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".claude/local"));
        dirs.push(home.join(".volta/bin"));
        dirs.push(home.join(".bun/bin"));
        dirs.push(home.join(".npm-global/bin"));
        if let Ok(nodes) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            for node in nodes.flatten() {
                dirs.push(node.path().join("bin"));
            }
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("npm"));
        }
    }
    dirs.into_iter().filter(|dir| dir.is_dir()).collect()
}

/// Every executable named `program` in `path` (in order) and then in the
/// well-known directories, one entry per real file: two links to one
/// binary count once, at the first place they were seen.
pub fn candidates(program: &str, path: Option<&OsStr>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = path
        .map(|path| std::env::split_paths(path).collect())
        .unwrap_or_default();
    dirs.extend(well_known_dirs());
    let names: Vec<String> = if cfg!(windows) {
        vec![
            format!("{program}.exe"),
            format!("{program}.cmd"),
            format!("{program}.bat"),
        ]
    } else {
        vec![program.to_string()]
    };
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut found: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        if dir.as_os_str().is_empty() {
            continue;
        }
        for name in &names {
            let candidate = dir.join(name);
            if !candidate.is_file() {
                continue;
            }
            let real = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
            if seen.contains(&real) {
                continue;
            }
            seen.push(real);
            found.push(candidate);
        }
    }
    found
}

/// The newest copy among `candidates`, each asked its version by
/// `version_of`; copies that answer nothing are skipped. Equal versions
/// keep the earlier (PATH-order) copy.
pub fn newest(
    candidates: &[PathBuf],
    version_of: impl Fn(&Path) -> Option<(String, [u64; 3])>,
) -> Option<Located> {
    let mut best: Option<Located> = None;
    for path in candidates {
        let Some((version, parsed)) = version_of(path) else {
            continue;
        };
        if best.as_ref().is_none_or(|best| parsed > best.parsed) {
            best = Some(Located {
                path: path.clone(),
                version,
                parsed,
            });
        }
    }
    best
}

/// `<path> --version`'s first line, parsed by the provider's own reader.
fn probe(path: &Path, parse: fn(&str) -> Option<(String, [u64; 3])>) -> Option<(String, [u64; 3])> {
    let output = Command::new(path).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse(&String::from_utf8_lossy(&output.stdout))
}

fn parser(provider: Provider) -> fn(&str) -> Option<(String, [u64; 3])> {
    match provider {
        Provider::Claude => super::claude::parse_version,
        Provider::Codex => super::codex::parse_version,
    }
}

fn bare_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    }
}

/// The copies found and probed, kept for the life of the process: a
/// version probe is a process start, and the answer does not change
/// under a running app. `rediscover` forgets it. One slot per Provider —
/// `Provider` is not hashable, and there are two.
#[derive(Default)]
struct Cache {
    claude: Option<Option<Located>>,
    codex: Option<Option<Located>>,
}

impl Cache {
    fn slot(&mut self, provider: Provider) -> &mut Option<Option<Located>> {
        match provider {
            Provider::Claude => &mut self.claude,
            Provider::Codex => &mut self.codex,
        }
    }
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}

/// The newest installed copy of `provider`'s CLI, probed once. None when
/// no copy answers `--version` anywhere Ferrite looked.
pub fn located(provider: Provider) -> Option<Located> {
    if let Some(known) = cache()
        .lock()
        .ok()
        .and_then(|mut cache| cache.slot(provider).clone())
    {
        return known;
    }
    let parse = parser(provider);
    let found = newest(
        &candidates(bare_name(provider), std::env::var_os("PATH").as_deref()),
        |path| probe(path, parse),
    );
    if let Ok(mut cache) = cache().lock() {
        *cache.slot(provider) = Some(found.clone());
    }
    found
}

/// The program to exec for `provider`: the newest copy's full path, else
/// the bare name (so a copy that appears later on PATH still runs, and the
/// spawn error names what was looked for).
pub fn program(provider: Provider) -> String {
    match located(provider) {
        Some(found) => found.path.to_string_lossy().into_owned(),
        None => bare_name(provider).to_string(),
    }
}

/// Forget the probes, so the next ask looks again — after the operator
/// installs or upgrades a CLI.
pub fn rediscover() {
    if let Ok(mut cache) = cache().lock() {
        *cache = Cache::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Discovery also walks real install directories. A fixture-only name
    // keeps the operator's installed Codex out of these version comparisons.
    #[cfg(unix)]
    const STUB_PROGRAM: &str = "ferrite-discovery-test-codex";

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-discover-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    fn stub(dir: &Path, version: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(STUB_PROGRAM);
        fs::write(&path, format!("#!/bin/sh\necho codex-cli {version}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// Two copies on PATH: the newer wins wherever it sits, and a link to
    /// the same binary is not a second candidate.
    #[cfg(unix)]
    #[test]
    fn the_newest_copy_wins_and_links_do_not_double_count() {
        let first = scratch("first");
        let second = scratch("second");
        let third = scratch("third");
        let older = stub(&first, "0.144.4");
        let newer = stub(&second, "0.153.0");
        std::os::unix::fs::symlink(&newer, third.join(STUB_PROGRAM)).unwrap();
        let path = std::env::join_paths([&first, &second, &third]).unwrap();
        let found = candidates(STUB_PROGRAM, Some(&path));
        assert!(
            found.starts_with(&[older.clone(), newer.clone()]),
            "{found:?}"
        );
        assert!(
            !found.contains(&third.join(STUB_PROGRAM)),
            "the link is the same file: {found:?}"
        );
        let best = newest(&found, |path| {
            probe(path, super::super::codex::parse_version)
        })
        .unwrap();
        assert_eq!(best.path, newer);
        assert_eq!(best.version, "0.153.0");
    }

    /// A copy that cannot answer is skipped rather than chosen or fatal.
    #[cfg(unix)]
    #[test]
    fn a_mute_copy_is_skipped() {
        let dir = scratch("mute");
        let mute = dir.join("codex");
        fs::write(&mute, "").unwrap();
        let good = scratch("good");
        let answering = stub(&good, "0.150.0");
        let best = newest(&[mute, answering.clone()], |path| {
            probe(path, super::super::codex::parse_version)
        })
        .unwrap();
        assert_eq!(best.path, answering);
    }

    #[test]
    fn equal_versions_keep_path_order() {
        let a = PathBuf::from("/a/codex");
        let b = PathBuf::from("/b/codex");
        let best = newest(&[a.clone(), b], |_| Some(("1.0.0".into(), [1, 0, 0]))).unwrap();
        assert_eq!(best.path, a);
    }

    #[test]
    fn nothing_found_is_none_and_the_program_is_the_bare_name() {
        assert_eq!(newest(&[], |_| None), None);
        assert_eq!(
            candidates("no-such-cli-here", Some(OsStr::new(""))),
            Vec::<PathBuf>::new()
        );
    }
}

//! Ferrite kept current from its own GitHub Releases (ADR 0020).
//!
//! The window asks GitHub for the newest release now and then (`check`),
//! fetches this platform's installer into `<dir>/update` and checks it
//! against the SHA-256 GitHub records for the asset (`download`), and
//! leaves it there. Nothing is replaced while Ferrite runs: the agents are
//! the app's children, so swapping the app under them would end their
//! turns. The swap happens at the next launch, before any Session starts
//! (`at_launch`, `install`); the window only offers to restart once no
//! agent is working. The release's notes are kept beside the installer, so
//! the relaunched window can say what changed.

use std::convert::Infallible;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::spawn::NoConsoleWindow;

/// Where Ferrite's releases are published (`.github/workflows/release.yml`).
const RELEASES: &str = "https://api.github.com/repos/josephwylie/ferrite/releases/latest";

/// Generous for one small JSON document; a check that overruns it is
/// simply tried again later.
const API_TIMEOUT: Duration = Duration::from_secs(10);

/// An upper bound for a few hundred MB, so a stalled download ends.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// The release asset this platform installs from, by the end of its name
/// (`MACOS_INSTALLER` and `WINDOWS_INSTALLER` in `release.yml`). `None`:
/// no installer is built for this platform.
const ASSET_SUFFIX: Option<&str> = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some("-aarch64-apple-darwin.dmg")
} else if cfg!(all(windows, target_arch = "x86_64")) {
    Some("-x86_64-pc-windows-msvc-setup.exe")
} else {
    None
};

/// The staged release's record, beside its installer.
const MANIFEST: &str = "release.json";

/// A published release newer than the running Ferrite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Without the tag's `v`: `0.5.0`.
    pub version: String,
    /// The release's body, Markdown.
    pub notes: String,
    pub asset: Asset,
}

/// This platform's installer within a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    /// Lowercase hex, as GitHub records it on upload.
    pub sha256: String,
}

/// A downloaded, verified installer waiting for the next launch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Staged {
    pub version: String,
    pub notes: String,
    /// The installer's file name, beside the manifest.
    installer: String,
    /// Set just before an install is tried: a launch that finds it set
    /// while still the older Ferrite knows that attempt did not land.
    #[serde(default)]
    attempted: bool,
}

/// What a launch should do about a staged release.
#[derive(Debug, PartialEq, Eq)]
pub enum AtLaunch {
    Nothing,
    /// A newer release is staged: install it before anything starts.
    Install(Staged),
    /// The last launch tried to install this and Ferrite is still the
    /// older one: the installer goes to the operator (`hand_over`).
    Failed(Staged),
    /// This launch is the release that was staged: its notes, said once.
    Installed(Staged),
}

/// `1.2.3` or `v1.2.3`; anything else (a pre-release suffix included) is
/// no version Ferrite updates to.
pub fn parse_version(text: &str) -> Option<[u64; 3]> {
    let mut parts = text.strip_prefix('v').unwrap_or(text).split('.');
    let mut version = [0; 3];
    for part in &mut version {
        *part = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(version)
}

/// Whether `candidate` is a later release than `current`.
pub fn newer(candidate: &str, current: &str) -> bool {
    matches!(
        (parse_version(candidate), parse_version(current)),
        (Some(candidate), Some(current)) if candidate > current
    )
}

/// Whether this copy of Ferrite can update itself: an installer is built
/// for the platform, and on macOS it runs from an app bundle to replace.
pub fn supported() -> bool {
    ASSET_SUFFIX.is_some() && (!cfg!(target_os = "macos") || bundle().is_some())
}

/// The release GitHub's `releases/latest` answer describes, with the
/// asset whose name ends in `suffix`.
pub fn parse_release(body: &str, suffix: &str) -> io::Result<Release> {
    #[derive(Deserialize)]
    struct Answer {
        tag_name: String,
        #[serde(default)]
        body: Option<String>,
        assets: Vec<AnswerAsset>,
    }
    #[derive(Deserialize)]
    struct AnswerAsset {
        name: String,
        browser_download_url: String,
        digest: Option<String>,
    }
    let answer: Answer = serde_json::from_str(body).map_err(io::Error::other)?;
    let version = answer.tag_name.trim_start_matches('v').to_string();
    if parse_version(&version).is_none() {
        return Err(io::Error::other(format!(
            "the latest release's tag `{}` is no version",
            answer.tag_name
        )));
    }
    let asset = answer
        .assets
        .into_iter()
        .find(|asset| asset.name.ends_with(suffix))
        .ok_or_else(|| io::Error::other(format!("{version} has no installer for this platform")))?;
    // The name becomes a file name in the staging directory.
    if asset.name.starts_with('.') || asset.name.contains(['/', '\\']) {
        return Err(io::Error::other(format!(
            "the installer's name `{}` is not a plain file name",
            asset.name
        )));
    }
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| io::Error::other(format!("{} has no published checksum", asset.name)))?;
    Ok(Release {
        version,
        notes: answer.body.unwrap_or_default(),
        asset: Asset {
            name: asset.name,
            url: asset.browser_download_url,
            sha256,
        },
    })
}

/// The newest release when it is newer than `current`. Blocks on the
/// network; call it off the UI thread.
pub fn check(current: &str) -> io::Result<Option<Release>> {
    let suffix = ASSET_SUFFIX
        .ok_or_else(|| io::Error::other("Ferrite does not update itself on this platform"))?;
    let body = ureq::get(RELEASES)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", concat!("Ferrite/", env!("CARGO_PKG_VERSION")))
        .config()
        .timeout_global(Some(API_TIMEOUT))
        .build()
        .call()
        .map_err(io::Error::other)?
        .body_mut()
        .read_to_string()
        .map_err(io::Error::other)?;
    let release = parse_release(&body, suffix)?;
    Ok(newer(&release.version, current).then_some(release))
}

fn staging(dir: &Path) -> PathBuf {
    dir.join("update")
}

/// Fetch `release`'s installer into `dir`'s staging directory, verify it
/// against its checksum and record it for the next launch, replacing
/// whatever was staged before. Blocks for as long as the download takes.
pub fn download(dir: &Path, release: &Release) -> io::Result<Staged> {
    let staging = staging(dir);
    match fs::remove_dir_all(&staging) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    fs::create_dir_all(&staging)?;
    let partial = staging.join(format!("{}.part", release.asset.name));
    let mut response = ureq::get(&release.asset.url)
        .config()
        .timeout_connect(Some(API_TIMEOUT))
        .timeout_recv_body(Some(DOWNLOAD_TIMEOUT))
        .build()
        .call()
        .map_err(io::Error::other)?;
    let mut file = fs::File::create(&partial)?;
    let digest = copy_hashing(&mut response.body_mut().as_reader(), &mut file)?;
    file.sync_all()?;
    drop(file);
    if digest != release.asset.sha256 {
        let _ = fs::remove_file(&partial);
        return Err(io::Error::other(format!(
            "{} does not match its published checksum",
            release.asset.name
        )));
    }
    fs::rename(&partial, staging.join(&release.asset.name))?;
    let staged = Staged {
        version: release.version.clone(),
        notes: release.notes.clone(),
        installer: release.asset.name.clone(),
        attempted: false,
    };
    write_manifest(&staging, &staged)?;
    Ok(staged)
}

/// Copy `reader` to `writer`, answering the SHA-256 of what passed, in
/// lowercase hex.
fn copy_hashing(reader: &mut impl Read, writer: &mut impl Write) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hasher.update(&buffer[..read]);
        writer.write_all(&buffer[..read])?;
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Written whole, then renamed over: a manifest is never half there.
fn write_manifest(staging: &Path, staged: &Staged) -> io::Result<()> {
    let text = serde_json::to_string_pretty(staged).map_err(io::Error::other)?;
    let partial = staging.join(format!("{MANIFEST}.tmp"));
    let mut file = fs::File::create(&partial)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    fs::rename(partial, staging.join(MANIFEST))
}

/// What the launch of `current` should do about `dir`'s staged release,
/// tidying as it reads: a release already shown or superseded goes, and
/// so does an installer left over from a hand-over.
pub fn at_launch(dir: &Path, current: &str) -> AtLaunch {
    let staging = staging(dir);
    let staged = fs::read_to_string(staging.join(MANIFEST))
        .ok()
        .and_then(|text| serde_json::from_str::<Staged>(&text).ok());
    match staged {
        Some(staged) if parse_version(&staged.version) == parse_version(current) => {
            let _ = fs::remove_dir_all(&staging);
            AtLaunch::Installed(staged)
        }
        Some(staged)
            if newer(&staged.version, current) && staging.join(&staged.installer).is_file() =>
        {
            if staged.attempted {
                AtLaunch::Failed(staged)
            } else {
                AtLaunch::Install(staged)
            }
        }
        _ => {
            let _ = fs::remove_dir_all(&staging);
            AtLaunch::Nothing
        }
    }
}

/// Install `staged` in place of the running Ferrite and hand over to it:
/// on macOS the new bundle replaces this one and this process becomes it;
/// on Windows the installer runs silently once this process has exited,
/// then starts Ferrite again, and this process exits. Returns only when
/// that could not be done.
pub fn install(dir: &Path, staged: &Staged) -> io::Result<Infallible> {
    let staging = staging(dir);
    write_manifest(
        &staging,
        &Staged {
            attempted: true,
            ..staged.clone()
        },
    )?;
    let installer = staging.join(&staged.installer);
    platform::install(&installer, &staging)
}

/// Give the installer to the operator, for when Ferrite could not install
/// it itself: the disk image opens, or the setup runs as usual. The
/// manifest goes, so the next launch does not try again (and has no notes
/// to show).
pub fn hand_over(dir: &Path, staged: &Staged) -> io::Result<()> {
    let staging = staging(dir);
    fs::remove_file(staging.join(MANIFEST))?;
    platform::open(&staging.join(&staged.installer))
}

/// Start Ferrite again once this process has exited, so a restart for an
/// update is one press: the new launch installs what is staged.
pub fn relaunch_after_exit() -> io::Result<()> {
    platform::relaunch_after_exit()
}

/// The `.app` this process runs from, if any.
fn bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|dir| dir.extension().is_some_and(|ext| ext == "app"))
        .map(Path::to_path_buf)
}

/// Run `command` detached from Ferrite: no stdio, so it outlives the app.
#[cfg(any(target_os = "macos", windows))]
fn detach(command: &mut Command) -> io::Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .no_console_window()
        .spawn()
        .map(drop)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::os::unix::process::CommandExt;

    /// Mount the image, copy its app beside the running bundle, swap the
    /// two by rename, then become the new copy. The running binary stays
    /// readable once its bundle moves: it is open, not looked up again.
    pub fn install(dmg: &Path, staging: &Path) -> io::Result<Infallible> {
        let bundle =
            bundle().ok_or_else(|| io::Error::other("Ferrite is not running from an app"))?;
        let parent = bundle
            .parent()
            .ok_or_else(|| io::Error::other("the app has no folder"))?;
        let name = bundle.file_name().unwrap_or_default().to_string_lossy();
        // The same binary inside the new bundle, found before the swap.
        let exe = std::env::current_exe()?;
        let exe = bundle.join(exe.strip_prefix(&bundle).map_err(io::Error::other)?);
        let fresh = parent.join(format!(".{name}.new"));
        let old = parent.join(format!(".{name}.old"));
        let mount = staging.join("mount");
        fs::create_dir_all(&mount)?;
        run(Command::new("/usr/bin/hdiutil")
            .args([
                "attach",
                "-nobrowse",
                "-readonly",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(&mount)
            .arg(dmg))?;
        let copied = (|| {
            let app = fs::read_dir(&mount)?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| path.extension().is_some_and(|ext| ext == "app"))
                .ok_or_else(|| io::Error::other("the disk image holds no app"))?;
            let _ = fs::remove_dir_all(&fresh);
            run(Command::new("/usr/bin/ditto").arg(&app).arg(&fresh))
        })();
        let detached = run(Command::new("/usr/bin/hdiutil")
            .args(["detach", "-quiet"])
            .arg(&mount));
        copied?;
        if let Err(error) = detached {
            crate::log!("the update's disk image did not detach: {error}");
        }
        let _ = fs::remove_dir_all(&old);
        fs::rename(&bundle, &old)?;
        if let Err(error) = fs::rename(&fresh, &bundle) {
            let _ = fs::rename(&old, &bundle);
            return Err(error);
        }
        let _ = fs::remove_dir_all(&old);
        let _ = fs::remove_file(dmg);
        Err(Command::new(exe).args(std::env::args_os().skip(1)).exec())
    }

    pub fn open(path: &Path) -> io::Result<()> {
        run(Command::new("/usr/bin/open").arg(path))
    }

    pub fn relaunch_after_exit() -> io::Result<()> {
        let bundle =
            bundle().ok_or_else(|| io::Error::other("Ferrite is not running from an app"))?;
        detach(
            Command::new("/bin/sh")
                .arg("-c")
                .arg(r#"while kill -0 "$0" 2>/dev/null; do sleep 0.2; done; exec /usr/bin/open "$1""#)
                .arg(std::process::id().to_string())
                .arg(bundle),
        )
    }

    fn run(command: &mut Command) -> io::Result<()> {
        let output = command.stdin(Stdio::null()).output()?;
        if output.status.success() {
            return Ok(());
        }
        Err(io::Error::other(format!(
            "{:?} {}: {}",
            command.get_program(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

#[cfg(windows)]
mod platform {
    use super::*;

    /// The setup runs from PowerShell once Ferrite has exited (its files
    /// are in use until then), silently but with its progress shown, and
    /// Ferrite starts again whatever the setup's outcome: a failed install
    /// is the older Ferrite finding its attempt (`AtLaunch::Failed`).
    pub fn install(setup: &Path, _staging: &Path) -> io::Result<Infallible> {
        let exe = std::env::current_exe()?;
        after_exit(&format!(
            "Start-Process -Wait -FilePath {} -ArgumentList '/SILENT','/SUPPRESSMSGBOXES','/NORESTART'; \
             Start-Process -FilePath {}",
            quote(setup),
            quote(&exe)
        ))?;
        std::process::exit(0)
    }

    pub fn open(setup: &Path) -> io::Result<()> {
        Command::new(setup).spawn().map(drop)
    }

    pub fn relaunch_after_exit() -> io::Result<()> {
        let exe = std::env::current_exe()?;
        after_exit(&format!("Start-Process -FilePath {}", quote(&exe)))
    }

    fn after_exit(then: &str) -> io::Result<()> {
        let script = format!(
            "Wait-Process -Id {} -ErrorAction SilentlyContinue; {then}",
            std::process::id()
        );
        detach(Command::new("powershell").args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script.as_str(),
        ]))
    }

    fn quote(path: &Path) -> String {
        super::powershell_quote(&path.to_string_lossy())
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use super::*;

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "Ferrite does not update itself on this platform",
        )
    }

    pub fn install(_: &Path, _: &Path) -> io::Result<Infallible> {
        Err(unsupported())
    }

    pub fn open(_: &Path) -> io::Result<()> {
        Err(unsupported())
    }

    pub fn relaunch_after_exit() -> io::Result<()> {
        Err(unsupported())
    }
}

/// `text` as one PowerShell single-quoted string: a quote inside is
/// doubled, and nothing else is special there.
#[cfg_attr(not(windows), allow(dead_code))]
fn powershell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUFFIX: &str = "-aarch64-apple-darwin.dmg";
    const DIGEST: &str = "3a1c6e0b2f5d4e7a9b8c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a";

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-self-update-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn answer(tag: &str, digest: Option<&str>) -> String {
        serde_json::json!({
            "tag_name": tag,
            "body": "## What's Changed\n* Self-update",
            "assets": [
                {
                    "name": format!("ferrite-{tag}-x86_64-pc-windows-msvc-setup.exe"),
                    "browser_download_url": "https://example.invalid/setup.exe",
                    "digest": digest,
                },
                {
                    "name": format!("ferrite-{tag}{SUFFIX}"),
                    "browser_download_url": "https://example.invalid/ferrite.dmg",
                    "digest": digest,
                },
            ],
        })
        .to_string()
    }

    fn stage(dir: &Path, version: &str, attempted: bool) -> Staged {
        let staging = staging(dir);
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("ferrite.dmg"), b"image").unwrap();
        let staged = Staged {
            version: version.into(),
            notes: "notes".into(),
            installer: "ferrite.dmg".into(),
            attempted,
        };
        write_manifest(&staging, &staged).unwrap();
        staged
    }

    #[test]
    fn versions_compare_numerically_and_tags_lose_their_v() {
        assert_eq!(parse_version("v0.10.2"), Some([0, 10, 2]));
        assert_eq!(parse_version("0.5.0-rc.1"), None);
        assert_eq!(parse_version("1.2"), None);
        assert!(newer("0.10.0", "0.9.9"));
        assert!(!newer("0.4.0", "0.4.0"));
        assert!(!newer("garbage", "0.4.0"));
    }

    #[test]
    fn the_release_names_this_platforms_installer_and_its_checksum() {
        let release =
            parse_release(&answer("v0.5.0", Some(&format!("sha256:{DIGEST}"))), SUFFIX).unwrap();
        assert_eq!(release.version, "0.5.0");
        assert_eq!(release.notes, "## What's Changed\n* Self-update");
        assert_eq!(
            release.asset.name,
            "ferrite-v0.5.0-aarch64-apple-darwin.dmg"
        );
        assert_eq!(release.asset.url, "https://example.invalid/ferrite.dmg");
        assert_eq!(release.asset.sha256, DIGEST);
    }

    /// No checksum, no download: an unverifiable installer is never run.
    #[test]
    fn a_release_without_a_checksum_is_refused() {
        assert!(parse_release(&answer("v0.5.0", None), SUFFIX).is_err());
        assert!(parse_release(&answer("v0.5.0", Some("sha1:abc")), SUFFIX).is_err());
        assert!(parse_release(
            &answer("v0.5.0", Some(&format!("sha256:{DIGEST}"))),
            ".AppImage"
        )
        .is_err());
    }

    #[test]
    fn the_hash_is_of_exactly_the_bytes_copied() {
        let mut out = Vec::new();
        let digest = copy_hashing(&mut &b"abc"[..], &mut out).unwrap();
        assert_eq!(out, b"abc");
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_staged_newer_release_installs_at_launch() {
        let dir = scratch("install");
        let staged = stage(&dir, "0.5.0", false);
        assert_eq!(at_launch(&dir, "0.4.0"), AtLaunch::Install(staged));
    }

    /// The attempt is marked first, so a setup that failed is handed to
    /// the operator next launch rather than retried at every launch.
    #[test]
    fn an_attempt_that_did_not_land_is_handed_over() {
        let dir = scratch("failed");
        let staged = stage(&dir, "0.5.0", true);
        assert_eq!(at_launch(&dir, "0.4.0"), AtLaunch::Failed(staged.clone()));
        // `hand_over` drops the manifest; the installer stays for the
        // operator until the following launch tidies it away.
        fs::remove_file(staging(&dir).join(MANIFEST)).unwrap();
        assert_eq!(at_launch(&dir, "0.4.0"), AtLaunch::Nothing);
        assert!(!staging(&dir).exists());
    }

    /// The new version's first launch takes the notes once.
    #[test]
    fn the_installed_release_shows_its_notes_once() {
        let dir = scratch("installed");
        let staged = stage(&dir, "0.5.0", true);
        assert_eq!(at_launch(&dir, "0.5.0"), AtLaunch::Installed(staged));
        assert_eq!(at_launch(&dir, "0.5.0"), AtLaunch::Nothing);
    }

    #[test]
    fn a_superseded_release_is_cleared() {
        let dir = scratch("superseded");
        stage(&dir, "0.5.0", false);
        assert_eq!(at_launch(&dir, "0.6.0"), AtLaunch::Nothing);
        assert!(!staging(&dir).exists());
    }

    #[test]
    fn powershell_quotes_double_inner_quotes() {
        assert_eq!(
            powershell_quote(r"C:\Users\o'neil\ferrite.exe"),
            r"'C:\Users\o''neil\ferrite.exe'"
        );
    }
}
